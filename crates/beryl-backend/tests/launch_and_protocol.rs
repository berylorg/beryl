use std::{
    error::Error as _,
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

#[cfg(unix)]
use std::{ffi::OsString, os::unix::ffi::OsStringExt};

use beryl_backend::{
    BackendLaunchSpec, BackendTransport, BackendWebSocketEndpoint, CompatibilityError,
    CompatibilityProbe, CompatibilitySnapshot, ConfigReadOptions, ConfigReadResponse,
    DynamicToolCallResponse, DynamicToolSpec, HardStopCapabilityProbe, HardStopTarget,
    HardStopTargetOutcome, InitializeResponse, ManagedBackendAuthMaterial,
    ManagedBackendClientOptions, ManagedBackendError, ManagedBackendLaunchOptions,
    ManagedBackendLaunchOptionsError, ManagedBackendServer, ManagedBackendSession,
    ManagedBackendStartupProgress, ManagedBackendStartupStage, ManagedWebSocketError,
    ModelListOptions, ModelListResponse, NonSteerableTurnKind, SortDirection,
    ThreadBranchCapabilityProbe, ThreadForkFailure, ThreadForkOptions, ThreadListOptions,
    ThreadListResponse, ThreadLoadedListResponse, ThreadSortKey, ThreadStartOptions, ThreadStatus,
    TurnStartOptions, TurnStatus, TurnStreamEvent, UserInput, active_turn_not_steerable_error,
};
use beryl_model::workspace::{RuntimeMode, WorkspaceId};
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket, accept_hdr};

#[cfg(all(target_os = "windows", feature = "lifecycle-test-support"))]
use beryl_backend::{
    combine_lifecycle_test_shutdown_results, launch_and_probe_lifecycle_test_with_options,
    spawn_lifecycle_test_server,
};

#[test]
fn host_windows_compatibility_stdio_launch_is_explicit() {
    let workspace = WorkspaceId::host_windows(r"C:\work\beryl");
    let launch = BackendLaunchSpec::managed_stdio_for_workspace(workspace.clone());
    let command = launch
        .command_line()
        .expect("host stdio command line should build");

    assert_eq!(launch.transport(), BackendTransport::ManagedStdio);
    assert_eq!(command.program(), "codex");
    assert_eq!(
        command.args(),
        &[
            "app-server".to_string(),
            "--listen".to_string(),
            "stdio://".to_string(),
        ]
    );
    assert_eq!(command.cwd(), Some(&PathBuf::from(r"C:\work\beryl")));
}

#[test]
fn websocket_transport_error_display_includes_source_detail() {
    let error = ManagedBackendError::WebSocketTransport {
        method: "thread/turns/list".to_string(),
        endpoint: "ws://127.0.0.1:49154".to_string(),
        source: ManagedWebSocketError::protocol("message too large"),
    };

    let display = error.to_string();

    assert!(display.contains("thread/turns/list"));
    assert!(display.contains("message too large"));
}

#[test]
fn wsl_linux_compatibility_stdio_launch_uses_bash_login_shell_and_process_group() {
    let workspace = WorkspaceId::wsl_linux("Ubuntu", "/work/beryl");
    let launch = BackendLaunchSpec::managed_stdio_for_workspace(workspace);
    let command = launch
        .command_line()
        .expect("WSL stdio command line should build");

    assert_eq!(command.program(), "wsl.exe");
    assert_wsl_launch_prefix(command.args(), "Ubuntu", "/work/beryl");
    assert_wsl_process_group_shell_command(command.args()[7].as_str(), &["stdio://"]);
    assert_eq!(command.cwd(), None);
}

#[test]
fn host_windows_managed_websocket_launch_uses_loopback_and_token_file() {
    let endpoint = BackendWebSocketEndpoint::loopback(49152);
    let token_file = PathBuf::from(r"C:\tmp\beryl-token.txt");
    let launch = BackendLaunchSpec::managed_websocket(
        RuntimeMode::HostWindows,
        r"C:\work\beryl",
        endpoint.clone(),
        token_file.clone(),
    );
    let command = launch
        .command_line()
        .expect("host websocket command line should build");

    let BackendTransport::ManagedWebSocket(config) = launch.transport() else {
        panic!("expected managed websocket transport");
    };
    assert_eq!(config.endpoint(), &endpoint);
    assert_eq!(config.backend_token_file_path(), token_file.as_path());
    assert!(config.endpoint().is_loopback());
    assert_eq!(config.endpoint().listen_url(), "ws://127.0.0.1:49152");
    assert_eq!(command.program(), "codex");
    assert_eq!(
        command.args(),
        &[
            "app-server".to_string(),
            "--listen".to_string(),
            "ws://127.0.0.1:49152".to_string(),
            "--ws-auth".to_string(),
            "capability-token".to_string(),
            "--ws-token-file".to_string(),
            r"C:\tmp\beryl-token.txt".to_string(),
        ]
    );
    assert_eq!(command.cwd(), Some(&PathBuf::from(r"C:\work\beryl")));
}

#[test]
fn host_windows_exact_program_is_used_directly_for_stdio_and_websocket() {
    let program = PathBuf::from(r"C:\Program Files\Codex\codex.exe");
    let options = ManagedBackendLaunchOptions::with_exact_host_windows_program(program.clone())
        .expect("absolute Unicode exact program should be accepted");

    let stdio = BackendLaunchSpec::managed_stdio_with_options(
        RuntimeMode::HostWindows,
        r"C:\work\beryl",
        options.clone(),
    )
    .expect("Host Windows stdio launch should accept an exact program");
    let stdio_command = stdio
        .command_line()
        .expect("Host Windows stdio command line should build");
    assert_eq!(stdio_command.program(), program.to_str().unwrap());
    assert_eq!(
        stdio_command.args(),
        &[
            "app-server".to_string(),
            "--listen".to_string(),
            "stdio://".to_string(),
        ]
    );
    assert_eq!(stdio_command.cwd(), Some(&PathBuf::from(r"C:\work\beryl")));

    let websocket = BackendLaunchSpec::managed_websocket_with_options(
        RuntimeMode::HostWindows,
        r"C:\work\beryl",
        BackendWebSocketEndpoint::loopback(49152),
        r"C:\tmp\beryl-token.txt",
        options,
    )
    .expect("Host Windows WebSocket launch should accept an exact program");
    let websocket_command = websocket
        .command_line()
        .expect("Host Windows WebSocket command line should build");
    assert_eq!(websocket_command.program(), program.to_str().unwrap());
    assert_eq!(
        websocket_command.args(),
        &[
            "app-server".to_string(),
            "--listen".to_string(),
            "ws://127.0.0.1:49152".to_string(),
            "--ws-auth".to_string(),
            "capability-token".to_string(),
            "--ws-token-file".to_string(),
            r"C:\tmp\beryl-token.txt".to_string(),
        ]
    );
    assert_eq!(
        websocket_command.cwd(),
        Some(&PathBuf::from(r"C:\work\beryl"))
    );
}

#[test]
fn exact_host_windows_program_rejects_empty_and_relative_paths() {
    assert!(matches!(
        ManagedBackendLaunchOptions::with_exact_host_windows_program(""),
        Err(ManagedBackendLaunchOptionsError::EmptyExactHostWindowsProgram)
    ));
    assert!(matches!(
        ManagedBackendLaunchOptions::with_exact_host_windows_program("codex.exe"),
        Err(ManagedBackendLaunchOptionsError::RelativeExactHostWindowsProgram { .. })
    ));
}

#[cfg(unix)]
#[test]
fn exact_host_windows_program_rejects_non_unicode_paths_when_representable() {
    let program = PathBuf::from(OsString::from_vec(vec![b'/', 0xff]));

    assert!(matches!(
        ManagedBackendLaunchOptions::with_exact_host_windows_program(program),
        Err(ManagedBackendLaunchOptionsError::NonUnicodeExactHostWindowsProgram { .. })
    ));
}

#[test]
fn exact_host_windows_program_is_rejected_for_wsl_before_managed_server_setup() {
    let options = ManagedBackendLaunchOptions::with_exact_host_windows_program(
        r"C:\Program Files\Codex\codex.exe",
    )
    .expect("absolute Unicode exact program should be accepted");

    let error = ManagedBackendServer::launch_with_options(
        RuntimeMode::WslLinux {
            distro_name: "Ubuntu".to_string(),
        },
        "/work/beryl",
        options,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        ManagedBackendError::InvalidLaunchOptions {
            source: ManagedBackendLaunchOptionsError::ExactHostWindowsProgramUnsupportedRuntime {
                runtime_mode: RuntimeMode::WslLinux { .. }
            }
        }
    ));
}

#[cfg(windows)]
#[test]
fn nonexistent_exact_host_windows_program_is_preserved_in_spawn_error() {
    let program = std::env::temp_dir().join("beryl phase2 nonexistent codex.exe");
    let program_text = program.to_str().unwrap().to_string();
    let options = ManagedBackendLaunchOptions::with_exact_host_windows_program(program)
        .expect("temporary-directory target should be an absolute Unicode path");
    let launch = BackendLaunchSpec::managed_stdio_with_options(
        RuntimeMode::HostWindows,
        r"C:\work\beryl",
        options,
    )
    .expect("Host Windows stdio launch should accept an exact program");

    let error = ManagedBackendSession::launch_and_probe(launch, Duration::from_millis(100))
        .expect_err("nonexistent exact program should fail to spawn");

    assert!(matches!(
        &error,
        ManagedBackendError::Spawn { program, .. } if program == &program_text
    ));
    assert!(error.to_string().contains(&program_text));
}

#[test]
fn wsl_linux_managed_websocket_launch_uses_loopback_bash_login_shell_and_process_group() {
    let endpoint = BackendWebSocketEndpoint::loopback(49153);
    let launch = BackendLaunchSpec::managed_websocket(
        RuntimeMode::WslLinux {
            distro_name: "Ubuntu".to_string(),
        },
        "/work/beryl",
        endpoint,
        "/tmp/beryl-token.txt",
    );
    let command = launch
        .command_line()
        .expect("WSL websocket command line should build");

    assert_eq!(command.program(), "wsl.exe");
    assert_wsl_launch_prefix(command.args(), "Ubuntu", "/work/beryl");
    assert_wsl_process_group_shell_command(
        command.args()[7].as_str(),
        &[
            "ws://127.0.0.1:49153",
            "--ws-auth",
            "capability-token",
            "--ws-token-file",
            "/tmp/beryl-token.txt",
        ],
    );
    assert_eq!(command.cwd(), None);
}

