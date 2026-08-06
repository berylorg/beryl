//! Bounded acceptance-session facade over Beryl's diagnostic child supervisor.

#[path = "acceptance_session/evidence.rs"]
mod evidence;
#[path = "acceptance_session/execution.rs"]
mod execution;
#[path = "acceptance_session/plan.rs"]
mod plan;
#[path = "acceptance_session/validation.rs"]
mod validation;

use std::{
    error::Error,
    fmt, io,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Serialize, Serializer};
use serde_json::Value;
use thiserror::Error;

use crate::diagnostic_child_supervisor::{
    DiagnosticAcceptanceCleanupAttempt, DiagnosticAcceptanceCleanupRetry,
    DiagnosticAcceptanceProcessOwner, DiagnosticAcceptanceStartupOwner, DiagnosticChildLaunch,
    DiagnosticChildStartOutcome, DiagnosticChildSupervisor, DiagnosticChildSupervisorError,
    MAX_DIAGNOSTIC_CHILD_EXECUTABLE_PATH_BYTES, MAX_DIAGNOSTIC_CHILD_WORKSPACE_PATH_BYTES,
};
use validation::{
    paths_overlap, require_directory, require_file, validate_absolute_path, validate_count,
    validate_duration, validate_run_identity,
};

pub use evidence::{
    AcceptanceCleanupAttemptEvidence, AcceptanceCleanupEvidence, AcceptanceEvidence,
    AcceptanceFixtureEvidence, AcceptanceKnownProcessIdentityEvidence, AcceptanceLimitsEvidence,
    AcceptancePayloadEvidence, AcceptanceProcessEvidence, AcceptanceProtocolIdentityRangeEvidence,
    AcceptancePublicationEvidence, AcceptanceRequestEvidence, AcceptanceResponseEvidence,
    AcceptanceStderrEvidence,
};
pub use plan::{AcceptanceRequest, CompiledAcceptanceRequest, compile_acceptance_requests};

pub const ACCEPTANCE_EVIDENCE_SCHEMA_VERSION: u32 = 5;
pub const MAX_ACCEPTANCE_RUN_ID_BYTES: usize = 128;
pub const MAX_ACCEPTANCE_HOME_PATH_BYTES: usize = 1024;
pub const MAX_ACCEPTANCE_EVIDENCE_PATH_BYTES: usize = 1024;
pub const MAX_ACCEPTANCE_REQUESTS: usize = 256;
pub const MAX_ACCEPTANCE_EXPANDED_REQUESTS: usize = MAX_ACCEPTANCE_REQUESTS
    * (crate::diagnostic_child_control::MAX_DIAGNOSTIC_WAIT_TIMEOUT_MS as usize
        / crate::diagnostic_child_control::MIN_DIAGNOSTIC_WAIT_POLL_INTERVAL_MS as usize);
pub const MAX_ACCEPTANCE_OUTPUT_BYTES: usize = 256 * 1024;
pub const MAX_ACCEPTANCE_STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
pub const MAX_ACCEPTANCE_REQUEST_TIMEOUT: Duration = Duration::from_secs(10 * 60);
pub const MAX_ACCEPTANCE_RUNTIME: Duration = Duration::from_secs(24 * 60 * 60);
pub const MAX_ACCEPTANCE_CLEANUP_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_ACCEPTANCE_STARTUP_ERROR_CHARS: usize = 4 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AcceptanceLaunchMode {
    #[default]
    FreshWorkspace,
    ExistingHomeRecovery,
}

impl AcceptanceLaunchMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FreshWorkspace => "fresh_workspace",
            Self::ExistingHomeRecovery => "existing_home_recovery",
        }
    }
}

