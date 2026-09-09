#![allow(dead_code)]

include!("../normal_terminal/server.rs");

enum YieldCommand {
    Call(&'static str),
    Complete,
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
            let load = read_json(&mut socket).unwrap();
            assert_eq!(load["method"], "thread/start");
            send_thread_load_metadata(
                &mut socket,
                load["id"].as_u64().unwrap(),
                CAS_THREAD_ID,
                false,
                "lifecycle-model",
                None,
            );
            let id = read_ordinary_turn_start(&mut socket, CAS_THREAD_ID);
            send_checked_user(
                &mut socket,
                CAS_THREAD_ID,
                "item/started",
                "startedAtMs",
                STARTED_AT_MS,
            );
            send_checked_user(
                &mut socket,
                CAS_THREAD_ID,
                "item/completed",
                "completedAtMs",
                COMPLETED_AT_MS,
            );
            send_turn_start_response(&mut socket, id);
            started_tx.send(()).unwrap();
            let mut request_id = 900;
            while let Ok(command) = commands_rx.recv_timeout(TIMEOUT) {
                match command {
                    YieldCommand::Call(outcome) => {
                        request_id += 1;
                        send_json(
                            &mut socket,
                            &format!(
                                r#"{{"method":"item/tool/call","id":{request_id},"params":{{"threadId":"{CAS_THREAD_ID}","turnId":"{CAS_TURN_ID}","callId":"yield-{request_id}","namespace":"beryl","tool":"yield","arguments":{{"outcome":"{outcome}"}}}}}}"#,
                            ),
                        );
                        let response = read_json(&mut socket).unwrap();
                        assert_eq!(response["id"], request_id);
                        responses_tx.send(response).unwrap();
                    }
                    YieldCommand::Complete => {
                        send_json(&mut socket, &terminal_wire());
                        while let Some(request) = read_json(&mut socket) {
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
}
