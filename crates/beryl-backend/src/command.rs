use std::path::PathBuf;

use beryl_model::{AdmittedHostPath, RuntimeId, RuntimeMode, RuntimeNativePath};
use thiserror::Error;

const LOOPBACK_WS_HOST: &str = "127.0.0.1";
const WEBSOCKET_AUTH_MODE: &str = "capability-token";
const WSL_RUNTIME_DIR_PREFIX: &str = "/tmp/beryl-codex-app-server";
pub(crate) const WSL_PROCESS_GROUP_NOT_READY_EXIT_CODE: i32 = 2;
const FIELD_CODEX_ARG: &str = "codex app-server argument";
const FIELD_WSL_INNER_COMMAND: &str = "WSL process-group inner shell command";
const FIELD_WSL_PID_FILE_PATH: &str = "WSL process-group PID file path";
const FIELD_WSL_RUNTIME_DIR: &str = "WSL runtime directory";

pub(crate) const MULTI_AGENT_V2_OVERRIDE: &str =
    "features.multi_agent_v2={enabled=true,expose_spawn_agent_model_overrides=true}";

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

#[derive(Debug, Error)]
pub enum ManagedBackendLaunchSpecError {
    #[error("the configured executable belongs to a different runtime mode")]
    ExecutableModeMismatch,
    #[error("the Host executable identities disagree")]
    HostExecutableIdentityMismatch,
    #[error("the execution root belongs to a different runtime mode")]
    WorkingDirectoryModeMismatch,
    #[error("the runtime token directory belongs to a different runtime mode")]
    TokenDirectoryModeMismatch,
    #[error("failed to generate the private WSL process boundary")]
    GenerateWslBoundary {
        #[source]
        source: getrandom::Error,
    },
}

/// Validated exact-path inputs for one Beryl-owned CAS process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedBackendLaunchSpec {
    runtime_id: RuntimeId,
    canonical_executable: AdmittedHostPath,
    runtime_mode: RuntimeMode,
    runtime_native_executable: RuntimeNativePath,
    working_directory: RuntimeNativePath,
    host_token_directory: AdmittedHostPath,
    runtime_token_directory: RuntimeNativePath,
    wsl_process_group_cleanup: Option<WslProcessGroupCleanup>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendWebSocketEndpoint {
    host: String,
    port: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WslProcessGroupCleanup {
    distro_name: String,
    runtime_directory: String,
    pid_file_path: String,
}

impl WslProcessGroupCleanup {
    pub(crate) fn new(distro_name: String) -> Result<Self, ManagedBackendLaunchSpecError> {
        let runtime_directory = next_wsl_runtime_directory()?;
        Ok(Self {
            distro_name,
            pid_file_path: format!("{runtime_directory}/process.pid"),
            runtime_directory,
        })
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
                wsl_process_group_shutdown_shell_command(
                    &self.pid_file_path,
                    &self.runtime_directory,
                )?,
            ],
            None,
        ))
    }
}

impl ManagedBackendLaunchSpec {
    pub fn new(
        runtime_id: RuntimeId,
        canonical_executable: AdmittedHostPath,
        runtime_mode: RuntimeMode,
        runtime_native_executable: RuntimeNativePath,
        working_directory: RuntimeNativePath,
        host_token_directory: AdmittedHostPath,
        runtime_token_directory: RuntimeNativePath,
    ) -> Result<Self, ManagedBackendLaunchSpecError> {
        if runtime_native_executable.mode() != &runtime_mode {
            return Err(ManagedBackendLaunchSpecError::ExecutableModeMismatch);
        }
        if matches!(runtime_mode, RuntimeMode::Host)
            && canonical_executable.as_str() != runtime_native_executable.as_str()
        {
            return Err(ManagedBackendLaunchSpecError::HostExecutableIdentityMismatch);
        }
        if working_directory.mode() != &runtime_mode {
            return Err(ManagedBackendLaunchSpecError::WorkingDirectoryModeMismatch);
        }
        if runtime_token_directory.mode() != &runtime_mode {
            return Err(ManagedBackendLaunchSpecError::TokenDirectoryModeMismatch);
        }
        let wsl_process_group_cleanup = match &runtime_mode {
            RuntimeMode::Host => None,
            RuntimeMode::Wsl(distribution) => Some(WslProcessGroupCleanup::new(
                distribution.as_str().to_string(),
            )?),
        };

        Ok(Self {
            runtime_id,
            canonical_executable,
            runtime_mode,
            runtime_native_executable,
            working_directory,
            host_token_directory,
            runtime_token_directory,
            wsl_process_group_cleanup,
        })
    }

    pub const fn runtime_id(&self) -> RuntimeId {
        self.runtime_id
    }

    pub fn canonical_executable(&self) -> &AdmittedHostPath {
        &self.canonical_executable
    }

    pub fn runtime_mode(&self) -> &RuntimeMode {
        &self.runtime_mode
    }

    pub fn runtime_native_executable(&self) -> &RuntimeNativePath {
        &self.runtime_native_executable
    }

    pub fn working_directory(&self) -> &RuntimeNativePath {
        &self.working_directory
    }

    pub fn host_token_directory(&self) -> &AdmittedHostPath {
        &self.host_token_directory
    }

    pub fn runtime_token_directory(&self) -> &RuntimeNativePath {
        &self.runtime_token_directory
    }

    pub fn display_label(&self) -> String {
        format!(
            "{} in {}",
            self.runtime_native_executable.as_str(),
            self.working_directory.as_str()
        )
    }

