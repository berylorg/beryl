use crate::WslSupervisorArtifact;
use std::{path::PathBuf, sync::Arc};

use beryl_model::{AdmittedHostPath, RuntimeId, RuntimeMode, RuntimeNativePath};
use thiserror::Error;

const LOOPBACK_WS_HOST: &str = "127.0.0.1";
const WEBSOCKET_AUTH_MODE: &str = "capability-token";
pub(crate) const MULTI_AGENT_V2_OVERRIDE: &str =
    "features.multi_agent_v2={enabled=true,expose_spawn_agent_model_overrides=true}";

#[derive(Debug, Error)]
#[error("backend command is unavailable: {field}")]
pub struct BackendCommandLineError {
    field: &'static str,
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
    wsl_supervisor_artifact: Option<Arc<WslSupervisorArtifact>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendWebSocketEndpoint {
    host: String,
    port: u16,
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
        Ok(Self {
            runtime_id,
            canonical_executable,
            runtime_mode,
            runtime_native_executable,
            working_directory,
            host_token_directory,
            runtime_token_directory,
            wsl_supervisor_artifact: None,
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
                let artifact =
                    self.wsl_supervisor_artifact
                        .as_ref()
                        .ok_or(BackendCommandLineError {
                            field: "WSL supervisor artifact",
                        })?;
                Ok(BackendCommandLine::new(
                    "wsl.exe",
                    vec![
                        "--distribution".into(),
                        distribution.as_str().into(),
                        "--cd".into(),
                        self.working_directory.as_str().into(),
                        "--exec".into(),
                        artifact.linux_path().into(),
                        "context-broker".into(),
                    ],
                    None,
                ))
            }
        }
    }

    pub fn with_wsl_supervisor_artifact(mut self, artifact: Arc<WslSupervisorArtifact>) -> Self {
        self.wsl_supervisor_artifact = Some(artifact);
        self
    }

    pub fn wsl_supervisor_artifact(&self) -> Option<Arc<WslSupervisorArtifact>> {
        self.wsl_supervisor_artifact.clone()
    }

    pub(crate) fn server_arguments(
        &self,
        endpoint: &BackendWebSocketEndpoint,
        token_path: &str,
        digest: &str,
    ) -> Vec<String> {
        managed_websocket_codex_args(endpoint, token_path, digest)
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