#[test]
fn wsl_linux_launch_keeps_supervised_shell_alive_while_waiting_for_process_group() {
    let endpoint = BackendWebSocketEndpoint::loopback(49155);
    let launch = BackendLaunchSpec::managed_websocket(
        RuntimeMode::WslLinux {
            distro_name: "Ubuntu".to_string(),
        },
        "/work/beryl",
        endpoint,
        "/tmp/beryl-token.txt",
    );
    let command = launch
        .command_line()
        .expect("WSL websocket command line should build");
    let shell = command.args()[7].as_str();
    let outer = wsl_launch_outer_shell_tokens(shell);
    let inner = wsl_launch_inner_shell_command(shell);
    let codex_args = wsl_launch_codex_args_from_inner(&inner);

    assert!(
        !outer
            .windows(2)
            .any(|window| window[0] == "exec" && window[1] == "setsid"),
        "WSL launch shell must not exec setsid because Beryl supervises the outer shell"
    );
    assert!(
        shell.contains("& child=$!"),
        "WSL launch shell must record the child PID"
    );
    assert!(
        shell.contains("wait \"$child\""),
        "WSL launch shell must wait for the child process group starter"
    );
    assert!(
        shell.contains("exit \"$status\""),
        "WSL launch shell must propagate the waited child status"
    );
    assert!(inner.contains("\"$$\""));
    assert_eq!(codex_args[0], "app-server");
    assert!(codex_args.iter().any(|arg| arg == "--listen"));
}

#[test]
fn wsl_linux_launch_shell_quotes_special_codex_arguments() {
    let endpoint = BackendWebSocketEndpoint::loopback(49156);
    let token_file = format!("/tmp/token dir/it'has $HOME; back\\slash caf\u{00e9}.txt");
    let distro_name = "Ubuntu Dev's";
    let cwd = "/work/beryl folder/$literal";
    let launch = BackendLaunchSpec::managed_websocket(
        RuntimeMode::WslLinux {
            distro_name: distro_name.to_string(),
        },
        cwd,
        endpoint,
        token_file.clone(),
    );
    let command = launch
        .command_line()
        .expect("WSL websocket command line should build");

    assert_eq!(command.program(), "wsl.exe");
    assert_wsl_launch_prefix(command.args(), distro_name, cwd);
    assert_eq!(
        wsl_launch_codex_args(command.args()[7].as_str()),
        vec![
            "app-server".to_string(),
            "--listen".to_string(),
            "ws://127.0.0.1:49156".to_string(),
            "--ws-auth".to_string(),
            "capability-token".to_string(),
            "--ws-token-file".to_string(),
            token_file,
        ]
    );
}

#[test]
fn wsl_linux_launch_rejects_nul_in_shell_quoted_argument() {
    let launch = BackendLaunchSpec::managed_websocket(
        RuntimeMode::WslLinux {
            distro_name: "Ubuntu".to_string(),
        },
        "/work/beryl",
        BackendWebSocketEndpoint::loopback(49157),
        "/tmp/beryl-token\0.txt",
    );
    let error = launch
        .command_line()
        .expect_err("NUL bytes should be rejected before spawning WSL");

    assert_eq!(error.field(), "codex app-server argument");
    assert_eq!(
        error
            .source()
            .and_then(|source| source.downcast_ref::<shlex::QuoteError>()),
        Some(&shlex::QuoteError::Nul)
    );
}

fn assert_wsl_launch_prefix(args: &[String], distro_name: &str, cwd: &str) {
    assert_eq!(args.len(), 8);
    assert_eq!(args[0], "--distribution");
    assert_eq!(args[1], distro_name);
    assert_eq!(args[2], "--cd");
    assert_eq!(args[3], cwd);
    assert_eq!(args[4], "--exec");
    assert_eq!(args[5], "/bin/bash");
    assert_eq!(args[6], "-lc");
}

fn assert_wsl_process_group_shell_command(shell: &str, expected_fragments: &[&str]) {
    let inner = wsl_launch_inner_shell_command(shell);
    let codex_args = wsl_launch_codex_args_from_inner(&inner);

    assert!(inner.contains("printf"));
    assert!(inner.contains("\"$$\""));
    assert!(inner.contains("/tmp/beryl-codex-app-server/process-"));
    assert!(inner.contains(".pid"));
    assert_eq!(codex_args[0], "app-server");
    assert!(codex_args.iter().any(|arg| arg == "--listen"));

    for fragment in expected_fragments {
        assert!(
            codex_args.iter().any(|arg| arg == fragment),
            "missing WSL launch fragment {fragment:?} in {codex_args:?}"
        );
    }
}

fn wsl_launch_codex_args(shell: &str) -> Vec<String> {
    let inner = wsl_launch_inner_shell_command(shell);
    wsl_launch_codex_args_from_inner(&inner)
}

fn wsl_launch_outer_shell_tokens(shell: &str) -> Vec<String> {
    shlex::split(shell).expect("outer WSL shell command should parse")
}

fn wsl_launch_inner_shell_command(shell: &str) -> String {
    let outer = wsl_launch_outer_shell_tokens(shell);
    assert_eq!(outer[0], "mkdir");
    assert_eq!(outer[1], "-p");
    assert_eq!(outer[2], "/tmp/beryl-codex-app-server");
    assert_eq!(outer[3], "&&");
    let setsid_index = outer
        .iter()
        .position(|arg| arg == "setsid")
        .expect("outer WSL shell command should start a setsid child");
    assert_ne!(
        outer
            .get(setsid_index.saturating_sub(1))
            .map(String::as_str),
        Some("exec")
    );
    assert_eq!(
        outer.get(setsid_index + 1).map(String::as_str),
        Some("/bin/bash")
    );
    assert_eq!(outer.get(setsid_index + 2).map(String::as_str), Some("-lc"));

    outer
        .get(setsid_index + 3)
        .expect("outer WSL shell command should pass an inner shell command")
        .clone()
}

fn wsl_launch_codex_args_from_inner(inner: &str) -> Vec<String> {
    let codex_start = inner
        .find("; codex ")
        .expect("inner WSL shell command should launch codex")
        + 2;
    let codex_end = inner[codex_start..]
        .find("; status=$?")
        .expect("inner WSL shell command should capture codex exit status")
        + codex_start;
    let codex_command = &inner[codex_start..codex_end];
    let mut codex_tokens =
        shlex::split(codex_command).expect("codex WSL shell command should parse");

    assert_eq!(codex_tokens.remove(0), "codex");
    codex_tokens
}

#[test]
fn websocket_auth_material_writes_redacted_token_file_and_cleans_up() {
    let mut auth = ManagedBackendAuthMaterial::generate(&RuntimeMode::HostWindows).unwrap();
    let host_path = auth.host_token_file_path().to_path_buf();
    assert_eq!(auth.backend_token_file_path(), host_path.as_path());
    assert_token_file_name(&host_path);

    let token = std::fs::read_to_string(auth.host_token_file_path()).unwrap();

    assert_eq!(token.len(), 64);
    assert!(token.chars().all(|ch| ch.is_ascii_hexdigit()));
    assert!(is_lowercase_hex(&token));
    assert_eq!(auth.authorization_header_value(), format!("Bearer {token}"));
    assert!(!format!("{auth:?}").contains(&token));

    let launch = BackendLaunchSpec::managed_websocket(
        RuntimeMode::HostWindows,
        r"C:\work\beryl",
        BackendWebSocketEndpoint::loopback(49154),
        auth.backend_token_file_path().to_path_buf(),
    );
    assert!(
        launch
            .command_line()
            .expect("host websocket command line should build")
            .args()
            .iter()
            .all(|arg| !arg.contains(&token))
    );

    auth.cleanup().unwrap();
    assert!(!host_path.exists());
}

fn assert_token_file_name(path: &Path) {
    let file_name = path
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .expect("token path should end with a UTF-8 file name");
    let nonce = file_name
        .strip_prefix("token-")
        .and_then(|suffix| suffix.strip_suffix(".txt"))
        .expect("token file name should include token nonce");

    assert_eq!(nonce.len(), 32);
    assert!(is_lowercase_hex(nonce));
}