    pub fn command_line(
        &self,
        endpoint: &BackendWebSocketEndpoint,
        runtime_token_file_path: &str,
        token_sha256: &str,
    ) -> Result<BackendCommandLine, BackendCommandLineError> {
        let codex_args =
            managed_websocket_codex_args(endpoint, runtime_token_file_path, token_sha256);
        match &self.runtime_mode {
            RuntimeMode::Host => Ok(BackendCommandLine::new(
                self.canonical_executable.as_str(),
                codex_args,
                Some(PathBuf::from(self.working_directory.as_str())),
            )),
            RuntimeMode::Wsl(distribution) => {
                let cleanup = self
                    .wsl_process_group_cleanup
                    .as_ref()
                    .expect("WSL launch specs own process-group cleanup");
                Ok(BackendCommandLine::new(
                    "wsl.exe",
                    vec![
                        "--distribution".to_string(),
                        distribution.as_str().to_string(),
                        "--cd".to_string(),
                        self.working_directory.as_str().to_string(),
                        "--exec".to_string(),
                        "/bin/bash".to_string(),
                        "-lc".to_string(),
                        managed_wsl_shell_command(
                            self.runtime_native_executable.as_str(),
                            &codex_args,
                            cleanup,
                        )?,
                    ],
                    None,
                ))
            }
        }
    }

    pub(crate) fn wsl_process_group_cleanup(&self) -> Option<WslProcessGroupCleanup> {
        self.wsl_process_group_cleanup.clone()
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

fn managed_websocket_codex_args(
    endpoint: &BackendWebSocketEndpoint,
    runtime_token_file_path: &str,
    token_sha256: &str,
) -> Vec<String> {
    vec![
        "app-server".to_string(),
        "--strict-config".to_string(),
        "-c".to_string(),
        MULTI_AGENT_V2_OVERRIDE.to_string(),
        "--listen".to_string(),
        endpoint.listen_url(),
        "--ws-auth".to_string(),
        WEBSOCKET_AUTH_MODE.to_string(),
        "--ws-token-file".to_string(),
        runtime_token_file_path.to_string(),
        "--ws-token-sha256".to_string(),
        token_sha256.to_string(),
    ]
}

fn managed_wsl_shell_command(
    runtime_native_executable: &str,
    codex_args: &[String],
    cleanup: &WslProcessGroupCleanup,
) -> Result<String, BackendCommandLineError> {
    let codex_command = codex_shell_command(runtime_native_executable, codex_args)?;
    let pid_file_path = quote_posix_shell_field(FIELD_WSL_PID_FILE_PATH, &cleanup.pid_file_path)?;
    let inner_command = format!(
        "pid_file={}; printf '%s\\n' \"$$\" > \"$pid_file\" || exit 1; trap 'rm -f \"$pid_file\"' EXIT; {codex_command}; status=$?; rm -f \"$pid_file\"; exit \"$status\"",
        pid_file_path
    );
    let runtime_dir = quote_posix_shell_field(FIELD_WSL_RUNTIME_DIR, &cleanup.runtime_directory)?;
    let inner_command = quote_posix_shell_field(FIELD_WSL_INNER_COMMAND, &inner_command)?;

    Ok(format!(
        "umask 077; mkdir -m 700 {runtime_dir} || exit 1; {{ setsid /bin/bash -lc {inner_command} & child=$!; wait \"$child\"; status=$?; rmdir {runtime_dir} 2>/dev/null || true; exit \"$status\"; }}"
    ))
}

fn codex_shell_command(
    runtime_native_executable: &str,
    args: &[String],
) -> Result<String, BackendCommandLineError> {
    let mut command = quote_posix_shell_field(FIELD_CODEX_ARG, runtime_native_executable)?;
    for arg in args {
        command.push(' ');
        command.push_str(&quote_posix_shell_field(FIELD_CODEX_ARG, arg)?);
    }
    Ok(command)
}

fn wsl_process_group_shutdown_shell_command(
    pid_file_path: &str,
    runtime_directory: &str,
) -> Result<String, BackendCommandLineError> {
    let pid_file_path = quote_posix_shell_field(FIELD_WSL_PID_FILE_PATH, pid_file_path)?;
    let runtime_directory = quote_posix_shell_field(FIELD_WSL_RUNTIME_DIR, runtime_directory)?;
    Ok(format!(
        "pid_file={}; runtime_dir={}; trap 'rm -f \"$pid_file\"; rmdir \"$runtime_dir\" 2>/dev/null || true' EXIT; for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40; do if [ -s \"$pid_file\" ]; then break; fi; sleep 0.05; done; pid=$(cat \"$pid_file\" 2>/dev/null) || exit {WSL_PROCESS_GROUP_NOT_READY_EXIT_CODE}; case \"$pid\" in ''|0|*[!0-9]*) exit {WSL_PROCESS_GROUP_NOT_READY_EXIT_CODE};; esac; kill -TERM -- -\"$pid\" 2>/dev/null || true; for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do if ! kill -0 -- -\"$pid\" 2>/dev/null; then exit 0; fi; sleep 0.05; done; kill -KILL -- -\"$pid\" 2>/dev/null || true; for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do if ! kill -0 -- -\"$pid\" 2>/dev/null; then exit 0; fi; sleep 0.05; done; exit 1",
        pid_file_path, runtime_directory,
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

fn next_wsl_runtime_directory() -> Result<String, ManagedBackendLaunchSpecError> {
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce)
        .map_err(|source| ManagedBackendLaunchSpecError::GenerateWslBoundary { source })?;
    Ok(format!("{WSL_RUNTIME_DIR_PREFIX}-{}", hex::encode(nonce)))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendCommandLine {
    program: String,
    args: Vec<String>,
    cwd: Option<PathBuf>,
}

impl BackendCommandLine {
    pub(crate) fn new(program: impl Into<String>, args: Vec<String>, cwd: Option<PathBuf>) -> Self {
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
