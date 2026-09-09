#![allow(dead_code)]

include!("../normal_terminal/server.rs");

pub struct CompactionServer {
    endpoint: BackendWebSocketEndpoint,
    started: Receiver<()>,
    complete: SyncSender<()>,
    compacted: Receiver<()>,
    handle: thread::JoinHandle<()>,
}

impl CompactionServer {
    pub fn spawn(expect_compaction: bool) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
        let (started_tx, started) = mpsc::sync_channel(1);
        let (complete, complete_rx) = mpsc::sync_channel(1);
        let (compacted_tx, compacted) = mpsc::sync_channel(1);
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
                "compaction-model",
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
            complete_rx.recv_timeout(TIMEOUT).unwrap();
            send_json(&mut socket, &terminal_wire());
            if expect_compaction {
                let compact = read_json(&mut socket).unwrap();
                assert_eq!(compact["method"], "thread/compact/start");
                assert_eq!(compact["params"], json!({"threadId":CAS_THREAD_ID}));
                send_json(
                    &mut socket,
                    &json!({"id":compact["id"], "result":{}}).to_string(),
                );
                compacted_tx.send(()).unwrap();
            }
            while let Some(request) = read_json(&mut socket) {
                assert_eq!(request["method"], "thread/unsubscribe");
                send_json(
                    &mut socket,
                    &json!({"id":request["id"],"result":{"status":"unsubscribed"}}).to_string(),
                );
            }
        });
        Self {
            endpoint,
            started,
            complete,
            compacted,
            handle,
        }
    }

    pub fn endpoint(&self) -> BackendWebSocketEndpoint {
        self.endpoint.clone()
    }
    pub fn wait_started(&self) {
        self.started.recv_timeout(TIMEOUT).unwrap();
    }
    pub fn finish_turn(&self) {
        self.complete.send(()).unwrap();
    }
    pub fn wait_compaction(&self) {
        self.compacted.recv_timeout(TIMEOUT).unwrap();
    }
    pub fn join(self) {
        self.handle.join().unwrap();
    }
}
