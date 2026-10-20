#![cfg(all(feature = "test-faults", target_os = "windows"))]

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, client::IntoClientRequest};

#[path = "managed_discussion_fixture/decoded.rs"]
mod decoded;

struct RuntimeChild(Child);

impl Drop for RuntimeChild {
    fn drop(&mut self) {
        if self.0.try_wait().unwrap().is_none() {
            self.0.kill().unwrap();
            self.0.wait().unwrap();
        }
    }
}

fn send(socket: &mut WebSocket<TcpStream>, message: Value) {
    socket.send(Message::text(message.to_string())).unwrap();
}

fn receive(socket: &mut WebSocket<TcpStream>) -> Value {
    serde_json::from_str(socket.read().unwrap().into_text().unwrap().as_str()).unwrap()
}

fn connect(address: SocketAddr, root: &Path, request_only: bool) -> WebSocket<TcpStream> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let stream = loop {
        match TcpStream::connect_timeout(&address, Duration::from_millis(100)) {
            Ok(stream) => break stream,
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "fixture connection failed: {error}"
                );
                thread::sleep(Duration::from_millis(10));
            }
        }
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut request = format!("ws://{address}").into_client_request().unwrap();
    request.headers_mut().insert(
        "authorization",
        "Bearer discussion-fixture-token".parse().unwrap(),
    );
    let (mut socket, _) = tungstenite::client(request, stream).unwrap();
    let mut capabilities = json!({"experimentalApi":true});
    if request_only {
        capabilities["optOutNotificationMethods"] = json!(["thread/started"]);
    }
    send(
        &mut socket,
        json!({"id":1,"method":"initialize","params":{
            "clientInfo":{"name":"beryl"},"capabilities":capabilities
        }}),
    );
    assert_eq!(receive(&mut socket)["id"], 1);
    send(&mut socket, json!({"method":"initialized"}));
    send(
        &mut socket,
        json!({"id":2,"method":"config/read","params":{
            "cwd":root,"includeLayers":false
        }}),
    );
    assert_eq!(receive(&mut socket)["id"], 2);
    socket
}

fn publish(root: &Path, name: &str, bytes: &[u8]) {
    let destination = root.join(name);
    let pending = destination.with_extension("pending");
    fs::write(&pending, bytes).unwrap();
    fs::rename(pending, destination).unwrap();
}

fn start_fixture(root: &Path) -> (RuntimeChild, SocketAddr) {
    fs::write(root.join("fixture-mode"), "discussion-flow").unwrap();
    let token = "discussion-fixture-token";
    fs::write(root.join("token"), token).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let child = RuntimeChild(
        Command::new(env!("CARGO_BIN_EXE_managed-runtime-fixture"))
            .current_dir(root)
            .args([
                "app-server",
                "--strict-config",
                "-c",
                "features.multi_agent_v2={enabled=true,expose_spawn_agent_model_overrides=true}",
                "--ws-auth",
                "capability-token",
                "--listen",
                &format!("ws://{address}"),
                "--ws-token-file",
                "token",
                "--ws-token-sha256",
                &format!("{:x}", Sha256::digest(token)),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    (child, address)
}

#[test]
fn controlled_discussion_fixture_serves_repeated_model_queries_while_an_idle_turn_is_active() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().to_owned();
    let _directory = if let Some(ledger) = std::env::var_os("BERYL_NATIVE_RETAINED_FIXTURE_LEDGER")
    {
        use std::io::Write;
        writeln!(
            fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(ledger)
                .unwrap(),
            "{}",
            root.display()
        )
        .unwrap();
        let _ = directory.keep();
        None
    } else {
        Some(directory)
    };
    let (mut child, address) = start_fixture(&root);
    let mut warmup = connect(address, &root, false);
    let mut execution = connect(address, &root, false);
    send(
        &mut execution,
        json!({"id":3,"method":"thread/start","params":{"cwd":root}}),
    );
    let projection = receive(&mut execution);
    let thread = projection["result"]["thread"]["id"].as_str().unwrap();
    send(
        &mut execution,
        json!({"id":4,"method":"turn/start","params":{
            "threadId":thread,"input":[{"type":"text","text":"source question"}]
        }}),
    );
    assert_eq!(receive(&mut execution)["method"], "item/started");
    assert_eq!(receive(&mut execution)["method"], "item/completed");
    let started = receive(&mut execution);
    assert_eq!(started["id"], 4);
    let turn = started["result"]["turn"]["id"].as_str().unwrap();
    for _ in 0..17 {
        let mut query = connect(address, &root, true);
        send(
            &mut query,
            json!({"id":3,"method":"model/list","params":{"limit":64,"includeHidden":false}}),
        );
        assert_eq!(receive(&mut query)["result"]["data"], json!([]));
        send(
            &mut query,
            json!({"id":4,"method":"thread/resume","params":{
                "threadId":thread,"cwd":root,"excludeTurns":true
            }}),
        );
        assert_eq!(receive(&mut query)["result"]["thread"]["id"], thread);
        query.close(None).unwrap();
    }
    thread::sleep(Duration::from_millis(250));
    let passage = "Exact café 🌸 assistant passage";
    publish(
        &root,
        &format!("discussion-assistant-{turn}.json"),
        &serde_json::to_vec(&json!({"text":passage})).unwrap(),
    );
    publish(&root, &format!("discussion-complete-{turn}"), b"");
    for method in ["item/started", "item/completed"] {
        let event = receive(&mut execution);
        assert_eq!(event["method"], method);
        assert_eq!(event["params"]["item"]["text"], passage);
    }
    assert_eq!(receive(&mut execution)["method"], "turn/completed");
    let emitted: Value = serde_json::from_slice(
        &fs::read(root.join(format!("discussion-assistant-emitted-{turn}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(emitted["text"], passage);
    execution.close(None).unwrap();
    warmup.close(None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "fixture exited with {status}");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "fixture did not join its connections"
        );
        thread::sleep(Duration::from_millis(10));
    }
}
