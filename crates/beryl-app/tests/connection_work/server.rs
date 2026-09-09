use beryl_backend::BackendWebSocketEndpoint;
use std::{
    net::TcpListener,
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
};
use tungstenite::{Message, accept};

enum Requests {
    DynamicTools(usize),
    Approval,
}

pub(super) struct Server {
    endpoint: BackendWebSocketEndpoint,
    send: SyncSender<Requests>,
    responses: Receiver<()>,
    worker: thread::JoinHandle<()>,
}

impl Server {
    pub(super) fn spawn() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
        let (send, commands) = mpsc::sync_channel(1);
        let (response, responses) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(super::TIMEOUT)).unwrap();
            stream.set_write_timeout(Some(super::TIMEOUT)).unwrap();
            let mut socket = accept(stream).unwrap();
            super::protocol::complete_admission(&mut socket);
            super::protocol::complete_projection(&mut socket);
            match commands.recv_timeout(super::TIMEOUT).unwrap() {
                Requests::DynamicTools(count) => for id in 1..=count {
                socket.send(Message::Text(format!(r#"{{"method":"item/tool/call","id":{id},"params":{{"threadId":"cas-thread","turnId":"cas-turn","callId":"work-{id}","namespace":"beryl","tool":"yield","arguments":{{"outcome":"phase_continue"}}}}}}"#).into())).unwrap();
                },
                Requests::Approval => socket.send(Message::Text(r#"{"method":"item/commandExecution/requestApproval","id":1,"params":{"threadId":"cas-thread","turnId":"cas-turn","itemId":"approval-item"}}"#.into())).unwrap(),
            }
            loop {
                match socket.read() {
                    Ok(Message::Text(text)) => {
                        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                        assert_eq!(value["id"], 1);
                        assert!(value.get("result").is_some());
                        response.send(()).unwrap();
                    }
                    Ok(Message::Close(_))
                    | Err(
                        tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed,
                    ) => break,
                    Err(tungstenite::Error::Io(error))
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::ConnectionReset
                                | std::io::ErrorKind::UnexpectedEof
                                | std::io::ErrorKind::ConnectionAborted
                        ) =>
                    {
                        break;
                    }
                    Ok(_) => {}
                    Err(error) => panic!("connection work server failed: {error}"),
                }
            }
        });
        Self {
            endpoint,
            send,
            responses,
            worker,
        }
    }
    pub(super) fn endpoint(&self) -> BackendWebSocketEndpoint {
        self.endpoint.clone()
    }
    pub(super) fn send_requests(&self, count: usize) {
        self.send.send(Requests::DynamicTools(count)).unwrap();
    }
    pub(super) fn send_approval(&self) {
        self.send.send(Requests::Approval).unwrap();
    }
    pub(super) fn wait_for_response(&self) {
        self.responses.recv_timeout(super::TIMEOUT).unwrap();
    }
    pub(super) fn join(self) {
        self.worker.join().unwrap();
    }
}
