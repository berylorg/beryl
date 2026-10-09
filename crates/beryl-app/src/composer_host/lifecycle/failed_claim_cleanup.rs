use super::super::publication::retained::RetainedComposerPublication;
use super::super::publication::{RetainedComposerCommandOutcome, RetainedComposerDisposal};
use super::*;
use beryl_home_store::{HomeCandidateRecoveryAccess, HomeCommand, HomeRecoveryCandidate};
use syndic_storage::{
    DraftEditorCandidateSessionDisposeOutcomeV1, DraftEditorCandidateSessionDisposeRequestV1,
    PreparedDraftEditorCandidateSessionDisposeV1,
};

mod publication_capture;
mod successor;
use publication_capture::RetiredClaimPublicationCapture;
pub(crate) use successor::{
    ComposerHostCompletedThreadSuccessorProgress, ComposerHostFailedThreadSuccessor,
};

pub(crate) struct ComposerHostRetiredClaimPredecessor {
    pub(crate) resident: Option<Box<super::failed_resident::ComposerHostFailedResident>>,
    saved: Option<ComposerHostSelectionSave>,
    original: Option<Box<RetainedComposerDisposal>>,
    unadmitted: Option<PreparedDraftEditorCandidateSessionDisposeV1>,
    recovery: Option<Box<RetainedComposerDisposal>>,
    publication: Option<Box<RetainedComposerPublication>>,
    publication_capture: Option<RetiredClaimPublicationCapture>,
    canonical_home: std::path::PathBuf,
    publication_settled: bool,
    disposed: bool,
}