fn is_lowercase_hex(value: &str) -> bool {
    value
        .as_bytes()
        .iter()
        .all(|&byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

#[test]
fn websocket_client_initializes_routes_responses_and_buffers_notifications() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/list"));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "method": "thread/name/updated",
                    "params": {
                        "threadId": "thread_123",
                        "threadName": "Buffered title"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "data": []
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = client
        .list_thread_page(&ThreadListOptions::page(1), Duration::from_secs(2))
        .unwrap();
    assert!(response.data.is_empty());

    let event = client
        .next_turn_stream_event(Duration::from_millis(10))
        .unwrap()
        .unwrap();
    assert_eq!(
        event,
        TurnStreamEvent::ThreadNameUpdated {
            thread_id: "thread_123".to_string(),
            thread_name: Some("Buffered title".to_string())
        }
    );

    server.join().unwrap();
}

#[test]
fn websocket_client_reads_large_single_frame_response() {
    const LARGE_PADDING_BYTES: usize = 17 * 1024 * 1024;

    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("model/list"));
        assert_eq!(request["params"], json!({ "limit": 1 }));

        let large_padding = "A".repeat(LARGE_PADDING_BYTES);
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "data": [],
                        "padding": large_padding
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let models = client
        .list_model_page(&ModelListOptions::page(1), Duration::from_secs(10))
        .unwrap();
    assert!(models.data.is_empty());

    server.join().unwrap();
}

#[test]
fn websocket_turn_start_serializes_ordered_user_input() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("turn/start"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_1",
                "input": [
                    {
                        "type": "text",
                        "text": "First fragment"
                    },
                    {
                        "type": "text",
                        "text": "Second fragment"
                    }
                ],
                "model": "gpt-5.5",
                "effort": "high"
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "turn": {
                            "id": "turn_1",
                            "items": [],
                            "status": "inProgress"
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = client
        .start_turn_with_user_input_options(
            "thread_1",
            vec![
                UserInput::text("First fragment"),
                UserInput::text("Second fragment"),
            ],
            TurnStartOptions::default()
                .with_model("gpt-5.5")
                .with_reasoning_effort("high"),
            Duration::from_secs(2),
        )
        .unwrap();

    assert_eq!(response.turn.id, "turn_1");
    server.join().unwrap();
}

#[test]
fn websocket_turn_start_serializes_hidden_developer_instructions_context() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("turn/start"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_1",
                "input": [
                    {
                        "type": "text",
                        "text": "Follow up"
                    }
                ],
                "collaborationMode": {
                    "mode": "default",
                    "settings": {
                        "model": "gpt-5.5",
                        "reasoning_effort": "high",
                        "developer_instructions": "Use the operator's project rules."
                    }
                }
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "turn": {
                            "id": "turn_1",
                            "items": [],
                            "status": "inProgress"
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = client
        .start_turn_with_user_input_options(
            "thread_1",
            vec![UserInput::text("Follow up")],
            TurnStartOptions::default().with_developer_instructions_context(
                Some("Use the operator's project rules.".to_string()),
                "gpt-5.5",
                Some("high".to_string()),
            ),
            Duration::from_secs(2),
        )
        .unwrap();

    assert_eq!(response.turn.id, "turn_1");
    server.join().unwrap();
}

#[test]
fn websocket_turn_start_serializes_disabled_developer_instructions_as_hidden_reset() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("turn/start"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_1",
                "input": [
                    {
                        "type": "text",
                        "text": "Follow up"
                    }
                ],
                "collaborationMode": {
                    "mode": "default",
                    "settings": {
                        "model": "gpt-5.5",
                        "developer_instructions": null
                    }
                }
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "turn": {
                            "id": "turn_1",
                            "items": [],
                            "status": "inProgress"
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = client
        .start_turn_with_user_input_options(
            "thread_1",
            vec![UserInput::text("Follow up")],
            TurnStartOptions::default().with_developer_instructions_context(None, "gpt-5.5", None),
            Duration::from_secs(2),
        )
        .unwrap();

    assert_eq!(response.turn.id, "turn_1");
    server.join().unwrap();
}

#[test]
fn websocket_thread_start_normalizes_dynamic_tools_and_developer_instructions() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/start"));
        assert_eq!(
            request["params"],
            json!({
                "cwd": "C:\\work\\beryl",
                "ephemeral": false,
                "developerInstructions": "Use project-specific review instructions.",
                "dynamicTools": [
                    {
                        "type": "namespace",
                        "name": "beryl",
                        "description": "Dynamic tools in this namespace.",
                        "tools": [
                            {
                                "type": "function",
                                "name": "apply_graph_patch",
                                "description": "Apply a bounded semantic graph patch.",
                                "inputSchema": {
                                    "type": "object",
                                    "required": ["ops"],
                                    "properties": {
                                        "ops": {
                                            "type": "array"
                                        }
                                    }
                                },
                                "deferLoading": true
                            },
                            {
                                "type": "function",
                                "name": "read_graph",
                                "description": "Read a bounded workspace graph summary.",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {},
                                    "additionalProperties": false
                                }
                            }
                        ]
                    },
                    {
                        "type": "function",
                        "name": "workspace_status",
                        "description": "Read workspace status.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {},
                            "additionalProperties": false
                        },
                        "deferLoading": false
                    },
                    {
                        "type": "namespace",
                        "name": "beryl_diagnostic",
                        "description": "Dynamic tools in this namespace.",
                        "tools": [
                            {
                                "type": "function",
                                "name": "status",
                                "description": "Read diagnostic child process lifecycle status.",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {},
                                    "additionalProperties": false
                                },
                                "deferLoading": false
                            }
                        ]
                    }
                ]
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "thread": {
                            "cliVersion": "0.128.0",
                            "createdAt": 1,
                            "cwd": "C:/work/beryl",
                            "ephemeral": false,
                            "id": "thread_1",
                            "modelProvider": "openai",
                            "preview": "",
                            "source": "appServer",
                            "status": {
                                "type": "idle"
                            },
                            "turns": [],
                            "updatedAt": 2
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = client
        .start_thread_with_options(
            &PathBuf::from(r"C:\work\beryl"),
            ThreadStartOptions::persistent()
                .with_developer_instructions("Use project-specific review instructions.")
                .with_dynamic_tool(
                    DynamicToolSpec::new(
                        "apply_graph_patch",
                        "Apply a bounded semantic graph patch.",
                        json!({
                            "type": "object",
                            "required": ["ops"],
                            "properties": {
                                "ops": {
                                    "type": "array"
                                }
                            }
                        }),
                    )
                    .with_namespace("beryl")
                    .with_defer_loading(true),
                )
                .with_dynamic_tool(
                    DynamicToolSpec::new(
                        "workspace_status",
                        "Read workspace status.",
                        json!({
                            "type": "object",
                            "properties": {},
                            "additionalProperties": false
                        }),
                    )
                    .with_defer_loading(false),
                )
                .with_dynamic_tool(
                    DynamicToolSpec::new(
                        "status",
                        "Read diagnostic child process lifecycle status.",
                        json!({
                            "type": "object",
                            "properties": {},
                            "additionalProperties": false
                        }),
                    )
                    .with_namespace("beryl_diagnostic")
                    .with_defer_loading(false),
                )
                .with_dynamic_tool(
                    DynamicToolSpec::new(
                        "read_graph",
                        "Read a bounded workspace graph summary.",
                        json!({
                            "type": "object",
                            "properties": {},
                            "additionalProperties": false
                        }),
                    )
                    .with_namespace("beryl"),
                ),
            Duration::from_secs(2),
        )
        .unwrap();

    assert_eq!(response.thread.summary().id, "thread_1");
    server.join().unwrap();
}

#[test]
fn websocket_thread_start_omits_developer_instructions_when_unset() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/start"));
        assert_eq!(
            request["params"],
            json!({
                "cwd": "C:\\work\\beryl",
                "ephemeral": false
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "thread": {
                            "cliVersion": "0.128.0",
                            "createdAt": 1,
                            "cwd": "C:/work/beryl",
                            "ephemeral": false,
                            "id": "thread_1",
                            "modelProvider": "openai",
                            "preview": "",
                            "source": "appServer",
                            "status": {
                                "type": "idle"
                            },
                            "turns": [],
                            "updatedAt": 2
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = client
        .start_thread(&PathBuf::from(r"C:\work\beryl"), Duration::from_secs(2))
        .unwrap();

    assert_eq!(response.thread.summary().id, "thread_1");
    server.join().unwrap();
}

#[test]
fn websocket_resume_and_unsubscribe_remain_separate_from_persistent_lifecycle_calls() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("thread/resume"));
        assert_eq!(
            request["params"],
            json!({ "threadId": "thread_persistent" })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "thread": {
                            "createdAt": 1,
                            "cwd": "C:/work/beryl",
                            "ephemeral": false,
                            "id": "thread_persistent",
                            "modelProvider": "openai",
                            "preview": "Persistent thread",
                            "status": { "type": "idle" },
                            "turns": [],
                            "updatedAt": 2
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();

        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("thread/unsubscribe"));
        assert_eq!(
            request["params"],
            json!({ "threadId": "thread_persistent" })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 3,
                    "result": { "status": "unsubscribed" }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let resumed = client
        .resume_thread("thread_persistent", Duration::from_secs(2))
        .unwrap();
    assert_eq!(resumed.thread.summary().id, "thread_persistent");
    let unsubscribe = client
        .unsubscribe_thread("thread_persistent", Duration::from_secs(2))
        .unwrap();
    assert_eq!(
        unsubscribe.status,
        beryl_backend::ThreadUnsubscribeStatus::Unsubscribed
    );

    server.join().unwrap();
}

#[test]
fn websocket_thread_lifecycle_requests_preserve_exact_wire_shapes_and_notifications() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/archive"));
        assert_eq!(
            request["params"],
            json!({ "threadId": "thread_persistent" })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "method": "thread/archived",
                    "params": { "threadId": "thread_persistent" }
                })
                .to_string(),
            ))
            .unwrap();
        socket
            .send(Message::text(
                json!({ "jsonrpc": "2.0", "id": 2, "result": {} }).to_string(),
            ))
            .unwrap();

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(3));
        assert_eq!(request["method"], json!("thread/unarchive"));
        assert_eq!(
            request["params"],
            json!({ "threadId": "thread_persistent" })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "method": "thread/unarchived",
                    "params": { "threadId": "thread_persistent" }
                })
                .to_string(),
            ))
            .unwrap();
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 3,
                    "result": {
                        "thread": {
                            "createdAt": 1,
                            "cwd": "C:/work/beryl",
                            "ephemeral": false,
                            "id": "thread_persistent",
                            "modelProvider": "openai",
                            "preview": "Lifecycle thread",
                            "status": { "type": "notLoaded" },
                            "turns": [],
                            "updatedAt": 2
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(4));
        assert_eq!(request["method"], json!("thread/delete"));
        assert_eq!(
            request["params"],
            json!({ "threadId": "thread_persistent" })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "method": "thread/deleted",
                    "params": { "threadId": "thread_persistent" }
                })
                .to_string(),
            ))
            .unwrap();
        socket
            .send(Message::text(
                json!({ "jsonrpc": "2.0", "id": 4, "result": {} }).to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    client
        .archive_thread("thread_persistent", Duration::from_secs(2))
        .unwrap();
    let restored = client
        .unarchive_thread("thread_persistent", Duration::from_secs(2))
        .unwrap();
    assert_eq!(restored.summary().id, "thread_persistent");
    assert_eq!(restored.status, ThreadStatus::NotLoaded);
    client
        .delete_thread("thread_persistent", Duration::from_secs(2))
        .unwrap();

    assert_eq!(
        client
            .next_turn_stream_event(Duration::from_millis(10))
            .unwrap(),
        Some(TurnStreamEvent::ThreadArchived {
            thread_id: "thread_persistent".to_string(),
        })
    );
    assert_eq!(
        client
            .next_turn_stream_event(Duration::from_millis(10))
            .unwrap(),
        Some(TurnStreamEvent::ThreadUnarchived {
            thread_id: "thread_persistent".to_string(),
        })
    );
    assert_eq!(
        client
            .next_turn_stream_event(Duration::from_millis(10))
            .unwrap(),
        Some(TurnStreamEvent::ThreadDeleted {
            thread_id: "thread_persistent".to_string(),
        })
    );

    server.join().unwrap();
}

#[test]
fn websocket_thread_archive_preserves_json_rpc_errors() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("thread/archive"));
        assert_eq!(request["params"], json!({ "threadId": "thread_active" }));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "error": {
                        "code": -32000,
                        "message": "thread cannot be archived while active"
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let error = client
        .archive_thread("thread_active", Duration::from_secs(2))
        .unwrap_err();
    let ManagedBackendError::RequestFailed { method, error } = error else {
        panic!("expected thread/archive request failure");
    };
    assert_eq!(method, "thread/archive");
    assert_eq!(error.code, -32000);
    assert_eq!(error.message, "thread cannot be archived while active");

    server.join().unwrap();
}

