use super::*;

const THREAD: &str = "managed-execution-thread";

pub(super) fn serve(socket: &mut WebSocket<TcpStream>, queued_successor: bool) {
    let first_ordinal =
        u32::from(queued_successor && std::path::Path::new("execution-started-0.json").exists());
    for ordinal in first_ordinal..if queued_successor { 2 } else { 1 } {
        let Some(projection) = read_projection(socket) else {
            return;
        };
        assert_eq!(
            projection["method"],
            if ordinal == 0 {
                "thread/start"
            } else {
                "thread/resume"
            }
        );
        if ordinal != 0 {
            assert_eq!(projection["params"]["threadId"], THREAD);
        }
        project(socket, &projection, ordinal != 0);
        let request = read_json(socket);
        assert_eq!(request["method"], "turn/start");
        assert_eq!(request["params"]["threadId"], THREAD);
        let input = request["params"]["input"].as_array().unwrap();
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["type"], "text");
        let text = input[0]["text"].as_str().unwrap();
        let turn = format!("managed-turn-{ordinal}");
        for (method, field, timestamp) in [
            ("item/started", "startedAtMs", 37_001 + ordinal * 10),
            ("item/completed", "completedAtMs", 37_002 + ordinal * 10),
        ] {
            let mut params = json!({
                "item": {"type":"userMessage", "id":format!("managed-user-{ordinal}"), "clientId":null,
                    "content":[{"type":"text", "text":text, "text_elements":[]}]},
                "threadId":THREAD, "turnId":turn
            });
            params[field] = json!(timestamp);
            send_json(socket, json!({"method":method,"params":params}));
        }
        send_json(
            socket,
            json!({"id":request["id"],"result":{"turn":{"id":turn,"items":[],"status":"inProgress"}}}),
        );
        fs::write(
            format!("execution-started-{ordinal}.json"),
            serde_json::to_vec(&json!({
                "pid":std::process::id(), "thread":THREAD, "turn":turn, "text":text
            }))
            .unwrap(),
        )
        .unwrap();
        if queued_successor && ordinal == 0 {
            let steer = read_json(socket);
            assert_eq!(steer["method"], "turn/steer");
            assert_eq!(steer["params"]["threadId"], THREAD);
            let id = steer["id"].as_u64().unwrap();
            socket.send(Message::Text(format!(
                r#"{{"error":{{"code":-32600,"data":{{"message":"steering denied","codexErrorInfo":{{"activeTurnNotSteerable":{{"turnKind":"review"}}}},"additionalDetails":null}},"message":"outer steering diagnostic"}},"id":{id}}}"#,
            ).into())).unwrap();
            fs::write("execution-steering-denied", "ready").unwrap();
        }
        let release = format!("execution-release-{ordinal}");
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !std::path::Path::new(&release).exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "execution release timed out"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        send_json(
            socket,
            json!({"method":"turn/completed","params":{
                "threadId":THREAD, "turn":{"id":turn,"items":[],"itemsView":"notLoaded","status":"completed","error":null,
                    "startedAt":37_001 + ordinal * 10,"completedAt":37_002 + ordinal * 10,"durationMs":1}
            }}),
        );
        let unsubscribe = read_json(socket);
        assert_eq!(unsubscribe["method"], "thread/unsubscribe");
        assert_eq!(unsubscribe["params"]["threadId"], THREAD);
        send_json(
            socket,
            json!({"id":unsubscribe["id"],"result":{"status":"unsubscribed"}}),
        );
        fs::write(format!("execution-unsubscribed-{ordinal}"), "ready").unwrap();
    }
    while let Ok(message) = socket.read() {
        if message.is_close() {
            break;
        }
        assert!(
            !message.is_text(),
            "unexpected execution after final terminal capture"
        );
    }
}

fn read_projection(socket: &mut WebSocket<TcpStream>) -> Option<Value> {
    loop {
        match socket.read().unwrap() {
            Message::Text(text) => return Some(serde_json::from_str(text.as_str()).unwrap()),
            Message::Close(_) => return None,
            _ => {}
        }
    }
}

fn project(socket: &mut WebSocket<TcpStream>, request: &Value, resumed: bool) {
    let cwd = env::current_dir().unwrap();
    let mut response = json!({"id":request["id"],"result":{
        "thread":{"id":THREAD,"extra":null,"sessionId":"managed-session","forkedFromId":null,
            "parentThreadId":null,"preview":"","ephemeral":false,"historyMode":"legacy","modelProvider":"openai",
            "createdAt":1,"updatedAt":2,"recencyAt":null,"status":{"type":"idle"},"path":null,
            "cwd":cwd,"cliVersion":"0.146.0","source":"appServer","threadSource":null,
            "agentNickname":null,"agentRole":null,"gitInfo":null,"name":null,"turns":[]},
        "model":"fixture-model","modelProvider":"openai","serviceTier":null,"cwd":cwd,
        "runtimeWorkspaceRoots":[],"instructionSources":[],"approvalPolicy":"never","approvalsReviewer":"user",
        "sandbox":{},"activePermissionProfile":null,"reasoningEffort":null,"multiAgentMode":"explicitRequestOnly"
    }});
    if resumed {
        response["result"]["initialTurnsPage"] = Value::Null;
        response["result"]["turnsBackwardsCursor"] = Value::Null;
        response["result"]["itemsBackwardsCursor"] = Value::Null;
    }
    send_json(socket, response);
}
