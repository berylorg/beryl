#![allow(dead_code)]

include!("../normal_terminal/server.rs");

pub struct AttemptServer {
    endpoint: BackendWebSocketEndpoint,
    handle: thread::JoinHandle<()>,
}

impl AttemptServer {
    pub fn spawn(
        load_method: &'static str,
        cas_thread_id: String,
        model: &'static str,
        reasoning: Option<&'static str>,
        expected_policies: Vec<Value>,
        complete: bool,
    ) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
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
            assert_eq!(load["method"], load_method);
            if load_method == "thread/resume" {
                assert_eq!(load["params"]["threadId"], cas_thread_id);
            }
            assert!(load["params"].get("developerInstructions").is_none());
            send_thread_load_metadata(
                &mut socket,
                load["id"].as_u64().unwrap(),
                &cas_thread_id,
                load_method == "thread/resume",
                model,
                reasoning,
            );
            for mut expected in expected_policies {
                expected["threadId"] = json!(cas_thread_id);
                expected["input"] = json!([{"type":"text", "text":SUBMITTED_TEXT}]);
                let request = read_json(&mut socket).unwrap();
                assert_eq!(request["method"], "turn/start");
                assert_eq!(request["params"], expected);
                let id = request["id"].as_u64().unwrap();
                if complete {
                    finish_ordinary_turn(&mut socket, &cas_thread_id, id);
                } else {
                    send_json(
                        &mut socket,
                        &json!({"error":{"code":-32602,"message":"exact rejection"},"id":id})
                            .to_string(),
                    );
                }
            }
            while let Some(request) = read_json(&mut socket) {
                assert_eq!(request["method"], "thread/unsubscribe");
                assert_eq!(request["params"]["threadId"], cas_thread_id);
                send_json(
                    &mut socket,
                    &json!({"id":request["id"],"result":{"status":"unsubscribed"}}).to_string(),
                );
            }
        });
        Self { endpoint, handle }
    }

    pub fn endpoint(&self) -> BackendWebSocketEndpoint {
        self.endpoint.clone()
    }

    pub fn join(self) {
        self.handle.join().unwrap();
    }
}

pub fn hidden_context(model: &str, reasoning: Option<&str>, instructions: Option<&str>) -> Value {
    let mut context = json!({"collaborationMode":{"mode":"default","settings":{
        "model":model,"reasoning_effort":reasoning,"developer_instructions":instructions
    }}});
    if reasoning.is_none() {
        context["collaborationMode"]["settings"]
            .as_object_mut()
            .unwrap()
            .remove("reasoning_effort");
    }
    context
}
