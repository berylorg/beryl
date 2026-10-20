use super::*;
use std::{io, path::Path, time::Instant};

pub(super) fn serve_connection(stream: TcpStream, authorization: &str, connection: usize) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        super::serve_connection(stream, authorization, connection);
    }));
    if let Err(failure) = result {
        let message = failure
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| failure.downcast_ref::<&str>().copied())
            .unwrap_or("non-string fixture panic");
        let message: String = message.chars().take(4096).collect();
        publish(
            &format!(
                "discussion-failure-{}-{connection}.json",
                std::process::id()
            ),
            &serde_json::to_vec(&json!({"message":message})).unwrap(),
        );
        std::panic::resume_unwind(failure);
    }
}

pub(super) fn serve_queries(socket: &mut WebSocket<TcpStream>, connection: usize) {
    let mut ordinal = 0;
    loop {
        let text = match socket.read() {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(frame)) => {
                record_connection_close(
                    connection,
                    &None,
                    json!({"kind":"close-frame",
                    "frame":frame.map(|frame| json!({"code":u16::from(frame.code),"reason":frame.reason.as_str()}))}),
                );
                return;
            }
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                record_connection_close(connection, &None, json!({"kind":"closed"}));
                return;
            }
            result => panic!("unexpected discussion query frame: {result:?}"),
        };
        assert!(text.len() <= 1_000_000);
        assert!(ordinal < 8, "discussion query bound exceeded");
        let request: Value = serde_json::from_str(&text).unwrap();
        publish(
            &format!(
                "discussion-query-{}-{connection}-{ordinal}.json",
                std::process::id()
            ),
            text.as_bytes(),
        );
        ordinal += 1;
        let result = match request["method"].as_str().unwrap() {
            "config/read" => {
                assert_eq!(
                    request["params"]["cwd"].as_str(),
                    env::current_dir().unwrap().to_str()
                );
                assert_eq!(request["params"]["includeLayers"], false);
                json!({"config":{"model":null,"model_reasoning_effort":null,
                "features":{"multi_agent_v2":{"enabled":true,"expose_spawn_agent_model_overrides":true}}},
                "origins":{
                    "features.multi_agent_v2.enabled":{"name":{"type":"sessionFlags"},"version":"0"},
                    "features.multi_agent_v2.expose_spawn_agent_model_overrides":{"name":{"type":"sessionFlags"},"version":"0"}
                }})
            }
            "model/list" => {
                assert!(
                    request["params"]["limit"]
                        .as_u64()
                        .is_some_and(|limit| limit <= 64)
                );
                json!({"data":[],"nextCursor":null})
            }
            "thread/resume" => {
                assert_eq!(request["params"]["excludeTurns"], true);
                let thread = request["params"]["threadId"].as_str().unwrap();
                assert!(thread.len() <= 256);
                project(socket, &request, thread, true);
                continue;
            }
            other => panic!("unexpected discussion query: {other}"),
        };
        send_json(socket, json!({"id":request["id"],"result":result}));
    }
}

