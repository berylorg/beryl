#![allow(dead_code)]

include!("../normal_terminal/server.rs");

pub struct ProcessSessionServer {
    endpoint: BackendWebSocketEndpoint,
    started: Receiver<usize>,
    finish: SyncSender<()>,
    handle: thread::JoinHandle<()>,
}

impl ProcessSessionServer {
    pub fn spawn(cas_thread_id: String, inputs: Vec<String>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
        let (started_sender, started) = mpsc::sync_channel(1);
        let (finish, finish_receiver) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name("process-session-server".to_owned())
            .spawn(move || {
                run_process_session_server(
                    listener,
                    &cas_thread_id,
                    inputs,
                    started_sender,
                    finish_receiver,
                );
            })
            .unwrap();
        Self {
            endpoint,
            started,
            finish,
            handle,
        }
    }

    pub fn endpoint(&self) -> BackendWebSocketEndpoint {
        self.endpoint.clone()
    }

    pub fn wait_for_turn_start(&self, ordinal: usize) -> bool {
        self.started.recv_timeout(TIMEOUT) == Ok(ordinal)
    }

    pub fn finish_turn(&self) {
        self.finish.send(()).unwrap();
    }

    pub fn join(self) {
        self.handle.join().unwrap();
    }
}

fn run_process_session_server(
    listener: TcpListener,
    cas_thread_id: &str,
    inputs: Vec<String>,
    started: SyncSender<usize>,
    finish: Receiver<()>,
) {
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
                AUTHORIZATION
            );
            Ok(response)
        },
    )
    .unwrap();
    socket.get_mut().set_read_timeout(Some(TIMEOUT)).unwrap();
    complete_admission(&mut socket);
    for (ordinal, input) in inputs.into_iter().enumerate() {
        complete_resume_projection(&mut socket, cas_thread_id);
        let request = read_json(&mut socket).expect("scheduled turn start");
        assert_eq!(request["method"], "turn/start");
        assert_eq!(request["params"]["threadId"], cas_thread_id);
        assert_eq!(
            request["params"]["input"],
            json!([{"type":"text","text":input}])
        );
        let id = request["id"].as_u64().unwrap();
        let turn_id = format!("process-turn-{ordinal}");
        for (method, field, timestamp) in [
            ("item/started", "startedAtMs", 37_001 + ordinal as u64 * 10),
            (
                "item/completed",
                "completedAtMs",
                37_002 + ordinal as u64 * 10,
            ),
        ] {
            let mut params = json!({
                "item":{"type":"userMessage","id":format!("process-user-{ordinal}"),"clientId":null,
                    "content":[{"type":"text","text":input,"text_elements":[]}]},
                "threadId":cas_thread_id,"turnId":turn_id,
            });
            params[field] = json!(timestamp);
            send_json(
                &mut socket,
                &json!({"method":method,"params":params}).to_string(),
            );
        }
        send_json(
            &mut socket,
            &json!({"id":id,"result":{"turn":{"id":turn_id,"items":[],"status":"inProgress"}}})
                .to_string(),
        );
        started.send(ordinal).unwrap();
        finish.recv_timeout(TIMEOUT).unwrap();
        send_json(&mut socket, &json!({"method":"turn/completed","params":{
            "threadId":cas_thread_id,
            "turn":{"id":turn_id,"items":[],"itemsView":"notLoaded","status":"completed","error":null,
                "startedAt":37_001 + ordinal as u64 * 10,"completedAt":37_002 + ordinal as u64 * 10,"durationMs":1}
        }}).to_string());
        complete_unsubscribe(&mut socket, cas_thread_id);
    }
    read_until_close(&mut socket).unwrap();
}
