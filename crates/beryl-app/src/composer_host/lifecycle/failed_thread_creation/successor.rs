use super::*;
use syndic_storage::{
    DraftEditorCandidateSessionAbandonFreshOutcomeV1,
    PreparedDraftEditorCandidateSessionAbandonFreshV1,
};
mod completed;
pub(crate) use completed::ComposerHostCompletedThreadSuccessorProgress;

pub(crate) struct ComposerHostFailedThreadSuccessor {
    binding: ComposerHostBinding,
    canonical_home: std::path::PathBuf,
    request: DraftEditorCandidateSessionDisposeRequestV1,
    prepared: Option<PreparedDraftEditorCandidateSessionAbandonFreshV1>,
    original: Option<RetainedComposerCommandOutcome>,
    recovery: Option<(
        PreparedDraftEditorCandidateSessionAbandonFreshV1,
        RetainedComposerCommandOutcome,
    )>,
    disposed: bool,
}

impl SyndicComposerHost {
    pub(crate) fn thread_creation_fresh_abandonment_request(
        &self,
        operation: DraftPieceOperationIdV1,
    ) -> Option<DraftEditorCandidateSessionDisposeRequestV1> {
        let active = self.active.as_ref()?;
        if active.session_disposed
            || !self.pending.is_empty()
            || self.live_operation_pending()
            || self.settlement_custody_in_use() != 0
            || self.submission_pending()
            || self.publication.lane.is_some()
            || self.lifecycle.has_barrier()
            || active.storage_candidate != active.activation_candidate
        {
            return None;
        }
        Some(DraftEditorCandidateSessionDisposeRequestV1::new(
            active.storage_candidate.draft_id(),
            active.storage_candidate.session_id(),
            operation,
            active.storage_candidate.session_generation(),
            syndic_storage::DraftRootHistoryPairV1::new(
                active.storage_candidate.root(),
                active.storage_candidate.history(),
            ),
        ))
    }
    pub(crate) fn retain_completed_thread_successor(
        self: Box<Self>,
        store: &HomeStore,
        prepared: PreparedDraftEditorCandidateSessionAbandonFreshV1,
        original: RetainedComposerCommandOutcome,
    ) -> Result<
        ComposerHostFailedThreadSuccessor,
        (
            Box<Self>,
            PreparedDraftEditorCandidateSessionAbandonFreshV1,
            RetainedComposerCommandOutcome,
        ),
    > {
        let Some(active) = self.active.as_ref() else {
            return Err((self, prepared, original));
        };
        let request = prepared.request();
        if original.known_commit() != Some(true)
            || store.home_id() != active.binding.home_id()
            || store.health().generation() != Some(active.binding.home_generation())
            || active.storage_candidate != active.activation_candidate
            || self.thread_creation_fresh_abandonment_request(request.operation_id())
                != Some(request)
            || !self.pending.is_empty()
            || self.live_operation_pending()
            || self.settlement_custody_in_use() != 0
            || self.submission_pending()
            || self.publication.lane.is_some()
            || self.lifecycle.has_barrier()
        {
            return Err((self, prepared, original));
        }
        Ok(ComposerHostFailedThreadSuccessor {
            binding: active.binding,
            canonical_home: store.canonical_path().to_owned(),
            request,
            prepared: Some(prepared),
            original: Some(original),
            recovery: None,
            disposed: true,
        })
    }
    pub(crate) fn retire_failed_thread_successor(
        self: Box<Self>,
        store: &HomeStore,
        operation: DraftPieceOperationIdV1,
        prepared: Option<PreparedDraftEditorCandidateSessionAbandonFreshV1>,
        original: Option<RetainedComposerCommandOutcome>,
    ) -> Result<
        ComposerHostFailedThreadSuccessor,
        (
            Box<Self>,
            Option<PreparedDraftEditorCandidateSessionAbandonFreshV1>,
            Option<RetainedComposerCommandOutcome>,
        ),
    > {
        let Some(active) = self.active.as_ref() else {
            return Err((self, prepared, original));
        };
        if store.health().state() != beryl_home_store::HomeHealthState::Failed
            || store.health().generation() != Some(active.binding.home_generation())
            || store.home_id() != active.binding.home_id()
            || active.session_disposed
            || !self.pending.is_empty()
            || self.live_operation_pending()
            || self.settlement_custody_in_use() != 0
            || self.submission_pending()
            || self.publication.lane.is_some()
            || self.lifecycle.has_barrier()
            || active.storage_candidate != active.activation_candidate
        {
            return Err((self, prepared, original));
        }
        let request = DraftEditorCandidateSessionDisposeRequestV1::new(
            active.storage_candidate.draft_id(),
            active.storage_candidate.session_id(),
            operation,
            active.storage_candidate.session_generation(),
            syndic_storage::DraftRootHistoryPairV1::new(
                active.storage_candidate.root(),
                active.storage_candidate.history(),
            ),
        );
        if prepared
            .as_ref()
            .is_some_and(|prepared| prepared.request() != request)
            || original.is_some() && prepared.is_none()
        {
            return Err((self, prepared, original));
        }
        Ok(ComposerHostFailedThreadSuccessor {
            binding: active.binding,
            canonical_home: store.canonical_path().to_owned(),
            request,
            prepared,
            original,
            recovery: None,
            disposed: false,
        })
    }
}

