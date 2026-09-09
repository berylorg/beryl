use beryl_backend::ManagedBackendLaunchSpec;

use super::*;
use crate::cas_projection::runtime_interest::{
    RuntimeInterest, RuntimeInterestConfig, RuntimeInterestError, RuntimeInterestKind,
    RuntimeInterestOwner,
};

impl ProjectionConnectionService {
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
        self.runtime_interest = Some(RuntimeInterestOwner::new(
            config,
            self.command_authorizer.clone(),
        ));
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
