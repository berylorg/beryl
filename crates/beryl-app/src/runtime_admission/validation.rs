use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::cas_projection::RuntimeTokenDirectory;
use beryl_backend::WslSupervisorArtifact;
use beryl_home_store::CommandCancellation;
use beryl_model::{AdmittedHostPath, RuntimeId, RuntimeLaunchForm, RuntimeMode, RuntimeNativePath};

mod backend;
mod filesystem;
mod filesystem_worker;
mod seams;

#[cfg(feature = "test-faults")]
pub use filesystem_worker::FilesystemWorkerTestControl;
#[cfg(feature = "test-faults")]
pub use seams::{
    RuntimeQualificationBackend, RuntimeQualificationCandidate, ValidationCleanup,
    ValidationFilesystem, ValidationFilesystemPath,
};
use seams::{RuntimeQualificationBackend as Backend, ValidationFilesystem as Filesystem};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidationLimits {
    pub path_bytes: usize,
    pub filesystem_timeout: Duration,
    pub qualification_timeout: Duration,
    pub foreground_control_capacity: usize,
}

impl Default for ValidationLimits {
    fn default() -> Self {
        Self {
            path_bytes: 4096,
            filesystem_timeout: Duration::from_secs(10),
            qualification_timeout: Duration::from_secs(30),
            foreground_control_capacity: 32,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ValidationIssue {
    #[error("selected path exceeds the validation bound or has unsupported syntax")]
    InvalidPath,
    #[error("selected path belongs to an unsupported or different environment")]
    EnvironmentMismatch,
    #[error("selected executable is not an accessible executable file")]
    ExecutableUnavailable,
    #[error("selected root is not an accessible directory")]
    DirectoryUnavailable,
    #[error("the runtime's exact user home could not be validated")]
    HomeUnavailable,
    #[error("runtime token directory cannot be mapped into the exact environment")]
    TokenDirectoryUnavailable,
    #[error("the immutable bundled WSL supervisor is unavailable")]
    SupervisorUnavailable,
    #[error("managed runtime launch failed")]
    Launch,
    #[error("managed foreground connection failed")]
    ForegroundConnection,
    #[error("managed foreground release initialization failed")]
    Initialize,
    #[error("managed foreground release or effective configuration was rejected")]
    ReleaseRejected,
    #[error("validation was cancelled")]
    Cancelled,
    #[error("validation exceeded its time bound")]
    Timeout,
    #[error("validation cleanup is not proven complete")]
    Cleanup,
    #[error("validation limits are outside the supported envelope")]
    InvalidLimits,
    #[error("filesystem validation worker failed")]
    FilesystemWorker,
}

pub struct ValidationError {
    issue: ValidationIssue,
    cleanup: Option<Box<dyn seams::ValidationCleanup>>,
    source: Option<Box<dyn std::error::Error + Send>>,
}

impl ValidationError {
    pub(crate) fn new(issue: ValidationIssue) -> Self {
        Self {
            issue,
            cleanup: None,
            source: None,
        }
    }

    pub(crate) fn with_cleanup(
        issue: ValidationIssue,
        cleanup: Box<dyn seams::ValidationCleanup>,
    ) -> Self {
        Self {
            issue,
            cleanup: Some(cleanup),
            source: None,
        }
    }

    pub(super) fn with_source(mut self, source: Option<Box<dyn std::error::Error + Send>>) -> Self {
        self.source = source;
        self
    }

    #[cfg(feature = "test-faults")]
    pub fn with_test_cleanup(
        issue: ValidationIssue,
        cleanup: Box<dyn seams::ValidationCleanup>,
    ) -> Self {
        Self::with_cleanup(issue, cleanup)
    }

    pub fn issue(&self) -> ValidationIssue {
        self.issue
    }

    pub fn has_cleanup_custody(&self) -> bool {
        self.cleanup.is_some()
    }

    pub fn dispose_cleanup(&mut self) -> Result<(), ValidationIssue> {
        if let Some(owner) = self.cleanup.as_mut() {
            owner.cleanup()?;
            if let Some(diagnostic) = owner.take_cleanup_failure() {
                self.source = Some(match self.source.take() {
                    Some(primary) => Box::new(CleanupDiagnostics {
                        primary,
                        cleanup: diagnostic,
                    }),
                    None => diagnostic,
                });
            }
        }
        self.cleanup = None;
        Ok(())
    }
}

#[derive(Debug)]
struct CleanupDiagnostics {
    primary: Box<dyn std::error::Error + Send>,
    cleanup: Box<dyn std::error::Error + Send>,
}

impl std::fmt::Display for CleanupDiagnostics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}; cleanup: {}", self.primary, self.cleanup)
    }
}

