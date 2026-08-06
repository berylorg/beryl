//! Typed command-line boundary for the bounded Beryl acceptance runner.

use std::{
    ffi::OsString,
    fs::File,
    io::Read,
    num::{NonZeroU64, NonZeroUsize},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use beryl_app::{
    AcceptanceCleanupFinalState, AcceptanceLaunchMode, AcceptanceLimits,
    AcceptancePublicationState, AcceptanceRequest, AcceptanceSession, AcceptanceSessionConfig,
    AcceptanceStartupCleanupOutcome, MAX_ACCEPTANCE_CLEANUP_TIMEOUT, MAX_ACCEPTANCE_OUTPUT_BYTES,
    MAX_ACCEPTANCE_REQUESTS, compile_acceptance_requests,
};
use clap::{Parser, ValueEnum};
use serde::Deserialize;
use serde_json::{Value, json};

pub const ACCEPTANCE_REQUEST_PLAN_SCHEMA_VERSION: u32 = 1;
pub const MAX_ACCEPTANCE_REQUEST_PLAN_BYTES: usize = 256 * 1024;
pub const MAX_ACCEPTANCE_REQUEST_PLAN_PATH_BYTES: usize = 1024;

#[doc(hidden)]
pub enum AcceptanceCliRecoveryOutcome {
    Reclaimed {
        pid: u32,
    },
    StillRetained {
        pid: u32,
        home_dir: PathBuf,
        executable_path: PathBuf,
        error: String,
    },
    AlreadyReclaimed,
}

#[doc(hidden)]
pub trait AcceptanceCliStartupFailure {
    fn message(&self) -> String;
    fn retry_cleanup(
        &mut self,
        timeout: Duration,
    ) -> std::result::Result<AcceptanceCliRecoveryOutcome, String>;
    fn has_owner(&self) -> bool;
    fn release_owner_fail_safe_nonblocking(&mut self) -> Option<u32>;
}

struct RealStartupFailure(beryl_app::AcceptanceSessionStartFailure);

#[derive(Clone, Debug, Parser)]
#[command(
    name = "beryl-acceptance",
    about = "Run bounded diagnostic commands against one frozen Beryl executable."
)]
pub struct AcceptanceCli {
    #[arg(
        long,
        value_name = "PATH",
        help = "Absolute frozen Beryl executable path"
    )]
    executable: PathBuf,

    #[arg(long, value_name = "PATH", help = "Absolute isolated Beryl home path")]
    isolated_home: PathBuf,

    #[arg(
        long,
        value_enum,
        default_value = "fresh-workspace",
        help = "Launch mode: fresh-workspace or existing-home-recovery"
    )]
    launch_mode: AcceptanceCliLaunchMode,

    #[arg(
        long,
        value_name = "PATH",
        help = "Absolute existing host workspace path required only by fresh-workspace launch"
    )]
    execution_workspace: Option<PathBuf>,

    #[arg(
        long,
        value_name = "PATH",
        help = "Absolute non-existing evidence JSON path"
    )]
    evidence: PathBuf,

    #[arg(long, value_name = "ID", help = "Stable bounded evidence run identity")]
    run_identity: String,

    #[arg(
        long,
        value_name = "PATH",
        help = "Absolute bounded JSON request-plan path"
    )]
    request_plan: PathBuf,

    #[arg(long, value_name = "MS", default_value = "5000")]
    startup_timeout_ms: NonZeroU64,

    #[arg(long, value_name = "MS", default_value = "10000")]
    request_timeout_ms: NonZeroU64,

    #[arg(long, value_name = "MS", default_value = "600000")]
    runtime_timeout_ms: NonZeroU64,

    #[arg(long, value_name = "COUNT", default_value = "64")]
    max_requests: NonZeroUsize,

    #[arg(long, value_name = "BYTES", default_value = "65536")]
    max_output_bytes: NonZeroUsize,

    #[arg(long, value_name = "MS", default_value = "11000")]
    cleanup_timeout_ms: NonZeroU64,

    #[arg(
        long,
        value_name = "MS",
        default_value = "11000",
        help = "Separate bounded retry budget after indeterminate startup or terminal cleanup"
    )]
    recovery_cleanup_timeout_ms: NonZeroU64,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum AcceptanceCliLaunchMode {
    FreshWorkspace,
    ExistingHomeRecovery,
}