impl Serialize for AcceptanceLaunchMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[derive(Debug, Error)]
pub enum AcceptanceSessionError {
    #[error("diagnostic acceptance sessions are supported only on host Windows")]
    UnsupportedPlatform,
    #[error("invalid acceptance session configuration: {0}")]
    InvalidConfiguration(String),
    #[error("failed to {action} {path}: {source}")]
    PathIo {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("unsupported diagnostic command {0:?}")]
    UnsupportedCommand(String),
    #[error("acceptance session exceeded its {limit} request limit")]
    RequestLimit { limit: usize },
    #[error("acceptance session exceeded its runtime limit of {limit:?}")]
    RuntimeLimit { limit: Duration },
    #[error("diagnostic request {request_id:?} failed: {message}")]
    DiagnosticRequest {
        request_id: Option<String>,
        message: String,
    },
    #[error("diagnostic child launch failed: {0}")]
    Launch(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceptanceDiagnosticStartupCauseKind {
    BerylHome,
    ExecutableIdentity,
    HostWorkspace,
    ProcessSpawn,
    ProcessPipes,
    RequestWrite,
    TransportThread,
    WriterSpawn,
    ReaderSpawn,
    UnsupportedHost,
    RequestTimeout,
    Protocol,
    StartupProtocol,
    ProcessControl,
    JobCreate,
    JobConfigure,
    JobAssign,
    JobTerminate,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("{message}")]
pub struct AcceptanceDiagnosticStartupCause {
    kind: AcceptanceDiagnosticStartupCauseKind,
    message: String,
}

impl AcceptanceDiagnosticStartupCause {
    pub fn kind(&self) -> AcceptanceDiagnosticStartupCauseKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Error)]
pub enum AcceptanceSessionStartCause {
    #[error("{0}")]
    Session(#[source] AcceptanceSessionError),
    #[error("diagnostic child startup failed: {0}")]
    Diagnostic(#[source] AcceptanceDiagnosticStartupCause),
}

impl From<AcceptanceSessionError> for AcceptanceSessionStartCause {
    fn from(error: AcceptanceSessionError) -> Self {
        Self::Session(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptanceStartupProcessIdentity {
    pid: u32,
    home_dir: PathBuf,
    executable_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AcceptanceCleanupFinalState {
    VerifiedReclaimed,
    Indeterminate {
        identity: AcceptanceStartupProcessIdentity,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AcceptancePublicationState {
    Published,
    Failed { error: String },
}

#[must_use = "terminal cleanup and publication outcomes must be inspected"]
pub struct AcceptanceFinishOutcome {
    evidence: AcceptanceEvidence,
    cleanup: AcceptanceCleanupFinalState,
    publication: AcceptancePublicationState,
    retained_owner: Option<DiagnosticAcceptanceProcessOwner>,
}

impl AcceptanceFinishOutcome {
    pub fn evidence(&self) -> &AcceptanceEvidence {
        &self.evidence
    }

    pub fn cleanup(&self) -> &AcceptanceCleanupFinalState {
        &self.cleanup
    }

    pub fn publication(&self) -> &AcceptancePublicationState {
        &self.publication
    }

    pub fn retained_identity(&self) -> Option<&AcceptanceStartupProcessIdentity> {
        match &self.cleanup {
            AcceptanceCleanupFinalState::Indeterminate { identity } => Some(identity),
            AcceptanceCleanupFinalState::VerifiedReclaimed => None,
        }
    }

    pub fn release_owner_fail_safe_nonblocking(
        &mut self,
    ) -> Option<AcceptanceStartupProcessIdentity> {
        let identity = self
            .retained_owner
            .as_mut()
            .and_then(DiagnosticAcceptanceProcessOwner::release_fail_safe_nonblocking)
            .map(startup_identity);
        drop(self.retained_owner.take());
        identity
    }
}

impl fmt::Debug for AcceptanceFinishOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AcceptanceFinishOutcome")
            .field("evidence", &self.evidence)
            .field("cleanup", &self.cleanup)
            .field("publication", &self.publication)
            .field("has_retained_owner", &self.retained_owner.is_some())
            .finish()
    }
}

impl AcceptanceStartupProcessIdentity {
    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn home_dir(&self) -> &Path {
        &self.home_dir
    }

    pub fn executable_path(&self) -> &Path {
        &self.executable_path
    }
}

#[must_use = "startup cleanup outcomes must be inspected"]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AcceptanceStartupCleanupOutcome {
    Reclaimed {
        identity: AcceptanceStartupProcessIdentity,
    },
    StillRetained {
        identity: AcceptanceStartupProcessIdentity,
        error: String,
    },
    AlreadyReclaimed,
}

#[must_use = "a startup failure can retain an exact process owner that requires explicit cleanup"]
pub struct AcceptanceSessionStartFailure {
    cause: AcceptanceSessionStartCause,
    initial_cleanup_error: Option<String>,
    identity: Option<AcceptanceStartupProcessIdentity>,
    owner: Option<DiagnosticAcceptanceStartupOwner>,
}

impl AcceptanceSessionStartFailure {
    fn without_owner(cause: AcceptanceSessionError) -> Self {
        Self {
            cause: cause.into(),
            initial_cleanup_error: None,
            identity: None,
            owner: None,
        }
    }

    fn without_owner_cause(cause: AcceptanceSessionStartCause) -> Self {
        Self {
            cause,
            initial_cleanup_error: None,
            identity: None,
            owner: None,
        }
    }

    pub fn cause(&self) -> &AcceptanceSessionStartCause {
        &self.cause
    }

    pub fn initial_cleanup_error(&self) -> Option<&str> {
        self.initial_cleanup_error.as_deref()
    }

    pub fn retained_identity(&self) -> Option<&AcceptanceStartupProcessIdentity> {
        self.owner.as_ref().and(self.identity.as_ref())
    }

    pub fn has_owner(&self) -> bool {
        self.owner.is_some()
    }

    pub fn retry_cleanup(
        &mut self,
        timeout: Duration,
    ) -> Result<AcceptanceStartupCleanupOutcome, AcceptanceSessionError> {
        if timeout.is_zero() || timeout > MAX_ACCEPTANCE_CLEANUP_TIMEOUT {
            return Err(AcceptanceSessionError::InvalidConfiguration(format!(
                "startup recovery cleanup timeout must be nonzero and at most {MAX_ACCEPTANCE_CLEANUP_TIMEOUT:?}"
            )));
        }
        let Some(owner) = self.owner.as_mut() else {
            return Ok(AcceptanceStartupCleanupOutcome::AlreadyReclaimed);
        };
        let (grace_timeout, termination_timeout) = cleanup_timeouts(timeout);
        match owner.retry_cleanup(grace_timeout, termination_timeout) {
            DiagnosticAcceptanceCleanupRetry::Reclaimed(identity) => {
                let identity = startup_identity(identity);
                drop(self.owner.take());
                self.identity = Some(identity.clone());
                Ok(AcceptanceStartupCleanupOutcome::Reclaimed { identity })
            }
            DiagnosticAcceptanceCleanupRetry::StillRetained { identity, error } => {
                let identity = startup_identity(identity);
                self.identity = Some(identity.clone());
                Ok(AcceptanceStartupCleanupOutcome::StillRetained {
                    identity,
                    error: bounded_startup_message(error.to_string()),
                })
            }
            DiagnosticAcceptanceCleanupRetry::AlreadyReclaimed => {
                drop(self.owner.take());
                Ok(AcceptanceStartupCleanupOutcome::AlreadyReclaimed)
            }
        }
    }

    pub fn release_owner_fail_safe_nonblocking(
        &mut self,
    ) -> Option<AcceptanceStartupProcessIdentity> {
        let identity = self.retained_identity().cloned();
        drop(self.owner.take());
        identity
    }
}

impl fmt::Debug for AcceptanceSessionStartFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AcceptanceSessionStartFailure")
            .field("cause", &self.cause)
            .field("initial_cleanup_error", &self.initial_cleanup_error)
            .field("identity", &self.identity)
            .field("has_owner", &self.has_owner())
            .finish()
    }
}

impl fmt::Display for AcceptanceSessionStartFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "acceptance session startup failed: {}",
            self.cause
        )?;
        if let Some(error) = &self.initial_cleanup_error {
            write!(formatter, "; initial cleanup was indeterminate: {error}")?;
        }
        if let Some(identity) = self.retained_identity() {
            write!(
                formatter,
                "; exact process {} remains retained",
                identity.pid
            )?;
        }
        Ok(())
    }
}

impl Error for AcceptanceSessionStartFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.cause)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptanceLimits {
    startup_timeout: Duration,
    request_timeout: Duration,
    runtime_timeout: Duration,
    max_requests: usize,
    max_output_bytes: usize,
    cleanup_timeout: Duration,
}

