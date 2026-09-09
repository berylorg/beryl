#![allow(dead_code)]

include!("../normal_terminal/server.rs");

pub struct MetadataServer {
    endpoint: BackendWebSocketEndpoint,
    handle: thread::JoinHandle<()>,
}

impl MetadataServer {
    pub fn spawn(
        method: &'static str,
        source: Option<String>,
        target: &'static str,
        model: &'static str,
        reasoning: Option<&'static str>,
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
            let request = read_json(&mut socket).unwrap();
            assert_eq!(request["method"], method);
            if let Some(source) = source {
                assert_eq!(request["params"]["threadId"], source);
            }
            send_thread_load_metadata(
                &mut socket,
                request["id"].as_u64().unwrap(),
                target,
                method == "thread/resume",
                model,
                reasoning,
            );
            assert!(
                read_json(&mut socket).is_none(),
                "metadata reuse must not issue another request"
            );
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
