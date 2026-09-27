use super::*;
use crate::exit_session::{ExitSessionValidation, ExitSessionValidationError};
use beryl_home_store::HomeRecoveryCandidate;

impl ExitSessionReconciliation {
    pub(crate) fn candidate_resolution(
        &self,
    ) -> Option<&Result<ReconciliationResolution, ReconciliationFailure>> {
        self.candidate_resolution.as_ref()
    }

    pub(crate) fn settle_candidate(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        session: &SessionState,
    ) -> Result<ExitSessionValidation, ExitSessionValidationError> {
        use ExitSessionValidationError as Error;
        if candidate.service_reference().configured_path() != self.publication.configured_home {
            return Err(Error::ForeignHome);
        }
        if self.candidate_resolution.is_none() {
            let access = candidate
                .recovery_access()
                .map_err(|e| Error::Read(e.to_string()))?;
            session
                .revision_candidate(&access)
                .map_err(|e| Error::Read(e.to_string()))?;
            self.candidate_resolution = Some(access.reconcile(&self.handle));
        }
        let committed = match self.candidate_resolution.as_ref() {
            Some(Ok(ReconciliationResolution::ExactOld)) => false,
            Some(Ok(ReconciliationResolution::ExactNew { .. })) => true,
            _ => return Err(Error::Unproven),
        };
        crate::exit_session::validation::validate(&self.publication, committed, candidate, session)
    }
}
