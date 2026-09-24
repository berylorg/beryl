use super::*;
use crate::discussion_settlement::{DiscussionPreparationFailure, DiscussionSettlementService};

impl PreparationContext {
    pub(super) fn run(
        &self,
        sessions: &ScheduledExecutionSessions,
        admission: &ScheduledOrdinaryAdmission,
        service: Option<&DiscussionSettlementService>,
    ) {
        let Ok(reservation) = handoff::reserve(self, admission, service) else {
            return;
        };
        if let Err(failure) = self.prepare_session(sessions, admission)
            && let Some(reservation) = reservation
        {
            handoff::settle(self, sessions, reservation, failure);
        }
    }

    fn prepare_session(
        &self,
        sessions: &ScheduledExecutionSessions,
        admission: &ScheduledOrdinaryAdmission,
    ) -> Result<(), DiscussionPreparationFailure> {
        let Some(acquisition) = admission.acquisition() else {
            return Ok(());
        };
        let admission_context = self.admission.with_acquisition(acquisition);
        let spec = match self.launch_spec(admission) {
            Ok(spec) => spec,
            Err(error) => {
                self.owner
                    .revoke_retry(admission.thread_id(), admission.execution_binding());
                return match error {
                    target::TargetError::Unavailable(failure) => Err(failure),
                    _ => Ok(()),
                };
            }
        };
        let mut execution_workers = None;
        let interest = self.owner.acquire_prepared_managed(
            spec.clone(),
            admission.execution_binding().clone(),
            admission.thread_id(),
            acquisition,
            || {
                let (readiness, execution) = self
                    .workers
                    .try_acquire_cold_preparation_or_arm()
                    .map_err(|_| RuntimeInterestError::WorkerCapacity)?;
                execution_workers = Some(execution);
                Ok((admission_context.clone(), readiness))
            },
        );
        let interest = match interest {
            Ok(interest) => interest,
            Err(RuntimeInterestError::Unavailable(failure)) => {
                return Err(handoff::runtime_failure(failure));
            }
            Err(_) => return Ok(()),
        };
        let execution_workers = match execution_workers {
            Some(workers) => workers,
            None => match self.workers.try_acquire_warm_preparation_or_arm() {
                Ok(workers) => workers,
                Err(_) => return Ok(()),
            },
        };
        let mut status = interest.status();
        while status == RuntimeInterestStatus::Starting {
            if !self.commands.is_open() || sessions.lock().closed {
                return Ok(());
            }
            status = interest.wait_for_change(status, self.timeout);
        }
        if let RuntimeInterestStatus::Unavailable(failure) = status {
            return Err(handoff::runtime_failure(failure));
        }
        if !matches!(status, RuntimeInterestStatus::Ready(_))
            || !self.matches_launch_spec(admission, &spec)?
            || sessions.lock().closed
        {
            return Ok(());
        }
        let (connector, ready) = match self.owner.session_connector(&interest) {
            Ok(ready) => ready,
            Err(crate::cas_projection::RuntimeSessionAdmissionError::RuntimeFailed(failure)) => {
                return Err(handoff::runtime_failure(failure));
            }
            Err(_) => return Ok(()),
        };
        let Some(identity) = connector.launch_identity() else {
            return Ok(());
        };
        let session = match admission_context.admit_with_reserved_workers(
            &connector,
            identity.runtime_id(),
            identity.process_generation(),
            Path::new(identity.working_directory().as_str()),
            self.timeout,
            execution_workers,
        ) {
            Ok(session) => session,
            Err(error) => {
                if matches!(error, crate::cas_projection::ProjectionSessionAdmissionError::CandidateConnection { .. }
                    | crate::cas_projection::ProjectionSessionAdmissionError::Initialization { .. }
                    | crate::cas_projection::ProjectionSessionAdmissionError::ReleaseAdmission { .. }) {
                    return match interest.invalidate_configuration(ready) {
                        Some(failure) => Err(handoff::runtime_failure(failure)),
                        None => Ok(()),
                    };
                }
                return Ok(());
            }
        };
        let session = match interest.publish_session(ready, session) {
            Ok(session) => session,
            Err(crate::cas_projection::RuntimeSessionAdmissionError::RuntimeFailed(failure)) => {
                return Err(handoff::runtime_failure(failure));
            }
            Err(_) => return Ok(()),
        };
        if !self.matches_launch_spec(admission, &spec)? || sessions.lock().closed {
            return Ok(());
        }
        let _ = sessions.register(
            admission.thread_id(),
            admission.execution_binding().clone(),
            session,
            self.config.policy.clone(),
            self.config.assets.clone(),
            Box::new(self.tools.clone()),
        );
        Ok(())
    }

    fn matches_launch_spec(
        &self,
        admission: &ScheduledOrdinaryAdmission,
        expected: &ManagedBackendLaunchSpec,
    ) -> Result<bool, DiscussionPreparationFailure> {
        match self.launch_spec(admission) {
            Ok(actual) => Ok(&actual == expected),
            Err(target::TargetError::Unavailable(failure)) => Err(failure),
            Err(_) => Ok(false),
        }
    }
}