#[test]
fn websocket_thread_unarchive_rejects_malformed_response() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("thread/unarchive"));
        assert_eq!(request["params"], json!({ "threadId": "thread_archived" }));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": { "thread": { "id": "thread_archived" } }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let error = client
        .unarchive_thread("thread_archived", Duration::from_secs(2))
        .unwrap_err();
    let ManagedBackendError::DeserializeResponse { method, .. } = error else {
        panic!("expected malformed thread/unarchive response");
    };
    assert_eq!(method, "thread/unarchive");

    server.join().unwrap();
}

#[test]
fn websocket_thread_fork_and_rollback_use_observed_branch_protocol() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/fork"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_source"
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "model": "gpt-5.5",
                        "modelProvider": "openai",
                        "reasoningEffort": "high",
                        "thread": {
                            "createdAt": 1,
                            "cwd": "C:/work/beryl",
                            "ephemeral": false,
                            "id": "thread_branch",
                            "modelProvider": "openai",
                            "preview": "First request",
                            "status": {
                                "type": "idle"
                            },
                            "turns": [
                                {
                                    "id": "turn_1",
                                    "items": [],
                                    "status": "completed"
                                },
                                {
                                    "id": "turn_2",
                                    "items": [],
                                    "status": "completed"
                                }
                            ],
                            "updatedAt": 2
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(3));
        assert_eq!(request["method"], json!("thread/rollback"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_branch",
                "numTurns": 1
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 3,
                    "result": {
                        "thread": {
                            "createdAt": 1,
                            "cwd": "C:/work/beryl",
                            "ephemeral": false,
                            "id": "thread_branch",
                            "modelProvider": "openai",
                            "preview": "First request",
                            "status": {
                                "type": "idle"
                            },
                            "turns": [
                                {
                                    "id": "turn_1",
                                    "items": [],
                                    "status": "completed"
                                }
                            ],
                            "updatedAt": 3
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let fork = client
        .fork_thread("thread_source", Duration::from_secs(2))
        .unwrap();
    assert_eq!(fork.thread.summary().id, "thread_branch");
    assert_eq!(fork.thread.status, ThreadStatus::Idle);
    assert_eq!(fork.thread.turns.len(), 2);
    assert_eq!(fork.thread.turns[0].status, TurnStatus::Completed);
    assert_eq!(fork.metadata().model.as_deref(), Some("gpt-5.5"));
    assert_eq!(fork.metadata().reasoning_effort.as_deref(), Some("high"));

    let rollback = client
        .rollback_thread("thread_branch", 1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(rollback.thread.summary().id, "thread_branch");
    assert_eq!(rollback.thread.turns.len(), 1);
    assert_eq!(rollback.thread.turns[0].id, "turn_1");
    server.join().unwrap();
}

#[test]
fn websocket_thread_fork_metadata_only_sets_exclude_turns() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/fork"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_source",
                "excludeTurns": true
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "model": "gpt-5.5",
                        "modelProvider": "openai",
                        "thread": {
                            "createdAt": 1,
                            "cwd": "C:/work/beryl",
                            "ephemeral": false,
                            "id": "thread_branch",
                            "modelProvider": "openai",
                            "preview": "First request",
                            "status": {
                                "type": "idle"
                            },
                            "turns": [],
                            "updatedAt": 2
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let fork = client
        .fork_thread_with_options(
            "thread_source",
            ThreadForkOptions::metadata_only(),
            Duration::from_secs(2),
        )
        .unwrap();
    assert_eq!(fork.thread.summary().id, "thread_branch");
    assert!(fork.thread.turns.is_empty());
    server.join().unwrap();
}

#[test]
fn websocket_thread_fork_commitment_preserves_explicit_rejection() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/fork"));
        assert_eq!(request["params"], json!({ "threadId": "thread_source" }));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "error": { "code": -32602, "message": "fork is rejected" }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let error = client
        .fork_thread_with_commitment("thread_source", Duration::from_secs(2))
        .expect_err("explicit JSON-RPC rejection must not be indeterminate");
    let ThreadForkFailure::NotCommitted { source } = error else {
        panic!("expected definitive thread/fork failure");
    };
    assert!(matches!(
        source,
        ManagedBackendError::RequestFailed { ref method, ref error }
            if method == "thread/fork" && error.code == -32602
    ));

    server.join().unwrap();
}

#[test]
fn websocket_thread_fork_commitment_timeout_is_indeterminate() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("thread/fork"));
        thread::sleep(Duration::from_millis(100));
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let error = client
        .fork_thread_with_commitment("thread_source", Duration::from_millis(20))
        .expect_err("lost fork response must be indeterminate");
    let ThreadForkFailure::Indeterminate { source } = error else {
        panic!("expected indeterminate thread/fork failure");
    };
    assert!(matches!(
        source,
        ManagedBackendError::RequestTimeout { ref method, .. } if method == "thread/fork"
    ));

    server.join().unwrap();
}

#[test]
fn websocket_thread_fork_commitment_transport_loss_is_indeterminate() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("thread/fork"));
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let error = client
        .fork_thread_with_commitment("thread_source", Duration::from_secs(2))
        .expect_err("transport loss after fork dispatch must be indeterminate");
    let ThreadForkFailure::Indeterminate { source } = error else {
        panic!("expected indeterminate thread/fork failure");
    };
    assert!(matches!(
        source,
        ManagedBackendError::WebSocketTransport { ref method, .. } if method == "thread/fork"
    ));

    server.join().unwrap();
}

#[test]
fn websocket_thread_fork_commitment_malformed_response_is_indeterminate() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("thread/fork"));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": { "thread": { "id": "unparseable_child" } }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let error = client
        .fork_thread_with_commitment("thread_source", Duration::from_secs(2))
        .expect_err("an unparseable fork result cannot identify a child safely");
    let ThreadForkFailure::Indeterminate { source } = error else {
        panic!("expected indeterminate thread/fork failure");
    };
    assert!(matches!(
        source,
        ManagedBackendError::DeserializeResponse { ref method, .. } if method == "thread/fork"
    ));

    server.join().unwrap();
}

#[test]
fn websocket_dynamic_tool_call_request_streams_and_response_serializes() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": "request_1",
                    "method": "item/tool/call",
                    "params": {
                        "threadId": "thread_1",
                        "turnId": "turn_1",
                        "callId": "call_1",
                        "namespace": "beryl",
                        "tool": "read_checklist",
                        "arguments": {
                            "nodeId": "node_1"
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();

        let response = read_json(&mut socket);
        assert_eq!(
            response,
            json!({
                "jsonrpc": "2.0",
                "id": "request_1",
                "result": {
                    "success": true,
                    "contentItems": [
                        {
                            "type": "inputText",
                            "text": "{\"ok\":true}"
                        }
                    ]
                }
            })
        );
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let event = client
        .next_turn_stream_event(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    let TurnStreamEvent::DynamicToolCallRequested(request) = event else {
        panic!("expected dynamic tool call request");
    };
    assert_eq!(request.thread_id(), "thread_1");
    assert_eq!(request.turn_id(), "turn_1");
    assert_eq!(request.call_id(), "call_1");
    assert_eq!(request.namespace(), Some("beryl"));
    assert_eq!(request.tool(), "read_checklist");
    assert_eq!(request.arguments(), &json!({ "nodeId": "node_1" }));

    client
        .respond_dynamic_tool_call(
            &request,
            &DynamicToolCallResponse::success_text("{\"ok\":true}"),
        )
        .unwrap();

    server.join().unwrap();
}

#[test]
fn websocket_dynamic_tool_call_request_defers_while_waiting_for_response() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/list"));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": "request_1",
                    "method": "item/tool/call",
                    "params": {
                        "threadId": "thread_1",
                        "turnId": "turn_1",
                        "callId": "call_1",
                        "tool": "read_workspace_graph_summary",
                        "arguments": {}
                    }
                })
                .to_string(),
            ))
            .unwrap();
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "data": []
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = client
        .list_thread_page(&ThreadListOptions::page(1), Duration::from_secs(2))
        .unwrap();
    assert!(response.data.is_empty());

    let event = client
        .next_turn_stream_event(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    let TurnStreamEvent::DynamicToolCallRequested(request) = event else {
        panic!("expected deferred dynamic tool call request");
    };
    assert_eq!(request.tool(), "read_workspace_graph_summary");

    server.join().unwrap();
}

