use std::{
    io::{self, Read},
    net::TcpListener,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use beryl_model::{
    AdmittedHostPath, CasProcessGeneration, RuntimeId, RuntimeLaunchForm, RuntimeMode,
    RuntimeNativePath,
};

use crate::{
    BackendWebSocketEndpoint, ForegroundSessionConfig, ManagedBackendError,
    ManagedBackendLaunchCleanup, ManagedBackendLaunchFailure, ManagedBackendLaunchSpec,
    ManagedBackendSession,
    auth::ManagedBackendAuthMaterial,
    launch_cleanup::ManagedProcessOwner,
    managed_process::SupervisedBackendProcess,
    websocket_transport::{
        ForegroundWebSocketCandidate, ForegroundWebSocketTransport, RequestOnlyWebSocketCandidate,
        RequestOnlyWebSocketTransport,
    },
    wsl_supervision::WslSupervision,
};

static NEXT_PROCESS_GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ManagedLaunchProvenance {
    Production(ManagedBackendLaunchIdentity),
    #[cfg(feature = "lifecycle-test-support")]
    LifecycleTest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedBackendLaunchIdentity(Arc<ManagedBackendLaunchIdentityInner>);

#[derive(Debug, Eq, PartialEq)]
struct ManagedBackendLaunchIdentityInner {
    runtime_id: RuntimeId,
    process_generation: CasProcessGeneration,
    runtime_mode: RuntimeMode,
    launch_form: RuntimeLaunchForm,
    canonical_executable: AdmittedHostPath,
    runtime_native_executable: RuntimeNativePath,
    working_directory: RuntimeNativePath,
}

impl ManagedBackendLaunchIdentity {
    fn new(
        launch_spec: &ManagedBackendLaunchSpec,
        process_generation: CasProcessGeneration,
    ) -> Self {
        Self(Arc::new(ManagedBackendLaunchIdentityInner {
            runtime_id: launch_spec.runtime_id(),
            process_generation,
            runtime_mode: launch_spec.runtime_mode().clone(),
            launch_form: launch_spec.launch_form(),
            canonical_executable: launch_spec.canonical_executable().clone(),
            runtime_native_executable: launch_spec.runtime_native_executable().clone(),
            working_directory: launch_spec.working_directory().clone(),
        }))
    }

    pub fn runtime_id(&self) -> RuntimeId {
        self.0.runtime_id
    }

    pub fn process_generation(&self) -> CasProcessGeneration {
        self.0.process_generation
    }

    pub fn runtime_mode(&self) -> &RuntimeMode {
        &self.0.runtime_mode
    }

    pub fn launch_form(&self) -> RuntimeLaunchForm {
        self.0.launch_form
    }

    pub fn canonical_executable(&self) -> &AdmittedHostPath {
        &self.0.canonical_executable
    }

    pub fn runtime_native_executable(&self) -> &RuntimeNativePath {
        &self.0.runtime_native_executable
    }

    pub fn working_directory(&self) -> &RuntimeNativePath {
        &self.0.working_directory
    }
}

/// Sole owner of one Beryl-launched, authenticated CAS process boundary.
#[derive(Debug)]
pub struct ManagedBackendServer {
    launch_spec: ManagedBackendLaunchSpec,
    endpoint: BackendWebSocketEndpoint,
    cleanup: ManagedBackendLaunchCleanup,
    provenance: ManagedLaunchProvenance,
}

#[derive(Clone)]
/// A cloneable capability for opening authenticated clients to one live CAS runtime.
///
/// Runtime launch and ownership remain outside this transport value. The target
/// runtime supervisor creates connectors only after admitting an exact configured
/// executable and its authentication boundary.
pub struct ManagedBackendClientConnector {
    endpoint: BackendWebSocketEndpoint,
    authorization_header_value: String,
    provenance: ManagedLaunchProvenance,
}

impl ManagedBackendServer {
    pub fn launch(
        launch_spec: ManagedBackendLaunchSpec,
    ) -> Result<Self, ManagedBackendLaunchFailure> {
        if matches!(launch_spec.runtime_mode(), RuntimeMode::Wsl(_))
            && launch_spec.wsl_supervisor_artifact().is_none()
        {
            return Err(ManagedBackendError::WslArtifactUnavailable.into());
        }
        let endpoint = BackendWebSocketEndpoint::loopback(select_loopback_port()?);
        let auth = ManagedBackendAuthMaterial::generate(
            launch_spec.host_token_directory(),
            launch_spec.runtime_token_directory(),
        )?;
        let mut cleanup = ManagedBackendLaunchCleanup::new(None, Some(auth));
        let launched = (|| -> Result<_, ManagedBackendError> {
            let auth = cleanup
                .auth
                .as_ref()
                .expect("authentication custody installed");
            match launch_spec.runtime_mode() {
                RuntimeMode::Host => {
                    let command_line = launch_spec.command_line(
                        &endpoint,
                        auth.backend_token_file_path(),
                        auth.token_sha256(),
                    )?;
                    let mut command = Command::new(command_line.program());
                    command.args(command_line.args());
                    if let Some(cwd) = command_line.cwd() {
                        command.current_dir(cwd);
                    }
                    command
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::piped());
                    let child = command
                        .spawn()
                        .map_err(|source| ManagedBackendError::Spawn {
                            program: command_line.program().to_string(),
                            source,
                        })?;
                    cleanup.process = Some(ManagedProcessOwner::Host(
                        SupervisedBackendProcess::new(child, launch_spec.display_label()),
                    ));
                    let Some(ManagedProcessOwner::Host(process)) = cleanup.process.as_mut() else {
                        unreachable!()
                    };
                    process.configure_host_tree()?;
                    let stderr = process
                        .take_stderr()
                        .ok_or(ManagedBackendError::MissingPipe {
                            stream_name: "stderr",
                        })?;
                    cleanup.stderr_reader =
                        Some(spawn_stderr_logger(stderr, launch_spec.display_label())?);
                }
                RuntimeMode::Wsl(distribution) => {
                    let request = beryl_wsl_supervisor::Frame::LaunchServer {
                        executable: launch_spec.runtime_native_executable().as_str().into(),
                        execution_root: launch_spec.working_directory().as_str().into(),
                        arguments: launch_spec.server_arguments(
                            &endpoint,
                            auth.backend_token_file_path(),
                            auth.token_sha256(),
                        ),
                    };
                    let owner = WslSupervision::new(
                        launch_spec
                            .wsl_supervisor_artifact()
                            .ok_or(ManagedBackendError::WslArtifactUnavailable)?,
                        distribution.as_str().into(),
                        launch_spec.working_directory().as_str().into(),
                    )?;
                    cleanup.process = Some(ManagedProcessOwner::Wsl(owner));
                    let Some(ManagedProcessOwner::Wsl(owner)) = cleanup.process.as_mut() else {
                        unreachable!()
                    };
                    owner.launch(request, Instant::now() + Duration::from_secs(5), None)?;
                }
            }
            allocate_process_generation()
        })();
        let process_generation = match launched {
            Ok(generation) => generation,
            Err(error) => return Err(ManagedBackendLaunchFailure::new(error, cleanup)),
        };
        let identity = ManagedBackendLaunchIdentity::new(&launch_spec, process_generation);
        Ok(Self {
            launch_spec,
            endpoint,
            cleanup,
            provenance: ManagedLaunchProvenance::Production(identity),
        })
    }
    pub fn launch_spec(&self) -> &ManagedBackendLaunchSpec {
        &self.launch_spec
    }

    pub fn endpoint(&self) -> &BackendWebSocketEndpoint {
        &self.endpoint
    }

    pub fn process_id(&self) -> Option<u32> {
        self.cleanup
            .process
            .as_ref()
            .and_then(ManagedProcessOwner::process_id)
    }

    pub fn is_process_alive(&mut self) -> bool {
        !self
            .cleanup
            .process
            .as_mut()
            .is_none_or(ManagedProcessOwner::has_exited)
    }

    pub fn client_connector(&self) -> ManagedBackendClientConnector {
        ManagedBackendClientConnector {
            endpoint: self.endpoint.clone(),
            authorization_header_value: self
                .cleanup
                .auth
                .as_ref()
                .expect("live server owns authentication")
                .authorization_header_value(),
            provenance: self.provenance.clone(),
        }
    }

    pub fn shutdown(&mut self) -> Result<(), ManagedBackendError> {
        self.cleanup.shutdown()
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub fn shutdown_wsl_with_timeout_for_lifecycle_test(
        &mut self,
        timeout: Duration,
    ) -> Result<(), ManagedBackendError> {
        if !self.cleanup.process_closed {
            let Some(ManagedProcessOwner::Wsl(owner)) = self.cleanup.process.as_mut() else {
                return Err(ManagedBackendError::WslSupervisionUnavailable);
            };
            owner.shutdown(Instant::now() + timeout.min(Duration::from_secs(5)))?;
            self.cleanup.process_closed = true;
        }
        self.cleanup.shutdown()
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub fn close_wsl_control_for_lifecycle_test(
        &mut self,
        role: beryl_wsl_supervisor::Role,
    ) -> Result<(), ManagedBackendError> {
        let Some(ManagedProcessOwner::Wsl(owner)) = self.cleanup.process.as_mut() else {
            return Err(ManagedBackendError::WslSupervisionUnavailable);
        };
        owner.close_control_for_lifecycle_test(role)
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub fn wsl_resource_custody_for_lifecycle_test(
        &self,
    ) -> Option<crate::lifecycle_test_support::WslOwnedResourceCustodyForLifecycleTest> {
        match self.cleanup.process.as_ref()? {
            ManagedProcessOwner::Wsl(owner) => Some(owner.resource_custody_for_lifecycle_test()),
            ManagedProcessOwner::Host(_) => None,
        }
    }

    /// Injects one post-termination stderr-join failure for lifecycle tests.
    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    pub fn fail_next_stderr_join_for_lifecycle_test(&mut self) {
        self.cleanup.fail_stderr_join_for_lifecycle_test = true;
    }
}

impl ManagedBackendClientConnector {
    /// Constructs an exact connector around a test-owned authenticated endpoint.
    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    #[must_use]
    pub fn for_lifecycle_test(
        endpoint: BackendWebSocketEndpoint,
        authorization_header_value: impl Into<String>,
    ) -> Self {
        Self {
            endpoint,
            authorization_header_value: authorization_header_value.into(),
            provenance: ManagedLaunchProvenance::LifecycleTest,
        }
    }

    /// Returns the admitted WebSocket endpoint for this runtime.
    pub fn endpoint(&self) -> &BackendWebSocketEndpoint {
        &self.endpoint
    }

    pub fn launch_identity(&self) -> Option<&ManagedBackendLaunchIdentity> {
        match &self.provenance {
            ManagedLaunchProvenance::Production(identity) => Some(identity),
            #[cfg(feature = "lifecycle-test-support")]
            ManagedLaunchProvenance::LifecycleTest => None,
        }
    }

    /// Opens one uninitialized foreground candidate with its immutable ingress profile.
    ///
    /// The profile is selected before the authenticated WebSocket handshake starts. Initialization
    /// remains a separate operation so callers can bind the ordered foreground sink before any
    /// initialized-profile traffic is admitted.
    pub fn connect_foreground_candidate(
        &self,
        config: ForegroundSessionConfig,
        timeout: Duration,
    ) -> Result<ManagedBackendSession, ManagedBackendError> {
        let transport = self.connect_foreground_transport_until(config, timeout)?;
        let mut session = ManagedBackendSession::from_foreground_websocket(transport)?;
        session.bind_managed_launch_provenance(self.provenance.clone());
        Ok(session)
    }

    /// Opens and initializes a request-only client session.
    pub fn connect_request_client(
        &self,
        timeout: Duration,
    ) -> Result<ManagedBackendSession, ManagedBackendError> {
        let transport = self.connect_request_only_transport_until(timeout)?;
        let mut session = ManagedBackendSession::from_request_only_websocket(transport)?;
        session.bind_managed_launch_provenance(self.provenance.clone());
        session.initialize_request_only(timeout)?;
        Ok(session)
    }

    /// Opens one uninitialized request-only candidate for profile-gate lifecycle tests.
    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    pub fn connect_request_candidate_for_lifecycle_test(
        &self,
        timeout: Duration,
    ) -> Result<ManagedBackendSession, ManagedBackendError> {
        let transport = self.connect_request_only_transport_until(timeout)?;
        let mut session = ManagedBackendSession::from_request_only_websocket(transport)?;
        session.bind_managed_launch_provenance(self.provenance.clone());
        Ok(session)
    }

    fn connect_foreground_transport_until(
        &self,
        config: ForegroundSessionConfig,
        timeout: Duration,
    ) -> Result<ForegroundWebSocketTransport, ManagedBackendError> {
        let mut candidate = ForegroundWebSocketCandidate::new(
            self.endpoint.clone(),
            self.authorization_header_value.clone(),
            config,
        );
        let deadline = Instant::now() + timeout;

        loop {
            match candidate.try_connect() {
                Ok(transport) => return Ok(transport),
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

    fn connect_request_only_transport_until(
        &self,
        timeout: Duration,
    ) -> Result<RequestOnlyWebSocketTransport, ManagedBackendError> {
        let candidate = RequestOnlyWebSocketCandidate::new(
            self.endpoint.clone(),
            self.authorization_header_value.clone(),
        );
        let deadline = Instant::now() + timeout;

        loop {
            match candidate.try_connect() {
                Ok(transport) => return Ok(transport),
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
            .field("endpoint", &self.endpoint)
            .field("authorization_header_value", &"<redacted>")
            .finish()
    }
}

fn select_loopback_port() -> Result<u16, ManagedBackendError> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|source| ManagedBackendError::SelectWebSocketPort { source })?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|source| ManagedBackendError::SelectWebSocketPort { source })
}

fn allocate_process_generation() -> Result<CasProcessGeneration, ManagedBackendError> {
    let value = NEXT_PROCESS_GENERATION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| ManagedBackendError::ProcessGenerationExhausted)?;
    CasProcessGeneration::new(value).map_err(|_| ManagedBackendError::ProcessGenerationExhausted)
}

fn spawn_stderr_logger(
    mut stderr: std::process::ChildStderr,
    launch: String,
) -> Result<thread::JoinHandle<()>, ManagedBackendError> {
    thread::Builder::new()
        .name("beryl-cas-stderr".to_string())
        .spawn(move || {
            let mut buffer = [0_u8; 4096];
            loop {
                match stderr.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        let text = String::from_utf8_lossy(&buffer[..count]);
                        tracing::debug!(launch = %launch, stderr = %text, "managed CAS stderr");
                    }
                    Err(error) => {
                        tracing::warn!(%error, launch = %launch, "failed to read managed CAS stderr");
                        break;
                    }
                }
            }
        })
        .map_err(|source| ManagedBackendError::SpawnStderrReader { source })
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
