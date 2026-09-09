use beryl_backend::ManagedBackendLaunchSpec;

use super::*;
use crate::cas_projection::runtime_interest::{
    RuntimeInterest, RuntimeInterestConfig, RuntimeInterestError, RuntimeInterestKind,
    RuntimeInterestOwner, RuntimeSessionAdmissionError,
};

impl ProjectionConnectionService {
    #[cfg(feature = "test-faults")]
    pub fn poll_runtime_retirements_for_test(
        &self,
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
    ) -> Result<(), crate::cas_projection::RuntimeFailure> {
        self.admission_context()
            .map_err(|_| crate::cas_projection::RuntimeFailure::AppRetirement)?
            .runtime_retirement(runtime_id, process_generation)
            .poll_retirements()
    }

    #[cfg(feature = "test-faults")]
    pub fn runtime_preparation_waits_for_test(&self, runtime_id: RuntimeId) -> (bool, bool, bool) {
        self.runtime_interest
            .as_ref()
            .expect("test service owns runtime interest")
            .preparation_waits(runtime_id)
    }

    #[cfg(feature = "test-faults")]
    pub fn checkout_scheduled_session_for_test(
        &self,
        thread_id: SyndicThreadId,
        binding: ExecutionBinding,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        let worker = self
            .try_acquire_scheduled_ordinary_worker()
            .map_err(|error| match error {
                ProjectionWorkerPermitError::CapacityFull { available } => {
                    ProjectionCoordinatorError::ProjectionWorkerCapacityFull { available }
                }
                ProjectionWorkerPermitError::Poisoned => {
                    ProjectionCoordinatorError::ProjectionWorkerPoolPoisoned
                }
            })?;
        let flight = self.begin_scheduled_ordinary_flight(thread_id)?;
        self.issue_scheduled_ordinary_execution(thread_id, binding, worker, flight)
    }

    pub fn admit_runtime_session(
        &self,
        interest: RuntimeInterest,
        timeout: Duration,
    ) -> Result<AdmittedProjectionSession, RuntimeSessionAdmissionError> {
        if timeout.is_zero() {
            return Err(RuntimeSessionAdmissionError::InvalidTimeout);
        }
        let (connector, ready) = self
            .runtime_interest
            .as_ref()
            .ok_or(RuntimeSessionAdmissionError::ServiceUnavailable)?
            .session_connector(&interest)?;
        let identity = connector
            .launch_identity()
            .ok_or(RuntimeSessionAdmissionError::RuntimeUnavailable)?;
        let session = match self
            .admission_context()
            .map_err(|_| RuntimeSessionAdmissionError::ServiceUnavailable)?
            .admit(
                &connector,
                identity.runtime_id(),
                identity.process_generation(),
                Path::new(identity.working_directory().as_str()),
                timeout,
            ) {
            Ok(session) => session,
            Err(error) => {
                if configuration_admission_unproven(&error) {
                    interest.invalidate_configuration(ready);
                }
                return Err(error.into());
            }
        };
        interest.publish_session(ready, session)
    }

    #[cfg(feature = "test-faults")]
    pub fn worker_pool_observer_for_test(
        &self,
    ) -> impl Fn() -> ProjectionWorkerPoolDiagnostics + Send + Sync + 'static {
        let workers = self.workers.clone();
        move || workers.diagnostics()
    }

    #[cfg(feature = "test-faults")]
    pub fn runtime_retirement_waiters_for_test(&self) -> usize {
        self.persistent_failure.as_ref().map_or(0, |coordinator| {
            coordinator.runtime_retirement_waiters_for_test()
        })
    }

    pub fn configure_runtime_interest(
        &mut self,
        config: RuntimeInterestConfig,
    ) -> Result<(), RuntimeInterestError> {
        if !self.command_authorizer.is_open() {
            return Err(RuntimeInterestError::Closed);
        }
        if self.runtime_interest.is_some() {
            return Err(RuntimeInterestError::AlreadyConfigured);
        }
        self.runtime_interest = Some(Arc::new(RuntimeInterestOwner::new(
            config,
            self.command_authorizer.clone(),
            self.scheduler_signal.clone(),
        )));
        Ok(())
    }

    pub fn acquire_runtime_interest(
        &self,
        spec: ManagedBackendLaunchSpec,
        binding: ExecutionBinding,
        kind: RuntimeInterestKind,
    ) -> Result<RuntimeInterest, RuntimeInterestError> {
        self.runtime_interest
            .as_ref()
            .ok_or(RuntimeInterestError::NotConfigured)?
            .acquire_managed(spec, binding, kind, || {
                self.admission_context()
                    .map_err(|_| RuntimeInterestError::AdmissionUnavailable)
            })
    }
}

fn configuration_admission_unproven(error: &ProjectionSessionAdmissionError) -> bool {
    let ProjectionSessionAdmissionError::ReleaseAdmission { source, .. } = error else {
        return false;
    };
    matches!(
        source.as_ref(),
        ManagedBackendError::ReleaseAdmissionEffectiveConfigUnproven
    ) || matches!(
        source.as_ref(),
        ManagedBackendError::ForegroundIngress {
            method,
            source: beryl_backend::ForegroundIngressError::MalformedResponse,
        } if method == "config/read"
    )
}
