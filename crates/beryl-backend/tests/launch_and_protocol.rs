use beryl_backend::{
    BackendWebSocketEndpoint, ManagedBackendError, ManagedBackendLaunchSpec,
    ManagedBackendLaunchSpecError, ManagedWebSocketError, WslSupervisorArtifact,
};
use beryl_model::{AdmittedHostPath, PathFlavor, RuntimeId, RuntimeMode, RuntimeNativePath};

const TOKEN_DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn both_launch_forms_supply_exact_authenticated_server_arguments_in_host_and_wsl() {
    use beryl_model::RuntimeLaunchForm;

    for mode in [RuntimeMode::Host, RuntimeMode::wsl("Ubuntu").unwrap()] {
        for launch_form in [
            RuntimeLaunchForm::StandaloneAppServer,
            RuntimeLaunchForm::CodexCli,
        ] {
            let (canonical, executable, root, tokens, flavor) = match &mode {
                RuntimeMode::Host => (
                    r"C:\selected\anything.exe",
                    r"C:\selected\anything.exe",
                    r"C:\root",
                    r"C:\tokens",
                    PathFlavor::Windows,
                ),
                RuntimeMode::Wsl(_) => (
                    r"\\wsl.localhost\Ubuntu\bin\anything",
                    "/bin/anything",
                    "/root",
                    "/tokens",
                    PathFlavor::Posix,
                ),
            };
            let spec = ManagedBackendLaunchSpec::new(
                runtime_id(),
                host_path(canonical),
                mode.clone(),
                launch_form,
                native_path(mode.clone(), flavor, executable),
                native_path(mode.clone(), flavor, root),
                host_path(r"C:\tokens"),
                native_path(mode.clone(), flavor, tokens),
            )
            .unwrap();
            assert_eq!(spec.launch_form(), launch_form);
            let endpoint = BackendWebSocketEndpoint::loopback(49152);
            let token_path = format!("{tokens}/selected-token");
            let arguments = spec.server_arguments(&endpoint, &token_path, TOKEN_DIGEST);
            let mut expected: Vec<String> = [
                "--strict-config",
                "-c",
                "features.multi_agent_v2={enabled=true,expose_spawn_agent_model_overrides=true}",
                "--listen",
                "ws://127.0.0.1:49152",
                "--ws-auth",
                "capability-token",
                "--ws-token-file",
                &token_path,
                "--ws-token-sha256",
                TOKEN_DIGEST,
            ]
            .into_iter()
            .map(str::to_owned)
            .collect();
            if launch_form == RuntimeLaunchForm::CodexCli {
                expected.insert(0, "app-server".into());
            }
            assert_eq!(arguments, expected);
            if mode == RuntimeMode::Host {
                let command = spec
                    .command_line(&endpoint, &token_path, TOKEN_DIGEST)
                    .unwrap();
                assert_eq!(command.program(), canonical);
                assert_eq!(command.args(), arguments);
            }
        }
    }
}

fn runtime_id() -> RuntimeId {
    RuntimeId::from_bytes([7; 16])
}

fn host_path(value: &str) -> AdmittedHostPath {
    AdmittedHostPath::from_admitted(PathFlavor::Windows, value).unwrap()
}

fn native_path(mode: RuntimeMode, flavor: PathFlavor, value: &str) -> RuntimeNativePath {
    RuntimeNativePath::from_admitted(mode, flavor, value).unwrap()
}