impl ComposerHostFailedThreadSuccessor {
    pub(crate) fn binding(&self) -> ComposerHostBinding {
        self.binding
    }
    pub(crate) fn settle_cleanup(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        storage: &syndic_storage::SyndicStorage,
    ) -> Result<(), String> {
        let access = candidate
            .recovery_access()
            .map_err(|error| error.to_string())?;
        if access.home_id() != self.binding.home_id()
            || access.canonical_path() != self.canonical_home
            || access.generation() == self.binding.home_generation()
        {
            return Err("successor cleanup requires fresh same-home candidate".into());
        }
        if let Some(original) = self.original.as_mut() {
            if original.reconcile(&access)? {
                validate_abandonment(&access, storage, self.prepared.as_ref().unwrap(), original)?;
                self.disposed = true;
                return Ok(());
            }
        }
        if let Some((prepared, recovery)) = self.recovery.as_mut() {
            if !recovery.reconcile(&access)? {
                return Err("successor cleanup original recovery command proved noncommit".into());
            }
            validate_abandonment(&access, storage, prepared, recovery)?;
            self.disposed = true;
            return Ok(());
        }
        if self.disposed {
            return Err("completed successor cleanup lost its original outcome".into());
        }
        let prepared = match &self.prepared {
            Some(prepared) => prepared.clone(),
            None => storage
                .prepare_abandon_fresh_draft_editor_candidate_session_candidate(
                    &access,
                    self.request,
                )
                .map_err(|error| error.to_string())?,
        };
        let mut command =
            HomeCommand::new(access.home_revision().map_err(|error| error.to_string())?);
        command
            .add(
                storage.abandon_fresh_draft_editor_candidate_session(
                    storage
                        .revision_candidate(&access)
                        .map_err(|error| error.to_string())?,
                    prepared.clone(),
                ),
            )
            .map_err(|error| error.to_string())?;
        self.recovery = Some((
            prepared,
            RetainedComposerCommandOutcome::new(access.execute(command)),
        ));
        let (prepared, recovery) = self.recovery.as_mut().unwrap();
        if !recovery.reconcile(&access)? {
            return Err("successor cleanup proved noncommit".into());
        }
        validate_abandonment(&access, storage, prepared, recovery)?;
        self.disposed = true;
        Ok(())
    }
}

fn validate_abandonment(
    access: &HomeCandidateRecoveryAccess<'_>,
    storage: &syndic_storage::SyndicStorage,
    prepared: &PreparedDraftEditorCandidateSessionAbandonFreshV1,
    outcome: &RetainedComposerCommandOutcome,
) -> Result<(), String> {
    match storage
        .reconcile_abandon_fresh_draft_editor_candidate_session_candidate(
            access,
            prepared,
            outcome
                .committed_classification()
                .ok_or("successor abandonment has no original commitment")?,
        )
        .map_err(|error| error.to_string())?
    {
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(_)
        | DraftEditorCandidateSessionAbandonFreshOutcomeV1::ExactReplay(_) => Ok(()),
        _ => Err("successor abandonment differs from original retained intent".into()),
    }
}
