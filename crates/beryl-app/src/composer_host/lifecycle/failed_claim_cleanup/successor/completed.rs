use super::*;

pub(crate) struct ComposerHostCompletedThreadSuccessorProgress {
    binding: ComposerHostBinding,
    canonical_home: std::path::PathBuf,
    prepared: PreparedDraftEditorCandidateSessionAbandonFreshV1,
    outcome: RetainedComposerCommandOutcome,
    _indeterminate_failure: Option<beryl_home_store::CommandError>,
}

impl ComposerHostFailedThreadSuccessor {
    pub(crate) fn settle_current(
        mut self,
        store: &HomeStore,
        storage: &syndic_storage::SyndicStorage,
    ) -> Result<ComposerHostCompletedThreadSuccessorProgress, (Self, String)> {
        let validation = (|| {
            if !self.disposed
                || store.health().state() != beryl_home_store::HomeHealthState::Healthy
                || store.home_id() != self.binding.home_id()
                || store.canonical_path() != self.canonical_home
                || store.health().generation() != Some(self.binding.home_generation())
            {
                return Err(
                    "completed successor requires its exact healthy original generation".to_owned(),
                );
            }
            storage.revision(store).map_err(|error| error.to_string())?;
            let outcome = self
                .original
                .as_ref()
                .ok_or("completed successor original outcome is missing")?
                .committed_classification()
                .ok_or("completed successor original outcome is not exact")?;
            match storage
                .reconcile_abandon_fresh_draft_editor_candidate_session(
                    store,
                    self.prepared
                        .as_ref()
                        .ok_or("completed successor original preparation is missing")?,
                    outcome,
                )
                .map_err(|error| error.to_string())?
            {
                DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(_)
                | DraftEditorCandidateSessionAbandonFreshOutcomeV1::ExactReplay(_) => Ok(()),
                _ => Err(
                    "completed successor terminal receipt does not match its original opening"
                        .into(),
                ),
            }
        })();
        if let Err(error) = validation {
            return Err((self, error));
        }
        let original = self.original.take().unwrap();
        let (outcome, failure) = match original {
            RetainedComposerCommandOutcome::Indeterminate {
                failure,
                result: Some(Ok(beryl_home_store::ReconciliationResolution::ExactNew { receipt })),
                ..
            } => (
                RetainedComposerCommandOutcome::Committed {
                    receipt,
                    later_failure: None,
                    local_finalization: None,
                },
                Some(failure),
            ),
            original => (original, None),
        };
        Ok(ComposerHostCompletedThreadSuccessorProgress {
            binding: self.binding,
            canonical_home: self.canonical_home,
            prepared: self.prepared.take().unwrap(),
            outcome,
            _indeterminate_failure: failure,
        })
    }
}

impl ComposerHostCompletedThreadSuccessorProgress {
    pub(crate) fn validate_candidate(
        &self,
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
            return Err("completed successor progress requires fresh same-home access".into());
        }
        validate_abandonment(&access, storage, &self.prepared, &self.outcome)
    }
}