impl AcceptanceLimits {
    pub fn new(
        startup_timeout: Duration,
        request_timeout: Duration,
        runtime_timeout: Duration,
        max_requests: usize,
        max_output_bytes: usize,
        cleanup_timeout: Duration,
    ) -> Result<Self, AcceptanceSessionError> {
        validate_duration(
            "startup timeout",
            startup_timeout,
            MAX_ACCEPTANCE_STARTUP_TIMEOUT,
        )?;
        validate_duration(
            "request timeout",
            request_timeout,
            MAX_ACCEPTANCE_REQUEST_TIMEOUT,
        )?;
        validate_duration("runtime timeout", runtime_timeout, MAX_ACCEPTANCE_RUNTIME)?;
        validate_duration(
            "cleanup timeout",
            cleanup_timeout,
            MAX_ACCEPTANCE_CLEANUP_TIMEOUT,
        )?;
        validate_count("request", max_requests, MAX_ACCEPTANCE_REQUESTS)?;
        validate_count("output byte", max_output_bytes, MAX_ACCEPTANCE_OUTPUT_BYTES)?;
        Ok(Self {
            startup_timeout,
            request_timeout,
            runtime_timeout,
            max_requests,
            max_output_bytes,
            cleanup_timeout,
        })
    }

    pub fn startup_timeout(&self) -> Duration {
        self.startup_timeout
    }