#[test]
fn websocket_notification_defers_while_waiting_for_response() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/list"));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "method": "thread/status/changed",
                    "params": {
                        "threadId": "thread_1",
                        "status": { "type": "active", "activeFlags": [] }
                    }
                })
                .to_string(),
            ))
            .unwrap();
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "data": []
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = client
        .list_thread_page(&ThreadListOptions::page(1), Duration::from_secs(2))
        .unwrap();
    assert!(response.data.is_empty());

    let event = client
        .next_turn_stream_event(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    assert_eq!(
        event,
        TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_1".to_string(),
            status: ThreadStatus::Active {
                active_flags: Vec::new()
            }
        }
    );

    server.join().unwrap();
}

#[test]
fn websocket_turn_steer_serializes_expected_turn_and_ordered_user_input() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("turn/steer"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_1",
                "expectedTurnId": "turn_1",
                "input": [
                    {
                        "type": "text",
                        "text": "First steering fragment"
                    },
                    {
                        "type": "text",
                        "text": "Second steering fragment"
                    }
                ]
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "turnId": "turn_1"
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = client
        .steer_turn_with_user_input(
            "thread_1",
            "turn_1",
            vec![
                UserInput::text("First steering fragment"),
                UserInput::text("Second steering fragment"),
            ],
            Duration::from_secs(2),
        )
        .unwrap();

    assert_eq!(response.turn_id, "turn_1");
    server.join().unwrap();
}

#[test]
fn websocket_hard_stop_requests_serialize_exact_backend_handles() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("turn/interrupt"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_parent",
                "turnId": "turn_parent"
            })
        );
        socket
            .send(Message::text(
                json!({ "jsonrpc": "2.0", "id": 2, "result": {} }).to_string(),
            ))
            .unwrap();

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(3));
        assert_eq!(request["method"], json!("command/exec/terminate"));
        assert_eq!(
            request["params"],
            json!({
                "processId": "proc_123"
            })
        );
        socket
            .send(Message::text(
                json!({ "jsonrpc": "2.0", "id": 3, "result": {} }).to_string(),
            ))
            .unwrap();

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(4));
        assert_eq!(request["method"], json!("thread/backgroundTerminals/clean"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_parent"
            })
        );
        socket
            .send(Message::text(
                json!({ "jsonrpc": "2.0", "id": 4, "result": {} }).to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    client
        .interrupt_turn("thread_parent", "turn_parent", Duration::from_secs(2))
        .unwrap();
    client
        .terminate_command_execution("proc_123", Duration::from_secs(2))
        .unwrap();
    client
        .clean_thread_background_terminals("thread_parent", Duration::from_secs(2))
        .unwrap();
    server.join().unwrap();
}

#[test]
fn websocket_hard_stop_turn_target_interrupts_exact_child_turn() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("turn/interrupt"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_child",
                "turnId": "turn_child"
            })
        );
        socket
            .send(Message::text(
                json!({ "jsonrpc": "2.0", "id": 2, "result": {} }).to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let outcome = client.request_hard_stop_target(
        &HardStopTarget::turn("thread_child", "turn_child"),
        Duration::from_secs(2),
    );
    assert!(outcome.is_success());
    server.join().unwrap();
}

#[test]
fn websocket_hard_stop_target_outcome_preserves_failed_target() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("command/exec/terminate"));
        assert_eq!(request["params"], json!({ "processId": "proc_missing" }));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "error": {
                        "code": -32000,
                        "message": "command exec process not found"
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let target = HardStopTarget::command_execution("proc_missing");
    let outcome = client.request_hard_stop_target(&target, Duration::from_secs(2));
    let HardStopTargetOutcome::Failed {
        target,
        method,
        message,
    } = outcome
    else {
        panic!("expected failed hard-stop target outcome");
    };

    assert_eq!(target, HardStopTarget::command_execution("proc_missing"));
    assert_eq!(method, "command/exec/terminate");
    assert!(message.contains("command exec process not found"));
    server.join().unwrap();
}

#[test]
fn websocket_hard_stop_capability_probe_reports_optional_method_support() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("command/exec/terminate"));
        assert_eq!(
            request["params"],
            json!({
                "processId": "beryl-hard-stop-probe"
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "error": {
                        "code": -32000,
                        "message": "command exec process not found"
                    }
                })
                .to_string(),
            ))
            .unwrap();

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(3));
        assert_eq!(request["method"], json!("thread/backgroundTerminals/clean"));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 3,
                    "error": {
                        "code": -32601,
                        "message": "method not found"
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let report = client
        .probe_hard_stop_capabilities(Duration::from_secs(2))
        .unwrap();
    assert_eq!(report.probe_results().len(), 2);
    assert_eq!(
        report.probe_results()[0].probe(),
        HardStopCapabilityProbe::CommandExecTerminate
    );
    assert!(report.probe_results()[0].supported());
    assert_eq!(
        report.probe_results()[1].probe(),
        HardStopCapabilityProbe::ThreadBackgroundTerminalsClean
    );
    assert!(!report.probe_results()[1].supported());
    assert!(report.capabilities().command_exec_terminate());
    assert!(!report.capabilities().thread_background_terminals_clean());
    server.join().unwrap();
}

#[test]
fn websocket_thread_branch_capability_probe_reports_optional_method_support() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/fork"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "00000000-0000-0000-0000-000000000000"
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "error": {
                        "code": -32600,
                        "message": "no rollout found for thread id"
                    }
                })
                .to_string(),
            ))
            .unwrap();

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(3));
        assert_eq!(request["method"], json!("thread/rollback"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "00000000-0000-0000-0000-000000000000",
                "numTurns": 1
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 3,
                    "error": {
                        "code": -32601,
                        "message": "method not found"
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let report = client
        .probe_thread_branch_capabilities(Duration::from_secs(2))
        .unwrap();
    assert_eq!(report.probe_results().len(), 2);
    assert_eq!(
        report.probe_results()[0].probe(),
        ThreadBranchCapabilityProbe::ThreadFork
    );
    assert!(report.probe_results()[0].supported());
    assert_eq!(
        report.probe_results()[1].probe(),
        ThreadBranchCapabilityProbe::ThreadRollback
    );
    assert!(!report.probe_results()[1].supported());
    assert!(report.capabilities().thread_fork());
    assert!(!report.capabilities().thread_rollback());
    assert!(!report.capabilities().thread_branching());
    server.join().unwrap();
}

#[test]
fn websocket_turn_steer_preserves_non_steerable_request_error() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("turn/steer"));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "error": {
                        "code": -32000,
                        "message": "active turn cannot be steered",
                        "data": {
                            "codexErrorInfo": {
                                "activeTurnNotSteerable": {
                                    "turnKind": "review"
                                }
                            }
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let error = client
        .steer_turn_with_user_input(
            "thread_1",
            "turn_1",
            vec![UserInput::text("Steer this")],
            Duration::from_secs(2),
        )
        .unwrap_err();

    let ManagedBackendError::RequestFailed { method, error } = error else {
        panic!("expected turn/steer request failure");
    };
    assert_eq!(method, "turn/steer");
    assert_eq!(error.code, -32000);
    assert_eq!(error.message, "active turn cannot be steered");
    assert_eq!(
        active_turn_not_steerable_error(&error).unwrap().turn_kind,
        NonSteerableTurnKind::Review
    );
    server.join().unwrap();
}

#[test]
fn websocket_clients_initialize_independently_and_start_request_ids_at_one() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = accept_hdr(
                stream,
                |request: &tungstenite::handshake::server::Request, response| {
                    assert_eq!(
                        request
                            .headers()
                            .get("authorization")
                            .unwrap()
                            .to_str()
                            .unwrap(),
                        "Bearer test-token"
                    );
                    Ok(response)
                },
            )
            .unwrap();
            expect_initialize(&mut socket, 1);
            expect_initialized(&mut socket);
            let request = read_json(&mut socket);
            assert_eq!(request["id"], json!(2));
            assert_eq!(request["method"], json!("model/list"));
            socket
                .send(Message::text(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 2,
                        "result": {
                            "data": []
                        }
                    })
                    .to_string(),
                ))
                .unwrap();
        }
    });

    for _ in 0..2 {
        let launch = websocket_test_launch(endpoint.clone());
        let mut client = ManagedBackendSession::connect_websocket(
            launch,
            endpoint.clone(),
            "Bearer test-token".to_string(),
            Duration::from_secs(2),
        )
        .unwrap();
        let models = client
            .list_model_page(&ModelListOptions::page(1), Duration::from_secs(2))
            .unwrap();
        assert!(models.data.is_empty());
    }

    server.join().unwrap();
}

