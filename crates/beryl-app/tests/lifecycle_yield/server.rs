#![allow(dead_code)]

include!("../normal_terminal/server.rs");

enum YieldCommand {
    Call(&'static str),
    CallUntilConnectionClose(&'static str),
    ResolveBranch,
    Complete,
    Compact,
    LiveCompaction,
    Lose,
}

pub struct YieldServer {
    endpoint: BackendWebSocketEndpoint,
    started: Receiver<()>,
    commands: SyncSender<YieldCommand>,
    responses: Receiver<Value>,
    handle: thread::JoinHandle<()>,
}

impl YieldServer {
    pub fn spawn() -> Self {
        Self::spawn_for(None)
    }

    pub fn spawn_resume(cas_thread_id: String) -> Self {
        Self::spawn_for(Some(cas_thread_id))
    }

    fn spawn_for(resume: Option<String>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
        let (started_tx, started) = mpsc::sync_channel(1);
        let (commands, commands_rx) = mpsc::sync_channel(1);
        let (responses_tx, responses) = mpsc::sync_channel(1);
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(TIMEOUT)).unwrap();
            stream.set_write_timeout(Some(TIMEOUT)).unwrap();
            let mut socket = accept_hdr(
                stream,
                |request: &tungstenite::handshake::server::Request, response| {
                    assert_eq!(request.headers()["authorization"], AUTHORIZATION);
                    Ok(response)
                },
            )
            .unwrap();
            complete_admission(&mut socket);
            let cas_thread_id = resume.as_deref().unwrap_or(CAS_THREAD_ID);
            let load = read_json(&mut socket).unwrap();
            assert_eq!(
                load["method"],
                if resume.is_some() {
                    "thread/resume"
                } else {
                    "thread/start"
                }
            );
            send_thread_load_metadata(
                &mut socket,
                load["id"].as_u64().unwrap(),
                cas_thread_id,
                resume.is_some(),
                "lifecycle-model",
                None,
            );
            let id = read_ordinary_turn_start(&mut socket, cas_thread_id);
            send_checked_user(
                &mut socket,
                cas_thread_id,
                "item/started",
                "startedAtMs",
                STARTED_AT_MS,
            );
            send_checked_user(
                &mut socket,
                cas_thread_id,
                "item/completed",
                "completedAtMs",
                COMPLETED_AT_MS,
            );
            send_turn_start_response(&mut socket, id);
            started_tx.send(()).unwrap();
            let mut request_id = 900;
            while let Ok(command) = commands_rx.recv_timeout(TIMEOUT) {
                match command {
                    YieldCommand::Call(outcome)
                    | YieldCommand::CallUntilConnectionClose(outcome) => {
                        request_id += 1;
                        send_json(
                            &mut socket,
                            &format!(
                                r#"{{"method":"item/tool/call","id":{request_id},"params":{{"threadId":"{cas_thread_id}","turnId":"{CAS_TURN_ID}","callId":"yield-{request_id}","namespace":"beryl","tool":"yield","arguments":{{"outcome":"{outcome}"}}}}}}"#,
                            ),
                        );
                        if matches!(command, YieldCommand::CallUntilConnectionClose(_)) {
                            read_until_close(&mut socket).unwrap();
                            return;
                        }
                        let response = read_json(&mut socket).unwrap();
                        assert_eq!(response["id"], request_id);
                        responses_tx.send(response).unwrap();
                    }
                    YieldCommand::ResolveBranch => {
                        request_id += 1;
                        send_json(
                            &mut socket,
                            &format!(
                                r#"{{"method":"item/tool/call","id":{request_id},"params":{{"threadId":"{cas_thread_id}","turnId":"{CAS_TURN_ID}","callId":"resolution-{request_id}","namespace":"beryl","tool":"resolve_branch_discussion","arguments":{{"resolution":"private resolution must not be retained or echoed"}}}}}}"#,
                            ),
                        );
                        let response = read_json(&mut socket).unwrap();
                        assert_eq!(response["id"], request_id);
                        responses_tx.send(response).unwrap();
                    }
                    YieldCommand::Complete
                    | YieldCommand::Compact
                    | YieldCommand::LiveCompaction => {
                        send_json(&mut socket, &terminal_wire_for(cas_thread_id));
                        if matches!(
                            command,
                            YieldCommand::Compact | YieldCommand::LiveCompaction
                        ) {
                            let compact = read_json(&mut socket).unwrap();
                            assert_eq!(compact["method"], "thread/compact/start");
                            send_json(
                                &mut socket,
                                &json!({"id":compact["id"],"result":{}}).to_string(),
                            );
                            if matches!(command, YieldCommand::LiveCompaction) {
                                send_json(
                                    &mut socket,
                                    &format!(
                                        r#"{{"method":"thread/status/changed","params":{{"threadId":"{cas_thread_id}","status":{{"type":"active","activeFlags":[]}}}}}}"#
                                    ),
                                );
                                send_json(
                                    &mut socket,
                                    &format!(
                                        r#"{{"method":"turn/started","params":{{"threadId":"{cas_thread_id}","turn":{{"id":"continuation-compaction","items":[],"itemsView":"notLoaded","status":"inProgress","error":null,"startedAt":1,"completedAt":null,"durationMs":null}}}}}}"#
                                    ),
                                );
                            }
                            responses_tx.send(compact).unwrap();
                        }
                        while let Some(request) = read_json(&mut socket) {
                            if request["method"] == "turn/interrupt" {
                                assert_eq!(request["params"]["turnId"], "continuation-compaction");
                                send_json(
                                    &mut socket,
                                    &json!({"id":request["id"],"result":{}}).to_string(),
                                );
                                continue;
                            }
                            assert_eq!(request["method"], "thread/unsubscribe");
                            send_json(
                                &mut socket,
                                &json!({
                                    "id":request["id"], "result":{"status":"unsubscribed"}
                                })
                                .to_string(),
                            );
                        }
                        return;
                    }
                    YieldCommand::Lose => {
                        socket.get_mut().shutdown(std::net::Shutdown::Both).unwrap();
                        return;
                    }
                }
            }
        });
        Self {
            endpoint,
            started,
            commands,
            responses,
            handle,
        }
    }

    pub fn endpoint(&self) -> BackendWebSocketEndpoint {
        self.endpoint.clone()
    }

    pub fn wait_started(&self) {
        self.started.recv_timeout(TIMEOUT).unwrap();
    }

    pub fn call(&self, outcome: &'static str) -> Value {
        self.commands.send(YieldCommand::Call(outcome)).unwrap();
        self.responses.recv_timeout(TIMEOUT).unwrap()
    }

    pub fn call_until_connection_close(&self, outcome: &'static str) {
        self.commands
            .send(YieldCommand::CallUntilConnectionClose(outcome))
            .unwrap();
    }

    pub fn resolve_branch(&self) -> Value {
        self.commands.send(YieldCommand::ResolveBranch).unwrap();
        self.responses.recv_timeout(TIMEOUT).unwrap()
    }

    pub fn finish(&self, incomplete: bool) {
        self.commands
            .send(if incomplete {
                YieldCommand::Lose
            } else {
                YieldCommand::Complete
            })
            .unwrap();
    }

    pub fn join(self) {
        self.handle.join().unwrap();
    }

    pub fn compact(&self) {
        self.commands.send(YieldCommand::Compact).unwrap();
        self.responses.recv_timeout(TIMEOUT).unwrap();
    }

    pub fn live_compaction(&self) {
        self.commands.send(YieldCommand::LiveCompaction).unwrap();
        self.responses.recv_timeout(TIMEOUT).unwrap();
    }
}
