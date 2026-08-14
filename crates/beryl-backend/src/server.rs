use std::{
    io,
    net::TcpListener,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use beryl_model::workspace::{RuntimeMode, WorkspaceId};

use crate::{
    BackendLaunchSpec, BackendWebSocketEndpoint, ManagedBackendAuthMaterial, ManagedBackendError,
    ManagedBackendProbeReport, ManagedBackendSession, ManagedBackendStartupProgress,
    ManagedBackendStartupStage,
    managed_process::SupervisedBackendProcess,
    session::{ManagedBackendClientOptions, spawn_stderr_logger},
};

const SERVER_PROCESS_CLOSE_GRACE_TIMEOUT: Duration = Duration::ZERO;
const MANAGED_PROCESS_KILL_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct ManagedBackendServer {
    launch_spec: BackendLaunchSpec,
    endpoint: BackendWebSocketEndpoint,
    auth: ManagedBackendAuthMaterial,
    process: SupervisedBackendProcess,
}

#[derive(Clone)]
pub struct ManagedBackendClientConnector {
    launch_spec: BackendLaunchSpec,
    endpoint: BackendWebSocketEndpoint,
    authorization_header_value: String,
}

impl ManagedBackendServer {
    pub fn launch_and_probe(
        runtime_mode: RuntimeMode,
        cwd: impl Into<PathBuf>,
        timeout: Duration,
    ) -> Result<(Self, ManagedBackendSession, ManagedBackendProbeReport), ManagedBackendError> {
        Self::launch_and_probe_with_progress(runtime_mode, cwd, timeout, |_| {})
    }

    pub fn launch_and_probe_for_workspace(
        workspace: WorkspaceId,
        timeout: Duration,
    ) -> Result<(Self, ManagedBackendSession, ManagedBackendProbeReport), ManagedBackendError> {
        Self::launch_and_probe(
            workspace.runtime_mode().clone(),
            workspace.canonical_path().to_path_buf(),
            timeout,
        )
    }

    pub fn launch_and_probe_with_progress<F>(
        runtime_mode: RuntimeMode,
        cwd: impl Into<PathBuf>,
        timeout: Duration,
        mut on_progress: F,
    ) -> Result<(Self, ManagedBackendSession, ManagedBackendProbeReport), ManagedBackendError>
    where
        F: FnMut(ManagedBackendStartupProgress),
    {
        on_progress(ManagedBackendStartupProgress::new(
            ManagedBackendStartupStage::LaunchProcess,
            None,
        ));

        let server = Self::launch(runtime_mode, cwd)?;
        let mut client = server.connect_client_uninitialized_until(timeout)?;
        let report = client.probe_compatibility(timeout, &mut on_progress)?;

        on_progress(ManagedBackendStartupProgress::new(
            ManagedBackendStartupStage::Ready,
            None,
        ));

        Ok((server, client, report))
    }

    pub fn launch(
        runtime_mode: RuntimeMode,
        cwd: impl Into<PathBuf>,
    ) -> Result<Self, ManagedBackendError> {
        let endpoint = BackendWebSocketEndpoint::loopback(select_loopback_port()?);
        let auth = ManagedBackendAuthMaterial::generate(&runtime_mode)?;
        let launch_spec = BackendLaunchSpec::managed_websocket(
            runtime_mode,
            cwd,
            endpoint.clone(),
            auth.backend_token_file_path().to_path_buf(),
        );
        let command_line = launch_spec.command_line()?;
        let mut command = Command::new(command_line.program());
        command.args(command_line.args());
        if let Some(cwd) = command_line.cwd() {
            command.current_dir(cwd);
        }
        command.stdin(Stdio::null());
        command.stdout(Stdio::null());
        command.stderr(Stdio::piped());

        let child = command
            .spawn()
            .map_err(|source| ManagedBackendError::Spawn {
                program: command_line.program().to_string(),
                source,
            })?;
        let mut process = SupervisedBackendProcess::new(launch_spec.clone(), child)?;
        let stderr = process
            .take_stderr()
            .ok_or(ManagedBackendError::MissingPipe {
                stream_name: "stderr",
            })?;
        spawn_stderr_logger(stderr, launch_spec.clone());

        Ok(Self {
            launch_spec,
            endpoint,
            auth,
            process,
        })
    }

    pub fn launch_spec(&self) -> &BackendLaunchSpec {
        &self.launch_spec
    }

    pub fn endpoint(&self) -> &BackendWebSocketEndpoint {
        &self.endpoint
    }

    pub fn process_id(&self) -> Option<u32> {
        self.process.process_id()
    }

    pub fn is_process_alive(&mut self) -> bool {
        !self.child_exited()
    }

    pub fn client_connector(&self) -> ManagedBackendClientConnector {
        ManagedBackendClientConnector {
            launch_spec: self.launch_spec.clone(),
            endpoint: self.endpoint.clone(),
            authorization_header_value: self.auth.authorization_header_value(),
        }
    }

    pub fn connect_client(
        &self,
        timeout: Duration,
    ) -> Result<ManagedBackendSession, ManagedBackendError> {
        self.client_connector().connect_client(timeout)
    }

    pub fn connect_client_with_options(
        &self,
        options: ManagedBackendClientOptions,
        timeout: Duration,
    ) -> Result<ManagedBackendSession, ManagedBackendError> {
        self.client_connector()
            .connect_client_with_options(options, timeout)
    }

    pub fn connect_request_client(
        &self,
        timeout: Duration,
    ) -> Result<ManagedBackendSession, ManagedBackendError> {
        self.client_connector().connect_request_client(timeout)
    }

    pub fn shutdown(&mut self) -> Result<(), ManagedBackendError> {
        let process_result = self.process.shutdown(
            SERVER_PROCESS_CLOSE_GRACE_TIMEOUT,
            MANAGED_PROCESS_KILL_TIMEOUT,
        );
        let auth_result = self.auth.cleanup();

        match (process_result, auth_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) => Err(error),
            (Ok(()), Err(error)) => Err(error),
            (Err(process_error), Err(auth_error)) => {
                tracing::warn!(
                    %auth_error,
                    "failed to clean up managed backend auth material after process shutdown failure"
                );
                Err(process_error)
            }
        }
    }

    fn connect_client_uninitialized_until(
        &self,
        timeout: Duration,
    ) -> Result<ManagedBackendSession, ManagedBackendError> {
        self.client_connector()
            .connect_client_uninitialized_until(timeout)
    }

    fn child_exited(&mut self) -> bool {
        self.process.has_exited()
    }
}