#[test]
fn websocket_request_only_client_initializes_with_notification_opt_outs() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        let request = read_json(&mut socket);
        assert_eq!(request["jsonrpc"], json!("2.0"));
        assert_eq!(request["id"], json!(1));
        assert_eq!(request["method"], json!("initialize"));
        assert_eq!(
            request["params"]["capabilities"]["experimentalApi"],
            json!(true)
        );

        let opt_out_methods = request["params"]["capabilities"]["optOutNotificationMethods"]
            .as_array()
            .unwrap();
        assert!(
            opt_out_methods
                .iter()
                .any(|method| method.as_str() == Some("thread/started"))
        );
        assert!(
            opt_out_methods
                .iter()
                .any(|method| method.as_str() == Some("thread/deleted"))
        );
        assert!(
            opt_out_methods
                .iter()
                .any(|method| method.as_str() == Some("item/completed"))
        );
        assert!(
            opt_out_methods
                .iter()
                .any(|method| method.as_str() == Some("error"))
        );

        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": {
                        "userAgent": "codex-cli 0.128.0",
                        "codexHome": "C:/Users/example/.codex",
                        "platformFamily": "windows",
                        "platformOs": "windows"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        expect_initialized(&mut socket);
    });

    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket_with_options(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        ManagedBackendClientOptions::request_only(),
        Duration::from_secs(2),
    )
    .unwrap();
    client.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn websocket_thread_read_metadata_uses_metadata_only_request_and_normalizes_nickname() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/read"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_child",
                "includeTurns": false
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "thread": {
                            "cliVersion": "0.128.0",
                            "createdAt": 1,
                            "cwd": "C:/work/beryl",
                            "ephemeral": false,
                            "id": "thread_child",
                            "modelProvider": "openai",
                            "preview": "",
                            "source": {
                                "subAgent": {
                                    "thread_spawn": {
                                        "agent_nickname": "Curie"
                                    }
                                }
                            },
                            "status": {
                                "type": "notLoaded"
                            },
                            "updatedAt": 2
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });

    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let summary = client
        .read_thread_metadata("thread_child", Duration::from_secs(2))
        .unwrap();
    assert_eq!(summary.id, "thread_child");
    assert_eq!(summary.agent_nickname.as_deref(), Some("Curie"));

    client.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn websocket_thread_read_metadata_details_preserve_runtime_metadata_when_exposed() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/read"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_child",
                "includeTurns": false
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "model": "gpt-5.5",
                        "modelProvider": "openai",
                        "reasoningEffort": "xhigh",
                        "thread": {
                            "agentNickname": "Curie",
                            "cliVersion": "0.128.0",
                            "createdAt": 1,
                            "cwd": "C:/work/beryl",
                            "ephemeral": false,
                            "id": "thread_child",
                            "modelProvider": "openai",
                            "preview": "",
                            "source": "subAgent",
                            "status": {
                                "type": "notLoaded"
                            },
                            "updatedAt": 2
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });

    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let metadata = client
        .read_thread_metadata_details("thread_child", Duration::from_secs(2))
        .unwrap();
    assert_eq!(metadata.thread.id, "thread_child");
    assert_eq!(metadata.thread.agent_nickname.as_deref(), Some("Curie"));
    assert_eq!(metadata.session_metadata.model.as_deref(), Some("gpt-5.5"));
    assert_eq!(
        metadata.session_metadata.reasoning_effort.as_deref(),
        Some("xhigh")
    );

    client.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn websocket_account_rate_limits_read_uses_null_params_and_deserializes_multi_bucket_view() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("account/rateLimits/read"));
        assert_eq!(request["params"], Value::Null);
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "rateLimits": {
                            "primary": {
                                "usedPercent": 55,
                                "windowDurationMins": 10080
                            }
                        },
                        "rateLimitsByLimitId": {
                            "codex": {
                                "limitId": "codex",
                                "limitName": "Codex",
                                "primary": {
                                    "usedPercent": 15,
                                    "windowDurationMins": 1440
                                },
                                "secondary": {
                                    "usedPercent": 55,
                                    "windowDurationMins": 10080
                                }
                            }
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });

    let launch = websocket_test_launch(endpoint.clone());
    let mut client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = client
        .read_account_rate_limits(Duration::from_secs(2))
        .unwrap();
    assert_eq!(
        response.rate_limits.primary.unwrap().window_duration_mins,
        Some(10080)
    );
    let by_limit_id = response.rate_limits_by_limit_id.unwrap();
    let codex = by_limit_id.get("codex").unwrap();
    assert_eq!(codex.limit_id.as_deref(), Some("codex"));
    assert_eq!(codex.limit_name.as_deref(), Some("Codex"));
    assert_eq!(codex.primary.as_ref().unwrap().used_percent, 15);

    client.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn compatibility_probe_responses_deserialize_from_observed_shapes() {
    let initialize: InitializeResponse = serde_json::from_value(json!({
        "userAgent": "codex-cli 0.118.0",
        "codexHome": "C:/Users/example/.codex",
        "platformFamily": "windows",
        "platformOs": "windows"
    }))
    .unwrap();

    let thread_list: ThreadListResponse = serde_json::from_value(json!({
        "data": [
            {
                "id": "thread_123",
                "cwd": "C:/work/beryl",
                "preview": "hello world",
                "createdAt": 1,
                "updatedAt": 2,
                "modelProvider": "openai",
                "ephemeral": false
            }
        ],
        "nextCursor": "cursor_1",
        "backwardsCursor": "cursor_0"
    }))
    .unwrap();

    let loaded_threads: ThreadLoadedListResponse = serde_json::from_value(json!({
        "data": ["thread_123"],
        "nextCursor": null
    }))
    .unwrap();

    let models: ModelListResponse = serde_json::from_value(json!({
        "data": [
            {
                "id": "gpt-5.5",
                "model": "gpt-5.5",
                "displayName": "GPT-5.5",
                "description": "Frontier model",
                "hidden": false,
                "supportedReasoningEfforts": [
                    {
                        "reasoningEffort": "low",
                        "description": "Fast responses with lighter reasoning"
                    },
                    {
                        "reasoningEffort": "medium",
                        "description": "Balances speed and reasoning depth"
                    },
                    {
                        "reasoningEffort": "high",
                        "description": "Greater reasoning depth"
                    },
                    {
                        "reasoningEffort": "xhigh",
                        "description": "Extra high reasoning depth"
                    }
                ],
                "defaultReasoningEffort": "medium",
                "inputModalities": ["text", "image"],
                "supportsPersonality": true,
                "additionalSpeedTiers": ["priority", "fast"],
                "isDefault": true
            }
        ],
        "nextCursor": "model_cursor"
    }))
    .unwrap();

    let config: ConfigReadResponse = serde_json::from_value(json!({
        "config": {
            "model": "gpt-5.5",
            "model_reasoning_effort": "xhigh"
        },
        "origins": {}
    }))
    .unwrap();

    assert_eq!(initialize.codex_home, "C:/Users/example/.codex");
    assert_eq!(thread_list.data.len(), 1);
    assert_eq!(thread_list.data[0].preview, "hello world");
    assert_eq!(thread_list.next_cursor.as_deref(), Some("cursor_1"));
    assert_eq!(thread_list.backwards_cursor.as_deref(), Some("cursor_0"));
    assert_eq!(loaded_threads.data, vec!["thread_123".to_string()]);
    assert_eq!(loaded_threads.next_cursor, None);
    assert_eq!(models.data.len(), 1);
    assert_eq!(models.data[0].id, "gpt-5.5");
    assert_eq!(models.data[0].display_name, "GPT-5.5");
    assert_eq!(
        models.data[0].supported_reasoning_efforts,
        vec![
            "low".to_string(),
            "medium".to_string(),
            "high".to_string(),
            "xhigh".to_string(),
        ]
    );
    assert_eq!(
        models.data[0].default_reasoning_effort.as_deref(),
        Some("medium")
    );
    assert_eq!(models.data[0].input_modalities, vec!["text", "image"]);
    assert!(models.data[0].supports_personality);
    assert!(models.data[0].is_default);
    assert_eq!(models.next_cursor.as_deref(), Some("model_cursor"));
    assert_eq!(config.config.model.as_deref(), Some("gpt-5.5"));
    assert_eq!(
        config.config.model_reasoning_effort.as_deref(),
        Some("xhigh")
    );
}

#[test]
fn model_list_deserializes_legacy_reasoning_effort_strings() {
    let models: ModelListResponse = serde_json::from_value(json!({
        "data": [
            {
                "id": "gpt-5.4-mini",
                "model": "gpt-5.4-mini",
                "displayName": "GPT-5.4 Mini",
                "supportedReasoningEfforts": ["low", "medium"]
            }
        ]
    }))
    .unwrap();

    assert_eq!(
        models.data[0].supported_reasoning_efforts,
        vec!["low".to_string(), "medium".to_string()]
    );
}

#[test]
fn model_list_deserializes_reasoning_effort_maps() {
    let models: ModelListResponse = serde_json::from_value(json!({
        "data": [
            {
                "id": "gpt-5.5",
                "model": "gpt-5.5",
                "displayName": "GPT-5.5",
                "supportedReasoningEfforts": {
                    "low": {
                        "description": "Fast responses with lighter reasoning"
                    },
                    "medium": {
                        "description": "Balances speed and reasoning depth"
                    },
                    "high": {
                        "description": "Greater reasoning depth"
                    }
                },
                "defaultReasoningEffort": {
                    "reasoningEffort": "medium",
                    "description": "Balances speed and reasoning depth"
                }
            }
        ]
    }))
    .unwrap();

    let mut efforts = models.data[0].supported_reasoning_efforts.clone();
    efforts.sort();
    assert_eq!(
        efforts,
        vec!["high".to_string(), "low".to_string(), "medium".to_string()]
    );
    assert_eq!(
        models.data[0].default_reasoning_effort.as_deref(),
        Some("medium")
    );
}

#[test]
fn thread_list_options_serialize_filter_sort_and_page_controls() {
    let options = ThreadListOptions::page(25)
        .with_cursor("cursor_1")
        .with_cwd(PathBuf::from("C:/work/beryl"))
        .updated_descending();

    assert_eq!(
        serde_json::to_value(options).unwrap(),
        json!({
            "cursor": "cursor_1",
            "limit": 25,
            "cwd": ["C:/work/beryl"],
            "sortKey": "updated_at",
            "sortDirection": "desc"
        })
    );

    assert_eq!(
        serde_json::to_value(ThreadListOptions {
            sort_key: Some(ThreadSortKey::CreatedAt),
            sort_direction: Some(SortDirection::Asc),
            ..ThreadListOptions::default()
        })
        .unwrap(),
        json!({
            "sortKey": "created_at",
            "sortDirection": "asc"
        })
    );
}

#[test]
fn model_list_options_serialize_page_and_hidden_controls() {
    let options = ModelListOptions::page(25)
        .with_cursor("model_cursor")
        .include_hidden();

    assert_eq!(
        serde_json::to_value(options).unwrap(),
        json!({
            "cursor": "model_cursor",
            "limit": 25,
            "includeHidden": true
        })
    );

    assert_eq!(
        serde_json::to_value(ModelListOptions::default()).unwrap(),
        json!({})
    );
}

#[test]
fn config_read_options_serialize_cwd_and_layer_controls() {
    assert_eq!(
        serde_json::to_value(ConfigReadOptions::for_cwd(PathBuf::from("C:/work/beryl"))).unwrap(),
        json!({
            "cwd": "C:/work/beryl"
        })
    );

    assert_eq!(
        serde_json::to_value(
            ConfigReadOptions::for_cwd(PathBuf::from("C:/work/beryl")).include_layers()
        )
        .unwrap(),
        json!({
            "cwd": "C:/work/beryl",
            "includeLayers": true
        })
    );

    assert_eq!(
        serde_json::to_value(ConfigReadOptions::default()).unwrap(),
        json!({})
    );
}

