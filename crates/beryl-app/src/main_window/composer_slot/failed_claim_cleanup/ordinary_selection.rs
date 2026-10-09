use super::*;

impl MainWindowFailedClaimRetirement {
    pub(crate) fn accept_ordinary_committed_claim(
        &mut self,
        committed: &crate::main_window::running_threads::activation::RunningThreadActivationCommit,
        candidate: &mut HomeRecoveryCandidate,
        state: &beryl_state::BerylState,
    ) -> Result<(), String> {
        if self.kind != crate::main_window::MainWindowClaimRetirementKind::OrdinarySelection
            || self
                .predecessor
                .as_ref()
                .is_none_or(|prior| !prior.predecessor_publication_is_settled())
            || committed.window.window_id() != self.prior.window_id()
            || self
                .committed_target
                .is_some_and(|target| target != committed.selection)
            || self
                .successor
                .as_ref()
                .is_some_and(|successor| successor.selection.claim() != committed.selection)
        {
            return Err(
                "original ordinary committed claim does not belong to this retirement".into(),
            );
        }
        committed
            .validate_candidate(
                &candidate
                    .recovery_access()
                    .map_err(|error| error.to_string())?,
                state,
            )
            .map_err(|error| error.to_string())?;
        self.committed_target = Some(committed.selection);
        self.claim_settled = true;
        Ok(())
    }

    pub(crate) fn settle_ordinary_noncommit_target(
        &mut self,
        original: &crate::app_services::RetiredOrdinarySelectionOperation,
        candidate: &mut HomeRecoveryCandidate,
        state: &beryl_state::BerylState,
        storage: &SyndicStorage,
    ) -> Result<(), String> {
        if self.kind != crate::main_window::MainWindowClaimRetirementKind::OrdinarySelection
            || self.committed_target.is_some()
            || self.claim_settled
            || self.predecessor_release.is_some()
        {
            return Err("ordinary noncommit has changed predecessor or claim custody".into());
        }
        original.qualify_prior_candidate(
            &candidate
                .recovery_access()
                .map_err(|error| error.to_string())?,
            state,
        )?;
        if let Some(successor) = self.successor.as_mut() {
            successor.host.settle_cleanup(candidate, storage)?;
        }
        if let Some(successor) = self.completed_successor.as_mut() {
            successor.retired.host.settle_cleanup(candidate, storage)?;
        }
        if let Some(progress) = self.completed_progress.as_ref() {
            progress.host.validate_candidate(candidate, storage)?;
        }
        if self.mounted_successor.is_some_and(|mounted| {
            self.successor_release
                .is_none_or(|release| release.selection() != mounted)
        }) {
            return Err("original unpublished ordinary target widget release is incomplete".into());
        }
        self.cleanup_settled = true;
        Ok(())
    }

    pub(crate) fn into_prior_after_ordinary_noncommit(
        mut self: Box<Self>,
    ) -> Result<MainWindowFailedComposerRetirement, Box<Self>> {
        if self.kind != crate::main_window::MainWindowClaimRetirementKind::OrdinarySelection
            || !self.cleanup_settled
            || self.committed_target.is_some()
            || self.claim_settled
            || self.predecessor_release.is_some()
        {
            return Err(self);
        }
        match (*self.predecessor.take().unwrap()).into_prior() {
            Ok(host) => Ok(
                MainWindowFailedComposerRetirement::from_failed_claim_cleanup(
                    self.prior,
                    host,
                    self.last_activation_generation,
                ),
            ),
            Err(host) => {
                self.predecessor = Some(Box::new(host));
                Err(self)
            }
        }
    }
}
