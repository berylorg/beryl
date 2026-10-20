use std::{
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
};

use beryl_backend::BackendWebSocketEndpoint;
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket, accept_hdr};

use crate::wire::{AUTHORIZATION, TIMEOUT, complete_admission};

pub enum Step {
    Load {
        method: &'static str,
        source: Option<String>,
        target: &'static str,
    },
    Inject {
        target: &'static str,
        items: Vec<Value>,
        reply: Reply,
    },
    Unsubscribe(&'static str),
    Turn {
        target: &'static str,
        text: &'static str,
    },
}

pub enum Reply {
    Success,
    SuccessAfter(Box<dyn FnOnce() + Send>),
    Rejected,
    Lost,
}

pub struct WireServer {
    endpoint: BackendWebSocketEndpoint,
    requests: Arc<Mutex<Vec<Value>>>,
    handle: thread::JoinHandle<()>,
}

impl WireServer {
    pub fn spawn(steps: Vec<Step>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = requests.clone();
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
            for step in steps {
                let request = read(&mut socket).expect("scripted backend request");
                observed.lock().unwrap().push(request.clone());
                match step {
                    Step::Load {
                        method,
                        source,
                        target,
                    } => {
                        assert_eq!(request["method"], method);
                        if let Some(source) = source {
                            assert_eq!(request["params"]["threadId"], source);
                        }
                        load(&mut socket, &request, target, method == "thread/resume");
                    }
                    Step::Inject {
                        target,
                        items,
                        reply,
                    } => {
                        assert_eq!(request["method"], "thread/inject_items");
                        assert_eq!(request["params"]["threadId"], target);
                        assert_eq!(request["params"]["items"], Value::Array(items));
                        match reply {
                            Reply::Success => {
                                send(&mut socket, json!({"id":request["id"],"result":{}}))
                            }
                            Reply::SuccessAfter(before_ack) => {
                                before_ack();
                                send(&mut socket, json!({"id":request["id"],"result":{}}));
                            }
                            Reply::Rejected => send_wire(
                                &mut socket,
                                format!(
                                    r#"{{"error":{{"code":-32602,"message":"context refused"}},"id":{}}}"#,
                                    request["id"]
                                ),
                            ),
                            Reply::Lost => {
                                socket.close(None).unwrap();
                                return;
                            }
                        }
                    }
                    Step::Unsubscribe(target) => {
                        assert_eq!(request["method"], "thread/unsubscribe");
                        assert_eq!(request["params"]["threadId"], target);
                        send(
                            &mut socket,
                            json!({"id":request["id"],"result":{"status":"unsubscribed"}}),
                        );
                    }
                    Step::Turn { target, text } => turn(&mut socket, request, target, text),
                }
            }
            assert!(
                read(&mut socket).is_none(),
                "no duplicate seed or unplanned native work"
            );
        });
        Self {
            endpoint,
            requests,
            handle,
        }
    }
    pub fn endpoint(&self) -> BackendWebSocketEndpoint {
        self.endpoint.clone()
    }
    pub fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }
    pub fn join(self) {
        self.handle.join().unwrap();
    }
}

fn load(socket: &mut WebSocket<TcpStream>, request: &Value, target: &str, resumed: bool) {
    let mut result = json!({"thread":{"id":target,"extra":null,"sessionId":"session-id","forkedFromId":null,
        "parentThreadId":null,"preview":"preview","ephemeral":false,"historyMode":"legacy","modelProvider":"openai",
        "createdAt":1,"updatedAt":2,"recencyAt":null,"status":{"type":"idle"},"path":null,"cwd":crate::EXECUTION_ROOT,
        "cliVersion":"0.146.0","source":"appServer","threadSource":null,"agentNickname":null,"agentRole":null,
        "gitInfo":null,"name":null,"turns":[]},"model":"gpt-5.6","modelProvider":"openai","serviceTier":null,
        "cwd":crate::EXECUTION_ROOT,"runtimeWorkspaceRoots":[],"instructionSources":[],"approvalPolicy":"never",
        "approvalsReviewer":"user","sandbox":{},"activePermissionProfile":null,"reasoningEffort":"high",
        "multiAgentMode":"explicitRequestOnly"});
    if resumed {
        result["initialTurnsPage"] = Value::Null;
        result["turnsBackwardsCursor"] = Value::Null;
        result["itemsBackwardsCursor"] = Value::Null;
    }
    send(socket, json!({"id":request["id"],"result":result}));
}

fn turn(socket: &mut WebSocket<TcpStream>, request: Value, target: &str, text: &str) {
    assert_eq!(request["method"], "turn/start");
    assert_eq!(request["params"]["threadId"], target);
    assert_eq!(
        request["params"]["input"],
        json!([{"type":"text","text":text}])
    );
    let target = serde_json::to_string(target).unwrap();
    let text = serde_json::to_string(text).unwrap();
    let turn = serde_json::to_string(&format!("context-local-turn-{}", request["id"])).unwrap();
    let item = serde_json::to_string(&format!("context-local-user-{}", request["id"])).unwrap();
    for (method, field) in [
        ("item/started", "startedAtMs"),
        ("item/completed", "completedAtMs"),
    ] {
        send_wire(
            socket,
            format!(
                r#"{{"method":"{method}","params":{{"item":{{"type":"userMessage","id":{item},"clientId":null,"content":[{{"type":"text","text":{text},"text_elements":[]}}]}},"threadId":{target},"turnId":{turn},"{field}":90005}}}}"#
            ),
        );
    }
    send_wire(
        socket,
        format!(
            r#"{{"id":{},"result":{{"turn":{{"id":{turn},"items":[],"status":"inProgress"}}}}}}"#,
            request["id"]
        ),
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    send_wire(
        socket,
        format!(
            r#"{{"method":"turn/completed","params":{{"threadId":{target},"turn":{{"id":{turn},"items":[],"itemsView":"notLoaded","status":"completed","error":null,"startedAt":90004,"completedAt":90005,"durationMs":1}}}},"emittedAtMs":{now}}}"#
        ),
    );
}

fn send_wire(socket: &mut WebSocket<TcpStream>, wire: String) {
    socket.send(Message::Text(wire.into())).unwrap();
}

fn send(socket: &mut WebSocket<TcpStream>, value: Value) {
    socket
        .send(Message::Text(value.to_string().into()))
        .unwrap();
}

fn read(socket: &mut WebSocket<TcpStream>) -> Option<Value> {
    loop {
        match socket.read() {
            Ok(Message::Text(value)) => return Some(serde_json::from_str(&value).unwrap()),
            Ok(Message::Close(_))
            | Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return None;
            }
            Ok(Message::Ping(value)) => socket.send(Message::Pong(value)).unwrap(),
            Ok(Message::Pong(_) | Message::Frame(_)) => {}
            value => panic!("unexpected context server response: {value:?}"),
        }
    }
}
