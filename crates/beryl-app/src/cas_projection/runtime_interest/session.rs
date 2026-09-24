use super::*;
use crate::cas_projection::{AdmittedProjectionSession, ProjectionSessionAdmissionError};

#[derive(Debug, Error)]
pub enum RuntimeSessionAdmissionError {
    #[error("execution admission requires required-work runtime interest")]
    RequiredWorkInterest,
    #[error("runtime interest belongs to another service owner")]
    OwnerMismatch,
    #[error("runtime interest has no current production readiness")]
    RuntimeUnavailable,
    #[error("the exact runtime interest failed: {0:?}")]
    RuntimeFailed(RuntimeFailure),
    #[error("session admission timeout must be positive")]
    InvalidTimeout,
    #[error("the current service admission context is unavailable")]
    ServiceUnavailable,
    #[error("the production foreground session was not admitted: {0}")]
    Admission(#[from] ProjectionSessionAdmissionError),
}

impl RuntimeInterestOwner {
    pub(in crate::cas_projection) fn session_connector(
        &self,
        interest: &RuntimeInterest,
    ) -> Result<(ManagedBackendClientConnector, RuntimeReadiness), RuntimeSessionAdmissionError>
    {
        if !Arc::ptr_eq(&self.shared, &interest.shared) {
            return Err(RuntimeSessionAdmissionError::OwnerMismatch);
        }
        if interest.kind != RuntimeInterestKind::RequiredWork {
            return Err(RuntimeSessionAdmissionError::RequiredWorkInterest);
        }
        let state = self.shared.lock();
        let ready = match interest.status_locked(&state) {
            RuntimeInterestStatus::Ready(ready) => ready,
            RuntimeInterestStatus::Unavailable(failure) => {
                return Err(RuntimeSessionAdmissionError::RuntimeFailed(failure));
            }
            _ => return Err(RuntimeSessionAdmissionError::RuntimeUnavailable),
        };
        let connector = state
            .runtimes
            .get(&interest.runtime_id)
            .and_then(|entry| entry.connector.clone())
            .ok_or(RuntimeSessionAdmissionError::RuntimeUnavailable)?;
        Ok((connector, ready))
    }
}

impl RuntimeInterest {
    pub(super) fn publication_readiness(
        &self,
        state: &RuntimeInterestState,
        ready: RuntimeReadiness,
    ) -> Result<(), RuntimeSessionAdmissionError> {
        match self.status_locked(state) {
            RuntimeInterestStatus::Unavailable(failure) => {
                Err(RuntimeSessionAdmissionError::RuntimeFailed(failure))
            }
            RuntimeInterestStatus::Ready(actual) if actual == ready => Ok(()),
            _ => Err(RuntimeSessionAdmissionError::RuntimeUnavailable),
        }
    }

    pub(in crate::cas_projection) fn invalidate_configuration(
        &self,
        ready: RuntimeReadiness,
    ) -> Option<RuntimeFailure> {
        let mut state = self.shared.lock();
        let status = self.status_locked(&state);
        if status == RuntimeInterestStatus::Ready(ready) {
            let entry = state
                .runtimes
                .get_mut(&self.runtime_id)
                .expect("current runtime");
            entry.connector = None;
            entry.status = RuntimeInterestStatus::Unavailable(RuntimeFailure::Admission);
            self.shared.changed.notify_all();
            return Some(RuntimeFailure::Admission);
        }
        match status {
            RuntimeInterestStatus::Unavailable(failure) => Some(failure),
            _ => None,
        }
    }

    pub(in crate::cas_projection) fn publish_session(
        self,
        ready: RuntimeReadiness,
        mut session: AdmittedProjectionSession,
    ) -> Result<AdmittedProjectionSession, RuntimeSessionAdmissionError> {
        let interest = Arc::new(self);
        let shared = Arc::clone(&interest.shared);
        let state = shared.lock();
        interest.publication_readiness(&state, ready)?;
        if session.runtime_id() != interest.runtime_id
            || session.process_generation() != ready.process_generation
        {
            return Err(RuntimeSessionAdmissionError::RuntimeUnavailable);
        }
        let command = shared
            .commands
            .authorize()
            .map_err(|_| RuntimeSessionAdmissionError::ServiceUnavailable)?;
        command
            .commit_if_current(|| {
                session.retain_runtime_interest(Arc::clone(&interest), ready.activity_period)
            })
            .map_err(|_| RuntimeSessionAdmissionError::ServiceUnavailable)??;
        Ok(session)
    }
}
