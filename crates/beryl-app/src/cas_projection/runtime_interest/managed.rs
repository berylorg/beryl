use std::path::Path;

use beryl_backend::{ManagedBackendLaunchFailure, ManagedBackendServer};

use super::*;
use crate::cas_projection::service_config::ProjectionWorkerPermitPair;
use crate::cas_projection::{AdmittedProjectionSession, service::ProjectionAdmissionContext};

impl RuntimeInterestOwner {
    pub(in crate::cas_projection) fn acquire_managed(
        &self,
        spec: ManagedBackendLaunchSpec,
        binding: ExecutionBinding,
        kind: RuntimeInterestKind,
        prepare: impl FnOnce() -> Result<ProjectionAdmissionContext, RuntimeInterestError>,
    ) -> Result<RuntimeInterest, RuntimeInterestError> {
        let acquisition =
            crate::cas_projection::acquisition::ProjectionAcquisition::admit(&self.shared.commands)
                .map_err(|_| RuntimeInterestError::Closed)?;
        let launch_spec = spec.clone();
        let timeout = self.shared.config.admission_timeout;
        self.acquire_with_retry(spec, binding, kind, None, false, &acquisition, || {
            let admission = prepare()?.with_acquisition(&acquisition);
            let workers = admission.reserve_runtime_workers()?;
            Ok(Box::new(move || {
                ManagedRuntime::launch(launch_spec, admission, workers, timeout)
                    .map(|runtime| Box::new(runtime) as Box<dyn RunningRuntime>)
            }))
        })
    }

    pub(in crate::cas_projection) fn acquire_prepared_managed(
        &self,
        spec: ManagedBackendLaunchSpec,
        binding: ExecutionBinding,
        thread_id: SyndicThreadId,
        acquisition: &crate::cas_projection::acquisition::ProjectionAcquisition,
        prepare: impl FnOnce() -> Result<
            (ProjectionAdmissionContext, ProjectionWorkerPermitPair),
            RuntimeInterestError,
        >,
    ) -> Result<RuntimeInterest, RuntimeInterestError> {
        let launch_spec = spec.clone();
        let timeout = self.shared.config.admission_timeout;
        let retry = self.scheduled_retry(thread_id, &binding);
        self.acquire_with_retry(
            spec,
            binding,
            RuntimeInterestKind::RequiredWork,
            retry,
            true,
            acquisition,
            || {
                let (admission, workers) = prepare()?;
                let admission = admission.with_acquisition(acquisition);
                Ok(Box::new(move || {
                    ManagedRuntime::launch(launch_spec, admission, workers, timeout)
                        .map(|runtime| Box::new(runtime) as Box<dyn RunningRuntime>)
                }))
            },
        )
    }
}

struct ManagedRuntime {
    server: Option<ManagedBackendServer>,
    session: Option<AdmittedProjectionSession>,
    app_resources: crate::cas_projection::service_registry::ProjectionRuntimeRetirement,
    retirement: Option<Result<(), RuntimeFailure>>,
    admission_failure: Option<crate::cas_projection::ProjectionSessionAdmissionError>,
}

impl ManagedRuntime {
    fn launch(
        spec: ManagedBackendLaunchSpec,
        admission: ProjectionAdmissionContext,
        workers: ProjectionWorkerPermitPair,
        timeout: Duration,
    ) -> Result<Self, RuntimeLaunchFailure> {
        let server =
            ManagedBackendServer::launch(spec).map_err(|failure| RuntimeLaunchFailure {
                failure: RuntimeFailure::Launch,
                cleanup: Some(Box::new(FailedManagedLaunch { failure })),
            })?;
        let connector = server.client_connector();
        let identity = connector
            .launch_identity()
            .expect("managed server owns production provenance");
        let mut runtime = Self {
            app_resources: admission
                .runtime_retirement(identity.runtime_id(), identity.process_generation()),
            server: Some(server),
            session: None,
            retirement: None,
            admission_failure: None,
        };
        let session = match admission.admit_with_reserved_workers(
            &connector,
            identity.runtime_id(),
            identity.process_generation(),
            Path::new(identity.working_directory().as_str()),
            timeout,
            workers,
        ) {
            Ok(session) => session,
            Err(error) => {
                runtime.admission_failure = Some(error);
                return Err(RuntimeLaunchFailure {
                    failure: RuntimeFailure::Admission,
                    cleanup: Some(Box::new(runtime)),
                });
            }
        };
        runtime.session = Some(session);
        Ok(runtime)
    }
}

impl RunningRuntime for ManagedRuntime {
    fn connector(&self) -> Option<ManagedBackendClientConnector> {
        self.server
            .as_ref()
            .map(ManagedBackendServer::client_connector)
    }

    fn process_generation(&self) -> CasProcessGeneration {
        self.session
            .as_ref()
            .expect("live managed session")
            .process_generation()
    }

    fn poll_health(&mut self) -> Result<(), RuntimeFailure> {
        if !self
            .server
            .as_mut()
            .expect("live managed process")
            .is_process_alive()
        {
            return Err(RuntimeFailure::ProcessExited);
        }
        if self
            .session
            .as_ref()
            .expect("live managed session")
            .connection()
            .is_retired()
        {
            return Err(RuntimeFailure::ConnectionLost);
        }
        self.app_resources.poll_retirements()
    }

    fn retire(&mut self) -> Result<(), RuntimeFailure> {
        if self.retirement == Some(Ok(())) {
            return Ok(());
        }
        let mut result = if self.app_resources.retire() {
            Ok(())
        } else {
            Err(RuntimeFailure::AppRetirement)
        };
        drop(self.session.take());
        if let Some(server) = self.server.as_mut()
            && server.shutdown().is_err()
        {
            result = Err(RuntimeFailure::BackendDisposal);
        }
        if result.is_ok() {
            self.server = None;
        }
        self.retirement = Some(result);
        result
    }
}

struct FailedManagedLaunch {
    failure: ManagedBackendLaunchFailure,
}

impl RunningRuntime for FailedManagedLaunch {
    fn process_generation(&self) -> CasProcessGeneration {
        panic!("failed launch cannot publish runtime readiness")
    }

    fn poll_health(&mut self) -> Result<(), RuntimeFailure> {
        Err(RuntimeFailure::Launch)
    }

    fn retire(&mut self) -> Result<(), RuntimeFailure> {
        self.failure
            .shutdown()
            .map_err(|_| RuntimeFailure::BackendDisposal)
    }
}

impl Drop for ManagedRuntime {
    fn drop(&mut self) {
        let _ = self.retire();
    }
}
