use beryl_backend::BackendWebSocketEndpoint;
use serde_json::Value;
use std::{
    collections::VecDeque,
    io,
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::Duration,
};
use tungstenite::{Message, accept_hdr};

pub(crate) const AUTHORIZATION: &str = "Bearer model-reader";
const MAX_REQUESTS: usize = 128;

struct Response {
    family: &'static str,
    payload: String,
}

impl Response {
    fn result(payload: String) -> Self {
        Self {
            family: "result",
            payload,
        }
    }
}

pub(crate) enum ModelResponse {
    Page {
        start: usize,
        count: usize,
        next: Option<String>,
    },
    Failure,
    ResumedThread {
        thread: String,
        root: String,
        model: String,
        reasoning: Option<String>,
    },
    Held {
        response: Box<ModelResponse>,
        reached: SyncSender<()>,
        release: Receiver<()>,
    },
}

pub(crate) struct HeldResponse {
    reached: Receiver<()>,
    release: SyncSender<()>,
}

impl HeldResponse {
    pub(crate) fn wait(&self) {
        self.reached.recv_timeout(super::TIMEOUT).unwrap();
    }
    pub(crate) fn release(&self) {
        let _ = self.release.try_send(());
    }
}

impl Drop for HeldResponse {
    fn drop(&mut self) {
        self.release();
    }
}

impl ModelResponse {
    pub(crate) fn resumed_thread(
        thread: &str,
        root: &str,
        model: &str,
        reasoning: Option<&str>,
    ) -> Self {
        assert!(
            thread.len() <= 256
                && root.len() <= 4096
                && model.len() <= 256
                && reasoning.is_none_or(|value| value.len() <= 256)
        );
        Self::ResumedThread {
            thread: thread.to_owned(),
            root: root.to_owned(),
            model: model.to_owned(),
            reasoning: reasoning.map(str::to_owned),
        }
    }

    fn check_request(&self, request: &Value) {
        match self {
            Self::ResumedThread { thread, root, .. } => {
                assert_eq!(request["method"], "thread/resume");
                assert_eq!(request["params"]["threadId"], thread.as_str());
                assert_eq!(request["params"]["cwd"], root.as_str());
                assert_eq!(request["params"]["excludeTurns"], true);
            }
            Self::Held { response, .. } => response.check_request(request),
            Self::Page { .. } | Self::Failure => assert_eq!(request["method"], "model/list"),
        }
    }
    pub(crate) fn page(count: usize, next: Option<&str>) -> Self {
        Self::page_range(0, count, next)
    }

