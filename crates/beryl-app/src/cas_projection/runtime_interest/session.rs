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
        let RuntimeInterestStatus::Ready(ready) = interest.status_locked(&state) else {
            return Err(RuntimeSessionAdmissionError::RuntimeUnavailable);
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
    pub(in crate::cas_projection) fn invalidate_configuration(&self, ready: RuntimeReadiness) {
        let mut state = self.shared.lock();
        if self.status_locked(&state) == RuntimeInterestStatus::Ready(ready) {
            let entry = state
                .runtimes
                .get_mut(&self.runtime_id)
                .expect("current runtime");
            entry.connector = None;
            entry.status = RuntimeInterestStatus::Unavailable(RuntimeFailure::Admission);
            self.shared.changed.notify_all();
        }
    }

    pub(in crate::cas_projection) fn publish_session(
        self,
        ready: RuntimeReadiness,
        mut session: AdmittedProjectionSession,
    ) -> Result<AdmittedProjectionSession, RuntimeSessionAdmissionError> {
        let shared = Arc::clone(&self.shared);
        let mut interest = Some(self);
        let state = shared.lock();
        let retained = interest.as_ref().expect("unpublished interest");
        if retained.status_locked(&state) != RuntimeInterestStatus::Ready(ready)
            || session.runtime_id() != retained.runtime_id
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
                session.retain_runtime_interest(
                    interest.take().expect("unpublished interest"),
                    ready.activity_period,
                )
            })
            .map_err(|_| RuntimeSessionAdmissionError::ServiceUnavailable)?;
        Ok(session)
    }
}
