use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use beryl_model::workspace::{RuntimeMode, WorkspaceId};
use thiserror::Error;

const MANAGED_STDIO_LISTEN_URL: &str = "stdio://";
const LOOPBACK_WS_HOST: &str = "127.0.0.1";
const WEBSOCKET_AUTH_MODE: &str = "capability-token";
const WSL_RUNTIME_DIR: &str = "/tmp/beryl-codex-app-server";
pub(crate) const WSL_PROCESS_GROUP_NOT_READY_EXIT_CODE: i32 = 2;
const FIELD_CODEX_ARG: &str = "codex app-server argument";
const FIELD_WSL_INNER_COMMAND: &str = "WSL process-group inner shell command";
const FIELD_WSL_PID_FILE_PATH: &str = "WSL process-group PID file path";
const FIELD_WSL_RUNTIME_DIR: &str = "WSL runtime directory";

static NEXT_WSL_CLEANUP_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Error)]
#[error("failed to quote {field} for POSIX shell command")]
pub struct BackendCommandLineError {
    field: &'static str,
    #[source]
    source: shlex::QuoteError,
}

impl BackendCommandLineError {
    pub fn field(&self) -> &'static str {
        self.field
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendTransport {
    ManagedStdio,
    ManagedWebSocket(BackendWebSocketConfig),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendWebSocketConfig {
    endpoint: BackendWebSocketEndpoint,
    backend_token_file_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendWebSocketEndpoint {
    host: String,
    port: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendLaunchSpec {
    runtime_mode: RuntimeMode,
    cwd: PathBuf,
    transport: BackendTransport,
    runtime_cleanup: Option<BackendRuntimeCleanup>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BackendRuntimeCleanup {
    WslLinuxProcessGroup(WslProcessGroupCleanup),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WslProcessGroupCleanup {
    distro_name: String,
    pid_file_path: String,
}

impl BackendLaunchSpec {
    pub fn managed_stdio(runtime_mode: RuntimeMode, cwd: impl Into<PathBuf>) -> Self {
        Self::new(runtime_mode, cwd.into(), BackendTransport::ManagedStdio)
    }

    fn new(runtime_mode: RuntimeMode, cwd: PathBuf, transport: BackendTransport) -> Self {
        let runtime_cleanup = BackendRuntimeCleanup::for_runtime_mode(&runtime_mode);
        Self {
            runtime_mode,
            cwd,
            transport,
            runtime_cleanup,
        }
    }

    pub fn managed_stdio_for_workspace(workspace: WorkspaceId) -> Self {
        Self::managed_stdio(
            workspace.runtime_mode().clone(),
            workspace.canonical_path().to_path_buf(),
        )
    }

    pub fn managed_websocket(
        runtime_mode: RuntimeMode,
        cwd: impl Into<PathBuf>,
        endpoint: BackendWebSocketEndpoint,
        backend_token_file_path: impl Into<PathBuf>,
    ) -> Self {
        Self::new(
            runtime_mode,
            cwd.into(),
            BackendTransport::ManagedWebSocket(BackendWebSocketConfig::new(
                endpoint,
                backend_token_file_path,
            )),
        )
    }

    pub fn runtime_mode(&self) -> &RuntimeMode {
        &self.runtime_mode
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn transport(&self) -> BackendTransport {
        self.transport.clone()
    }

    pub(crate) fn wsl_process_group_cleanup(&self) -> Option<&WslProcessGroupCleanup> {
        match self.runtime_cleanup.as_ref()? {
            BackendRuntimeCleanup::WslLinuxProcessGroup(cleanup) => Some(cleanup),
        }
    }

    pub fn display_label(&self) -> String {
        format!(
            "{} {}",
            self.runtime_mode.display_name(),
            self.cwd.display()
        )
    }

    pub(crate) fn launch_program_label(&self) -> &'static str {
        match &self.runtime_mode {
            RuntimeMode::HostWindows => "codex",
            RuntimeMode::WslLinux { .. } => "wsl.exe",
        }
    }

    pub fn command_line(&self) -> Result<BackendCommandLine, BackendCommandLineError> {
        match &self.transport {
            BackendTransport::ManagedStdio => match &self.runtime_mode {
                RuntimeMode::HostWindows => Ok(BackendCommandLine::new(
                    "codex",
                    managed_stdio_codex_args(),
                    Some(self.cwd.clone()),
                )),
                RuntimeMode::WslLinux { distro_name } => {
                    let cleanup = self
                        .wsl_process_group_cleanup()
                        .expect("WSL launch specs must include WSL cleanup state");
                    // Debian-style user installs often add codex to PATH from ~/.profile.
                    let args = vec![
                        "--distribution".to_string(),
                        distro_name.clone(),
                        "--cd".to_string(),
                        self.cwd.display().to_string(),
                        "--exec".to_string(),
                        "/bin/bash".to_string(),
                        "-lc".to_string(),
                        managed_stdio_wsl_shell_command(cleanup)?,
                    ];

                    Ok(BackendCommandLine::new("wsl.exe", args, None))
                }
            },
            BackendTransport::ManagedWebSocket(config) => match &self.runtime_mode {
                RuntimeMode::HostWindows => Ok(BackendCommandLine::new(
                    "codex",
                    managed_websocket_codex_args(config),
                    Some(self.cwd.clone()),
                )),
                RuntimeMode::WslLinux { distro_name } => {
                    let cleanup = self
                        .wsl_process_group_cleanup()
                        .expect("WSL launch specs must include WSL cleanup state");
                    let args = vec![
                        "--distribution".to_string(),
                        distro_name.clone(),
                        "--cd".to_string(),
                        self.cwd.display().to_string(),
                        "--exec".to_string(),
                        "/bin/bash".to_string(),
                        "-lc".to_string(),
                        managed_websocket_wsl_shell_command(config, cleanup)?,
                    ];

                    Ok(BackendCommandLine::new("wsl.exe", args, None))
                }
            },
        }
    }
}

impl BackendRuntimeCleanup {
    fn for_runtime_mode(runtime_mode: &RuntimeMode) -> Option<Self> {
        match runtime_mode {
            RuntimeMode::HostWindows => None,
            RuntimeMode::WslLinux { distro_name } => Some(Self::WslLinuxProcessGroup(
                WslProcessGroupCleanup::new(distro_name.clone()),
            )),
        }
    }
}

impl WslProcessGroupCleanup {
    fn new(distro_name: String) -> Self {
        Self {
            distro_name,
            pid_file_path: next_wsl_pid_file_path(),
        }
    }

    pub(crate) fn distro_name(&self) -> &str {
        &self.distro_name
    }

    pub(crate) fn shutdown_command_line(
        &self,
    ) -> Result<BackendCommandLine, BackendCommandLineError> {
        Ok(BackendCommandLine::new(
            "wsl.exe",
            vec![
                "--distribution".to_string(),
                self.distro_name.clone(),
                "--exec".to_string(),
                "/bin/bash".to_string(),
                "-lc".to_string(),
                wsl_process_group_shutdown_shell_command(&self.pid_file_path)?,
            ],
            None,
        ))
    }
}

impl BackendWebSocketConfig {
    pub fn new(
        endpoint: BackendWebSocketEndpoint,
        backend_token_file_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            endpoint,
            backend_token_file_path: backend_token_file_path.into(),
        }
    }

    pub fn endpoint(&self) -> &BackendWebSocketEndpoint {
        &self.endpoint
    }

    pub fn backend_token_file_path(&self) -> &Path {
        &self.backend_token_file_path
    }
}

impl BackendWebSocketEndpoint {
    pub fn loopback(port: u16) -> Self {
        Self {
            host: LOOPBACK_WS_HOST.to_string(),
            port,
        }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn listen_url(&self) -> String {
        format!("ws://{}:{}", self.host, self.port)
    }

    pub fn is_loopback(&self) -> bool {
        self.host == LOOPBACK_WS_HOST
    }
}

fn managed_stdio_codex_args() -> Vec<String> {
    vec![
        "app-server".to_string(),
        "--listen".to_string(),
        MANAGED_STDIO_LISTEN_URL.to_string(),
    ]
}

fn managed_stdio_wsl_shell_command(
    cleanup: &WslProcessGroupCleanup,
) -> Result<String, BackendCommandLineError> {
    managed_wsl_shell_command(managed_stdio_codex_args(), cleanup)
}

fn managed_websocket_codex_args(config: &BackendWebSocketConfig) -> Vec<String> {
    vec![
        "app-server".to_string(),
        "--listen".to_string(),
        config.endpoint.listen_url(),
        "--ws-auth".to_string(),
        WEBSOCKET_AUTH_MODE.to_string(),
        "--ws-token-file".to_string(),
        config.backend_token_file_path.display().to_string(),
    ]
}

fn managed_websocket_wsl_shell_command(
    config: &BackendWebSocketConfig,
    cleanup: &WslProcessGroupCleanup,
) -> Result<String, BackendCommandLineError> {
    managed_wsl_shell_command(managed_websocket_codex_args(config), cleanup)
}

fn managed_wsl_shell_command(
    codex_args: Vec<String>,
    cleanup: &WslProcessGroupCleanup,
) -> Result<String, BackendCommandLineError> {
    let codex_command = codex_shell_command(&codex_args)?;
    let pid_file_path = quote_posix_shell_field(FIELD_WSL_PID_FILE_PATH, &cleanup.pid_file_path)?;
    let inner_command = format!(
        "pid_file={}; printf '%s\\n' \"$$\" > \"$pid_file\" || exit 1; trap 'rm -f \"$pid_file\"' EXIT; {codex_command}; status=$?; rm -f \"$pid_file\"; exit \"$status\"",
        pid_file_path
    );
    let runtime_dir = quote_posix_shell_field(FIELD_WSL_RUNTIME_DIR, WSL_RUNTIME_DIR)?;
    let inner_command = quote_posix_shell_field(FIELD_WSL_INNER_COMMAND, &inner_command)?;

    Ok(format!(
        "mkdir -p {runtime_dir} && {{ setsid /bin/bash -lc {inner_command} & child=$!; wait \"$child\"; status=$?; exit \"$status\"; }}"
    ))
}

fn wsl_process_group_shutdown_shell_command(
    pid_file_path: &str,
) -> Result<String, BackendCommandLineError> {
    let pid_file_path = quote_posix_shell_field(FIELD_WSL_PID_FILE_PATH, pid_file_path)?;
    Ok(format!(
        "pid_file={}; for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40; do if [ -s \"$pid_file\" ]; then break; fi; sleep 0.05; done; pid=$(cat \"$pid_file\" 2>/dev/null) || exit {WSL_PROCESS_GROUP_NOT_READY_EXIT_CODE}; case \"$pid\" in ''|0|*[!0-9]*) rm -f \"$pid_file\"; exit {WSL_PROCESS_GROUP_NOT_READY_EXIT_CODE};; esac; kill -TERM -- -\"$pid\" 2>/dev/null || true; for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do if ! kill -0 -- -\"$pid\" 2>/dev/null; then rm -f \"$pid_file\"; exit 0; fi; sleep 0.05; done; kill -KILL -- -\"$pid\" 2>/dev/null || true; for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do if ! kill -0 -- -\"$pid\" 2>/dev/null; then rm -f \"$pid_file\"; exit 0; fi; sleep 0.05; done; rm -f \"$pid_file\"; exit 1",
        pid_file_path,
    ))
}

fn codex_shell_command(args: &[String]) -> Result<String, BackendCommandLineError> {
    let mut command = "codex".to_string();
    for arg in args {
        command.push(' ');
        command.push_str(&quote_posix_shell_field(FIELD_CODEX_ARG, arg)?);
    }
    Ok(command)
}

fn quote_posix_shell_field(
    field: &'static str,
    value: &str,
) -> Result<String, BackendCommandLineError> {
    shlex::try_quote(value)
        .map(std::borrow::Cow::into_owned)
        .map_err(|source| BackendCommandLineError { field, source })
}

fn next_wsl_pid_file_path() -> String {
    let process_id = std::process::id();
    let sequence = NEXT_WSL_CLEANUP_ID.fetch_add(1, Ordering::Relaxed);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();

    format!("{WSL_RUNTIME_DIR}/process-{process_id}-{millis}-{sequence}.pid")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendCommandLine {
    program: String,
    args: Vec<String>,
    cwd: Option<PathBuf>,
}

impl BackendCommandLine {
    pub fn new(program: impl Into<String>, args: Vec<String>, cwd: Option<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args,
            cwd,
        }
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn cwd(&self) -> Option<&PathBuf> {
        self.cwd.as_ref()
    }
}