    pub fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    pub fn runtime_timeout(&self) -> Duration {
        self.runtime_timeout
    }

    pub fn max_requests(&self) -> usize {
        self.max_requests
    }

    pub fn max_output_bytes(&self) -> usize {
        self.max_output_bytes
    }

    pub fn cleanup_timeout(&self) -> Duration {
        self.cleanup_timeout
    }
}

#[derive(Clone, Debug)]
pub struct AcceptanceSessionConfig {
    executable_path: PathBuf,
    isolated_home: PathBuf,
    launch_mode: AcceptanceLaunchMode,
    execution_workspace: Option<PathBuf>,
    evidence_path: PathBuf,
    run_identity: String,
    limits: AcceptanceLimits,
    recovery_cleanup_timeout: Duration,
}

impl AcceptanceSessionConfig {
    pub fn new(
        executable_path: impl Into<PathBuf>,
        isolated_home: impl Into<PathBuf>,
        launch_mode: AcceptanceLaunchMode,
        execution_workspace: Option<PathBuf>,
        evidence_path: impl Into<PathBuf>,
        run_identity: impl Into<String>,
        limits: AcceptanceLimits,
        recovery_cleanup_timeout: Duration,
    ) -> Result<Self, AcceptanceSessionError> {
        let config = Self {
            executable_path: executable_path.into(),
            isolated_home: isolated_home.into(),
            launch_mode,
            execution_workspace,
            evidence_path: evidence_path.into(),
            run_identity: run_identity.into(),
            limits,
            recovery_cleanup_timeout,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn evidence_path(&self) -> &Path {
        &self.evidence_path
    }

    pub fn launch_mode(&self) -> AcceptanceLaunchMode {
        self.launch_mode
    }

    pub fn execution_workspace(&self) -> Option<&Path> {
        self.execution_workspace.as_deref()
    }

    pub fn limits(&self) -> &AcceptanceLimits {
        &self.limits
    }

    pub fn recovery_cleanup_timeout(&self) -> Duration {
        self.recovery_cleanup_timeout
    }

    fn validate(&self) -> Result<(), AcceptanceSessionError> {
        #[cfg(not(target_os = "windows"))]
        return Err(AcceptanceSessionError::UnsupportedPlatform);

        validate_run_identity(&self.run_identity)?;
        validate_duration(
            "recovery cleanup timeout",
            self.recovery_cleanup_timeout,
            MAX_ACCEPTANCE_CLEANUP_TIMEOUT,
        )?;
        validate_absolute_path(
            "executable",
            &self.executable_path,
            MAX_DIAGNOSTIC_CHILD_EXECUTABLE_PATH_BYTES,
        )?;
        validate_absolute_path(
            "isolated home",
            &self.isolated_home,
            MAX_ACCEPTANCE_HOME_PATH_BYTES,
        )?;
        match (self.launch_mode, self.execution_workspace.as_deref()) {
            (AcceptanceLaunchMode::FreshWorkspace, Some(workspace)) => {
                validate_absolute_path(
                    "execution workspace",
                    workspace,
                    MAX_DIAGNOSTIC_CHILD_WORKSPACE_PATH_BYTES,
                )?;
                require_directory(workspace, "inspect execution workspace")?;
            }
            (AcceptanceLaunchMode::FreshWorkspace, None) => {
                return Err(AcceptanceSessionError::InvalidConfiguration(
                    "fresh-workspace launch requires an execution workspace".to_string(),
                ));
            }
            (AcceptanceLaunchMode::ExistingHomeRecovery, Some(_)) => {
                return Err(AcceptanceSessionError::InvalidConfiguration(
                    "existing-home recovery launch forbids an execution workspace".to_string(),
                ));
            }
            (AcceptanceLaunchMode::ExistingHomeRecovery, None) => {
                require_directory(&self.isolated_home, "inspect isolated home")?;
            }
        }
        validate_absolute_path(
            "evidence",
            &self.evidence_path,
            MAX_ACCEPTANCE_EVIDENCE_PATH_BYTES,
        )?;
        require_file(&self.executable_path, "inspect executable")?;
        if self.launch_mode == AcceptanceLaunchMode::FreshWorkspace && self.isolated_home.exists() {
            require_directory(&self.isolated_home, "inspect isolated home")?;
        }
        if self.evidence_path.exists() {
            return Err(AcceptanceSessionError::InvalidConfiguration(format!(
                "evidence path {} already exists",
                self.evidence_path.display()
            )));
        }
        let evidence_parent = self.evidence_path.parent().ok_or_else(|| {
            AcceptanceSessionError::InvalidConfiguration(
                "evidence path must have an existing parent directory".to_string(),
            )
        })?;
        require_directory(evidence_parent, "inspect evidence parent")?;

        let mut paths = vec![
            ("executable", self.executable_path.as_path()),
            ("isolated home", self.isolated_home.as_path()),
            ("evidence", self.evidence_path.as_path()),
        ];
        if let Some(workspace) = self.execution_workspace.as_deref() {
            paths.push(("execution workspace", workspace));
        }
        for (index, (left_label, left)) in paths.iter().enumerate() {
            for (right_label, right) in paths.iter().skip(index + 1) {
                if paths_overlap(left, right) {
                    return Err(AcceptanceSessionError::InvalidConfiguration(format!(
                        "{left_label} path {} collides with {right_label} path {}",
                        left.display(),
                        right.display()
                    )));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct AcceptanceResponse {
    request_id: Option<String>,
    result: Value,
}

impl AcceptanceResponse {
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    pub fn result(&self) -> &Value {
        &self.result
    }

    pub fn into_result(self) -> Value {
        self.result
    }
}

pub struct AcceptanceSession {
    config: AcceptanceSessionConfig,
    supervisor: DiagnosticChildSupervisor,
    started_at: Instant,
    evidence: Option<evidence::EvidenceBuilder>,
    cleanup_grace_timeout: Duration,
    cleanup_termination_timeout: Duration,
    finished: bool,
}

impl AcceptanceSession {
    pub fn start(config: AcceptanceSessionConfig) -> Result<Self, AcceptanceSessionStartFailure> {
        config.validate().map_err(|error| {
            if matches!(
                &error,
                AcceptanceSessionError::PathIo { path, .. } if path == &config.executable_path
            ) {
                AcceptanceSessionStartFailure::without_owner_cause(executable_identity_cause(error))
            } else {
                AcceptanceSessionStartFailure::without_owner(error)
            }
        })?;
        let (executable_bytes, executable_sha256) =
            evidence::executable_identity(&config.executable_path).map_err(|error| {
                AcceptanceSessionStartFailure::without_owner_cause(executable_identity_cause(error))
            })?;
        let (cleanup_grace_timeout, cleanup_termination_timeout) =
            cleanup_timeouts(config.limits.cleanup_timeout);
        let started_at = Instant::now();
        let started_at_unix_millis = unix_millis(SystemTime::now());
        let launch = DiagnosticChildLaunch::new(
            config.isolated_home.clone(),
            config.executable_path.clone(),
        );
        let launch = match config.execution_workspace.clone() {
            Some(workspace) => launch.with_host_workspace(workspace),
            None => launch,
        };
        let mut supervisor = DiagnosticChildSupervisor::default();
        let identity = match supervisor.start_for_acceptance(
            launch,
            config.limits.startup_timeout,
            cleanup_grace_timeout,
            cleanup_termination_timeout,
        ) {
            Ok(DiagnosticChildStartOutcome::Started(identity)) => identity,
            Ok(DiagnosticChildStartOutcome::AlreadyRunning(_)) => {
                return Err(AcceptanceSessionStartFailure::without_owner(
                    AcceptanceSessionError::Launch(
                        "acceptance supervisor reported an already-running child".to_string(),
                    ),
                ));
            }
            Err(error) => {
                let (cause, initial_cleanup_error, owner) = error.into_parts();
                let identity = owner
                    .as_ref()
                    .map(|owner| startup_identity(owner.identity()));
                return Err(AcceptanceSessionStartFailure {
                    cause: diagnostic_start_cause(cause),
                    initial_cleanup_error: initial_cleanup_error
                        .map(|error| bounded_startup_message(error.to_string())),
                    identity,
                    owner,
                });
            }
        };
        let evidence = evidence::EvidenceBuilder::new(
            &config,
            identity.executable_path,
            executable_bytes,
            executable_sha256,
            identity.pid,
            started_at_unix_millis,
            duration_millis(cleanup_grace_timeout),
            duration_millis(cleanup_termination_timeout),
        );
        Ok(Self {
            config,
            supervisor,
            started_at,
            evidence: Some(evidence),
            cleanup_grace_timeout,
            cleanup_termination_timeout,
            finished: false,
        })
    }

    pub fn finish(mut self) -> AcceptanceFinishOutcome {
        let supervisor = std::mem::take(&mut self.supervisor);
        let mut owner = supervisor.into_acceptance_process_owner();
        self.finished = true;
        let mut attempts = Vec::with_capacity(2);
        let mut stderr = crate::diagnostic_child_supervisor::DiagnosticStderrSnapshot::default();
        let mut reclaimed = owner.is_none();
        if let Some(owner) = owner.as_mut() {
            let cleanup_started = Instant::now();
            if let Some(attempt) =
                owner.cleanup_attempt(self.cleanup_grace_timeout, self.cleanup_termination_timeout)
            {
                reclaimed = attempt.reclaimed();
                stderr = attempt.stderr.clone();
                attempts.push(cleanup_attempt_evidence(
                    1,
                    self.config.limits.cleanup_timeout,
                    cleanup_started.elapsed(),
                    attempt,
                ));
            }
        }
        if !reclaimed {
            let recovery_cleanup_timeout = self.config.recovery_cleanup_timeout;
            let (grace_timeout, termination_timeout) = cleanup_timeouts(recovery_cleanup_timeout);
            let cleanup_started = Instant::now();
            if let Some(attempt) = owner
                .as_mut()
                .and_then(|owner| owner.cleanup_attempt(grace_timeout, termination_timeout))
            {
                reclaimed = attempt.reclaimed();
                stderr = attempt.stderr.clone();
                attempts.push(cleanup_attempt_evidence(
                    2,
                    recovery_cleanup_timeout,
                    cleanup_started.elapsed(),
                    attempt,
                ));
            }
        }
        let retained_identity = if reclaimed {
            None
        } else {
            owner
                .as_ref()
                .map(|owner| startup_identity(owner.identity()))
        };
        let retained_evidence = retained_identity.as_ref().map(known_process_evidence);
        let finished_at = SystemTime::now();
        let run_duration = duration_millis(self.started_at.elapsed());
        let evidence = self
            .evidence
            .as_mut()
            .expect("active acceptance session has evidence");
        evidence.complete_cleanup(
            unix_millis(finished_at),
            run_duration,
            attempts,
            retained_evidence,
            stderr,
        );
        let evidence = self
            .evidence
            .take()
            .expect("finished acceptance session has evidence");
        let (evidence, publication) = evidence.publish(&self.config.evidence_path);
        let publication = match publication {
            Ok(()) => AcceptancePublicationState::Published,
            Err(_) => AcceptancePublicationState::Failed {
                error: evidence
                    .publication
                    .error
                    .as_ref()
                    .expect("failed publication records structured bounded evidence")
                    .bounded_prefix
                    .clone(),
            },
        };
        let cleanup = retained_identity
            .map(|identity| AcceptanceCleanupFinalState::Indeterminate { identity })
            .unwrap_or(AcceptanceCleanupFinalState::VerifiedReclaimed);
        AcceptanceFinishOutcome {
            evidence,
            cleanup,
            publication,
            retained_owner: if reclaimed { None } else { owner },
        }
    }
}

impl Drop for AcceptanceSession {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let supervisor = std::mem::take(&mut self.supervisor);
        if let Some(mut owner) = supervisor.into_acceptance_process_owner() {
            let _ =
                owner.cleanup_attempt(self.cleanup_grace_timeout, self.cleanup_termination_timeout);
            drop(owner);
        }
    }
}

#[cfg(test)]
pub(crate) fn executable_identity_for_test(
    path: &Path,
) -> Result<(u64, String), AcceptanceSessionError> {
    evidence::executable_identity(path)
}

fn cleanup_attempt_evidence(
    ordinal: usize,
    budget: Duration,
    duration: Duration,
    attempt: DiagnosticAcceptanceCleanupAttempt,
) -> evidence::PendingCleanupAttemptEvidence {
    let residue = if attempt.reclaimed() {
        "verified_reclaimed"
    } else {
        "indeterminate"
    };
    evidence::PendingCleanupAttemptEvidence {
        ordinal,
        budget_millis: duration_millis(budget),
        duration_millis: duration_millis(duration),
        phase: attempt.phase.to_string(),
        termination_method: attempt.termination_method.to_string(),
        error: attempt.error.map(|error| error.to_string()),
        known_process: AcceptanceKnownProcessIdentityEvidence {
            pid: attempt.identity.pid,
            executable_path: attempt.identity.executable_path,
            isolated_home: attempt.identity.home_dir,
        },
        residue: residue.to_string(),
    }
}

fn known_process_evidence(
    identity: &AcceptanceStartupProcessIdentity,
) -> AcceptanceKnownProcessIdentityEvidence {
    AcceptanceKnownProcessIdentityEvidence {
        pid: identity.pid,
        executable_path: identity.executable_path.clone(),
        isolated_home: identity.home_dir.clone(),
    }
}

fn cleanup_timeouts(total: Duration) -> (Duration, Duration) {
    let graceful = (total / 3).min(Duration::from_millis(250));
    let termination = (total - graceful) / 2;
    (graceful, termination)
}

fn startup_identity(
    identity: crate::diagnostic_child_supervisor::DiagnosticChildIdentity,
) -> AcceptanceStartupProcessIdentity {
    AcceptanceStartupProcessIdentity {
        pid: identity.pid,
        home_dir: identity.home_dir,
        executable_path: identity.executable_path,
    }
}

fn bounded_startup_message(message: String) -> String {
    let mut chars = message.chars();
    let bounded = chars
        .by_ref()
        .take(MAX_ACCEPTANCE_STARTUP_ERROR_CHARS)
        .collect::<String>();
    if chars.next().is_some() {
        format!("{bounded}…")
    } else {
        bounded
    }
}

fn executable_identity_cause(error: AcceptanceSessionError) -> AcceptanceSessionStartCause {
    AcceptanceSessionStartCause::Diagnostic(AcceptanceDiagnosticStartupCause {
        kind: AcceptanceDiagnosticStartupCauseKind::ExecutableIdentity,
        message: bounded_startup_message(error.to_string()),
    })
}

fn diagnostic_start_cause(error: DiagnosticChildSupervisorError) -> AcceptanceSessionStartCause {
    let kind = match &error {
        DiagnosticChildSupervisorError::BerylHomeDir(_)
        | DiagnosticChildSupervisorError::HomeCollidesWithSupervisor { .. } => {
            AcceptanceDiagnosticStartupCauseKind::BerylHome
        }
        DiagnosticChildSupervisorError::CurrentExecutable { .. }
        | DiagnosticChildSupervisorError::InvalidExecutablePath { .. }
        | DiagnosticChildSupervisorError::ExecutablePathAccess { .. } => {
            AcceptanceDiagnosticStartupCauseKind::ExecutableIdentity
        }
        DiagnosticChildSupervisorError::InvalidHostWorkspacePath { .. }
        | DiagnosticChildSupervisorError::HostWorkspacePathAccess { .. } => {
            AcceptanceDiagnosticStartupCauseKind::HostWorkspace
        }
        DiagnosticChildSupervisorError::Spawn { .. } => {
            AcceptanceDiagnosticStartupCauseKind::ProcessSpawn
        }
        DiagnosticChildSupervisorError::MissingStdin
        | DiagnosticChildSupervisorError::MissingStdout
        | DiagnosticChildSupervisorError::MissingStderr => {
            AcceptanceDiagnosticStartupCauseKind::ProcessPipes
        }
        DiagnosticChildSupervisorError::WriteRequest { .. } => {
            AcceptanceDiagnosticStartupCauseKind::RequestWrite
        }
        DiagnosticChildSupervisorError::WriterThreadPanicked
        | DiagnosticChildSupervisorError::ReaderThreadPanicked
        | DiagnosticChildSupervisorError::StderrThreadPanicked => {
            AcceptanceDiagnosticStartupCauseKind::TransportThread
        }
        DiagnosticChildSupervisorError::SpawnWriter { .. } => {
            AcceptanceDiagnosticStartupCauseKind::WriterSpawn
        }
        DiagnosticChildSupervisorError::SpawnStdoutReader { .. }
        | DiagnosticChildSupervisorError::SpawnStderrReader { .. } => {
            AcceptanceDiagnosticStartupCauseKind::ReaderSpawn
        }
        DiagnosticChildSupervisorError::UnsupportedAcceptanceHost => {
            AcceptanceDiagnosticStartupCauseKind::UnsupportedHost
        }
        DiagnosticChildSupervisorError::RequestTimeout { .. } => {
            AcceptanceDiagnosticStartupCauseKind::RequestTimeout
        }
        DiagnosticChildSupervisorError::ProtocolEof
        | DiagnosticChildSupervisorError::Protocol(_)
        | DiagnosticChildSupervisorError::ChildError { .. } => {
            AcceptanceDiagnosticStartupCauseKind::Protocol
        }
        DiagnosticChildSupervisorError::StartupProtocolTimeout { .. }
        | DiagnosticChildSupervisorError::StartupProtocolEof
        | DiagnosticChildSupervisorError::StartupProtocolMalformed { .. }
        | DiagnosticChildSupervisorError::StartupProtocolRejected { .. }
        | DiagnosticChildSupervisorError::StartupProtocolIncompatible { .. } => {
            AcceptanceDiagnosticStartupCauseKind::StartupProtocol
        }
        DiagnosticChildSupervisorError::QueryStatus { .. }
        | DiagnosticChildSupervisorError::Terminate { .. } => {
            AcceptanceDiagnosticStartupCauseKind::ProcessControl
        }
        #[cfg(target_os = "windows")]
        DiagnosticChildSupervisorError::CreateProcessJob { .. } => {
            AcceptanceDiagnosticStartupCauseKind::JobCreate
        }
        #[cfg(target_os = "windows")]
        DiagnosticChildSupervisorError::ConfigureProcessJob { .. } => {
            AcceptanceDiagnosticStartupCauseKind::JobConfigure
        }
        #[cfg(target_os = "windows")]
        DiagnosticChildSupervisorError::AssignProcessToJob { .. } => {
            AcceptanceDiagnosticStartupCauseKind::JobAssign
        }
        #[cfg(target_os = "windows")]
        DiagnosticChildSupervisorError::TerminateProcessJob { .. } => {
            AcceptanceDiagnosticStartupCauseKind::JobTerminate
        }
    };
    AcceptanceSessionStartCause::Diagnostic(AcceptanceDiagnosticStartupCause {
        kind,
        message: bounded_startup_message(error.to_string()),
    })
}

#[cfg(test)]
pub(crate) fn diagnostic_start_cause_for_test(
    error: DiagnosticChildSupervisorError,
) -> AcceptanceSessionStartCause {
    diagnostic_start_cause(error)
}

fn duration_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn unix_millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
