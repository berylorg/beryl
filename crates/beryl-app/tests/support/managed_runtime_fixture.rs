use std::{
    env, fs,
    net::{SocketAddr, TcpListener, TcpStream},
    time::Duration,
};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tungstenite::{
    Message, WebSocket, accept_hdr,
    handshake::server::{Request, Response},
};

#[path = "managed_runtime_fixture/execution.rs"]
mod execution;
#[path = "managed_runtime_fixture/paused_handoff.rs"]
mod paused_handoff;

fn main() {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let launch_form = match arguments.first().map(String::as_str) {
        Some("app-server") => "CodexCli",
        Some("--strict-config") => "StandaloneAppServer",
        other => panic!("unexpected launch form arguments: {other:?}"),
    };
    let subcommand_count = arguments
        .iter()
        .filter(|argument| argument.as_str() == "app-server")
        .count();
    assert_eq!(subcommand_count, usize::from(launch_form == "CodexCli"));
    fs::write(
        "runtime-launch-form.json",
        serde_json::to_vec(
            &json!({ "launch_form": launch_form, "app_server_subcommands": subcommand_count }),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        arguments
            .iter()
            .any(|argument| argument == "--strict-config")
    );
    assert_eq!(
        argument(&arguments, "-c"),
        "features.multi_agent_v2={enabled=true,expose_spawn_agent_model_overrides=true}"
    );
    assert_eq!(argument(&arguments, "--ws-auth"), "capability-token");
    let address: SocketAddr = argument(&arguments, "--listen")
        .strip_prefix("ws://")
        .unwrap()
        .parse()
        .unwrap();
    assert!(address.ip().is_loopback());
    let token = fs::read_to_string(argument(&arguments, "--ws-token-file")).unwrap();
    assert!(
        format!("{:x}", Sha256::digest(token.as_bytes()))
            == argument(&arguments, "--ws-token-sha256")
    );
    let authorization = format!("Bearer {token}");
    let listener = TcpListener::bind(address).unwrap();
    listener.set_nonblocking(true).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let mut connections: Vec<std::thread::JoinHandle<()>> = Vec::new();
    while connections.len() < 8 && connections.first().is_none_or(|first| !first.is_finished()) {
        assert!(
            std::time::Instant::now() < deadline,
            "fixture lifetime exceeded"
        );
        match listener.accept() {
            Ok((stream, _)) => {
                let authorization = authorization.clone();
                let index = connections.len();
                connections.push(std::thread::spawn(move || {
                    serve_connection(stream, &authorization, index);
                }));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("fixture accept failed: {error}"),
        }
    }
    for connection in connections {
        connection.join().unwrap();
    }
}

fn serve_connection(stream: TcpStream, authorization: &str, index: usize) {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut socket = accept_hdr(stream, |request: &Request, response: Response| {
        assert!(
            request
                .headers()
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                == Some(authorization)
        );
        Ok(response)
    })
    .unwrap();
    let initialize = read_json(&mut socket);
    assert_eq!(initialize["method"], "initialize");
    assert_eq!(initialize["params"]["clientInfo"]["name"], "beryl");
    assert_eq!(
        initialize["params"]["capabilities"]["experimentalApi"],
        true
    );
    assert!(
        initialize["params"]["capabilities"]
            .get("optOutNotificationMethods")
            .is_none()
    );
    send_json(
        &mut socket,
        json!({
            "id": initialize["id"],
            "result": {
                "userAgent": "beryl/0.146.0", "codexHome": "C:\\fixture-codex",
                "platformFamily": "windows", "platformOs": "windows"
            }
        }),
    );
    assert_eq!(read_json(&mut socket)["method"], "initialized");
    let config = read_json(&mut socket);
    assert_eq!(config["method"], "config/read");
    assert_eq!(
        config["params"]["cwd"].as_str(),
        env::current_dir().unwrap().to_str()
    );
    assert_eq!(config["params"]["includeLayers"], false);
    assert!(config["params"].get("threadId").is_none());
    let mode = fs::read_to_string("fixture-mode").unwrap_or_default();
    let reject = mode == "reject-config" || mode == "pause-reject-config";
    fs::write(
        if index == 0 {
            "runtime-evidence.json".to_owned()
        } else {
            format!("runtime-session-evidence-{index}.json")
        },
        serde_json::to_vec(&json!({
            "pid": std::process::id(),
            "authenticated": true,
            "foreground": true,
            "methods": ["initialize", "initialized", "config/read"],
            "rejected_config": reject
        }))
        .unwrap(),
    )
    .unwrap();
    if mode == "drop-config" {
        return;
    }
    if mode == "pause-config"
        || mode == "pause-reject-config"
        || (mode == "pause-session-config" && index > 0)
    {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !std::path::Path::new("release-config").exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "fixture config release timed out"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    send_json(
        &mut socket,
        json!({
            "id": config["id"],
            "result": {
            "config": {"model": null, "model_reasoning_effort": null, "features": {"multi_agent_v2": {
                    "enabled": !reject, "expose_spawn_agent_model_overrides": true
                }}},
                "origins": {
                    "features.multi_agent_v2.enabled": {"name": {"type": "sessionFlags"}, "version": "0"},
                    "features.multi_agent_v2.expose_spawn_agent_model_overrides": {"name": {"type": "sessionFlags"}, "version": "0"}
                }
            }
        }),
    );
    if mode == "paused-handoff" && index > 0 {
        paused_handoff::serve(&mut socket);
        return;
    }
    if matches!(
        mode.as_str(),
        "execution-lifetime"
            | "execution-next"
            | "execution-compaction"
            | "execution-compaction-shutdown"
            | "execution-shutdown"
    ) && index > 0
    {
        execution::serve(&mut socket, &mode);
        return;
    }
    if matches!(
        mode.as_str(),
        "pause-projection"
            | "projection-lifetime"
            | "projection-multiple-bindings"
            | "projection-reject-unsubscribe"
    ) && index > 0
    {
        let projection = read_json(&mut socket);
        assert_eq!(projection["method"], "thread/start");
        fs::write("runtime-projection-evidence.json", serde_json::to_vec(&json!({
            "pid": std::process::id(), "method": projection["method"], "cwd": projection["params"]["cwd"]
        })).unwrap()).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while mode == "pause-projection" && !std::path::Path::new("release-projection").exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "fixture projection release timed out"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        let response = json!({"id": projection["id"], "result": {
            "thread": {"id": "prepared-thread", "extra": null, "sessionId": "prepared-session", "forkedFromId": null,
                "parentThreadId": null, "preview": "", "ephemeral": false, "historyMode": "legacy", "modelProvider": "openai",
                "createdAt": 1, "updatedAt": 2, "recencyAt": null, "status": {"type": "idle"}, "path": null,
                "cwd": projection["params"]["cwd"], "cliVersion": "0.146.0", "source": "appServer", "threadSource": null,
                "agentNickname": null, "agentRole": null, "gitInfo": null, "name": null, "turns": []},
            "model": "fixture-model", "modelProvider": "openai", "serviceTier": null, "cwd": projection["params"]["cwd"],
            "runtimeWorkspaceRoots": [], "instructionSources": [], "approvalPolicy": "never", "approvalsReviewer": "user",
            "sandbox": {}, "activePermissionProfile": null, "reasoningEffort": null, "multiAgentMode": "explicitRequestOnly"
        }});
        let mut response = response;
        if mode == "projection-reject-unsubscribe" {
            response["result"]["thread"]["id"] =
                json!(format!("prepared-thread-{}-{index}", std::process::id()));
        }
        if mode == "projection-multiple-bindings" {
            let cwd = projection["params"]["cwd"].as_str().unwrap();
            let root = std::path::Path::new(cwd)
                .file_name()
                .unwrap()
                .to_string_lossy();
            response["result"]["thread"]["id"] = json!(format!("prepared-{root}"));
            fs::write(
                format!("runtime-projection-evidence-{index}.json"),
                serde_json::to_vec(&projection).unwrap(),
            )
            .unwrap();
        }
        let _ = socket.send(Message::Text(response.to_string().into()));
    }
    if mode == "reject-native-recovery" && index > 0 {
        let mut ordinal = 0;
        while let Ok(message) = socket.read() {
            if message.is_close() {
                break;
            }
            if !message.is_text() {
                continue;
            }
            let request: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
            fs::write(
                format!("runtime-native-request-{ordinal}.json"),
                serde_json::to_vec(&request).unwrap(),
            )
            .unwrap();
            ordinal += 1;
            assert_eq!(request["method"], "thread/resume");
            send_json(
                &mut socket,
                json!({"id": request["id"],
                "error": {"code": -32600, "message": "fixture native source unavailable"}}),
            );
        }
        return;
    }
    while let Ok(message) = socket.read() {
        if message.is_close() {
            break;
        }
        if matches!(
            mode.as_str(),
            "projection-lifetime"
                | "projection-multiple-bindings"
                | "projection-reject-unsubscribe"
        ) && message.is_text()
        {
            let request: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
            assert_eq!(request["method"], "thread/unsubscribe");
            if mode == "projection-reject-unsubscribe" {
                fs::write(
                    "runtime-unsubscribe-evidence.json",
                    serde_json::to_vec(&request).unwrap(),
                )
                .unwrap();
                send_json(
                    &mut socket,
                    json!({"id": request["id"], "error": {
                        "code": -32600, "message": "fixture unsubscribe unavailable"
                    }}),
                );
                std::process::exit(3);
            }
            send_json(
                &mut socket,
                json!({"id": request["id"], "result": {"status": "unsubscribed"}}),
            );
            continue;
        }
        assert!(
            !message.is_text(),
            "unexpected request after runtime admission"
        );
    }
}

fn argument<'a>(arguments: &'a [String], name: &str) -> &'a str {
    &arguments[arguments
        .iter()
        .position(|argument| argument == name)
        .unwrap()
        + 1]
}

fn read_json(socket: &mut WebSocket<TcpStream>) -> Value {
    serde_json::from_str(socket.read().unwrap().into_text().unwrap().as_str()).unwrap()
}

fn send_json(socket: &mut WebSocket<TcpStream>, value: Value) {
    socket
        .send(Message::text(serde_json::to_string(&value).unwrap()))
        .unwrap();
}