pub(super) fn serve(socket: &mut WebSocket<TcpStream>, connection: usize) {
    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(50);
    let mut active: Option<(String, String)> = None;
    let mut tool = None;
    let mut ordinal = 0;
    let mut turns = 0;
    while Instant::now() < deadline {
        if let Some((thread, turn)) = &active {
            let polling = format!("discussion-active-poll-{turn}");
            if !Path::new(&polling).exists() {
                publish(&polling, b"active");
            }
            let resolve = format!("discussion-resolve-{turn}.json");
            if tool.is_none() && Path::new(&resolve).exists() {
                let arguments: Value =
                    serde_json::from_slice(&fs::read(&resolve).unwrap()).unwrap();
                let id = format!("resolution-{turn}");
                let request = format!(
                    r#"{{"method":"item/tool/call","id":{id},"params":{{"threadId":{thread},"turnId":{turn},"callId":{id},"namespace":"beryl","tool":"resolve_branch_discussion","arguments":{arguments}}}}}"#,
                    id = serde_json::to_string(&id).unwrap(),
                    thread = serde_json::to_string(thread).unwrap(),
                    turn = serde_json::to_string(turn).unwrap(),
                );
                socket.send(Message::Text(request.into())).unwrap();
                tool = Some(id);
            }
            if Path::new(&format!("discussion-complete-{turn}")).exists() {
                publish(
                    &format!("discussion-completion-observed-{turn}"),
                    b"observed",
                );
                assert!(
                    tool.is_none(),
                    "resolution response must settle before terminal completion"
                );
                let assistant = format!("discussion-assistant-{turn}.json");
                if Path::new(&assistant).exists() {
                    let bytes = fs::read(&assistant).unwrap();
                    assert!(bytes.len() <= 100_000);
                    let value: Value = serde_json::from_slice(&bytes).unwrap();
                    let text = value["text"].as_str().unwrap();
                    assert!(text.len() <= 65_536);
                    let item = format!(
                        r#"{{"type":"agentMessage","id":{},"text":{},"phase":"final_answer"}}"#,
                        serde_json::to_string(&format!("assistant-{turn}")).unwrap(),
                        serde_json::to_string(text).unwrap(),
                    );
                    send_lifecycle_item(socket, thread, turn, &item, 37002);
                    publish(&format!("discussion-assistant-emitted-{turn}.json"), &bytes);
                }
                let terminal = format!(
                    r#"{{"method":"turn/completed","params":{{"threadId":{},"turn":{{"id":{},"items":[],"itemsView":"notLoaded","status":"completed","error":null,"startedAt":37001,"completedAt":37004,"durationMs":3}}}}}}"#,
                    serde_json::to_string(thread).unwrap(),
                    serde_json::to_string(turn).unwrap(),
                );
                socket.send(Message::Text(terminal.into())).unwrap();
                active = None;
            }
        }
        let message = match socket.read() {
            Ok(Message::Text(text)) => serde_json::from_str::<Value>(&text).unwrap(),
            Ok(Message::Close(frame)) => {
                record_connection_close(
                    connection,
                    &active,
                    json!({"kind":"close-frame",
                    "frame":frame.map(|frame| json!({"code":u16::from(frame.code),"reason":frame.reason.as_str()}))}),
                );
                return;
            }
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                record_connection_close(connection, &active, json!({"kind":"closed"}));
                return;
            }
            Ok(Message::Ping(payload)) => {
                socket.send(Message::Pong(payload)).unwrap();
                continue;
            }
            Ok(_) => continue,
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionReset | io::ErrorKind::UnexpectedEof
                ) =>
            {
                record_connection_close(
                    connection,
                    &active,
                    json!({"kind":"io","error":format!("{:?}",error.kind())}),
                );
                return;
            }
            Err(error) => panic!("discussion transport failed: {error}"),
        };
        assert!(ordinal < 32, "discussion request bound exceeded");
        let encoded = serde_json::to_vec(&message).unwrap();
        assert!(encoded.len() <= 1_000_000);
        publish(
            &format!(
                "discussion-request-{}-{connection}-{ordinal}.json",
                std::process::id()
            ),
            &encoded,
        );
        ordinal += 1;
        if message.get("method").is_none() {
            let id = tool
                .take()
                .expect("only the pending resolution may receive a response");
            assert_eq!(message["id"], id);
            let (_, turn) = active.as_ref().unwrap();
            fs::remove_file(format!("discussion-resolve-{turn}.json")).unwrap();
            publish(
                &format!("discussion-response-{turn}.json"),
                &serde_json::to_vec(&message).unwrap(),
            );
            continue;
        }
        match message["method"].as_str().unwrap() {
            "thread/start" | "thread/fork" | "thread/resume" => {
                assert!(active.is_none());
                let resumed = message["method"] == "thread/resume";
                let thread = if resumed {
                    message["params"]["threadId"].as_str().unwrap().to_owned()
                } else {
                    format!(
                        "discussion-thread-{}-{connection}-{ordinal}",
                        std::process::id()
                    )
                };
                project(socket, &message, &thread, resumed);
            }
            "thread/inject_items" => {
                assert!(active.is_none());
                send_json(socket, json!({"id":message["id"],"result":{}}));
            }
            "turn/start" => {
                assert!(active.is_none());
                assert!(turns < 8);
                let thread = message["params"]["threadId"].as_str().unwrap().to_owned();
                let turn = format!(
                    "discussion-turn-{}-{connection}-{turns}",
                    std::process::id()
                );
                turns += 1;
                let content = message["params"]["input"].as_array().unwrap();
                assert_eq!(content.len(), 1);
                assert_eq!(content[0]["type"], "text");
                let text = content[0]["text"].as_str().unwrap();
                if text.starts_with("Discussion resolution:\n\n")
                    && Path::new("discussion-reject-generated-once").exists()
                {
                    fs::remove_file("discussion-reject-generated-once").unwrap();
                    socket.send(Message::Text(format!(
                        r#"{{"error":{{"code":-32602,"message":"exact generated turn was not accepted"}},"id":{}}}"#,
                        message["id"],
                    ).into())).unwrap();
                    publish(
                        &format!("discussion-rejected-{turn}.json"),
                        &serde_json::to_vec(&message).unwrap(),
                    );
                    continue;
                }
                let item = format!(
                    r#"{{"type":"userMessage","id":{},"clientId":null,"content":[{{"type":"text","text":{},"text_elements":[]}}]}}"#,
                    serde_json::to_string(&format!("user-{turn}")).unwrap(),
                    serde_json::to_string(text).unwrap(),
                );
                send_lifecycle_item(socket, &thread, &turn, &item, 37001);
                send_json(
                    socket,
                    json!({"id":message["id"],"result":{"turn":{"id":turn,"items":[],"status":"inProgress"}}}),
                );
                publish(
                    &format!("discussion-started-{turn}.json"),
                    &serde_json::to_vec(&json!({"thread":thread,"turn":turn,"input":content}))
                        .unwrap(),
                );
                active = Some((thread, turn));
            }
            "turn/steer" => {
                assert!(active.is_some());
                socket.send(Message::Text(format!(
                    r#"{{"error":{{"code":-32600,"data":{{"message":"steering denied","codexErrorInfo":{{"activeTurnNotSteerable":{{"turnKind":"review"}}}},"additionalDetails":null}},"message":"outer steering diagnostic"}},"id":{}}}"#,
                    message["id"],
                ).into())).unwrap();
            }
            "thread/unsubscribe" => {
                assert!(active.is_none());
                send_json(
                    socket,
                    json!({"id":message["id"],"result":{"status":"unsubscribed"}}),
                );
            }
            other => panic!("unexpected discussion request: {other}"),
        }
    }
    panic!("discussion fixture lifetime exceeded");
}