#[test]
fn host_managed_launch_uses_exact_executable_and_atomic_native_spawn_config() {
    let mode = RuntimeMode::host();
    let launch = ManagedBackendLaunchSpec::new(
        runtime_id(),
        host_path(r"C:\Codex\codex.exe"),
        mode.clone(),
        beryl_model::RuntimeLaunchForm::CodexCli,
        native_path(mode.clone(), PathFlavor::Windows, r"C:\Codex\codex.exe"),
        native_path(mode.clone(), PathFlavor::Windows, r"C:\Work\beryl"),
        host_path(r"C:\Beryl\tokens"),
        native_path(mode, PathFlavor::Windows, r"C:\Beryl\tokens"),
    )
    .unwrap();
    let command = launch
        .command_line(
            &BackendWebSocketEndpoint::loopback(49152),
            r"C:\Beryl\tokens\token.txt",
            TOKEN_DIGEST,
        )
        .unwrap();

    assert_eq!(command.program(), r"C:\Codex\codex.exe");
    assert_eq!(
        command.cwd().unwrap(),
        &std::path::PathBuf::from(r"C:\Work\beryl")
    );
    assert_eq!(command.args()[0], "app-server");
    assert_eq!(command.args()[1], "--strict-config");
    assert_eq!(command.args()[2], "-c");
    assert_eq!(
        command.args()[3],
        "features.multi_agent_v2={enabled=true,expose_spawn_agent_model_overrides=true}"
    );
    assert_eq!(command.args().iter().filter(|arg| *arg == "-c").count(), 1);
    assert!(!command.args().iter().any(|arg| arg == "--enable"));
    assert!(
        command
            .args()
            .windows(2)
            .any(|pair| { pair == ["--listen", "ws://127.0.0.1:49152"] })
    );
    assert!(
        command
            .args()
            .windows(2)
            .any(|pair| { pair == ["--ws-auth", "capability-token"] })
    );
    assert!(
        command
            .args()
            .windows(2)
            .any(|pair| { pair == ["--ws-token-file", r"C:\Beryl\tokens\token.txt"] })
    );
    assert!(
        command
            .args()
            .windows(2)
            .any(|pair| { pair == ["--ws-token-sha256", TOKEN_DIGEST] })
    );
}

#[test]
fn host_launch_rejects_disagreeing_executable_identities() {
    let mode = RuntimeMode::host();
    let error = ManagedBackendLaunchSpec::new(
        runtime_id(),
        host_path(r"C:\Codex\selected.exe"),
        mode.clone(),
        beryl_model::RuntimeLaunchForm::CodexCli,
        native_path(mode.clone(), PathFlavor::Windows, r"C:\Codex\other.exe"),
        native_path(mode.clone(), PathFlavor::Windows, r"C:\Work\beryl"),
        host_path(r"C:\Beryl\tokens"),
        native_path(mode, PathFlavor::Windows, r"C:\Beryl\tokens"),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        ManagedBackendLaunchSpecError::HostExecutableIdentityMismatch
    ));
}

#[test]
fn wsl_managed_launch_uses_exact_distro_broker_and_immutable_artifact() {
    let mode = RuntimeMode::wsl("Ubuntu-24.04").unwrap();
    let launch = ManagedBackendLaunchSpec::new(
        runtime_id(),
        host_path(r"\\wsl.localhost\Ubuntu-24.04\home\operator\bin\codex"),
        mode.clone(),
        beryl_model::RuntimeLaunchForm::CodexCli,
        native_path(mode.clone(), PathFlavor::Posix, "/home/operator/bin/codex"),
        native_path(mode.clone(), PathFlavor::Posix, "/work/beryl"),
        host_path(r"\\wsl.localhost\Ubuntu-24.04\tmp\beryl-token-files"),
        native_path(mode, PathFlavor::Posix, "/tmp/beryl-token-files"),
    )
    .unwrap()
    .with_wsl_supervisor_artifact(std::sync::Arc::new(
        WslSupervisorArtifact::from_verified_release(
            std::path::PathBuf::from(r"C:\Beryl\beryl-wsl-supervisor"),
            "/mnt/c/Beryl/beryl-wsl-supervisor".into(),
            [7; 32],
            1,
            std::sync::Arc::new(tempfile::tempfile().unwrap()),
        )
        .unwrap(),
    ));
    let command = launch
        .command_line(
            &BackendWebSocketEndpoint::loopback(49153),
            "/tmp/beryl-token-files/token.txt",
            TOKEN_DIGEST,
        )
        .unwrap();

    assert_eq!(command.program(), "wsl.exe");
    assert_eq!(
        command.args(),
        [
            "--distribution",
            "Ubuntu-24.04",
            "--cd",
            "/work/beryl",
            "--exec",
            "/mnt/c/Beryl/beryl-wsl-supervisor",
            "context-broker",
        ]
    );
    assert!(!command.args().iter().any(|argument| argument == "--user"));
    assert!(
        !command
            .args()
            .iter()
            .any(|argument| argument.contains(TOKEN_DIGEST))
    );
}

#[test]
fn launch_spec_rejects_cross_runtime_paths() {
    let host = RuntimeMode::host();
    let wsl = RuntimeMode::wsl("Ubuntu").unwrap();
    let error = ManagedBackendLaunchSpec::new(
        runtime_id(),
        host_path(r"C:\Codex\codex.exe"),
        host.clone(),
        beryl_model::RuntimeLaunchForm::CodexCli,
        native_path(wsl, PathFlavor::Posix, "/usr/bin/codex"),
        native_path(host.clone(), PathFlavor::Windows, r"C:\Work\beryl"),
        host_path(r"C:\Beryl\tokens"),
        native_path(host, PathFlavor::Windows, r"C:\Beryl\tokens"),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        ManagedBackendLaunchSpecError::ExecutableModeMismatch
    ));
}

