use super::{
    ValidationError, ValidationIssue,
    seams::{RuntimeQualificationBackend, RuntimeQualificationCandidate, ValidationCleanup},
};
use beryl_backend::{
    ForegroundSessionConfig, ManagedBackendLaunchSpec, ManagedBackendServer, ManagedBackendSession,
};
use beryl_model::RuntimeNativePath;
use std::{num::NonZeroUsize, path::Path, time::Duration};

pub(super) struct ProductionBackend;

impl RuntimeQualificationBackend for ProductionBackend {
    fn launch(
        &self,
        spec: ManagedBackendLaunchSpec,
        foreground_control_capacity: usize,
    ) -> Result<Box<dyn RuntimeQualificationCandidate>, ValidationError> {
        let foreground = ForegroundSessionConfig::new(
            NonZeroUsize::new(foreground_control_capacity).ok_or(ValidationIssue::InvalidLimits)?,
        );
        let server = match ManagedBackendServer::launch(spec) {
            Ok(server) => server,
            Err(failure) => {
                if failure.cleanup_pending() {
                    return Err(ValidationError::with_cleanup(
                        ValidationIssue::Launch,
                        Box::new(FailedLaunchCleanup(Some(failure))),
                    ));
                }
                return Err(ValidationError::new(ValidationIssue::Launch)
                    .with_source(Some(Box::new(failure))));
            }
        };
        Ok(Box::new(ProductionCandidate {
            server: Some(server),
            session: None,
            diagnostics: QualificationDiagnostics::default(),
            foreground,
        }))
    }
}

struct ProductionCandidate {
    server: Option<ManagedBackendServer>,
    session: Option<ManagedBackendSession>,
    foreground: ForegroundSessionConfig,
    diagnostics: QualificationDiagnostics,
}

impl RuntimeQualificationCandidate for ProductionCandidate {
    fn connect(&mut self, timeout: Duration) -> Result<(), ValidationIssue> {
        let server = self.server.as_ref().ok_or(ValidationIssue::Launch)?;
        match server
            .client_connector()
            .connect_foreground_candidate(self.foreground, timeout)
        {
            Ok(session) => self.session = Some(session),
            Err(error) => {
                self.diagnostics.primary = Some(error);
                return Err(ValidationIssue::ForegroundConnection);
            }
        }
        Ok(())
    }

    fn initialize(&mut self, timeout: Duration) -> Result<(), ValidationIssue> {
        if let Err(error) = self
            .session
            .as_mut()
            .ok_or(ValidationIssue::ForegroundConnection)?
            .initialize_foreground(timeout)
        {
            self.diagnostics.primary = Some(error);
            return Err(ValidationIssue::Initialize);
        }
        Ok(())
    }

    fn admit(
        &mut self,
        home: &RuntimeNativePath,
        timeout: Duration,
    ) -> Result<(), ValidationIssue> {
        let server = self.server.as_ref().ok_or(ValidationIssue::Launch)?;
        let admission = match self
            .session
            .as_mut()
            .ok_or(ValidationIssue::ForegroundConnection)?
            .admit_release(Path::new(home.as_str()), timeout)
        {
            Ok(admission) => admission,
            Err(error) => {
                self.diagnostics.primary = Some(error);
                return Err(ValidationIssue::ReleaseRejected);
            }
        };
        if Some(admission.launch_identity()) != server.client_connector().launch_identity() {
            return Err(ValidationIssue::ReleaseRejected);
        }
        Ok(())
    }

    fn take_failure(&mut self) -> Option<Box<dyn std::error::Error + Send>> {
        self.diagnostics.first()?;
        let diagnostics = std::mem::take(&mut self.diagnostics);
        Some(Box::new(diagnostics))
    }
}

impl ValidationCleanup for ProductionCandidate {
    fn cleanup(&mut self) -> Result<(), ValidationIssue> {
        let mut failed = false;
        if let Some(session) = self.session.as_mut() {
            match session.shutdown() {
                Ok(()) => self.session = None,
                Err(error) => {
                    self.diagnostics.session_cleanup.get_or_insert(error);
                    failed = true;
                }
            }
        }
        if let Some(server) = self.server.as_mut() {
            match server.shutdown() {
                Ok(()) => self.server = None,
                Err(error) => {
                    self.diagnostics.server_cleanup.get_or_insert(error);
                    failed = true;
                }
            }
        }
        if failed {
            return Err(ValidationIssue::Cleanup);
        }
        Ok(())
    }
    fn failure(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.diagnostics
            .first()
            .map(|_| &self.diagnostics as &(dyn std::error::Error + 'static))
    }
}

struct FailedLaunchCleanup(Option<beryl_backend::ManagedBackendLaunchFailure>);

impl ValidationCleanup for FailedLaunchCleanup {
    fn cleanup(&mut self) -> Result<(), ValidationIssue> {
        if let Some(failure) = self.0.as_mut() {
            failure.shutdown().map_err(|_| ValidationIssue::Cleanup)?;
        }
        Ok(())
    }
    fn failure(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0
            .as_ref()
            .map(|failure| failure as &(dyn std::error::Error + 'static))
    }
    fn take_cleanup_failure(&mut self) -> Option<Box<dyn std::error::Error + Send>> {
        self.0
            .take()
            .map(|failure| Box::new(failure) as Box<dyn std::error::Error + Send>)
    }
}

#[derive(Debug, Default)]
struct QualificationDiagnostics {
    primary: Option<beryl_backend::ManagedBackendError>,
    session_cleanup: Option<beryl_backend::ManagedBackendError>,
    server_cleanup: Option<beryl_backend::ManagedBackendError>,
}

impl QualificationDiagnostics {
    fn first(&self) -> Option<&beryl_backend::ManagedBackendError> {
        self.primary
            .as_ref()
            .or(self.session_cleanup.as_ref())
            .or(self.server_cleanup.as_ref())
    }
}

impl std::fmt::Display for QualificationDiagnostics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(primary) = &self.primary {
            write!(f, "{primary}")?;
        }
        if let Some(error) = &self.session_cleanup {
            write!(f, "; foreground cleanup: {error}")?;
        }
        if let Some(error) = &self.server_cleanup {
            write!(f, "; managed process cleanup: {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for QualificationDiagnostics {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.first()
            .map(|error| error as &(dyn std::error::Error + 'static))
    }
}

impl Drop for ProductionCandidate {
    fn drop(&mut self) {
        if self.cleanup().is_err() {
            tracing::error!("runtime validation dropped unresolved managed cleanup custody");
        }
    }
}
