use std::{
    io::{self, BufReader, Read, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    task::{Poll, Waker},
    thread,
    time::{Duration, Instant},
};

use serde_json::json;

use crate::diagnostic_child_protocol::{
    BoundedLineRead, DIAGNOSTIC_CHILD_PROTOCOL_NAME, DIAGNOSTIC_CHILD_PROTOCOL_VERSION,
    DiagnosticChildCommand, DiagnosticProtocolError, DiagnosticProtocolRequest,
    DiagnosticProtocolResponse, MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES, parse_request_frame,
    read_bounded_line_bytes, write_response_frame,
};

const DIAGNOSTIC_TARGET_REQUEST_QUEUE_CAPACITY: usize = 16;
const DIAGNOSTIC_TARGET_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const DIAGNOSTIC_TARGET_REQUEST_PENDING: u8 = 0;
const DIAGNOSTIC_TARGET_REQUEST_CANCELLED: u8 = 1;
const DIAGNOSTIC_TARGET_REQUEST_CLAIMED: u8 = 2;

#[derive(Clone)]
struct DiagnosticTargetShellRequestSender {
    sender: SyncSender<DiagnosticTargetShellRequest>,
    response_timeout: Duration,
    terminal: Arc<AtomicBool>,
    wake: Arc<Mutex<Option<Waker>>>,
}

pub(crate) struct DiagnosticTargetServer {
    pub(crate) receiver: Receiver<DiagnosticTargetShellRequest>,
    pub(crate) terminal: Arc<AtomicBool>,
    wake: Arc<Mutex<Option<Waker>>>,
}

#[derive(Clone)]
pub(crate) struct DiagnosticTargetShutdown {
    terminal: Arc<AtomicBool>,
    wake: Arc<Mutex<Option<Waker>>>,
}

impl DiagnosticTargetShutdown {
    pub(crate) fn request(&self) {
        self.terminal.store(true, Ordering::Release);
        let wake = self.wake.lock().expect("diagnostic wake").take();
        if let Some(wake) = wake {
            wake.wake();
        }
    }
}

impl DiagnosticTargetServer {
    pub(crate) async fn next(&self) -> Option<DiagnosticTargetShellRequest> {
        std::future::poll_fn(|cx| {
            let mut wake = self.wake.lock().expect("diagnostic wake");
            if self.terminal.load(Ordering::Acquire) {
                return Poll::Ready(None);
            }
            match self.receiver.try_recv() {
                Ok(request) => Poll::Ready(Some(request)),
                Err(mpsc::TryRecvError::Disconnected) => Poll::Ready(None),
                Err(mpsc::TryRecvError::Empty) => {
                    *wake = Some(cx.waker().clone());
                    Poll::Pending
                }
            }
        })
        .await
    }

    #[cfg(test)]
    pub(crate) fn test_endpoint() -> (
        Self,
        DiagnosticTargetShutdown,
        SyncSender<DiagnosticTargetShellRequest>,
    ) {
        let (sender, receiver) = mpsc::sync_channel(1);
        let terminal = Arc::new(AtomicBool::new(false));
        let wake = Arc::new(Mutex::new(None));
        let signal = DiagnosticTargetShutdown {
            terminal: terminal.clone(),
            wake: wake.clone(),
        };
        let server = Self {
            receiver,
            terminal,
            wake,
        };
        (server, signal, sender)
    }
}

pub(crate) enum DiagnosticTargetShellRequest {
    Execute(DiagnosticTargetCommandRequest),
    Shutdown,
}

pub(crate) struct DiagnosticTargetCommandRequest {
    request: DiagnosticProtocolRequest,
    response_sender: SyncSender<DiagnosticProtocolResponse>,
    control: Arc<DiagnosticTargetRequestControl>,
}

struct DiagnosticTargetRequestControl {
    state: AtomicU8,
    expires_at: Instant,
}

pub(crate) fn spawn_diagnostic_target_stdio_server() -> io::Result<DiagnosticTargetServer> {
    let (sender, receiver) = mpsc::sync_channel(DIAGNOSTIC_TARGET_REQUEST_QUEUE_CAPACITY);
    let terminal = Arc::new(AtomicBool::new(false));
    let wake = Arc::new(Mutex::new(None));
    let shell_sender = DiagnosticTargetShellRequestSender {
        sender,
        response_timeout: DIAGNOSTIC_TARGET_RESPONSE_TIMEOUT,
        terminal: terminal.clone(),
        wake: wake.clone(),
    };
    thread::Builder::new()
        .name("beryl-diagnostic-stdio".into())
        .spawn(move || {
            run_diagnostic_target_stdio_loop(shell_sender, io::stdin(), io::stdout());
        })?;
    Ok(DiagnosticTargetServer {
        receiver,
        terminal,
        wake,
    })
}

impl DiagnosticTargetCommandRequest {
    pub(crate) fn request(&self) -> &DiagnosticProtocolRequest {
        &self.request
    }

    pub(crate) fn try_claim(&self) -> bool {
        self.control.try_claim()
    }

    pub(crate) fn respond(self, response: DiagnosticProtocolResponse) {
        let _ = self.response_sender.try_send(response);
    }
}

impl DiagnosticTargetShellRequestSender {
    fn request(&self, request: DiagnosticProtocolRequest) -> DiagnosticProtocolResponse {
        let request_id = request.id().to_string();
        let (response_sender, response_receiver) = mpsc::sync_channel(1);
        let control = Arc::new(DiagnosticTargetRequestControl::new(self.response_timeout));
        let shell_request = DiagnosticTargetShellRequest::Execute(DiagnosticTargetCommandRequest {
            request,
            response_sender,
            control: control.clone(),
        });

        match self.sender.try_send(shell_request) {
            Ok(()) => {
                let wake = self.wake.lock().expect("diagnostic wake").take();
                if let Some(wake) = wake {
                    wake.wake();
                }
            }
            Err(TrySendError::Full(_)) => {
                return DiagnosticProtocolResponse::error(
                    Some(request_id),
                    "shell_busy",
                    "Beryl diagnostic target shell request bridge is busy.",
                );
            }
            Err(TrySendError::Disconnected(_)) => {
                return DiagnosticProtocolResponse::error(
                    Some(request_id),
                    "shell_unavailable",
                    "Beryl diagnostic target shell stopped receiving requests.",
                );
            }
        }

        match response_receiver.recv_timeout(self.response_timeout) {
            Ok(response) => response,
            Err(_) => {
                control.cancel();
                DiagnosticProtocolResponse::error(
                    Some(request_id),
                    "shell_timeout",
                    "Timed out waiting for Beryl diagnostic target shell response.",
                )
            }
        }
    }

    fn shutdown(&self) {
        DiagnosticTargetShutdown {
            terminal: self.terminal.clone(),
            wake: self.wake.clone(),
        }
        .request();
        let _ = self.sender.try_send(DiagnosticTargetShellRequest::Shutdown);
    }
}

impl DiagnosticTargetRequestControl {
    fn new(timeout: Duration) -> Self {
        Self {
            state: AtomicU8::new(DIAGNOSTIC_TARGET_REQUEST_PENDING),
            expires_at: Instant::now() + timeout,
        }
    }

    fn cancel(&self) {
        let _ = self.state.compare_exchange(
            DIAGNOSTIC_TARGET_REQUEST_PENDING,
            DIAGNOSTIC_TARGET_REQUEST_CANCELLED,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    fn try_claim(&self) -> bool {
        if Instant::now() >= self.expires_at {
            self.cancel();
            return false;
        }
        self.state
            .compare_exchange(
                DIAGNOSTIC_TARGET_REQUEST_PENDING,
                DIAGNOSTIC_TARGET_REQUEST_CLAIMED,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }
}

fn run_diagnostic_target_stdio_loop(
    shell_sender: DiagnosticTargetShellRequestSender,
    input: impl Read,
    mut output: impl Write,
) {
    let mut reader = BufReader::new(input);
    let mut handshaken = false;
    loop {
        let read = read_bounded_line_bytes(&mut reader, MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES);
        let response = match read {
            Ok(BoundedLineRead::Eof) => {
                shell_sender.shutdown();
                break;
            }
            Ok(BoundedLineRead::Line(line)) => match parse_request_frame(&line) {
                Ok(Some(request)) if request.command() == DiagnosticChildCommand::Handshake => {
                    handshaken = true;
                    Some(handshake_response(request.id()))
                }
                Ok(Some(request)) if handshaken => Some(shell_sender.request(request)),
                Ok(Some(request)) => Some(DiagnosticProtocolResponse::error(
                    Some(request.id().to_owned()),
                    "handshake_required",
                    "The diagnostic protocol handshake is required.",
                )),
                Ok(None) => None,
                Err(error) => Some(protocol_error_response(error)),
            },
            Ok(BoundedLineRead::LineTooLong { .. }) => Some(protocol_error_response(
                DiagnosticProtocolError::FrameTooLarge {
                    limit: MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES,
                },
            )),
            Err(_) => {
                shell_sender.shutdown();
                break;
            }
        };

        let Some(response) = response else {
            continue;
        };
        if write_response_frame(&mut output, response).is_err() {
            shell_sender.shutdown();
            break;
        }
    }
}

fn protocol_error_response(error: DiagnosticProtocolError) -> DiagnosticProtocolResponse {
    DiagnosticProtocolResponse::error(None, error.kind(), error.to_string())
}

fn handshake_response(request_id: &str) -> DiagnosticProtocolResponse {
    DiagnosticProtocolResponse::success(
        request_id,
        json!({
            "protocol": DIAGNOSTIC_CHILD_PROTOCOL_NAME,
            "protocolVersion": DIAGNOSTIC_CHILD_PROTOCOL_VERSION,
        }),
    )
}

#[cfg(test)]
#[path = "../tests/unit/diagnostic_target.rs"]
mod tests;