fn send_lifecycle_item(
    socket: &mut WebSocket<TcpStream>,
    thread: &str,
    turn: &str,
    item: &str,
    started: u64,
) {
    let thread = serde_json::to_string(thread).unwrap();
    let turn = serde_json::to_string(turn).unwrap();
    for (method, field, timestamp) in [
        ("item/started", "startedAtMs", started),
        ("item/completed", "completedAtMs", started + 1),
    ] {
        let event = format!(
            r#"{{"method":"{method}","params":{{"item":{item},"threadId":{thread},"turnId":{turn},"{field}":{timestamp}}}}}"#,
        );
        socket.send(Message::Text(event.into())).unwrap();
    }
}

fn record_connection_close(connection: usize, active: &Option<(String, String)>, detail: Value) {
    publish(
        &format!("discussion-closed-{}-{connection}.json", std::process::id()),
        &serde_json::to_vec(&json!({"active":active.as_ref().map(|(_,turn)|turn),"detail":detail}))
            .unwrap(),
    );
}

fn publish(name: &str, bytes: &[u8]) {
    let pending = format!("{name}.pending");
    assert!(!Path::new(name).exists());
    fs::write(&pending, bytes).unwrap();
    fs::rename(pending, name).unwrap();
}

fn project(socket: &mut WebSocket<TcpStream>, request: &Value, thread: &str, resumed: bool) {
    let cwd = env::current_dir().unwrap();
    let mut result = json!({"thread":{"id":thread,"extra":null,"sessionId":format!("session-{thread}"),
        "forkedFromId":null,"parentThreadId":null,"preview":"","ephemeral":false,"historyMode":"legacy",
        "modelProvider":"openai","createdAt":1,"updatedAt":2,"recencyAt":null,"status":{"type":"idle"},
        "path":null,"cwd":cwd,"cliVersion":"0.146.0","source":"appServer","threadSource":null,
        "agentNickname":null,"agentRole":null,"gitInfo":null,"name":null,"turns":[]},
        "model":"fixture-model","modelProvider":"openai","serviceTier":null,"cwd":cwd,
        "runtimeWorkspaceRoots":[],"instructionSources":[],"approvalPolicy":"never","approvalsReviewer":"user",
        "sandbox":{},"activePermissionProfile":null,"reasoningEffort":null,"multiAgentMode":"explicitRequestOnly"});
    if resumed {
        for field in [
            "initialTurnsPage",
            "turnsBackwardsCursor",
            "itemsBackwardsCursor",
        ] {
            result[field] = Value::Null;
        }
    }
    send_json(socket, json!({"id":request["id"],"result":result}));
}