impl SyndicComposerHost {
    pub(crate) fn failed_claim_predecessor_binding_matches(
        &self,
        expected: ComposerHostBinding,
    ) -> bool {
        self.active.as_ref().map_or_else(
            || {
                self.last_generation == Some(expected.host_generation())
                    && self
                        .publication
                        .retained_disposal
                        .as_ref()
                        .is_some_and(|retained| {
                            retained.binding == expected
                                && retained.outcome.known_commit() == Some(true)
                        })
            },
            |active| active.binding == expected,
        )
    }
    pub(crate) fn completed_claim_predecessor_runtime_is_drained(
        &self,
        store: &HomeStore,
        expected: ComposerHostBinding,
    ) -> bool {
        store.health().state() == beryl_home_store::HomeHealthState::Failed
            && store.home_id() == expected.home_id()
            && store.health().generation() == Some(expected.home_generation())
            && self.last_generation == Some(expected.host_generation())
            && self
                .active
                .as_ref()
                .is_none_or(|active| active.binding == expected && active.session_disposed)
            && self.pending.is_empty()
            && !self.live_operation_pending()
            && self.settlement_custody_in_use() == 0
            && !self.submission_pending()
            && self.publication.lane.is_none()
            && self.publication.retained_disposal.is_none()
            && self.publication.retained.is_none()
    }
    pub(crate) fn take_completed_claim_disposal(
        &mut self,
        store: &HomeStore,
        expected: ComposerHostBinding,
    ) -> Result<ComposerHostRetiredClaimPredecessor, ComposerHostError> {
        if self.last_generation != Some(expected.host_generation())
            || self
                .active
                .as_ref()
                .is_some_and(|active| active.binding != expected || !active.session_disposed)
            || self.publication.lane.is_some()
            || !self.pending.is_empty()
            || self.live_operation_pending()
            || self.settlement_custody_in_use() != 0
            || self.submission_pending()
            || self
                .publication
                .retained_disposal
                .as_ref()
                .is_none_or(|retained| {
                    retained.binding != expected || retained.outcome.known_commit() != Some(true)
                })
        {
            return Err(ComposerHostError::LifecycleBlocked);
        }
        Ok(ComposerHostRetiredClaimPredecessor {
            resident: None,
            saved: None,
            original: self.publication.retained_disposal.take(),
            unadmitted: None,
            recovery: None,
            publication: self.publication.retained.take(),
            publication_capture: None,
            canonical_home: store.canonical_path().to_owned(),
            publication_settled: false,
            disposed: true,
        })
    }
    pub(crate) fn retire_failed_claim_cleanup(
        mut self: Box<Self>,
        store: &HomeStore,
        saved: Option<ComposerHostSelectionSave>,
        committed: bool,
        markers: &crate::composer_marker_seal::DraftMarkerSealRetainedFlights,
    ) -> Result<ComposerHostRetiredClaimPredecessor, Box<Self>> {
        if store.health().state() != beryl_home_store::HomeHealthState::Failed
            || !self.pending.is_empty()
            || self.live_operation_pending()
            || self.settlement_custody_in_use() != 0
            || self.submission_pending()
        {
            return Err(self);
        }
        let disposal_lane = matches!(
            self.publication.lane.as_deref(),
            Some(ComposerHostPublicationLane::Disposal(_))
        );
        let disposed = self
            .active
            .as_ref()
            .is_none_or(|active| active.session_disposed);
        if (disposal_lane || disposed) && (!committed || saved.is_none()) {
            return Err(self);
        }
        if let Some(saved) = saved {
            if saved.binding().home_id() != store.home_id()
                || store.health().generation() != Some(saved.binding().home_generation())
                || self.last_generation != Some(saved.binding().host_generation())
                || self.active.as_ref().is_some_and(|active| {
                    active.binding != saved.binding()
                        || active.storage_candidate != saved.checkpoint()
                        || active.durable_selector != saved.selector()
                })
                || self
                    .publication
                    .retained_disposal
                    .as_ref()
                    .is_some_and(|retained| retained.binding != saved.binding())
            {
                return Err(self);
            }
        }
        if disposed
            && self
                .publication
                .retained_disposal
                .as_ref()
                .is_none_or(|retained| retained.outcome.known_commit() != Some(true))
        {
            return Err(self);
        }
        let lane = if disposal_lane {
            self.publication.lane.take()
        } else {
            None
        };
        let unadmitted = match lane.as_deref() {
            Some(ComposerHostPublicationLane::Disposal(pending))
                if self.publication.retained_disposal.is_none() =>
            {
                Some(pending.prepared.clone())
            }
            _ => None,
        };
        let original = self.publication.retained_disposal.take();
        if disposed {
            return Ok(ComposerHostRetiredClaimPredecessor {
                resident: None,
                saved,
                original,
                unadmitted,
                recovery: None,
                publication: self.publication.retained.take(),
                publication_capture: None,
                canonical_home: store.canonical_path().to_owned(),
                publication_settled: false,
                disposed: true,
            });
        }
        let publication = if original.is_some() || unadmitted.is_some() {
            self.publication.retained.take()
        } else {
            None
        };
        match self.retire_failed_resident_with_marker_custody(store, markers) {
            Ok(resident) => Ok(ComposerHostRetiredClaimPredecessor {
                resident: Some(Box::new(resident)),
                saved,
                original,
                unadmitted,
                recovery: None,
                publication,
                publication_capture: None,
                canonical_home: store.canonical_path().to_owned(),
                publication_settled: false,
                disposed: false,
            }),
            Err(mut host) => {
                host.publication.retained_disposal = original;
                if let Some(publication) = publication {
                    host.publication.retained = Some(publication);
                }
                if let Some(lane) = lane {
                    host.publication.lane = Some(lane);
                }
                Err(host)
            }
        }
    }
}

