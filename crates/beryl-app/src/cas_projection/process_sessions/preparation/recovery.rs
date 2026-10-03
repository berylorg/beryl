use super::*;
use crate::cas_projection::{ProjectionCancellationToken, RuntimeFailureSnapshot};

impl PreparationContext {
    pub(in crate::cas_projection::process_sessions) fn prepare_recovery_session(
        &self,
        sessions: &ScheduledExecutionSessions,
        snapshot: RuntimeFailureSnapshot,
        thread_id: SyndicThreadId,
        binding: &ExecutionBinding,
        cancellation: &ProjectionCancellationToken,
        acquisition: &crate::cas_projection::acquisition::ProjectionAcquisition,
    ) -> Result<AdmittedProjectionSession, SelectedProjectionRecoveryError> {
        let unavailable = SelectedProjectionRecoveryError::Unavailable;
        let spec = self
            .launch_spec_for(thread_id, binding)
            .map_err(|_| unavailable)?;
        self.owner
            .authorize_selected_retry(snapshot, thread_id, binding.clone())
            .map_err(|_| unavailable)?;
        let admission = self.admission.with_acquisition(acquisition);
        let mut execution_workers = None;
        let interest = self
            .owner
            .acquire_prepared_managed(
                spec.clone(),
                binding.clone(),
                thread_id,
                acquisition,
                || {
                    let (readiness, execution) = self
                        .workers
                        .try_acquire_cold_preparation_or_arm()
                        .map_err(|_| RuntimeInterestError::WorkerCapacity)?;
                    execution_workers = Some(execution);
                    Ok((admission.clone(), readiness))
                },
            )
            .map_err(|error| match error {
                RuntimeInterestError::Unavailable(_) => SelectedProjectionRecoveryError::Failed,
                _ => unavailable,
            })?;
        let workers = match execution_workers {
            Some(workers) => workers,
            None => self
                .workers
                .try_acquire_warm_preparation_or_arm()
                .map_err(|_| unavailable)?,
        };
        let mut status = interest.status();
        let deadline = std::time::Instant::now() + self.timeout;
        while status == RuntimeInterestStatus::Starting {
            if cancellation.is_cancelled()
                || !self.commands.is_open()
                || sessions.lock().closed
                || std::time::Instant::now() >= deadline
            {
                return Err(unavailable);
            }
            status = interest.wait_for_change(status, Duration::from_millis(25));
        }
        if matches!(status, RuntimeInterestStatus::Unavailable(_)) {
            return Err(SelectedProjectionRecoveryError::Failed);
        }
        if !matches!(status, RuntimeInterestStatus::Ready(_))
            || cancellation.is_cancelled()
            || self
                .launch_spec_for(thread_id, binding)
                .map_err(|_| unavailable)?
                != spec
            || sessions.lock().closed
        {
            return Err(unavailable);
        }
        let (connector, ready) = self
            .owner
            .session_connector(&interest)
            .map_err(|_| unavailable)?;
        let identity = connector.launch_identity().ok_or(unavailable)?;
        let session = admission
            .admit_with_reserved_workers(
                &connector,
                identity.runtime_id(),
                identity.process_generation(),
                Path::new(identity.working_directory().as_str()),
                self.timeout,
                workers,
            )
            .map_err(|error| {
                if matches!(error,
                crate::cas_projection::ProjectionSessionAdmissionError::CandidateConnection { .. }
                | crate::cas_projection::ProjectionSessionAdmissionError::Initialization { .. }
                | crate::cas_projection::ProjectionSessionAdmissionError::ReleaseAdmission { .. })
                {
                    interest.invalidate_configuration(ready);
                }
                SelectedProjectionRecoveryError::Failed
            })?;
        let session = interest
            .publish_session(ready, session)
            .map_err(|_| unavailable)?;
        if cancellation.is_cancelled()
            || self
                .launch_spec_for(thread_id, binding)
                .map_err(|_| unavailable)?
                != spec
            || sessions.lock().closed
        {
            return Err(unavailable);
        }
        Ok(session)
    }
}