impl ManagedBackendClientConnector {
    pub fn endpoint(&self) -> &BackendWebSocketEndpoint {
        &self.endpoint
    }

    pub fn launch_spec(&self) -> &BackendLaunchSpec {
        &self.launch_spec
    }

    pub fn connect_client(
        &self,
        timeout: Duration,
    ) -> Result<ManagedBackendSession, ManagedBackendError> {
        self.connect_client_with_options(ManagedBackendClientOptions::foreground(), timeout)
    }

    pub fn connect_client_with_options(
        &self,
        options: ManagedBackendClientOptions,
        timeout: Duration,
    ) -> Result<ManagedBackendSession, ManagedBackendError> {
        let mut session = self.connect_client_uninitialized_until(timeout)?;
        session.initialize_client_with_options(&options, timeout)?;
        Ok(session)
    }

    pub fn connect_request_client(
        &self,
        timeout: Duration,
    ) -> Result<ManagedBackendSession, ManagedBackendError> {
        self.connect_client_with_options(ManagedBackendClientOptions::request_only(), timeout)
    }

    fn connect_client_uninitialized_until(
        &self,
        timeout: Duration,
    ) -> Result<ManagedBackendSession, ManagedBackendError> {
        let deadline = Instant::now() + timeout;

        loop {
            match ManagedBackendSession::connect_websocket_uninitialized(
                self.launch_spec.clone(),
                self.endpoint.clone(),
                self.authorization_header_value.clone(),
            ) {
                Ok(session) => return Ok(session),
                Err(error) if retry_websocket_connect(&error) => {
                    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                        return Err(error);
                    };
                    thread::sleep(remaining.min(Duration::from_millis(50)));
                }
                Err(error) => return Err(error),
            }
        }
    }
}

impl std::fmt::Debug for ManagedBackendClientConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedBackendClientConnector")
            .field("launch_spec", &self.launch_spec)
            .field("endpoint", &self.endpoint)
            .field("authorization_header_value", &"<redacted>")
            .finish()
    }
}

impl Drop for ManagedBackendServer {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            tracing::warn!(%error, "failed to shut down managed backend server");
        }
    }
}

fn select_loopback_port() -> Result<u16, ManagedBackendError> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|source| ManagedBackendError::SelectWebSocketPort { source })?;
    let port = listener
        .local_addr()
        .map_err(|source| ManagedBackendError::SelectWebSocketPort { source })?
        .port();
    Ok(port)
}

fn retry_websocket_connect(error: &ManagedBackendError) -> bool {
    let ManagedBackendError::ConnectWebSocket { source, .. } = error else {
        return false;
    };
    matches!(
        source.io_error_kind(),
        Some(
            io::ErrorKind::ConnectionRefused
                | io::ErrorKind::NotConnected
                | io::ErrorKind::TimedOut
                | io::ErrorKind::WouldBlock
        )
    )
}
