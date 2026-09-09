use std::path::Path;

use beryl_backend::ManagedBackendServer;

use super::*;
use crate::cas_projection::{AdmittedProjectionSession, service::ProjectionAdmissionContext};

impl RuntimeInterestOwner {
    pub(in crate::cas_projection) fn acquire_managed(
        &self,
        spec: ManagedBackendLaunchSpec,
        binding: ExecutionBinding,
        kind: RuntimeInterestKind,
        prepare: impl FnOnce() -> Result<ProjectionAdmissionContext, RuntimeInterestError>,
    ) -> Result<RuntimeInterest, RuntimeInterestError> {
        let launch_spec = spec.clone();
        let timeout = self.shared.config.admission_timeout;
        self.acquire(spec, binding, kind, || {
            let admission = prepare()?;
            Ok(Box::new(move || {
                ManagedRuntime::launch(launch_spec, admission, timeout)
                    .map(|runtime| Box::new(runtime) as Box<dyn RunningRuntime>)
            }))
        })
    }
}

struct ManagedRuntime {
    server: Option<ManagedBackendServer>,
    session: Option<AdmittedProjectionSession>,
    app_resources: crate::cas_projection::service_registry::ProjectionRuntimeRetirement,
    retirement: Option<Result<(), RuntimeFailure>>,
}

impl ManagedRuntime {
    fn launch(
        spec: ManagedBackendLaunchSpec,
        admission: ProjectionAdmissionContext,
        timeout: Duration,
    ) -> Result<Self, RuntimeFailure> {
        let server = ManagedBackendServer::launch(spec).map_err(|_| RuntimeFailure::Launch)?;
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
        };
        let session = match admission.admit(
            &connector,
            identity.runtime_id(),
            identity.process_generation(),
            Path::new(identity.working_directory().as_str()),
            timeout,
        ) {
            Ok(session) => session,
            Err(_) => {
                return match runtime.retire() {
                    Ok(()) => Err(RuntimeFailure::Admission),
                    Err(failure) => Err(failure),
                };
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
        Ok(())
    }

    fn retire(&mut self) -> Result<(), RuntimeFailure> {
        if let Some(result) = self.retirement {
            return result;
        }
        let mut result = if self.app_resources.retire() {
            Ok(())
        } else {
            Err(RuntimeFailure::AppRetirement)
        };
        drop(self.session.take());
        if let Some(mut server) = self.server.take()
            && server.shutdown().is_err()
        {
            result = Err(RuntimeFailure::BackendDisposal);
        }
        self.retirement = Some(result);
        result
    }
}

impl Drop for ManagedRuntime {
    fn drop(&mut self) {
        let _ = self.retire();
    }
}
