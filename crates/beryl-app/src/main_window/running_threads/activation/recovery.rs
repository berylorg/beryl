use super::*;
use beryl_home_store::HomeCandidateRecoveryAccess;

impl RunningThreadActivation {
    pub(crate) fn validate_prior_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        state: &BerylState,
    ) -> Result<(), RunningThreadActivationError> {
        match state
            .session()
            .classify_window_claim_replacement_candidate(access, &self.prepared)?
        {
            WindowClaimReplacementState::Original => Ok(()),
            _ => Err(RunningThreadActivationError::Collision),
        }
    }
}

impl RunningThreadActivationRejected {
    pub(crate) fn validate_prior_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        state: &BerylState,
    ) -> Result<(), RunningThreadActivationError> {
        self.activation.validate_prior_candidate(access, state)
    }
}

impl RunningThreadActivationCommit {
    pub(crate) fn validate_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        state: &BerylState,
    ) -> Result<(), RunningThreadActivationError> {
        let revision = access.home_revision()?;
        if state
            .session()
            .classify_window_claim_replacement_candidate(access, &self.prepared)?
            != WindowClaimReplacementState::Committed
        {
            return Err(RunningThreadActivationError::Collision);
        }
        for thread in self.rows.thread_ids() {
            let row = state
                .catalog()
                .current_row_source_candidate(
                    access,
                    thread,
                    CatalogPointReadLimit::schema_maximum(),
                )?
                .ok_or(RunningThreadActivationError::Collision)?;
            if !self.rows.matches_row(row.row()) {
                return Err(RunningThreadActivationError::Collision);
            }
        }
        if access.home_revision()? != revision {
            return Err(RunningThreadActivationError::SourceChanged);
        }
        Ok(())
    }
}

impl RunningThreadActivationPending {
    pub(crate) fn reconcile_candidate(
        mut self,
        access: &HomeCandidateRecoveryAccess<'_>,
        state: &BerylState,
    ) -> RunningThreadActivationOutcome {
        if let Some(handle) = &self.handle {
            match access.reconcile(handle) {
                Ok(ReconciliationResolution::ExactOld) => {
                    return match self.activation.validate_prior_candidate(access, state) {
                        Ok(()) => RunningThreadActivationOutcome::NotCommitted(
                            RunningThreadActivationRejected {
                                activation: self.activation,
                                rows: Some(self.rows),
                                problem: RunningThreadActivationError::ReconciledOld,
                            },
                        ),
                        Err(problem) => {
                            self.problem = problem;
                            RunningThreadActivationOutcome::Pending(self)
                        }
                    };
                }
                Ok(ReconciliationResolution::ExactNew { receipt }) => {
                    self.receipt = Some(receipt);
                    self.handle = None;
                }
                Ok(
                    ReconciliationResolution::ExactSuccessor { .. }
                    | ReconciliationResolution::Collision,
                ) => {
                    self.problem = RunningThreadActivationError::Collision;
                    return RunningThreadActivationOutcome::Pending(self);
                }
                Err(problem) => {
                    self.problem = problem.into();
                    return RunningThreadActivationOutcome::Pending(self);
                }
            }
        }
        let validation = (|| -> Result<(), RunningThreadActivationError> {
            let revision = access.home_revision()?;
            if self.receipt.is_none()
                || state
                    .session()
                    .classify_window_claim_replacement_candidate(
                        access,
                        &self.activation.prepared,
                    )?
                    != WindowClaimReplacementState::Committed
            {
                return Err(RunningThreadActivationError::Collision);
            }
            for thread in self.rows.thread_ids() {
                let row = state
                    .catalog()
                    .current_row_source_candidate(
                        access,
                        thread,
                        CatalogPointReadLimit::schema_maximum(),
                    )?
                    .ok_or(RunningThreadActivationError::Collision)?;
                if !self.rows.matches_row(row.row()) {
                    return Err(RunningThreadActivationError::Collision);
                }
            }
            if access.home_revision()? != revision {
                return Err(RunningThreadActivationError::SourceChanged);
            }
            Ok(())
        })();
        if let Err(problem) = validation {
            self.problem = problem;
            return RunningThreadActivationOutcome::Pending(self);
        }
        RunningThreadActivationOutcome::Settled(RunningThreadActivationCommit {
            window: self.activation.prepared.future_window().clone(),
            selection: self.activation.prepared.future_selection(),
            claim: self.activation.prepared.future_claim(),
            receipt: self.receipt.take().unwrap(),
            later_failure: self.later_failure,
            local_finalization: self.local_finalization,
            prepared: self.activation.prepared,
            rows: self.rows,
        })
    }
}