#[test]
fn websocket_transport_error_display_includes_source_detail() {
    let error = ManagedBackendError::WebSocketTransport {
        method: "thread/read".to_string(),
        endpoint: "ws://127.0.0.1:49154".to_string(),
        source: ManagedWebSocketError::protocol("message too large"),
    };

    let display = error.to_string();

    assert!(display.contains("thread/read"));
    assert!(display.contains("message too large"));
}

#[cfg(all(target_os = "windows", feature = "lifecycle-test-support"))]
mod managed_launch_lifecycle {
    use std::{
        fs,
        net::{TcpListener, TcpStream},
        path::{Path, PathBuf},
        thread,
        time::Duration,
    };

    use beryl_backend::{
        BackendWebSocketEndpoint, ManagedBackendClientConnector, ManagedBackendError,
        ManagedBackendLaunchSpec, ManagedBackendServer,
    };
    use beryl_model::{AdmittedHostPath, PathFlavor, RuntimeId, RuntimeMode, RuntimeNativePath};
    use tungstenite::{Message, WebSocket, accept_hdr};

    const AUTHORIZATION: &str = "Bearer lifecycle-test-only";
    const TIMEOUT: Duration = Duration::from_secs(2);

    #[test]
    fn production_launch_redacts_token_cleans_material_and_exposes_identity() {
        production_launch_identity(beryl_model::RuntimeLaunchForm::CodexCli);
        production_launch_identity(beryl_model::RuntimeLaunchForm::StandaloneAppServer);
    }

    fn production_launch_identity(launch_form: beryl_model::RuntimeLaunchForm) {
        let token_directory =
            tempfile::tempdir().expect("task token directory should be creatable");
        let launch = host_launch_spec(token_directory.path(), launch_form);
        let expected_runtime = launch.runtime_id();
        let expected_executable = launch.canonical_executable().clone();
        let mut server = ManagedBackendServer::launch(launch)
            .expect("the exact Host test executable should form a managed child boundary");
        let process_id = server
            .process_id()
            .expect("managed server should retain its exact child identity");
        assert!(server.endpoint().is_loopback());
        assert_eq!(server.endpoint().host(), "127.0.0.1");

        let token_file = single_token_file(token_directory.path());
        let raw_token =
            fs::read_to_string(&token_file).expect("managed token file should be readable");
        assert!(!raw_token.is_empty());
        let debug = format!("{server:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains(raw_token.trim()));

        let connector = server.client_connector();
        let identity = connector
            .launch_identity()
            .expect("only a production managed server mints a production connector");
        assert_eq!(identity.runtime_id(), expected_runtime);
        assert_eq!(identity.canonical_executable(), &expected_executable);
        assert_eq!(identity.launch_form(), launch_form);
        assert!(identity.process_generation().get() > 0);

        server
            .shutdown()
            .expect("explicit managed-server shutdown should release its child boundary");
        assert!(
            !server.is_process_alive(),
            "managed child {process_id} survived explicit shutdown"
        );
        assert!(!token_file.exists(), "shutdown must clean token material");
        assert!(
            fs::read_dir(token_directory.path())
                .expect("task token directory should remain readable")
                .next()
                .is_none(),
            "managed launch left token material behind"
        );

        drop(server);
        token_directory
            .close()
            .expect("task token directory should be removable after shutdown");
    }

    #[test]
    fn shutdown_cleans_token_before_reporting_stderr_join_failure() {
        let token_directory =
            tempfile::tempdir().expect("task token directory should be creatable");
        let mut server = ManagedBackendServer::launch(host_launch_spec(
            token_directory.path(),
            beryl_model::RuntimeLaunchForm::CodexCli,
        ))
        .expect("the exact Host test executable should form a managed child boundary");
        let token_file = single_token_file(token_directory.path());
        server.fail_next_stderr_join_for_lifecycle_test();

        let error = server
            .shutdown()
            .expect_err("the lifecycle seam should report one stderr-join failure");

        assert!(matches!(error, ManagedBackendError::StderrReaderPanicked));
        assert!(
            !token_file.exists(),
            "confirmed process termination must clean token material before stderr-join failure"
        );
        drop(server);
        token_directory
            .close()
            .expect("task token directory should be removable after failed diagnostic cleanup");
    }

    #[test]
    fn lifecycle_test_connector_is_rejected_before_release_admission() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("test listener should bind");
        let endpoint = BackendWebSocketEndpoint::loopback(
            listener
                .local_addr()
                .expect("test listener should report a port")
                .port(),
        );
        let server = thread::spawn(move || assert_no_release_admission_request(listener));

        let connector = ManagedBackendClientConnector::for_lifecycle_test(endpoint, AUTHORIZATION);
        assert!(connector.launch_identity().is_none());
        let mut session = connector
            .connect_request_candidate_for_lifecycle_test(TIMEOUT)
            .expect("test connector should open its isolated test endpoint");
        let error = session
            .admit_release(Path::new(r"C:\\work\\beryl"), TIMEOUT)
            .expect_err("a test-only connector must not create production release admission");
        assert!(matches!(
            error,
            ManagedBackendError::ReleaseAdmissionManagedLaunchProvenanceMissing
        ));
        session
            .shutdown()
            .expect("test session should release its task-owned connection");
        server
            .join()
            .expect("test endpoint should observe no release-admission request");
    }

