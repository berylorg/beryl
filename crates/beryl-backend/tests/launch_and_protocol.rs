use beryl_backend::{
    BackendWebSocketEndpoint, ManagedBackendError, ManagedBackendLaunchSpec,
    ManagedBackendLaunchSpecError, ManagedWebSocketError,
};
use beryl_model::{AdmittedHostPath, PathFlavor, RuntimeId, RuntimeMode, RuntimeNativePath};

const TOKEN_DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

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
fn wsl_managed_launch_uses_exact_distro_native_executable_and_private_boundary() {
    let mode = RuntimeMode::wsl("Ubuntu-24.04").unwrap();
    let launch = ManagedBackendLaunchSpec::new(
        runtime_id(),
        host_path(r"\\wsl.localhost\Ubuntu-24.04\home\operator\bin\codex"),
        mode.clone(),
        native_path(mode.clone(), PathFlavor::Posix, "/home/operator/bin/codex"),
        native_path(mode.clone(), PathFlavor::Posix, "/work/beryl"),
        host_path(r"\\wsl.localhost\Ubuntu-24.04\tmp\beryl-token-files"),
        native_path(mode, PathFlavor::Posix, "/tmp/beryl-token-files"),
    )
    .unwrap();
    let command = launch
        .command_line(
            &BackendWebSocketEndpoint::loopback(49153),
            "/tmp/beryl-token-files/token.txt",
            TOKEN_DIGEST,
        )
        .unwrap();

    assert_eq!(command.program(), "wsl.exe");
    assert_eq!(
        &command.args()[..7],
        [
            "--distribution",
            "Ubuntu-24.04",
            "--cd",
            "/work/beryl",
            "--exec",
            "/bin/bash",
            "-lc",
        ]
    );
    let shell = &command.args()[7];
    assert!(shell.starts_with("umask 077; mkdir -m 700 /tmp/beryl-codex-app-server-"));
    assert!(shell.contains("setsid /bin/bash -lc"));
    assert!(shell.contains("/home/operator/bin/codex app-server --strict-config"));
    assert!(shell.contains(
        "features.multi_agent_v2={enabled=true,expose_spawn_agent_model_overrides=true}"
    ));
    assert!(shell.contains("/tmp/beryl-token-files/token.txt"));
    assert!(shell.contains(TOKEN_DIGEST));
    assert!(!shell.contains(" codex app-server"));
}

#[test]
fn launch_spec_rejects_cross_runtime_paths() {
    let host = RuntimeMode::host();
    let wsl = RuntimeMode::wsl("Ubuntu").unwrap();
    let error = ManagedBackendLaunchSpec::new(
        runtime_id(),
        host_path(r"C:\Codex\codex.exe"),
        host.clone(),
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
        let token_directory =
            tempfile::tempdir().expect("task token directory should be creatable");
        let launch = host_launch_spec(token_directory.path());
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
        let mut server = ManagedBackendServer::launch(host_launch_spec(token_directory.path()))
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

    fn host_launch_spec(token_directory: &Path) -> ManagedBackendLaunchSpec {
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