impl std::error::Error for CleanupDiagnostics {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.primary.as_ref())
    }
}

impl std::fmt::Debug for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidationError")
            .field("issue", &self.issue)
            .field("retained_cleanup", &self.has_cleanup_custody())
            .finish()
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.issue.fmt(f)?;
        if self.has_cleanup_custody() {
            f.write_str("; exact cleanup custody is retained")?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|source| source.as_ref() as &(dyn std::error::Error + 'static))
            .or_else(|| self.cleanup.as_ref().and_then(|owner| owner.failure()))
    }
}

impl From<ValidationIssue> for ValidationError {
    fn from(issue: ValidationIssue) -> Self {
        Self::new(issue)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedExecutable {
    canonical_executable: AdmittedHostPath,
    runtime_native_executable: RuntimeNativePath,
}

impl ResolvedExecutable {
    pub fn canonical_executable(&self) -> &AdmittedHostPath {
        &self.canonical_executable
    }
    pub fn mode(&self) -> &RuntimeMode {
        self.runtime_native_executable.mode()
    }
    pub fn runtime_native_executable(&self) -> &RuntimeNativePath {
        &self.runtime_native_executable
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmittedRoot {
    canonical_path: RuntimeNativePath,
    display_path: AdmittedHostPath,
}

impl AdmittedRoot {
    pub fn canonical_path(&self) -> &RuntimeNativePath {
        &self.canonical_path
    }
    pub fn runtime_native_path(&self) -> &RuntimeNativePath {
        &self.canonical_path
    }
    pub fn display_path(&self) -> &AdmittedHostPath {
        &self.display_path
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmittedRuntime {
    runtime_id: RuntimeId,
    launch_form: RuntimeLaunchForm,
    executable: ResolvedExecutable,
    home: AdmittedRoot,
}

impl AdmittedRuntime {
    pub fn runtime_id(&self) -> RuntimeId {
        self.runtime_id
    }
    pub fn launch_form(&self) -> RuntimeLaunchForm {
        self.launch_form
    }
    pub fn canonical_executable(&self) -> &AdmittedHostPath {
        self.executable.canonical_executable()
    }
    pub fn mode(&self) -> &RuntimeMode {
        self.executable.mode()
    }
    pub fn runtime_native_executable(&self) -> &RuntimeNativePath {
        self.executable.runtime_native_executable()
    }
    pub fn home_root(&self) -> &RuntimeNativePath {
        self.home.canonical_path()
    }
    pub fn home_display_path(&self) -> &AdmittedHostPath {
        self.home.display_path()
    }
}

#[derive(Clone)]
pub struct RuntimePathValidator {
    filesystem: Arc<dyn Filesystem>,
    backend: Arc<dyn Backend>,
    token_directory: RuntimeTokenDirectory,
    limits: ValidationLimits,
    wsl_supervisor_artifact: Option<Arc<WslSupervisorArtifact>>,
}

impl RuntimePathValidator {
    pub fn new(
        token_directory: RuntimeTokenDirectory,
        limits: ValidationLimits,
        wsl_supervisor_artifact: Option<Arc<WslSupervisorArtifact>>,
    ) -> Result<Self, ValidationError> {
        Self::with_seams(
            token_directory,
            limits,
            Arc::new(filesystem::ProductionFilesystem::new(
                wsl_supervisor_artifact.clone(),
            )),
            Arc::new(backend::ProductionBackend),
            wsl_supervisor_artifact,
        )
    }

    fn with_seams(
        token_directory: RuntimeTokenDirectory,
        limits: ValidationLimits,
        filesystem: Arc<dyn Filesystem>,
        backend: Arc<dyn Backend>,
        wsl_supervisor_artifact: Option<Arc<WslSupervisorArtifact>>,
    ) -> Result<Self, ValidationError> {
        if limits.path_bytes == 0
            || limits.path_bytes > 4096
            || limits.filesystem_timeout.is_zero()
            || limits.filesystem_timeout > Duration::from_secs(30)
            || limits.qualification_timeout.is_zero()
            || limits.qualification_timeout > Duration::from_secs(120)
            || limits.foreground_control_capacity == 0
            || limits.foreground_control_capacity > 128
        {
            return Err(ValidationIssue::InvalidLimits.into());
        }
        Ok(Self {
            filesystem,
            backend,
            token_directory,
            limits,
            wsl_supervisor_artifact,
        })
    }

    #[cfg(feature = "test-faults")]
    pub fn with_test_seams(
        token_directory: RuntimeTokenDirectory,
        limits: ValidationLimits,
        filesystem: Arc<dyn Filesystem>,
        backend: Arc<dyn Backend>,
    ) -> Result<Self, ValidationError> {
        Self::with_seams(token_directory, limits, filesystem, backend, None)
    }

    #[cfg(feature = "test-faults")]
    pub fn with_test_supervisor_artifact(mut self, artifact: Arc<WslSupervisorArtifact>) -> Self {
        self.wsl_supervisor_artifact = Some(artifact);
        self
    }

    #[cfg(feature = "test-faults")]
    pub fn observe_test_native_worker(
        &self,
        cancellation: &CommandCancellation,
        operation: impl FnOnce(
            CommandCancellation,
        ) -> Result<seams::ValidationFilesystemPath, ValidationError>
        + Send
        + 'static,
    ) -> Result<AdmittedRoot, ValidationError> {
        let facts = filesystem_worker::observe_with(
            Instant::now() + self.limits.filesystem_timeout,
            cancellation,
            operation,
        )?;
        checked_facts(facts, &RuntimeMode::Host, self.limits.path_bytes)
    }

    #[cfg(feature = "test-faults")]
    pub fn observe_test_native_worker_with_control(
        &self,
        cancellation: &CommandCancellation,
        operation: impl FnOnce(
            CommandCancellation,
        ) -> Result<seams::ValidationFilesystemPath, ValidationError>
        + Send
        + 'static,
        control: FilesystemWorkerTestControl,
    ) -> Result<AdmittedRoot, ValidationError> {
        let facts = filesystem_worker::observe_test(
            Instant::now() + self.limits.filesystem_timeout,
            cancellation,
            operation,
            control,
        )?;
        checked_facts(facts, &RuntimeMode::Host, self.limits.path_bytes)
    }

    pub fn resolve_executable(
        &self,
        selected: &Path,
        cancellation: &CommandCancellation,
    ) -> Result<ResolvedExecutable, ValidationError> {
        let deadline = Instant::now() + self.limits.filesystem_timeout;
        let selected = checked_text(selected, self.limits.path_bytes)?;
        let derived = filesystem::derive_environment(selected)?;
        check_running(deadline, cancellation)?;
        let facts = self.filesystem.executable(
            Path::new(selected),
            deadline,
            cancellation,
            self.limits.path_bytes,
        )?;
        check_running(deadline, cancellation)?;
        let root = checked_facts(facts, &derived, self.limits.path_bytes)?;
        Ok(ResolvedExecutable {
            canonical_executable: root.display_path,
            runtime_native_executable: root.canonical_path,
        })
    }

    pub fn resolve_root(
        &self,
        selected: &Path,
        mode: &RuntimeMode,
        cancellation: &CommandCancellation,
    ) -> Result<AdmittedRoot, ValidationError> {
        let deadline = Instant::now() + self.limits.filesystem_timeout;
        let selected = checked_text(selected, self.limits.path_bytes)?;
        if filesystem::derive_environment(selected)? != *mode {
            return Err(ValidationIssue::EnvironmentMismatch.into());
        }
        check_running(deadline, cancellation)?;
        let facts = self.filesystem.directory(
            Path::new(selected),
            mode,
            deadline,
            cancellation,
            self.limits.path_bytes,
        )?;
        check_running(deadline, cancellation)?;
        checked_facts(facts, mode, self.limits.path_bytes)
    }

    pub fn qualify_runtime(
        &self,
        runtime_id: RuntimeId,
        executable: ResolvedExecutable,
        launch_form: RuntimeLaunchForm,
        cancellation: &CommandCancellation,
    ) -> Result<AdmittedRuntime, ValidationError> {
        if matches!(executable.mode(), RuntimeMode::Wsl(_))
            && self.wsl_supervisor_artifact.is_none()
        {
            return Err(ValidationIssue::SupervisorUnavailable.into());
        }
        let home_deadline = Instant::now() + self.limits.filesystem_timeout;
        check_running(home_deadline, cancellation)?;
        let home = self.filesystem.home(
            executable.mode(),
            home_deadline,
            cancellation,
            self.limits.path_bytes,
        )?;
        check_running(home_deadline, cancellation)?;
        let home = checked_facts(home, executable.mode(), self.limits.path_bytes)?;
        let native_tokens = self
            .token_directory
            .runtime_path(executable.mode())
            .ok_or(ValidationIssue::TokenDirectoryUnavailable)?;
        let mut spec = beryl_backend::ManagedBackendLaunchSpec::new(
            runtime_id,
            executable.canonical_executable.clone(),
            executable.mode().clone(),
            launch_form,
            executable.runtime_native_executable.clone(),
            home.canonical_path.clone(),
            self.token_directory.host().clone(),
            native_tokens,
        )
        .map_err(|_| ValidationIssue::InvalidPath)?;
        if let Some(artifact) = self.wsl_supervisor_artifact.as_ref() {
            spec = spec.with_wsl_supervisor_artifact(artifact.clone());
        }
        let deadline = Instant::now() + self.limits.qualification_timeout;
        check_running(deadline, cancellation)?;
        let mut candidate = self
            .backend
            .launch(spec, self.limits.foreground_control_capacity)?;
        let qualification = (|| {
            candidate.connect(remaining(deadline, cancellation)?)?;
            candidate.initialize(remaining(deadline, cancellation)?)?;
            candidate.admit(home.canonical_path(), remaining(deadline, cancellation)?)?;
            check_running(deadline, cancellation)
        })();
        let cleanup = candidate.cleanup();
        let qualification = qualification.and_then(|()| check_running(deadline, cancellation));
        if cleanup.is_err() {
            return Err(ValidationError::with_cleanup(
                qualification.err().unwrap_or(ValidationIssue::Cleanup),
                Box::new(seams::CandidateCleanup(candidate)),
            ));
        }
        let diagnostic = candidate.take_failure();
        qualification.map_err(|issue| ValidationError::new(issue).with_source(diagnostic))?;
        Ok(AdmittedRuntime {
            runtime_id,
            launch_form,
            executable,
            home,
        })
    }
}

pub(super) fn check_running(
    deadline: Instant,
    cancellation: &CommandCancellation,
) -> Result<(), ValidationIssue> {
    if cancellation.is_cancelled() {
        return Err(ValidationIssue::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(ValidationIssue::Timeout);
    }
    Ok(())
}

fn remaining(
    deadline: Instant,
    cancellation: &CommandCancellation,
) -> Result<Duration, ValidationIssue> {
    check_running(deadline, cancellation)?;
    Ok(deadline.saturating_duration_since(Instant::now()))
}

fn checked_text(path: &Path, bound: usize) -> Result<&str, ValidationIssue> {
    let text = path.to_str().ok_or(ValidationIssue::InvalidPath)?;
    if text.is_empty() || text.len() > bound || text.chars().any(char::is_control) {
        return Err(ValidationIssue::InvalidPath);
    }
    Ok(text)
}

fn checked_facts(
    facts: seams::ValidationFilesystemPath,
    mode: &RuntimeMode,
    bound: usize,
) -> Result<AdmittedRoot, ValidationError> {
    checked_text(Path::new(&facts.host_path), bound)?;
    checked_text(Path::new(&facts.native_path), bound)?;
    if &facts.mode != mode || filesystem::derive_environment(&facts.host_path)? != *mode {
        return Err(ValidationIssue::EnvironmentMismatch.into());
    }
    let flavor = filesystem::host_flavor();
    let display_path = AdmittedHostPath::from_admitted(flavor, &facts.host_path)
        .map_err(|_| ValidationIssue::InvalidPath)?;
    let native_flavor = if matches!(mode, RuntimeMode::Host) {
        flavor
    } else {
        beryl_model::PathFlavor::Posix
    };
    let canonical_path =
        RuntimeNativePath::from_admitted(mode.clone(), native_flavor, &facts.native_path)
            .map_err(|_| ValidationIssue::InvalidPath)?;
    if matches!(mode, RuntimeMode::Host) && facts.host_path != facts.native_path {
        return Err(ValidationIssue::EnvironmentMismatch.into());
    }
    if let RuntimeMode::Wsl(_) = mode {
        if filesystem::wsl_native_path(&facts.host_path)? != facts.native_path {
            return Err(ValidationIssue::EnvironmentMismatch.into());
        }
    }
    Ok(AdmittedRoot {
        canonical_path,
        display_path,
    })
}
