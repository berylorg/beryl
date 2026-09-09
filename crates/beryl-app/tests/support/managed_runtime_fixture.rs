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

fn main() {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    assert_eq!(arguments.first().map(String::as_str), Some("app-server"));
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
    let (stream, _) = listener.accept().unwrap();
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
                == Some(authorization.as_str())
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
    let reject = mode == "reject-config";
    fs::write(
        "runtime-evidence.json",
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
    if mode == "pause-config" {
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
    while let Ok(message) = socket.read() {
        if message.is_close() {
            break;
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
