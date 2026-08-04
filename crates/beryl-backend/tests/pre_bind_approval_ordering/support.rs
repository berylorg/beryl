use std::{
    net::{TcpListener, TcpStream},
    num::NonZeroUsize,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use beryl_backend::{
    ApprovalInterruption, ApprovalOperationCompletion, ApprovalRequestKind,
    ApprovalResponseDisposition, BackendWebSocketEndpoint, ExactForegroundTurn,
    ForegroundSessionConfig, ManagedBackendClientConnector, ManagedBackendSession,
    OrderedTurnStreamCompletion, OrderedTurnStreamOperation, OrderedTurnStreamRejection,
    OrderedTurnStreamSink, OrderedTurnStreamSubmitCause, OrderedTurnStreamSubmitError,
    PreBindControlDiagnostics, StopAttemptCorrelation, StopAttemptDisposition,
    StopOperationCorrelation,
};
use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration, CasThreadId,
    CasTurnId, RuntimeId,
};
use serde_json::Value;
use tungstenite::{Message, WebSocket, accept_hdr};

pub const AUTHORIZATION: &str = "Bearer phase-46-pre-bind-token";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalTrace {
    pub request_id: i64,
    pub disposition: ApprovalResponseDisposition,
}

#[derive(Clone, Copy)]
pub enum SinkFailure {
    Submit(OrderedTurnStreamSubmitCause),
    Target(OrderedTurnStreamSubmitCause),
    MissingPermissionStopOwner,
    UnexpectedNonInterruptingStopOwner,
    WrongPermissionTarget,
}

pub struct SinkHarness {
    pub sink: Box<dyn OrderedTurnStreamSink>,
    pub trace: Arc<Mutex<Vec<ApprovalTrace>>>,
}

struct RecordingSink {
    trace: Arc<Mutex<Vec<ApprovalTrace>>>,
    fail_at: Option<(usize, SinkFailure)>,
    submissions: usize,
}

pub fn sink_harness(fail_at: Option<(usize, SinkFailure)>) -> SinkHarness {
    let trace = Arc::new(Mutex::new(Vec::new()));
    SinkHarness {
        sink: Box::new(RecordingSink {
            trace: Arc::clone(&trace),
            fail_at,
            submissions: 0,
        }),
        trace,
    }
}

impl OrderedTurnStreamSink for RecordingSink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        let OrderedTurnStreamOperation::Approval(request) = operation else {
            return Err(OrderedTurnStreamSubmitError::new(
                operation,
                OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::SchemaMismatch),
            ));
        };
        self.submissions += 1;
        self.trace
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(ApprovalTrace {
                request_id: request.request_id().as_i64().unwrap(),
                disposition: request.response_disposition(),
            });

        match self.fail_at {
            Some((at, SinkFailure::Submit(cause))) if at == self.submissions => {
                Err(OrderedTurnStreamSubmitError::new(
                    OrderedTurnStreamOperation::Approval(request),
                    cause,
                ))
            }
            Some((at, SinkFailure::Target(cause))) if at == self.submissions => {
                Ok(OrderedTurnStreamCompletion::Approval(
                    ApprovalOperationCompletion::TargetFailed { request, cause },
                ))
            }
            Some((at, SinkFailure::MissingPermissionStopOwner)) if at == self.submissions => Ok(
                OrderedTurnStreamCompletion::Approval(ApprovalOperationCompletion::Routed {
                    interruption: ApprovalInterruption::NotRequired,
                }),
            ),
            Some((at, SinkFailure::UnexpectedNonInterruptingStopOwner))
                if at == self.submissions =>
            {
                let request_id = request.request_id().as_i64().unwrap();
                Ok(OrderedTurnStreamCompletion::Approval(
                    ApprovalOperationCompletion::Routed {
                        interruption: ApprovalInterruption::DurableStopOwned {
                            operation: StopOperationCorrelation::from_bytes([0x33; 16]),
                            target: exact_target(request_id),
                            attempt_disposition: StopAttemptDisposition::ClaimedNotDispatched(
                                StopAttemptCorrelation::from_bytes([0x43; 16]),
                            ),
                        },
                    },
                ))
            }
            Some((at, SinkFailure::WrongPermissionTarget)) if at == self.submissions => {
                let request_id = request.request_id().as_i64().unwrap();
                Ok(OrderedTurnStreamCompletion::Approval(
                    ApprovalOperationCompletion::Routed {
                        interruption: ApprovalInterruption::DurableStopOwned {
                            operation: StopOperationCorrelation::from_bytes([0x32; 16]),
                            target: exact_target(request_id + 1),
                            attempt_disposition: StopAttemptDisposition::ClaimedNotDispatched(
                                StopAttemptCorrelation::from_bytes([0x42; 16]),
                            ),
                        },
                    },
                ))
            }
            _ => Ok(OrderedTurnStreamCompletion::Approval(
                ApprovalOperationCompletion::Routed {
                    interruption: if request.kind().separate_interruption_required() {
                        ApprovalInterruption::DurableStopOwned {
                            operation: StopOperationCorrelation::from_bytes([0x31; 16]),
                            target: exact_target(request.request_id().as_i64().unwrap()),
                            attempt_disposition: StopAttemptDisposition::ClaimedNotDispatched(
                                StopAttemptCorrelation::from_bytes([0x41; 16]),
                            ),
                        }
                    } else {
                        ApprovalInterruption::NotRequired
                    },
                },
            )),
        }
    }
}