impl ComposerHostRetiredClaimPredecessor {
    pub(crate) fn predecessor_publication_is_settled(&self) -> bool {
        self.publication_settled
    }
    pub(crate) fn bind_original_save(
        &mut self,
        saved: ComposerHostSelectionSave,
    ) -> Result<(), String> {
        if self.saved.is_some_and(|prior| {
            prior.binding() != saved.binding()
                || prior.checkpoint() != saved.checkpoint()
                || prior.selector() != saved.selector()
        }) || self
            .original
            .as_ref()
            .is_some_and(|original| original.binding != saved.binding())
        {
            return Err("predecessor disposal and original saved source differ".into());
        }
        self.saved = Some(saved);
        Ok(())
    }
    pub(crate) fn settle_predecessor_publication(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        storage: &syndic_storage::SyndicStorage,
    ) -> Result<bool, String> {
        if let Some(capture) = self.publication_capture.as_ref() {
            capture.validate(
                self.resident
                    .as_ref()
                    .ok_or("original captured predecessor publication lost its resident")?,
            )?;
        }
        if self.publication_settled && self.disposed {
            let saved = self
                .saved
                .ok_or("disposed predecessor original save is missing")?;
            let access = candidate
                .recovery_access()
                .map_err(|error| error.to_string())?;
            if access.home_id() != saved.binding().home_id()
                || access.canonical_path() != self.canonical_home
                || access.generation() == saved.binding().home_generation()
            {
                return Err("disposed predecessor requires fresh same-home access".into());
            }
            let retained = self
                .original
                .as_ref()
                .filter(|retained| retained.outcome.known_commit() == Some(true))
                .or_else(|| {
                    self.recovery
                        .as_ref()
                        .filter(|retained| retained.outcome.known_commit() == Some(true))
                })
                .ok_or("disposed predecessor has no exact retained disposal outcome")?;
            authenticate_disposal(&access, storage, retained)?;
            return Ok(true);
        }
        if self.original.is_none()
            && self.unadmitted.is_none()
            && self.publication.is_none()
            && !self.disposed
        {
            let resident = self
                .resident
                .as_mut()
                .ok_or("original predecessor resident is missing")?;
            let saved = resident.qualify_saved(candidate, storage)?;
            self.publication_settled = true;
            return Ok(saved);
        }
        let saved = self
            .saved
            .ok_or("completed predecessor original save proof is missing")?;
        let access = candidate
            .recovery_access()
            .map_err(|error| error.to_string())?;
        if access.home_id() != saved.binding().home_id()
            || access.canonical_path() != self.canonical_home
            || access.generation() == saved.binding().home_generation()
        {
            return Err("predecessor publication requires fresh same-home candidate".into());
        }
        if let Some(publication) = self.publication.as_mut() {
            if !publication.outcome.reconcile(&access)? {
                return Err("disposed predecessor original publication proved noncommit".into());
            }
            match storage
                .reconcile_draft_editor_candidate_publication_candidate(
                    &access,
                    &publication.prepared.syndic,
                    publication.outcome.committed_classification().unwrap(),
                )
                .map_err(|error| error.to_string())?
            {
                syndic_storage::DraftEditorCandidatePublicationOutcomeV1::Published(
                    selector,
                    _,
                ) if selector == saved.selector() => {}
                syndic_storage::DraftEditorCandidatePublicationOutcomeV1::ExactReplay(receipt)
                    if receipt.successor_selector() == saved.selector() => {}
                _ => {
                    return Err(
                        "disposed predecessor original publication correspondence changed".into(),
                    );
                }
            }
        }
        if let Some(original) = self.original.as_mut() {
            if original.outcome.reconcile(&access)? {
                authenticate_disposal(&access, storage, original)?;
                self.disposed = true;
                self.publication_settled = true;
                return Ok(true);
            }
        }
        if self.disposed {
            return Err("completed predecessor original disposal is unavailable".into());
        }
        let qualified = self
            .resident
            .as_mut()
            .ok_or("original predecessor resident is missing")?
            .qualify_saved(candidate, storage)?;
        if !qualified {
            return Err("original saved predecessor changed after disposal noncommit".into());
        }
        self.publication_settled = true;
        Ok(true)
    }
    pub(crate) fn disposal_complete(&self) -> bool {
        self.disposed
    }
    pub(crate) fn saved(&self) -> Option<ComposerHostSelectionSave> {
        self.saved
    }
    pub(crate) fn into_prior(
        self,
    ) -> Result<super::failed_resident::ComposerHostFailedResident, Self> {
        if !self.publication_settled
            || self.disposed
            || self.original.is_some()
            || self.unadmitted.is_some()
            || self.recovery.is_some()
            || self.resident.is_none()
        {
            return Err(self);
        }
        Ok(*self.resident.unwrap())
    }
    pub(crate) fn settle_disposal(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        storage: &syndic_storage::SyndicStorage,
    ) -> Result<bool, String> {
        if !self.publication_settled {
            return Err("original predecessor publication has not settled".into());
        }
        let saved = self
            .saved
            .ok_or("committed predecessor original save proof is missing")?;
        let access = candidate
            .recovery_access()
            .map_err(|error| error.to_string())?;
        if access.home_id() != saved.binding().home_id()
            || access.canonical_path() != self.canonical_home
            || access.generation() == saved.binding().home_generation()
        {
            return Err("committed predecessor cleanup has another candidate".into());
        }
        if let Some(original) = self.original.as_mut() {
            if original.outcome.reconcile(&access)? {
                authenticate_disposal(&access, storage, original)?;
                self.disposed = true;
                return Ok(true);
            }
        }
        if let Some(recovery) = self.recovery.as_mut() {
            if recovery.outcome.reconcile(&access)? {
                authenticate_disposal(&access, storage, recovery)?;
                self.disposed = true;
                return Ok(true);
            }
            return Err("predecessor recovery disposal proved noncommit".into());
        }
        if self.disposed {
            return Err("completed predecessor disposal lost original outcome".into());
        }
        let prepared = match self
            .original
            .as_ref()
            .map(|original| original.prepared.clone())
            .or_else(|| self.unadmitted.clone())
        {
            Some(prepared) => prepared,
            None => {
                let mut bytes = [0; 16];
                getrandom::fill(&mut bytes).map_err(|error| error.to_string())?;
                let request = DraftEditorCandidateSessionDisposeRequestV1::new(
                    saved.checkpoint().draft_id(),
                    saved.checkpoint().session_id(),
                    syndic_storage::DraftPieceOperationIdV1::from_bytes(bytes),
                    saved.checkpoint().session_generation(),
                    syndic_storage::DraftRootHistoryPairV1::new(
                        saved.selector().root(),
                        saved.selector().history(),
                    ),
                );
                storage
                    .prepare_dispose_draft_editor_candidate_session_candidate(&access, request)
                    .map_err(|error| error.to_string())?
            }
        };
        let mut command =
            HomeCommand::new(access.home_revision().map_err(|error| error.to_string())?);
        command
            .add(
                storage
                    .dispose_draft_editor_candidate_session_candidate(
                        &access,
                        storage
                            .revision_candidate(&access)
                            .map_err(|error| error.to_string())?,
                        prepared.clone(),
                    )
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        self.recovery = Some(Box::new(RetainedComposerDisposal {
            binding: saved.binding(),
            prepared,
            outcome: RetainedComposerCommandOutcome::new(access.execute(command)),
        }));
        let recovery = self.recovery.as_mut().unwrap();
        if !recovery.outcome.reconcile(&access)? {
            return Err("predecessor recovery disposal proved noncommit".into());
        }
        authenticate_disposal(&access, storage, recovery)?;
        self.disposed = true;
        Ok(true)
    }
}

fn authenticate_disposal(
    access: &HomeCandidateRecoveryAccess<'_>,
    storage: &syndic_storage::SyndicStorage,
    retained: &RetainedComposerDisposal,
) -> Result<(), String> {
    let outcome = retained
        .outcome
        .committed_classification()
        .ok_or("predecessor disposal has no original committed outcome")?;
    match storage
        .reconcile_draft_editor_candidate_session_disposal_candidate(
            access,
            &retained.prepared,
            outcome,
        )
        .map_err(|error| error.to_string())?
    {
        DraftEditorCandidateSessionDisposeOutcomeV1::Disposed(_)
        | DraftEditorCandidateSessionDisposeOutcomeV1::ExactReplay(_) => Ok(()),
        _ => Err("predecessor disposal does not match its exact original custody".into()),
    }
}
