use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use thiserror::Error;

const LOOPBACK_WS_HOST: &str = "127.0.0.1";
const WSL_RUNTIME_DIR: &str = "/tmp/beryl-codex-app-server";
pub(crate) const WSL_PROCESS_GROUP_NOT_READY_EXIT_CODE: i32 = 2;
const FIELD_WSL_PID_FILE_PATH: &str = "WSL process-group PID file path";

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
pub struct BackendWebSocketEndpoint {
    host: String,
    port: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WslProcessGroupCleanup {
    distro_name: String,
    pid_file_path: String,
}

impl WslProcessGroupCleanup {
    pub(crate) fn new(distro_name: String) -> Self {
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

fn wsl_process_group_shutdown_shell_command(
    pid_file_path: &str,
) -> Result<String, BackendCommandLineError> {
    let pid_file_path = quote_posix_shell_field(FIELD_WSL_PID_FILE_PATH, pid_file_path)?;
    Ok(format!(
        "pid_file={}; for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40; do if [ -s \"$pid_file\" ]; then break; fi; sleep 0.05; done; pid=$(cat \"$pid_file\" 2>/dev/null) || exit {WSL_PROCESS_GROUP_NOT_READY_EXIT_CODE}; case \"$pid\" in ''|0|*[!0-9]*) rm -f \"$pid_file\"; exit {WSL_PROCESS_GROUP_NOT_READY_EXIT_CODE};; esac; kill -TERM -- -\"$pid\" 2>/dev/null || true; for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do if ! kill -0 -- -\"$pid\" 2>/dev/null; then rm -f \"$pid_file\"; exit 0; fi; sleep 0.05; done; kill -KILL -- -\"$pid\" 2>/dev/null || true; for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do if ! kill -0 -- -\"$pid\" 2>/dev/null; then rm -f \"$pid_file\"; exit 0; fi; sleep 0.05; done; rm -f \"$pid_file\"; exit 1",
        pid_file_path,
    ))
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
