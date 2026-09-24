use super::*;

pub(super) fn serve(socket: &mut WebSocket<TcpStream>) {
    while let Ok(message) = socket.read() {
        if message.is_close() {
            return;
        }
        if !message.is_text() {
            continue;
        }
        let request: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
        match request["method"].as_str().unwrap() {
            "thread/start" | "thread/resume" => {
                fs::write("handoff-projection-observed", "ready").unwrap();
                let resumed = request["method"] == "thread/resume";
                let thread_id = if resumed {
                    request["params"]["threadId"].clone()
                } else {
                    json!("prepared-handoff-thread")
                };
                let root = env::current_dir().unwrap().to_str().unwrap().to_owned();
                let mut result = json!({
                    "thread": {"id": thread_id, "extra": null, "sessionId": "prepared-session",
                        "forkedFromId": null, "parentThreadId": null, "preview": "", "ephemeral": false,
                        "historyMode": "legacy", "modelProvider": "openai", "createdAt": 1,
                        "updatedAt": 2, "recencyAt": null, "status": {"type": "idle"}, "path": null,
                        "cwd": root, "cliVersion": "0.146.0", "source": "appServer", "threadSource": null,
                        "agentNickname": null, "agentRole": null, "gitInfo": null, "name": null, "turns": []},
                    "model": "fixture-model", "modelProvider": "openai", "serviceTier": null,
                    "cwd": root, "runtimeWorkspaceRoots": [], "instructionSources": [], "approvalPolicy": "never",
                    "approvalsReviewer": "user", "sandbox": {}, "activePermissionProfile": null,
                    "reasoningEffort": null, "multiAgentMode": "explicitRequestOnly"
                });
                if resumed {
                    result["initialTurnsPage"] = Value::Null;
                    result["turnsBackwardsCursor"] = Value::Null;
                    result["itemsBackwardsCursor"] = Value::Null;
                }
                send_json(socket, json!({"id": request["id"], "result": result}));
            }
            "thread/unsubscribe" => send_json(
                socket,
                json!({"id": request["id"], "result": {"status": "unsubscribed"}}),
            ),
            "turn/start" => {
                fs::write("unexpected-handoff-dispatch", "dispatched").unwrap();
                panic!("runtime recovery must not retry the paused handoff");
            }
            method => panic!("unexpected paused-handoff method: {method}"),
        }
    }
}