impl From<AcceptanceCliLaunchMode> for AcceptanceLaunchMode {
    fn from(mode: AcceptanceCliLaunchMode) -> Self {
        match mode {
            AcceptanceCliLaunchMode::FreshWorkspace => Self::FreshWorkspace,
            AcceptanceCliLaunchMode::ExistingHomeRecovery => Self::ExistingHomeRecovery,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RequestPlan {
    schema_version: u32,
    requests: Vec<RequestPlanEntry>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RequestPlanEntry {
    command: String,
    #[serde(default = "empty_params")]
    params: Value,
    timeout_millis: Option<NonZeroU64>,
}

impl AcceptanceCliStartupFailure for RealStartupFailure {
    fn message(&self) -> String {
        self.0.to_string()
    }

    fn retry_cleanup(
        &mut self,
        timeout: Duration,
    ) -> std::result::Result<AcceptanceCliRecoveryOutcome, String> {
        self.0
            .retry_cleanup(timeout)
            .map(|outcome| match outcome {
                AcceptanceStartupCleanupOutcome::Reclaimed { identity } => {
                    AcceptanceCliRecoveryOutcome::Reclaimed {
                        pid: identity.pid(),
                    }
                }
                AcceptanceStartupCleanupOutcome::StillRetained { identity, error } => {
                    AcceptanceCliRecoveryOutcome::StillRetained {
                        pid: identity.pid(),
                        home_dir: identity.home_dir().to_path_buf(),
                        executable_path: identity.executable_path().to_path_buf(),
                        error,
                    }
                }
                AcceptanceStartupCleanupOutcome::AlreadyReclaimed => {
                    AcceptanceCliRecoveryOutcome::AlreadyReclaimed
                }
            })
            .map_err(|error| error.to_string())
    }

    fn has_owner(&self) -> bool {
        self.0.has_owner()
    }

    fn release_owner_fail_safe_nonblocking(&mut self) -> Option<u32> {
        self.0
            .release_owner_fail_safe_nonblocking()
            .map(|identity| identity.pid())
    }
}

impl AcceptanceCli {
    pub fn parse_from_env() -> Self {
        <Self as Parser>::parse()
    }

    pub fn try_parse_from<I, T>(args: I) -> std::result::Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        <Self as Parser>::try_parse_from(args)
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn run_identity(&self) -> &str {
        &self.run_identity
    }

    pub fn request_plan(&self) -> &Path {
        &self.request_plan
    }

    pub fn launch_mode(&self) -> AcceptanceLaunchMode {
        self.launch_mode.into()
    }

    pub fn max_requests(&self) -> usize {
        self.max_requests.get()
    }

    pub fn run(self) -> Result<()> {
        self.run_with_starter(|config| {
            AcceptanceSession::start(config).map_err(|failure| {
                Box::new(RealStartupFailure(failure)) as Box<dyn AcceptanceCliStartupFailure>
            })
        })
    }

    #[doc(hidden)]
    pub fn run_with_starter(
        self,
        starter: impl FnOnce(
            AcceptanceSessionConfig,
        ) -> std::result::Result<
            AcceptanceSession,
            Box<dyn AcceptanceCliStartupFailure>,
        >,
    ) -> Result<()> {
        if self.max_output_bytes.get() > MAX_ACCEPTANCE_OUTPUT_BYTES {
            bail!(
                "max-output-bytes exceeds the Beryl-owned limit of {MAX_ACCEPTANCE_OUTPUT_BYTES}"
            );
        }
        let recovery_cleanup_timeout = millis(self.recovery_cleanup_timeout_ms);
        if recovery_cleanup_timeout > MAX_ACCEPTANCE_CLEANUP_TIMEOUT {
            bail!(
                "recovery-cleanup-timeout-ms exceeds the Beryl-owned limit of {}ms",
                MAX_ACCEPTANCE_CLEANUP_TIMEOUT.as_millis()
            );
        }
        let plan = load_request_plan(&self.request_plan, self.max_requests.get())?;
        let limits = AcceptanceLimits::new(
            millis(self.startup_timeout_ms),
            millis(self.request_timeout_ms),
            millis(self.runtime_timeout_ms),
            self.max_requests.get(),
            self.max_output_bytes.get(),
            millis(self.cleanup_timeout_ms),
        )?;
        let compiled_requests = compile_acceptance_requests(
            plan.requests
                .into_iter()
                .map(|entry| {
                    let request = AcceptanceRequest::new(entry.command, entry.params)?;
                    match entry.timeout_millis {
                        Some(timeout) => request.with_timeout(Duration::from_millis(timeout.get())),
                        None => Ok(request),
                    }
                })
                .collect::<std::result::Result<Vec<_>, _>>()?,
            &limits,
        )?;
        let config = AcceptanceSessionConfig::new(
            &self.executable,
            &self.isolated_home,
            self.launch_mode.into(),
            self.execution_workspace,
            &self.evidence,
            &self.run_identity,
            limits,
            recovery_cleanup_timeout,
        )?;
        let mut session = match starter(config) {
            Ok(session) => session,
            Err(mut failure) => {
                let failure_message = failure.message();
                let recovery = match failure.retry_cleanup(recovery_cleanup_timeout) {
                    Ok(recovery) => recovery,
                    Err(error) => {
                        let identity = failure.release_owner_fail_safe_nonblocking();
                        bail!(
                            "{failure_message}; explicit recovery cleanup was rejected: {error}; issued non-waiting fail-safe closure for retained process {identity:?}"
                        );
                    }
                };
                let recovery_message = startup_recovery_message(&recovery);
                if failure.has_owner() {
                    let identity = failure
                        .release_owner_fail_safe_nonblocking()
                        .expect("retained startup owner has an exact identity");
                    bail!(
                        "{failure_message}; {recovery_message}; issued non-waiting fail-safe closure for retained process {identity}"
                    );
                }
                bail!("{failure_message}; {recovery_message}");
            }
        };
        let mut request_failure = None;
        for request in compiled_requests {
            if let Err(error) = session.execute_compiled_request(request) {
                request_failure = Some(error);
                break;
            }
        }
        let mut outcome = session.finish();
        let mut terminal_failures = Vec::with_capacity(2);
        if let AcceptanceCleanupFinalState::Indeterminate { identity } = outcome.cleanup() {
            terminal_failures.push(format!(
                "terminal cleanup remained indeterminate for exact process {} at home {} using executable {}",
                identity.pid(),
                identity.home_dir().display(),
                identity.executable_path().display()
            ));
        }
        if let AcceptancePublicationState::Failed { error } = outcome.publication() {
            terminal_failures.push(format!("evidence publication failed: {error}"));
        }
        if outcome.retained_identity().is_some() {
            let identity = outcome
                .release_owner_fail_safe_nonblocking()
                .expect("retained terminal owner has an exact identity");
            terminal_failures.push(format!(
                "issued non-waiting fail-safe closure for retained process {}",
                identity.pid()
            ));
        }
        if let Some(error) = request_failure {
            terminal_failures.insert(0, error.to_string());
        }
        if !terminal_failures.is_empty() {
            bail!(terminal_failures.join("; "));
        }
        Ok(())
    }
}

fn startup_recovery_message(outcome: &AcceptanceCliRecoveryOutcome) -> String {
    match outcome {
        AcceptanceCliRecoveryOutcome::Reclaimed { pid } => format!(
            "explicit recovery verified process {} stopped, reaped, and transport threads joined",
            pid
        ),
        AcceptanceCliRecoveryOutcome::StillRetained {
            pid,
            home_dir,
            executable_path,
            error,
        } => format!(
            "explicit recovery remained indeterminate for process {} at home {} using executable {}: {error}",
            pid,
            home_dir.display(),
            executable_path.display()
        ),
        AcceptanceCliRecoveryOutcome::AlreadyReclaimed => {
            "no startup process owner remained for explicit recovery".to_string()
        }
    }
}

fn load_request_plan(path: &Path, max_requests: usize) -> Result<RequestPlan> {
    let path_label = path.display().to_string();
    if !path.is_absolute()
        || path_label.len() > MAX_ACCEPTANCE_REQUEST_PLAN_PATH_BYTES
        || path_label.trim().is_empty()
    {
        bail!(
            "request-plan path must be nonempty, absolute, and at most {} bytes",
            MAX_ACCEPTANCE_REQUEST_PLAN_PATH_BYTES
        );
    }
    if max_requests > MAX_ACCEPTANCE_REQUESTS {
        bail!("max-requests exceeds the Beryl-owned limit of {MAX_ACCEPTANCE_REQUESTS}");
    }
    let metadata = path
        .metadata()
        .with_context(|| format!("failed to inspect request plan {}", path.display()))?;
    if !metadata.is_file() {
        bail!("request plan {} must be a regular file", path.display());
    }
    let mut bytes = Vec::with_capacity(
        usize::try_from(metadata.len())
            .unwrap_or(MAX_ACCEPTANCE_REQUEST_PLAN_BYTES)
            .min(MAX_ACCEPTANCE_REQUEST_PLAN_BYTES),
    );
    File::open(path)
        .with_context(|| format!("failed to open request plan {}", path.display()))?
        .take((MAX_ACCEPTANCE_REQUEST_PLAN_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read request plan {}", path.display()))?;
    if bytes.len() > MAX_ACCEPTANCE_REQUEST_PLAN_BYTES {
        bail!(
            "request plan {} exceeds {} bytes",
            path.display(),
            MAX_ACCEPTANCE_REQUEST_PLAN_BYTES
        );
    }
    let plan: RequestPlan = serde_json::from_slice(&bytes)
        .with_context(|| format!("request plan {} is not valid schema JSON", path.display()))?;
    if plan.schema_version != ACCEPTANCE_REQUEST_PLAN_SCHEMA_VERSION {
        bail!(
            "request plan schema version {} is incompatible with version {}",
            plan.schema_version,
            ACCEPTANCE_REQUEST_PLAN_SCHEMA_VERSION
        );
    }
    if plan.requests.is_empty() {
        bail!("request plan must contain at least one request");
    }
    if plan.requests.len() > max_requests {
        bail!(
            "request plan contains {} requests, exceeding configured limit {max_requests}",
            plan.requests.len()
        );
    }
    for request in &plan.requests {
        if !request.params.is_object() {
            bail!(
                "request params for {:?} must be a JSON object",
                request.command
            );
        }
    }
    Ok(plan)
}

fn millis(value: NonZeroU64) -> Duration {
    Duration::from_millis(value.get())
}

fn empty_params() -> Value {
    json!({})
}
