use super::*;

pub(super) enum InitialServiceAttemptState {
    Initial,
    Preparing,
    Published,
    Retired(ProcessAdmissionFence),
    Blocked,
}

impl ProcessServiceOwner {
    pub(super) fn admit_initial_attempt(
        &mut self,
        candidate: &HomeOpenPublication,
    ) -> Result<(), AppServiceOpenError> {
        if self.graph.is_some() || self.failed_close.is_some() {
            return Err(AppServiceOpenError::AlreadyInstalled);
        }
        if !matches!(
            self.attempt,
            InitialServiceAttemptState::Initial | InitialServiceAttemptState::Retired(_)
        ) {
            return Err(AppServiceOpenError::AlreadyInstalled);
        }
        if candidate.home_id() != self.home_id {
            return Err(AppServiceOpenError::ForeignHome);
        }
        self.require_settled_custody()
            .map_err(AppServiceOpenError::UnsettledCustody)?;
        if let InitialServiceAttemptState::Retired(fence) = &self.attempt {
            self.process
                .execution_permit()
                .prepare_reopening(fence)
                .map_err(AppServiceOpenError::Reopening)?
                .reopen();
        }
        self.attempt = InitialServiceAttemptState::Preparing;
        Ok(())
    }

    pub(super) fn record_initial_retirement(&mut self) -> Result<(), ProcessAdmissionError> {
        self.attempt = InitialServiceAttemptState::Blocked;
        let fence = self.process.fence()?;
        self.attempt = InitialServiceAttemptState::Retired(fence);
        Ok(())
    }
}