pub fn exact_target(request_id: i64) -> ExactForegroundTurn {
    ExactForegroundTurn::new(
        RuntimeId::from_bytes([0x21; 16]),
        CasLoadedSessionGeneration::new(
            CasProcessGeneration::new(7).unwrap(),
            CasLoadedThreadGeneration::new(11).unwrap(),
        ),
        CasThreadId::new(format!("thread_{request_id}")).unwrap(),
        CasTurnId::new(format!("turn_{request_id}")).unwrap(),
    )
}

pub fn connect_foreground(
    endpoint: BackendWebSocketEndpoint,
    pre_bind_control_capacity: usize,
) -> ManagedBackendSession {
    let connector = ManagedBackendClientConnector::for_lifecycle_test(endpoint, AUTHORIZATION);
    connector
        .connect_foreground_candidate(
            ForegroundSessionConfig::new(nonzero_usize(pre_bind_control_capacity)),
            Duration::from_secs(2),
        )
        .unwrap()
}

fn nonzero_usize(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}

pub fn assert_pre_bind_diagnostics(
    session: &ManagedBackendSession,
    capacity: usize,
    current: usize,
    high_water: usize,
    admissions: u64,
    full: u64,
) {
    assert_eq!(
        session.pre_bind_control_diagnostics(),
        Some(PreBindControlDiagnostics {
            capacity,
            current,
            high_water,
            admissions,
            full,
        }),
    );
}

pub fn spawn_server<F, T>(script: F) -> (BackendWebSocketEndpoint, thread::JoinHandle<T>)
where
    F: FnOnce(&mut WebSocket<TcpStream>) -> T + Send + 'static,
    T: Send + 'static,
{
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
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
                    AUTHORIZATION,
                );
                Ok(response)
            },
        )
        .unwrap();
        socket
            .get_mut()
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        script(&mut socket)
    });
    (endpoint, server)
}

pub fn send_approval(
    socket: &mut WebSocket<TcpStream>,
    request_id: i64,
    kind: ApprovalRequestKind,
) {
    let method = kind.method();
    send_json(
        socket,
        &format!(
            r#"{{"method":"{method}","id":{request_id},"params":{{"threadId":"thread_{request_id}","turnId":"turn_{request_id}","itemId":"item_{request_id}","opaque":{{"ignored":true}}}}}}"#,
        ),
    );
}

pub fn send_unavailable_compact(socket: &mut WebSocket<TcpStream>) {
    send_json(
        socket,
        r#"{"method":"thread/started","params":{"threadId":"thread_late"}}"#,
    );
}

pub fn send_initialize_response(socket: &mut WebSocket<TcpStream>, request_id: u64) {
    send_json(
        socket,
        &format!(
            r#"{{"id":{request_id},"result":{{"userAgent":"beryl/0.146.0","codexHome":"C:\\codex","platformFamily":"windows","platformOs":"windows"}}}}"#,
        ),
    );
}

pub fn read_denial_id(socket: &mut WebSocket<TcpStream>) -> i64 {
    let value = read_text_json(socket).expect("expected one denial response");
    assert_eq!(value.get("jsonrpc").and_then(Value::as_str), Some("2.0"));
    let result = value.get("result").and_then(Value::as_object).unwrap();
    assert!(
        result
            .get("decision")
            .is_some_and(|value| value == "cancel")
            || result.get("scope").is_some_and(|value| value == "turn"),
    );
    value.get("id").and_then(Value::as_i64).unwrap()
}

pub fn read_text_json(socket: &mut WebSocket<TcpStream>) -> Option<Value> {
    loop {
        match socket.read() {
            Ok(Message::Text(text)) => return Some(serde_json::from_str(text.as_ref()).unwrap()),
            Ok(Message::Close(_)) => return None,
            Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => {}
            Ok(Message::Binary(bytes)) => panic!("unexpected binary message: {bytes:?}"),
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return None;
            }
            Err(tungstenite::Error::Io(source))
                if matches!(
                    source.kind(),
                    std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::UnexpectedEof
                ) =>
            {
                return None;
            }
            Err(error) => panic!("unexpected server read failure: {error}"),
        }
    }
}

pub fn expect_close_without_text(socket: &mut WebSocket<TcpStream>) {
    assert_eq!(read_text_json(socket), None);
}

fn send_json(socket: &mut WebSocket<TcpStream>, json: &str) {
    socket.send(Message::Text(json.into())).unwrap();
}