#[test]
fn compatibility_snapshot_exposes_required_probes_and_runtime_validation() {
    let host_snapshot = CompatibilitySnapshot::from_initialize_response(&InitializeResponse {
        user_agent: "codex-cli 0.118.0".to_string(),
        codex_home: "C:/Users/example/.codex".to_string(),
        platform_family: "windows".to_string(),
        platform_os: "windows".to_string(),
    });

    assert_eq!(
        host_snapshot.required_method_probes(),
        &[
            CompatibilityProbe::ConfigRead,
            CompatibilityProbe::ModelList,
            CompatibilityProbe::ThreadList,
            CompatibilityProbe::ThreadCompactStart,
            CompatibilityProbe::ThreadLoadedList,
            CompatibilityProbe::ThreadNameSet,
            CompatibilityProbe::ThreadRead,
            CompatibilityProbe::ThreadResumeMetadata,
            CompatibilityProbe::ThreadUnsubscribe,
            CompatibilityProbe::ThreadTurnsList,
            CompatibilityProbe::TurnInterrupt,
            CompatibilityProbe::TurnSteer,
        ]
    );
    assert_eq!(
        host_snapshot
            .required_method_probes()
            .iter()
            .map(|probe| probe.method())
            .collect::<Vec<_>>(),
        vec![
            "config/read",
            "model/list",
            "thread/list",
            "thread/compact/start",
            "thread/loaded/list",
            "thread/name/set",
            "thread/read",
            "thread/resume",
            "thread/unsubscribe",
            "thread/turns/list",
            "turn/interrupt",
            "turn/steer",
        ]
    );
    assert_eq!(
        host_snapshot.validate_runtime_mode(&RuntimeMode::HostWindows),
        Ok(())
    );
    assert!(matches!(
        host_snapshot.validate_runtime_mode(&RuntimeMode::WslLinux {
            distro_name: "Ubuntu".to_string()
        }),
        Err(CompatibilityError::PlatformFamilyMismatch {
            expected_platform_family: "unix",
            actual_platform_family,
            ..
        }) if actual_platform_family == "windows"
    ));

    let wsl_snapshot = CompatibilitySnapshot::from_initialize_response(&InitializeResponse {
        user_agent: "codex-cli 0.118.0".to_string(),
        codex_home: "/home/example/.codex".to_string(),
        platform_family: "unix".to_string(),
        platform_os: "linux".to_string(),
    });

    assert_eq!(
        wsl_snapshot.validate_runtime_mode(&RuntimeMode::WslLinux {
            distro_name: "Ubuntu".to_string()
        }),
        Ok(())
    );
}

#[test]
fn managed_backend_startup_progress_exposes_ordered_operator_steps() {
    assert_eq!(
        ManagedBackendStartupStage::ordered(),
        &[
            ManagedBackendStartupStage::LaunchProcess,
            ManagedBackendStartupStage::InitializeHandshake,
            ManagedBackendStartupStage::ValidateRuntime,
            ManagedBackendStartupStage::VerifyRequiredMethods,
            ManagedBackendStartupStage::Ready,
        ]
    );
    assert_eq!(
        ManagedBackendStartupStage::VerifyRequiredMethods.display_label(),
        "Verify required backend methods"
    );

    let progress = ManagedBackendStartupProgress::new(
        ManagedBackendStartupStage::VerifyRequiredMethods,
        Some("thread/list".to_string()),
    );

    assert_eq!(
        progress.stage(),
        ManagedBackendStartupStage::VerifyRequiredMethods
    );
    assert_eq!(progress.detail(), Some("thread/list"));
}

#[cfg(all(target_os = "windows", feature = "lifecycle-test-support"))]
#[test]
fn guarded_probe_returns_initialized_session_and_preserves_server_identity() {
    let (endpoint, fake_server) = spawn_fake_app_server_accepting_any_auth(|mut socket| {
        serve_successful_compatibility_probe(&mut socket);
    });
    let mut server = spawn_lifecycle_test_server(endpoint).unwrap();
    let process_id = server.process_id().expect("test server process id");
    let token_file = managed_server_token_file(&server);
    let mut progress = Vec::new();

    let (session, report) = server
        .connect_and_probe_with_progress(Duration::from_secs(2), |update| {
            progress.push((update.stage(), update.detail().map(str::to_owned)));
        })
        .expect("guarded probe should succeed");

    assert!(session.process_id().is_none());
    assert_eq!(server.process_id(), Some(process_id));
    assert!(server.is_process_alive());
    assert_eq!(report.method_successes().len(), 12);
    assert_eq!(
        progress.first().map(|(stage, _)| *stage),
        Some(ManagedBackendStartupStage::InitializeHandshake)
    );
    assert_eq!(
        progress.last().map(|(stage, _)| *stage),
        Some(ManagedBackendStartupStage::Ready)
    );
    assert_eq!(
        progress
            .iter()
            .filter(|(stage, _)| *stage == ManagedBackendStartupStage::VerifyRequiredMethods)
            .count(),
        12
    );

    drop(session);
    server.shutdown().expect("explicit shutdown should succeed");
    assert_token_file_absent(&token_file);
    wait_for_windows_process_exit(process_id, Duration::from_secs(2))
        .expect("managed test process should be reaped");
    fake_server.join().unwrap();
}

#[cfg(all(target_os = "windows", feature = "lifecycle-test-support"))]
#[test]
fn compound_probe_uses_canonical_orchestration_with_options_and_complete_progress() {
    let (endpoint, fake_server) = spawn_fake_app_server_accepting_any_auth(|mut socket| {
        serve_successful_compatibility_probe(&mut socket);
    });
    let exact_program = r"C:\fixture\codex.exe";
    let options = ManagedBackendLaunchOptions::with_exact_host_windows_program(exact_program)
        .expect("absolute fixture program should be accepted");
    let mut progress = Vec::new();

    let (mut server, session, report) = launch_and_probe_lifecycle_test_with_options(
        endpoint,
        RuntimeMode::HostWindows,
        r"C:\work\beryl",
        options,
        Duration::from_secs(2),
        |update| progress.push((update.stage(), update.detail().map(str::to_owned))),
    )
    .expect("canonical compound orchestration should succeed");

    let command_line = server.launch_spec().command_line().unwrap();
    assert_eq!(command_line.program(), exact_program);
    assert_eq!(command_line.cwd(), Some(&PathBuf::from(r"C:\work\beryl")));
    assert_eq!(
        server.launch_spec().runtime_mode(),
        &RuntimeMode::HostWindows
    );
    assert_eq!(server.launch_spec().cwd(), Path::new(r"C:\work\beryl"));
    assert!(session.process_id().is_none());
    assert_eq!(session.launch_spec(), server.launch_spec());
    assert_eq!(report.initialize().user_agent, "codex-cli 0.125.0");
    assert_eq!(report.initialize().codex_home, "C:/Users/example/.codex");
    assert_eq!(report.compatibility().platform_family(), "windows");
    assert_eq!(report.compatibility().platform_os(), "windows");
    assert!(report.compatibility().requires_method_probes());
    assert_eq!(report.config_defaults().model, None);
    assert_eq!(report.config_defaults().model_reasoning_effort, None);
    assert!(report.model_list().is_empty());
    assert_eq!(
        report
            .method_successes()
            .iter()
            .map(|success| success.probe().method())
            .collect::<Vec<_>>(),
        [
            "config/read",
            "model/list",
            "thread/list",
            "thread/compact/start",
            "thread/loaded/list",
            "thread/name/set",
            "thread/read",
            "thread/resume",
            "thread/unsubscribe",
            "thread/turns/list",
            "turn/interrupt",
            "turn/steer",
        ]
    );
    assert!(report.thread_branch_capabilities().thread_branching());
    assert_eq!(progress, expected_compound_probe_progress());

    let process_id = server.process_id().expect("test server process id");
    let token_file = managed_server_token_file(&server);
    drop(session);
    server.shutdown().expect("explicit shutdown should succeed");
    assert_token_file_absent(&token_file);
    wait_for_windows_process_exit(process_id, Duration::from_secs(2))
        .expect("managed test process should be reaped");
    fake_server.join().unwrap();
}

#[cfg(all(target_os = "windows", feature = "lifecycle-test-support"))]
#[test]
fn shutdown_combination_retains_process_and_auth_failures() {
    let process_error = ManagedBackendError::ShutdownTimeout {
        launch: "process-fixture".to_string(),
        timeout: Duration::from_secs(1),
    };
    let auth_error = ManagedBackendError::CleanUpWebSocketTokenFile {
        path: PathBuf::from(r"C:\fixture\token.txt"),
        source: std::io::Error::other("auth-fixture"),
    };

    let error = combine_lifecycle_test_shutdown_results(Err(process_error), Err(auth_error))
        .expect_err("two failures must remain a combined shutdown error");
    let display = error.to_string();
    let ManagedBackendError::ShutdownProcessAndAuth { process, auth } = error else {
        panic!("expected the typed combined shutdown error");
    };

    assert!(matches!(
        *process,
        ManagedBackendError::ShutdownTimeout { ref launch, .. } if launch == "process-fixture"
    ));
    assert!(matches!(
        *auth,
        ManagedBackendError::CleanUpWebSocketTokenFile { ref path, .. }
            if path == &PathBuf::from(r"C:\fixture\token.txt")
    ));
    assert!(display.contains("process-fixture"));
    assert!(display.contains(r"C:\fixture\token.txt"));
    assert!(display.contains("auth-fixture"));
}