    fn host_launch_spec(
        token_directory: &Path,
        launch_form: beryl_model::RuntimeLaunchForm,
    ) -> ManagedBackendLaunchSpec {
        let executable = powershell_executable();
        let executable = executable
            .to_str()
            .expect("Host PowerShell path should be valid UTF-8");
        let working_directory = std::env::current_dir()
            .expect("test working directory should be available")
            .display()
            .to_string();
        let token_directory = token_directory
            .to_str()
            .expect("task token directory should be valid UTF-8");
        let mode = RuntimeMode::host();
        ManagedBackendLaunchSpec::new(
            RuntimeId::from_bytes([9; 16]),
            admitted_host_path(executable),
            mode.clone(),
            launch_form,
            admitted_native_path(mode.clone(), executable),
            admitted_native_path(mode.clone(), &working_directory),
            admitted_host_path(token_directory),
            admitted_native_path(mode, token_directory),
        )
        .expect("Host launch test paths should share one exact runtime mode")
    }

    fn powershell_executable() -> PathBuf {
        PathBuf::from(
            std::env::var_os("SystemRoot").expect("Windows SystemRoot should be available"),
        )
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe")
    }

    fn admitted_host_path(value: &str) -> AdmittedHostPath {
        AdmittedHostPath::from_admitted(PathFlavor::Windows, value)
            .expect("test Host path should be admitted")
    }

    fn admitted_native_path(mode: RuntimeMode, value: &str) -> RuntimeNativePath {
        RuntimeNativePath::from_admitted(mode, PathFlavor::Windows, value)
            .expect("test native path should be admitted")
    }

    fn single_token_file(directory: &Path) -> PathBuf {
        let mut entries = fs::read_dir(directory)
            .expect("task token directory should be readable")
            .map(|entry| entry.expect("task token entry should be readable"));
        let token_file = entries
            .next()
            .expect("managed launch should create exactly one token file")
            .path();
        assert!(
            entries.next().is_none(),
            "managed launch created multiple token files"
        );
        token_file
    }

    fn assert_no_release_admission_request(listener: TcpListener) {
        let (stream, _) = listener
            .accept()
            .expect("test endpoint should accept one client");
        let mut socket = accept_authenticated_socket(stream);
        socket
            .get_mut()
            .set_read_timeout(Some(TIMEOUT))
            .expect("test socket timeout should be configurable");
        match socket.read() {
            Ok(Message::Close(_))
            | Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {}
            Ok(message) => {
                panic!("test-only connector crossed release admission: {message:?}")
            }
            Err(error) => panic!("test endpoint read failed: {error}"),
        }
    }

    fn accept_authenticated_socket(stream: TcpStream) -> WebSocket<TcpStream> {
        accept_hdr(
            stream,
            |request: &tungstenite::handshake::server::Request, response| {
                assert_eq!(
                    request
                        .headers()
                        .get("authorization")
                        .expect("test connector should authenticate")
                        .to_str()
                        .expect("test authorization should be valid text"),
                    AUTHORIZATION
                );
                Ok(response)
            },
        )
        .expect("test endpoint should complete WebSocket handshake")
    }
}
