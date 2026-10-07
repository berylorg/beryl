use super::{ValidationError, ValidationIssue};
use beryl_home_store::CommandCancellation;
use beryl_model::{RuntimeMode, RuntimeNativePath};
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct ValidationFilesystemPath {
    pub mode: RuntimeMode,
    pub host_path: String,
    pub native_path: String,
}

pub trait ValidationFilesystem: Send + Sync {
    fn executable(
        &self,
        selected: &Path,
        deadline: Instant,
        cancellation: &CommandCancellation,
        path_bytes: usize,
    ) -> Result<ValidationFilesystemPath, ValidationError>;
    fn directory(
        &self,
        selected: &Path,
        mode: &RuntimeMode,
        deadline: Instant,
        cancellation: &CommandCancellation,
        path_bytes: usize,
    ) -> Result<ValidationFilesystemPath, ValidationError>;
    fn home(
        &self,
        mode: &RuntimeMode,
        deadline: Instant,
        cancellation: &CommandCancellation,
        path_bytes: usize,
    ) -> Result<ValidationFilesystemPath, ValidationError>;
}

pub trait ValidationCleanup: Send {
    fn cleanup(&mut self) -> Result<(), ValidationIssue>;
    fn take_cleanup_failure(&mut self) -> Option<Box<dyn std::error::Error + Send>> {
        None
    }
    fn failure(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

pub trait RuntimeQualificationCandidate: ValidationCleanup {
    fn connect(&mut self, timeout: Duration) -> Result<(), ValidationIssue>;
    fn initialize(&mut self, timeout: Duration) -> Result<(), ValidationIssue>;
    fn admit(&mut self, home: &RuntimeNativePath, timeout: Duration)
    -> Result<(), ValidationIssue>;
    fn take_failure(&mut self) -> Option<Box<dyn std::error::Error + Send>> {
        None
    }
}

pub trait RuntimeQualificationBackend: Send + Sync {
    fn launch(
        &self,
        spec: beryl_backend::ManagedBackendLaunchSpec,
        foreground_control_capacity: usize,
    ) -> Result<Box<dyn RuntimeQualificationCandidate>, ValidationError>;
}

pub(super) struct CandidateCleanup(pub Box<dyn RuntimeQualificationCandidate>);

impl ValidationCleanup for CandidateCleanup {
    fn cleanup(&mut self) -> Result<(), ValidationIssue> {
        self.0.cleanup()
    }
    fn failure(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.failure()
    }
    fn take_cleanup_failure(&mut self) -> Option<Box<dyn std::error::Error + Send>> {
        self.0.take_failure()
    }
}