#[cfg(all(target_os = "windows", feature = "lifecycle-test-support"))]
#[test]
fn guarded_probe_connect_failure_retains_server_for_explicit_cleanup() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    drop(listener);

    let mut server = spawn_lifecycle_test_server(endpoint).unwrap();
    let process_id = server.process_id().expect("test server process id");
    let token_file = managed_server_token_file(&server);
    let error = server
        .connect_and_probe(Duration::from_millis(150))
        .expect_err("closed listener must fail the bounded connection");

    assert!(matches!(
        error,
        ManagedBackendError::ConnectWebSocket { .. }
    ));
    assert_eq!(server.process_id(), Some(process_id));
    assert!(server.is_process_alive());
    server.shutdown().expect("explicit shutdown should succeed");
    assert_token_file_absent(&token_file);
    wait_for_windows_process_exit(process_id, Duration::from_secs(2))
        .expect("managed test process should be reaped");
}

#[cfg(all(target_os = "windows", feature = "lifecycle-test-support"))]
#[test]
fn guarded_probe_initialize_failure_retains_server_for_explicit_cleanup() {
    let (endpoint, fake_server) = spawn_fake_app_server_accepting_any_auth(|mut socket| {
        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("initialize"));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": {
                        "userAgent": "codex-cli incompatible",
                        "codexHome": "/home/example/.codex",
                        "platformFamily": "unix",
                        "platformOs": "linux"
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let mut server = spawn_lifecycle_test_server(endpoint).unwrap();
    let process_id = server.process_id().expect("test server process id");
    let token_file = managed_server_token_file(&server);

    let error = server
        .connect_and_probe(Duration::from_secs(2))
        .expect_err("runtime mismatch must fail initialize compatibility");

    assert!(matches!(error, ManagedBackendError::Compatibility(_)));
    assert_eq!(server.process_id(), Some(process_id));
    server.shutdown().expect("explicit shutdown should succeed");
    assert_token_file_absent(&token_file);
    wait_for_windows_process_exit(process_id, Duration::from_secs(2))
        .expect("managed test process should be reaped");
    fake_server.join().unwrap();
}

#[cfg(all(target_os = "windows", feature = "lifecycle-test-support"))]
#[test]
fn guarded_probe_required_method_failure_retains_server_for_explicit_cleanup() {
    let (endpoint, fake_server) = spawn_fake_app_server_accepting_any_auth(|mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);
        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("config/read"));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "error": { "code": -32601, "message": "method not found" }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let mut server = spawn_lifecycle_test_server(endpoint).unwrap();
    let process_id = server.process_id().expect("test server process id");
    let token_file = managed_server_token_file(&server);

    let error = server
        .connect_and_probe(Duration::from_secs(2))
        .expect_err("required method rejection must fail compatibility");

    assert!(matches!(
        error,
        ManagedBackendError::RequestFailed { ref method, .. } if method == "config/read"
    ));
    assert_eq!(server.process_id(), Some(process_id));
    server.shutdown().expect("explicit shutdown should succeed");
    assert_token_file_absent(&token_file);
    wait_for_windows_process_exit(process_id, Duration::from_secs(2))
        .expect("managed test process should be reaped");
    fake_server.join().unwrap();
}

fn websocket_test_launch(endpoint: BackendWebSocketEndpoint) -> BackendLaunchSpec {
    BackendLaunchSpec::managed_websocket(
        RuntimeMode::HostWindows,
        r"C:\work\beryl",
        endpoint,
        r"C:\tmp\beryl-token.txt",
    )
}

fn spawn_fake_app_server<F>(
    expected_auth: &'static str,
    handler: F,
) -> (BackendWebSocketEndpoint, thread::JoinHandle<()>)
where
    F: FnOnce(WebSocket<TcpStream>) + Send + 'static,
{
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let socket = accept_hdr(
            stream,
            |request: &tungstenite::handshake::server::Request, response| {
                assert_eq!(
                    request
                        .headers()
                        .get("authorization")
                        .unwrap()
                        .to_str()
                        .unwrap(),
                    expected_auth
                );
                Ok(response)
            },
        )
        .unwrap();
        handler(socket);
    });

    (endpoint, server)
}

#[cfg(all(target_os = "windows", feature = "lifecycle-test-support"))]
fn spawn_fake_app_server_accepting_any_auth<F>(
    handler: F,
) -> (BackendWebSocketEndpoint, thread::JoinHandle<()>)
where
    F: FnOnce(WebSocket<TcpStream>) + Send + 'static,
{
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let socket = accept_hdr(
            stream,
            |request: &tungstenite::handshake::server::Request, response| {
                assert!(request.headers().get("authorization").is_some());
                Ok(response)
            },
        )
        .unwrap();
        handler(socket);
    });

    (endpoint, server)
}

#[cfg(all(target_os = "windows", feature = "lifecycle-test-support"))]
fn managed_server_token_file(server: &ManagedBackendServer) -> PathBuf {
    let BackendTransport::ManagedWebSocket(config) = server.launch_spec().transport() else {
        panic!("managed server must retain a managed WebSocket launch spec");
    };
    config.backend_token_file_path().to_path_buf()
}

#[cfg(all(target_os = "windows", feature = "lifecycle-test-support"))]
fn serve_successful_compatibility_probe(socket: &mut WebSocket<TcpStream>) {
    expect_initialize(socket, 1);
    expect_initialized(socket);

    for request_id in 2..=15 {
        let request = read_json(socket);
        let result = match request["method"].as_str().unwrap() {
            "config/read" => Some(json!({ "config": {} })),
            "model/list" | "thread/list" | "thread/loaded/list" => Some(json!({ "data": [] })),
            "thread/unsubscribe" => Some(json!({ "status": "unsubscribed" })),
            _ => None,
        };
        let response = match result {
            Some(result) => json!({ "jsonrpc": "2.0", "id": request_id, "result": result }),
            None => json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "error": { "code": -32000, "message": "probe target does not exist" }
            }),
        };
        socket.send(Message::text(response.to_string())).unwrap();
    }
}

#[cfg(all(target_os = "windows", feature = "lifecycle-test-support"))]
fn expected_compound_probe_progress() -> Vec<(ManagedBackendStartupStage, Option<String>)> {
    let mut progress = vec![
        (ManagedBackendStartupStage::LaunchProcess, None),
        (ManagedBackendStartupStage::InitializeHandshake, None),
        (ManagedBackendStartupStage::ValidateRuntime, None),
    ];
    for method in [
        "config/read",
        "model/list",
        "thread/list",
        "thread/compact/start",
        "thread/loaded/list",
        "thread/name/set",
        "thread/read",
        "thread/resume",
        "thread/unsubscribe",
        "thread/turns/list",
        "turn/interrupt",
        "turn/steer",
    ] {
        progress.push((
            ManagedBackendStartupStage::VerifyRequiredMethods,
            Some(method.to_string()),
        ));
    }
    progress.push((ManagedBackendStartupStage::Ready, None));
    progress
}

#[cfg(all(target_os = "windows", feature = "lifecycle-test-support"))]
fn assert_token_file_absent(path: &Path) {
    match std::fs::metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("failed to verify token file {path:?} is absent: {error}"),
        Ok(_) => panic!("managed token file {path:?} remains after explicit shutdown"),
    }
}

#[cfg(all(target_os = "windows", feature = "lifecycle-test-support"))]
fn wait_for_windows_process_exit(process_id: u32, timeout: Duration) -> Result<(), String> {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if !windows_process_is_running(process_id)? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(25));
    }
    if windows_process_is_running(process_id)? {
        Err(format!("process {process_id} survived explicit shutdown"))
    } else {
        Ok(())
    }
}

#[cfg(all(target_os = "windows", feature = "lifecycle-test-support"))]
fn windows_process_is_running(process_id: u32) -> Result<bool, String> {
    let status = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "try {{ Get-Process -Id {process_id} -ErrorAction Stop | Out-Null; exit 0 }} catch {{ if ($_.FullyQualifiedErrorId -eq 'NoProcessFoundForGivenId,Microsoft.PowerShell.Commands.GetProcessCommand') {{ exit 1 }} [Console]::Error.WriteLine($_.Exception.Message); exit 2 }}"
            ),
        ])
        .status()
        .map_err(|error| format!("failed to query process {process_id}: {error}"))?;
    match status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        Some(code) => Err(format!(
            "process query for {process_id} exited with unexpected status {code}"
        )),
        None => Err(format!(
            "process query for {process_id} exited without a canonical status code"
        )),
    }
}

fn expect_initialize(socket: &mut WebSocket<TcpStream>, request_id: u64) {
    let request = read_json(socket);
    assert_eq!(request["jsonrpc"], json!("2.0"));
    assert_eq!(request["id"], json!(request_id));
    assert_eq!(request["method"], json!("initialize"));
    assert_eq!(request["params"]["clientInfo"]["name"], json!("beryl"));
    assert_eq!(
        request["params"]["capabilities"]["experimentalApi"],
        json!(true)
    );
    assert_thread_started_not_opted_out(&request);
    socket
        .send(Message::text(
            json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "result": {
                    "userAgent": "codex-cli 0.125.0",
                    "codexHome": "C:/Users/example/.codex",
                    "platformFamily": "windows",
                    "platformOs": "windows"
                }
            })
            .to_string(),
        ))
        .unwrap();
}

fn assert_thread_started_not_opted_out(request: &Value) {
    let opt_out_methods = request["params"]["capabilities"]
        .get("optOutNotificationMethods")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(
        opt_out_methods
            .iter()
            .all(|method| method.as_str() != Some("thread/started")),
        "initialize must not opt out of thread/started notifications"
    );
    assert!(
        opt_out_methods
            .iter()
            .all(|method| method.as_str() != Some("error")),
        "foreground initialize must not opt out of error notifications"
    );
}

fn expect_initialized(socket: &mut WebSocket<TcpStream>) {
    let notification = read_json(socket);
    assert_eq!(notification["jsonrpc"], json!("2.0"));
    assert_eq!(notification["method"], json!("initialized"));
    assert!(notification.get("id").is_none());
}

fn read_json(socket: &mut WebSocket<TcpStream>) -> Value {
    loop {
        match socket.read().unwrap() {
            Message::Text(text) => return serde_json::from_str(text.as_str()).unwrap(),
            Message::Ping(_) | Message::Pong(_) => {}
            Message::Close(frame) => panic!("websocket closed before JSON message: {frame:?}"),
            other => panic!("expected websocket text JSON message, got {other:?}"),
        }
    }
}