    pub(crate) fn page_range(start: usize, count: usize, next: Option<&str>) -> Self {
        assert!(count <= 64);
        start.checked_add(count).expect("bounded model range");
        Self::Page {
            start,
            count,
            next: next.map(str::to_owned),
        }
    }
    fn result(self, stop: &AtomicBool) -> Option<Response> {
        match self {
            Self::Page { start, count, next } => Some(
                Response::result(format!(r#"{{"data":[{}],"nextCursor":{}}}"#, (start..start+count).map(|index| format!(r#"{{"id":"id-{index}","model":"model-{index}","displayName":"Model {index}","hidden":false,"supportedReasoningEfforts":["low","high","future-provider-effort"],"defaultReasoningEffort":"high","isDefault":{}}}"#, index == 0)).collect::<Vec<_>>().join(","), serde_json::to_string(&next).unwrap())),
            ),
            Self::Failure => Some(Response { family: "error", payload: r#"{"code":-32602,"message":"synthetic bounded model request failure"}"#.to_owned() }),
            Self::ResumedThread { thread, root, model, reasoning } => {
                let thread = serde_json::to_string(&thread).unwrap();
                let root = serde_json::to_string(&root).unwrap();
                let model = serde_json::to_string(&model).unwrap();
                let reasoning = serde_json::to_string(&reasoning).unwrap();
                Some(Response::result(format!(r#"{{"thread":{{"id":{thread},"extra":null,"sessionId":"session-id","forkedFromId":null,"parentThreadId":null,"preview":"preview","ephemeral":false,"historyMode":"legacy","modelProvider":"openai","createdAt":1,"updatedAt":2,"recencyAt":null,"status":{{"type":"idle"}},"path":null,"cwd":{root},"cliVersion":"0.146.0","source":"appServer","threadSource":null,"agentNickname":null,"agentRole":null,"gitInfo":null,"name":null,"turns":[]}},"model":{model},"modelProvider":"openai","serviceTier":null,"cwd":{root},"runtimeWorkspaceRoots":[],"instructionSources":[],"approvalPolicy":"never","approvalsReviewer":"user","sandbox":{{}},"activePermissionProfile":null,"reasoningEffort":{reasoning},"multiAgentMode":"explicitRequestOnly","initialTurnsPage":null,"turnsBackwardsCursor":null,"itemsBackwardsCursor":null}}"#)))
            }
            Self::Held {
                response,
                reached,
                release,
            } => {
                reached.send(()).unwrap();
                let deadline = std::time::Instant::now() + super::TIMEOUT;
                loop {
                    if stop.load(Ordering::Acquire) {
                        return None;
                    }
                    match release.recv_timeout(Duration::from_millis(25)) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => assert!(
                            std::time::Instant::now() < deadline,
                            "held model response was not released"
                        ),
                    }
                }
                response.result(stop)
            }
        }
    }
}

struct Shared {
    stop: AtomicBool,
    fail_config: AtomicBool,
    requests: Mutex<Vec<Value>>,
    responses: Mutex<VecDeque<ModelResponse>>,
    streams: Mutex<Vec<TcpStream>>,
    admission_frames: Mutex<Vec<String>>,
    errors: Mutex<Vec<String>>,
    resumed_threads: Mutex<Vec<String>>,
}

pub(crate) struct ProtocolServer {
    endpoint: BackendWebSocketEndpoint,
    shared: Arc<Shared>,
    thread: Option<thread::JoinHandle<()>>,
}

impl ProtocolServer {
    pub(crate) fn new() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            fail_config: AtomicBool::new(false),
            requests: Mutex::new(Vec::new()),
            responses: Mutex::new(VecDeque::new()),
            streams: Mutex::new(Vec::new()),
            admission_frames: Mutex::new(Vec::new()),
            errors: Mutex::new(Vec::new()),
            resumed_threads: Mutex::new(Vec::new()),
        });
        let owner = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("model-read-protocol".into())
            .spawn(move || {
                let mut children = Vec::new();
                while !owner.stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let mut streams = owner.streams.lock().unwrap();
                            if owner.stop.load(Ordering::Acquire) {
                                break;
                            }
                            assert!(
                                streams.len() < 32,
                                "model protocol connection bound exceeded"
                            );
                            streams.push(stream.try_clone().unwrap());
                            drop(streams);
                            let owner = Arc::clone(&owner);
                            children.push(thread::spawn(move || serve(stream, owner)));
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(1))
                        }
                        Err(error) => panic!("model protocol accept failed: {error}"),
                    }
                }
                for child in children {
                    child.join().unwrap();
                }
            })
            .unwrap();
        Self {
            endpoint,
            shared,
            thread: Some(thread),
        }
    }
    pub(crate) fn endpoint(&self) -> BackendWebSocketEndpoint {
        self.endpoint.clone()
    }
    pub(crate) fn enqueue(&self, response: ModelResponse) {
        let mut responses = self.shared.responses.lock().unwrap();
        assert!(responses.len() < 8);
        responses.push_back(response);
    }
    pub(crate) fn fail_next_config_read(&self) {
        assert!(!self.shared.fail_config.swap(true, Ordering::AcqRel));
    }
    pub(crate) fn hold(&self, response: ModelResponse) -> HeldResponse {
        let (reached, receiver) = mpsc::sync_channel(1);
        let (release, commands) = mpsc::sync_channel(1);
        self.enqueue(ModelResponse::Held {
            response: Box::new(response),
            reached,
            release: commands,
        });
        HeldResponse {
            reached: receiver,
            release,
        }
    }
    pub(crate) fn requests(&self, method: &str) -> Vec<Value> {
        self.shared
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request["method"] == method)
            .cloned()
            .collect()
    }

    pub(crate) fn diagnostics(&self) -> String {
        let methods: Vec<_> = self
            .shared
            .requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| (request["method"].clone(), request["id"].clone()))
            .collect();
        format!(
            "connections={}; requests={methods:?}; admission_frames={:?}; errors={:?}",
            self.shared.streams.lock().unwrap().len(),
            self.shared.admission_frames.lock().unwrap(),
            self.shared.errors.lock().unwrap()
        )
    }
}

impl Drop for ProtocolServer {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        for stream in self.shared.streams.lock().unwrap().iter() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        self.thread.take().unwrap().join().unwrap();
    }
}

