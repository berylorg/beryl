use beryl_home_store::HomeCandidateRecoveryAccess;

use super::*;

impl SameWindowThreadUnadmitted {
    pub fn qualify_candidate_original(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        state: &BerylState,
    ) -> Result<(), SameWindowThreadError> {
        match state
            .session()
            .classify_window_claim_replacement_candidate(access, &self.replacement)
            .map_err(RunningThreadActivationError::from)?
        {
            WindowClaimReplacementState::Original => Ok(()),
            _ => Err(SameWindowThreadError::Collision),
        }
    }
}

impl SameWindowThreadCommit {
    pub fn validate_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        state: &BerylState,
    ) -> Result<(), SameWindowThreadError> {
        if self.window != *self.replacement.future_window()
            || self.selection != self.replacement.future_selection()
            || self.claim != self.replacement.future_claim()
            || self.receipt != self.original_receipt
        {
            return Err(SameWindowThreadError::Collision);
        }
        audit_candidate(access, state, &self.replacement, &self.rows)
    }
}

impl SameWindowThreadPending {
    pub fn reconcile_candidate(
        mut self,
        access: &HomeCandidateRecoveryAccess<'_>,
        state: &BerylState,
    ) -> SameWindowThreadOutcome {
        if self.unavailable {
            return SameWindowThreadOutcome::Unavailable(self);
        }
        if let Some(handle) = &self.handle {
            match access.reconcile(handle) {
                Ok(ReconciliationResolution::ExactOld) => {
                    match state
                        .session()
                        .classify_window_claim_replacement_candidate(access, &self.replacement)
                    {
                        Ok(WindowClaimReplacementState::Original) => {
                            return SameWindowThreadOutcome::NotCommitted(
                                SameWindowThreadError::ReconciledOld,
                            );
                        }
                        Ok(_) => return self.candidate_collision(),
                        Err(error) => {
                            self.problem = RunningThreadActivationError::from(error).into();
                            return SameWindowThreadOutcome::Pending(self);
                        }
                    }
                }
                Ok(ReconciliationResolution::ExactNew { receipt }) => self.receipt = Some(receipt),
                Ok(
                    ReconciliationResolution::ExactSuccessor { .. }
                    | ReconciliationResolution::Collision,
                ) => return self.candidate_collision(),
                Err(error) => {
                    self.problem = RunningThreadActivationError::from(error).into();
                    return SameWindowThreadOutcome::Pending(self);
                }
            }
        }
        if self.receipt.is_none() {
            return self.candidate_collision();
        }
        match audit_candidate(access, state, &self.replacement, &self.rows) {
            Ok(()) => self.settled(),
            Err(SameWindowThreadError::Collision) => self.candidate_collision(),
            Err(error) => {
                self.problem = error;
                SameWindowThreadOutcome::Pending(self)
            }
        }
    }

    fn candidate_collision(mut self) -> SameWindowThreadOutcome {
        self.problem = SameWindowThreadError::Collision;
        self.unavailable = true;
        SameWindowThreadOutcome::Unavailable(self)
    }
}

fn audit_candidate(
    access: &HomeCandidateRecoveryAccess<'_>,
    state: &BerylState,
    replacement: &PreparedWindowClaimReplacement,
    rows: &CatalogClaimReplacementAudit,
) -> Result<(), SameWindowThreadError> {
    let revision = access
        .home_revision()
        .map_err(RunningThreadActivationError::from)?;
    let mut exact = state
        .session()
        .classify_window_claim_replacement_candidate(access, replacement)
        .map_err(RunningThreadActivationError::from)?
        == WindowClaimReplacementState::Committed;
    for thread in rows.thread_ids() {
        let row = state
            .catalog()
            .current_row_source_candidate(access, thread, CatalogPointReadLimit::schema_maximum())
            .map_err(|error| match error {
                beryl_state::CatalogCurrentRowError::Read(error) => {
                    RunningThreadActivationError::from(error).into()
                }
                _ => SameWindowThreadError::Collision,
            })?;
        exact &= row.is_some_and(|row| rows.matches_row(row.row()));
    }
    if access
        .home_revision()
        .map_err(RunningThreadActivationError::from)?
        != revision
    {
        return Err(SameWindowThreadError::SourceChanged);
    }
    if !exact {
        return Err(SameWindowThreadError::Collision);
    }
    Ok(())
}
