use std::{
    io::{self, BufReader, Read, Write},
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
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
use crate::shell::liveness_diagnostics::{
    LivenessCategory, LivenessFlags, LivenessTransition, ShellLivenessDiagnostics, shared_liveness,
};

const DIAGNOSTIC_TARGET_REQUEST_QUEUE_CAPACITY: usize = 16;
const DIAGNOSTIC_TARGET_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const DIAGNOSTIC_TARGET_REQUEST_PENDING: u8 = 0;
const DIAGNOSTIC_TARGET_REQUEST_CANCELLED: u8 = 1;
const DIAGNOSTIC_TARGET_REQUEST_CLAIMED: u8 = 2;

#[derive(Clone)]
pub(crate) struct DiagnosticTargetShellRequestSender {
    sender: SyncSender<DiagnosticTargetShellRequest>,
    response_timeout: Duration,
    diagnostics: Arc<ShellLivenessDiagnostics>,
}

pub(crate) enum DiagnosticTargetShellRequest {
    Execute(DiagnosticTargetCommandRequest),
    Shutdown,
}

pub(crate) struct DiagnosticTargetCommandRequest {
    request: DiagnosticProtocolRequest,
    response_sender: SyncSender<DiagnosticProtocolResponse>,
    control: Arc<DiagnosticTargetRequestControl>,
    diagnostics: Arc<ShellLivenessDiagnostics>,
}

struct DiagnosticTargetRequestControl {
    state: AtomicU8,
    expires_at: Instant,
}

pub(crate) fn spawn_diagnostic_target_stdio_server() -> Receiver<DiagnosticTargetShellRequest> {
    let (sender, receiver) = mpsc::sync_channel(DIAGNOSTIC_TARGET_REQUEST_QUEUE_CAPACITY);
    let shell_sender = DiagnosticTargetShellRequestSender {
        sender,
        response_timeout: DIAGNOSTIC_TARGET_RESPONSE_TIMEOUT,
        diagnostics: Arc::clone(shared_liveness()),
    };
    thread::spawn(move || {
        run_diagnostic_target_stdio_loop(shell_sender, io::stdin(), io::stdout());
    });
    receiver
}

impl DiagnosticTargetCommandRequest {
    pub(crate) fn request(&self) -> &DiagnosticProtocolRequest {
        &self.request
    }

    pub(crate) fn try_claim(&self) -> bool {
        let claimed = self.control.try_claim();
        self.diagnostics.record(
            if claimed {
                LivenessTransition::DiagnosticClaim
            } else {
                LivenessTransition::DiagnosticExpired
            },
            LivenessCategory::DiagnosticControl,
            LivenessFlags::default(),
        );
        claimed
    }

    pub(crate) fn respond(self, response: DiagnosticProtocolResponse) {
        self.diagnostics.record(
            LivenessTransition::DiagnosticResponse,
            LivenessCategory::DiagnosticControl,
            LivenessFlags::default(),
        );
        let _ = self.response_sender.send(response);
    }
}

impl DiagnosticTargetShellRequestSender {
    pub(crate) fn request(&self, request: DiagnosticProtocolRequest) -> DiagnosticProtocolResponse {
        let request_id = request.id().to_string();
        let (response_sender, response_receiver) = mpsc::sync_channel(1);
        let control = Arc::new(DiagnosticTargetRequestControl::new(self.response_timeout));
        let shell_request = DiagnosticTargetShellRequest::Execute(DiagnosticTargetCommandRequest {
            request,
            response_sender,
            control: control.clone(),
            diagnostics: Arc::clone(&self.diagnostics),
        });

        let accounting = self.diagnostics.diagnostic_enqueue_begin();
        match self.sender.try_send(shell_request) {
            Ok(()) => accounting.commit(),
            Err(TrySendError::Full(_)) => {
                accounting.rollback(true);
                return DiagnosticProtocolResponse::error(
                    Some(request_id),
                    "shell_busy",
                    "Beryl diagnostic target shell request bridge is busy.",
                );
            }
            Err(TrySendError::Disconnected(_)) => {
                accounting.rollback(false);
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
                self.diagnostics.record(
                    LivenessTransition::DiagnosticTimeout,
                    LivenessCategory::DiagnosticControl,
                    LivenessFlags::default(),
                );
                DiagnosticProtocolResponse::error(
                    Some(request_id),
                    "shell_timeout",
                    "Timed out waiting for Beryl diagnostic target shell response.",
                )
            }
        }
    }

    fn shutdown(&self) {
        let accounting = self.diagnostics.diagnostic_enqueue_begin();
        match self.sender.try_send(DiagnosticTargetShellRequest::Shutdown) {
            Ok(()) => accounting.commit(),
            Err(TrySendError::Full(_)) => accounting.rollback(true),
            Err(TrySendError::Disconnected(_)) => accounting.rollback(false),
        }
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

pub(crate) fn run_diagnostic_target_stdio_loop(
    shell_sender: DiagnosticTargetShellRequestSender,
    input: impl Read,
    mut output: impl Write,
) {
    let mut reader = BufReader::new(input);
    loop {
        let read = read_bounded_line_bytes(&mut reader, MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES);
        let response = match read {
            Ok(BoundedLineRead::Eof) => {
                shell_sender.shutdown();
                break;
            }
            Ok(BoundedLineRead::Line(line)) => match parse_request_frame(&line) {
                Ok(Some(request)) if request.command() == DiagnosticChildCommand::Handshake => {
                    Some(handshake_response(request.id()))
                }
                Ok(Some(request)) if request.command() == DiagnosticChildCommand::ReadLiveness => {
                    shell_sender.diagnostics.record(
                        LivenessTransition::DiagnosticRead,
                        LivenessCategory::DiagnosticControl,
                        LivenessFlags::default(),
                    );
                    Some(DiagnosticProtocolResponse::success(
                        request.id(),
                        shell_sender.diagnostics.snapshot_value(),
                    ))
                }
                Ok(Some(request)) => Some(shell_sender.request(request)),
                Ok(None) => None,
                Err(error) => Some(protocol_error_response(error)),
            },
            Ok(BoundedLineRead::LineTooLong { .. }) => Some(protocol_error_response(
                DiagnosticProtocolError::FrameTooLarge {
                    limit: MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES,
                },
            )),
            Err(source) => Some(DiagnosticProtocolResponse::error(
                None,
                "read_error",
                source.to_string(),
            )),
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
pub(crate) fn diagnostic_target_request_channel_for_test(
    capacity: usize,
    response_timeout: Duration,
    diagnostics: Arc<ShellLivenessDiagnostics>,
) -> (
    DiagnosticTargetShellRequestSender,
    Receiver<DiagnosticTargetShellRequest>,
) {
    let (sender, receiver) = mpsc::sync_channel(capacity);
    (
        DiagnosticTargetShellRequestSender {
            sender,
            response_timeout,
            diagnostics,
        },
        receiver,
    )
}