fn serve(stream: TcpStream, shared: Arc<Shared>) {
    stream.set_nonblocking(false).unwrap();
    stream.set_read_timeout(Some(super::TIMEOUT)).unwrap();
    stream.set_write_timeout(Some(super::TIMEOUT)).unwrap();
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
    loop {
        let message = match socket.read() {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_))
            | Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return;
            }
            Ok(Message::Ping(data)) => {
                let _ = socket.send(Message::Pong(data));
                continue;
            }
            Ok(_) => continue,
            Err(tungstenite::Error::Io(error))
                if !shared.stop.load(Ordering::Acquire)
                    && matches!(
                        error.kind(),
                        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                    ) =>
            {
                continue;
            }
            Err(tungstenite::Error::Io(error))
                if shared.stop.load(Ordering::Acquire)
                    || matches!(
                        error.kind(),
                        io::ErrorKind::UnexpectedEof
                            | io::ErrorKind::ConnectionReset
                            | io::ErrorKind::ConnectionAborted
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::WouldBlock
                    ) =>
            {
                let mut errors = shared.errors.lock().unwrap();
                if errors.len() < 32 {
                    errors.push(format!("read: {error}"));
                }
                return;
            }
            Err(error) => panic!("model protocol read failed: {error}"),
        };
        let request: Value = serde_json::from_str(&message).unwrap();
        {
            let mut requests = shared.requests.lock().unwrap();
            assert!(requests.len() < MAX_REQUESTS);
            requests.push(request.clone());
        }
        let response = match request["method"].as_str().unwrap() {
            "initialize" => {
                Response::result(r#"{"userAgent":"beryl/0.146.0","codexHome":"C:\\codex","platformFamily":"windows","platformOs":"windows"}"#.to_owned())
            }
            "initialized" => continue,
            "config/read" => {
                if shared.fail_config.swap(false, Ordering::AcqRel) {
                    ModelResponse::Failure.result(&shared.stop).unwrap()
                } else {
                Response::result(r#"{"config":{"model":"actual-model","model_reasoning_effort":null,"features":{"multi_agent_v2":{"enabled":true,"expose_spawn_agent_model_overrides":true}}},"origins":{"features.multi_agent_v2.enabled":{"name":{"type":"sessionFlags"},"version":"0"},"features.multi_agent_v2.expose_spawn_agent_model_overrides":{"name":{"type":"sessionFlags"},"version":"0"}}}"#.to_owned())
                }
            }
            "model/list" | "thread/resume" => {
                let next = shared
                    .responses
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("model request must have an exact bounded response");
                next.check_request(&request);
                if request["method"] == "thread/resume" {
                    let mut threads = shared.resumed_threads.lock().unwrap();
                    assert!(threads.len() < 8);
                    threads.push(request["params"]["threadId"].as_str().unwrap().to_owned());
                }
                let Some(response) = next.result(&shared.stop) else {
                    return;
                };
                response
            }
            "thread/unsubscribe" => {
                let thread = request["params"]["threadId"].as_str().unwrap();
                let mut threads = shared.resumed_threads.lock().unwrap();
                let index = threads.iter().position(|loaded| loaded == thread).expect("unsubscribe must match the exact resumed thread");
                threads.remove(index);
                Response::result(r#"{"status":"unsubscribed"}"#.to_owned())
            }
            method => panic!("unexpected model source protocol method: {method}"),
        };
        let envelope = format!(
            r#"{{"id":{},"{}":{}}}"#,
            request["id"], response.family, response.payload
        );
        assert!(
            serde_json::from_str::<Value>(&envelope).is_ok(),
            "invalid model fixture frame"
        );
        if matches!(
            request["method"].as_str(),
            Some("initialize" | "config/read")
        ) {
            let mut frames = shared.admission_frames.lock().unwrap();
            assert!(frames.len() < 64 && envelope.len() <= 1024);
            frames.push(envelope.clone());
        }
        if let Err(error) = socket.send(Message::Text(envelope.into())) {
            let mut errors = shared.errors.lock().unwrap();
            if errors.len() < 32 {
                errors.push(format!("send: {error}"));
            }
            drop(errors);
            if shared.stop.load(Ordering::Acquire)
                || matches!(
                    error,
                    tungstenite::Error::Io(_)
                        | tungstenite::Error::ConnectionClosed
                        | tungstenite::Error::AlreadyClosed
                )
            {
                return;
            }
            panic!("model protocol send failed: {error}");
        }
    }
}
