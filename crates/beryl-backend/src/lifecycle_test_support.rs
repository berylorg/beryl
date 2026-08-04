//! Test-only helpers for managed backend lifecycle integration tests.

use std::{
    cell::Cell,
    io::{self, Read},
    process::{Command, Stdio},
    time::Duration,
};

use beryl_model::{CasItemId, CasThreadId, CasTurnId, DynamicToolCallId, DynamicToolName};
use beryl_stream::PageLease;

use crate::{
    ApprovalRequest, ApprovalRequestId, ApprovalRequestKind, ApprovalResponseDisposition,
    ManagedBackendError, ProviderObservationFragment, ProviderValueContext,
    managed_process::SupervisedBackendProcess,
};

mod response;
mod turn;

pub use response::IncomingJsonExpectation;
use response::{test_expectation_slot, test_expectation_state};
pub use turn::{normal_turn_terminal, thread_closed_operation};

pub type LifecycleTestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Closed result of exercising the private incremental incoming-JSON boundary.
#[doc(hidden)]
#[derive(Debug)]
pub enum IncomingJsonTestOutcome {
    /// One compact approval and only the private identity/buffering facts needed by tests.
    Approval {
        request: ApprovalRequest,
        responder_matches: bool,
        maximum_buffered_input_bytes: usize,
    },
    /// One unknown or known-no-owner notification was structurally discarded.
    DiscardedNotification,
    /// One exact typed bounded response result from the non-dispatching seam.
    Response {
        id: u64,
        result: crate::BoundedResponseResult,
    },
    /// One exact matching bounded JSON-RPC rejection.
    Rejection {
        code: i64,
        diagnostic: Box<str>,
        diagnostic_was_truncated: bool,
        data_was_present: bool,
        verdict: Option<crate::JsonRpcErrorVerdict>,
    },
    /// A typed, content-free approval normalization failure.
    ApprovalError {
        kind: ApprovalRequestKind,
        source: crate::ApprovalRequestSchemaError,
    },
    /// One bounded foreground ingress outcome.
    IngressError(crate::ForegroundIngressError),
    /// A content-free JSON, correlation, or streamed-machine failure.
    OtherError,
}

/// Result plus bounded-buffering and expectation-state facts.
#[doc(hidden)]
#[derive(Debug)]
pub struct IncomingJsonTestResult {
    pub outcome: IncomingJsonTestOutcome,
    pub maximum_buffered_input_bytes: usize,
    pub consumed_input_bytes: usize,
    pub expectation_after: IncomingJsonExpectation,
}

/// Exercises the private incremental incoming-JSON decoder without exposing its implementation.
#[doc(hidden)]
pub fn decode_incoming_json_for_test(
    input: &[u8],
    input_buffer_bytes: usize,
    expectation: IncomingJsonExpectation,
) -> IncomingJsonTestResult {
    let mut slot = test_expectation_slot(expectation);
    let consumed = Cell::new(0);
    let decoded = crate::incoming_json::decode_reader(
        CountingReader::new(input, &consumed),
        input_buffer_bytes,
        &mut slot,
    );
    let (outcome, maximum_buffered_input_bytes, consumed_input_bytes) = match decoded {
        Ok(decoded) => match decoded.incoming {
            crate::incoming_json::DecodedIncoming::Approval(approval) => {
                let (request, responder) = approval.into_parts();
                let responder_matches = responder.matches(&request);
                (
                    IncomingJsonTestOutcome::Approval {
                        request,
                        responder_matches,
                        maximum_buffered_input_bytes: decoded.stats.maximum_buffered_input_bytes,
                    },
                    decoded.stats.maximum_buffered_input_bytes,
                    decoded.stats.input_bytes,
                )
            }
            crate::incoming_json::DecodedIncoming::DiscardedNotification => (
                IncomingJsonTestOutcome::DiscardedNotification,
                decoded.stats.maximum_buffered_input_bytes,
                decoded.stats.input_bytes,
            ),
            crate::incoming_json::DecodedIncoming::Response { id, result } => (
                IncomingJsonTestOutcome::Response { id, result },
                decoded.stats.maximum_buffered_input_bytes,
                decoded.stats.input_bytes,
            ),
            crate::incoming_json::DecodedIncoming::Rejection { error, .. } => {
                let outcome = IncomingJsonTestOutcome::Rejection {
                    code: error.code(),
                    diagnostic: error.message().into(),
                    diagnostic_was_truncated: error.message_was_truncated(),
                    data_was_present: error.data_was_present(),
                    verdict: error.verdict(),
                };
                (
                    outcome,
                    decoded.stats.maximum_buffered_input_bytes,
                    decoded.stats.input_bytes,
                )
            }
            crate::incoming_json::DecodedIncoming::OrderedHandled => (
                IncomingJsonTestOutcome::OtherError,
                decoded.stats.maximum_buffered_input_bytes,
                decoded.stats.input_bytes,
            ),
        },
        Err(crate::incoming_json::DecodeReaderError::Approval { kind, source }) => (
            IncomingJsonTestOutcome::ApprovalError { kind, source },
            0,
            consumed.get(),
        ),
        Err(crate::incoming_json::DecodeReaderError::Envelope(source)) => (
            IncomingJsonTestOutcome::IngressError(source),
            0,
            consumed.get(),
        ),
        Err(
            crate::incoming_json::DecodeReaderError::Json(_)
            | crate::incoming_json::DecodeReaderError::Correlation(_)
            | crate::incoming_json::DecodeReaderError::Steering(_)
            | crate::incoming_json::DecodeReaderError::Provider(_)
            | crate::incoming_json::DecodeReaderError::DynamicTool(_)
            | crate::incoming_json::DecodeReaderError::Ordered(_)
            | crate::incoming_json::DecodeReaderError::OrderedUnexpectedCompletion,
        ) => (IncomingJsonTestOutcome::OtherError, 0, consumed.get()),
    };
    let expectation_after = test_expectation_state(&slot, expectation);
    IncomingJsonTestResult {
        outcome,
        maximum_buffered_input_bytes,
        consumed_input_bytes,
        expectation_after,
    }
}

/// Forces reader loss after one incomplete prefix and returns the terminal expectation state.
#[doc(hidden)]
pub fn decode_incoming_json_transport_loss_for_test(
    prefix: &[u8],
    expectation: IncomingJsonExpectation,
) -> IncomingJsonExpectation {
    let mut slot = test_expectation_slot(expectation);
    let result = crate::incoming_json::decode_reader(
        TransportLossReader { remaining: prefix },
        3,
        &mut slot,
    );
    assert!(result.is_err(), "forced transport loss must fail decoding");
    test_expectation_state(&slot, expectation)
}

/// Exercises one provider-capable message without constructing an initialized live session.
#[doc(hidden)]
pub fn decode_provider_json_for_test(
    input: &[u8],
    input_buffer_bytes: usize,
    sink: &mut dyn crate::OrderedTurnStreamSink,
) -> Result<crate::OrderedTurnStreamProgress, ManagedBackendError> {
    decode_provider_reader_for_test(
        CountingReader::new(input, &Cell::new(0)),
        input_buffer_bytes,
        Some(sink),
    )
}

/// Exercises one provider-capable message with a prescribed first reader split.
#[doc(hidden)]
pub fn decode_provider_json_at_split_for_test(
    input: &[u8],
    split_at: usize,
    sink: &mut dyn crate::OrderedTurnStreamSink,
) -> Result<crate::OrderedTurnStreamProgress, ManagedBackendError> {
    assert!(split_at > 0 && split_at < input.len());
    decode_provider_reader_for_test(
        SplitReader::new(input, split_at),
        input.len().max(1),
        Some(sink),
    )
}

/// Exercises one provider-capable message without an ordered sink.
#[doc(hidden)]
pub fn decode_provider_json_without_sink_for_test(
    input: &[u8],
    input_buffer_bytes: usize,
) -> Result<crate::OrderedTurnStreamProgress, ManagedBackendError> {
    decode_provider_reader_for_test(
        CountingReader::new(input, &Cell::new(0)),
        input_buffer_bytes,
        None,
    )
}

/// Forces transport loss after a provider prefix without reviving a live initialize fixture.
#[doc(hidden)]
pub fn decode_provider_transport_loss_for_test(
    prefix: &[u8],
    input_buffer_bytes: usize,
    sink: &mut dyn crate::OrderedTurnStreamSink,
) -> Result<crate::OrderedTurnStreamProgress, ManagedBackendError> {
    decode_provider_reader_for_test(
        TransportLossReader { remaining: prefix },
        input_buffer_bytes,
        Some(sink),
    )
}

fn decode_provider_reader_for_test(
    reader: impl Read,
    input_buffer_bytes: usize,
    sink: Option<&mut dyn crate::OrderedTurnStreamSink>,
) -> Result<crate::OrderedTurnStreamProgress, ManagedBackendError> {
    let mut expectation = crate::incoming_json::ResponseExpectationSlot::default();
    match crate::incoming_json::decode_reader_with_provider(
        reader,
        input_buffer_bytes,
        None,
        sink,
        1,
        &mut expectation,
    ) {
        Ok(decoded) => match decoded.incoming {
            crate::incoming_json::DecodedIncoming::OrderedHandled
            | crate::incoming_json::DecodedIncoming::DiscardedNotification => {
                Ok(crate::OrderedTurnStreamProgress::Progress)
            }
            _ => Err(ManagedBackendError::UnexpectedMessageShape),
        },
        Err(crate::incoming_json::DecodeReaderError::Provider(source)) => {
            Err(ManagedBackendError::ProviderObservation {
                method: "provider lifecycle test".to_string(),
                source,
            })
        }
        Err(crate::incoming_json::DecodeReaderError::Steering(source)) => {
            Err(ManagedBackendError::SteeringUserMessage {
                method: "provider lifecycle test".to_string(),
                source,
            })
        }
        Err(crate::incoming_json::DecodeReaderError::DynamicTool(source)) => {
            Err(ManagedBackendError::DynamicToolCall {
                method: "provider lifecycle test".to_string(),
                source,
            })
        }
        Err(crate::incoming_json::DecodeReaderError::Ordered(source)) => {
            Err(ManagedBackendError::OrderedTurnStream {
                method: "provider lifecycle test".to_string(),
                source,
            })
        }
        Err(crate::incoming_json::DecodeReaderError::OrderedUnexpectedCompletion) => {
            Err(ManagedBackendError::OrderedTurnStreamUnexpectedCompletion {
                method: "provider lifecycle test".to_string(),
            })
        }
        Err(crate::incoming_json::DecodeReaderError::Envelope(source)) => {
            Err(ManagedBackendError::ForegroundIngress {
                method: "provider lifecycle test".to_string(),
                source,
            })
        }
        Err(crate::incoming_json::DecodeReaderError::Correlation(source)) => {
            Err(ManagedBackendError::StreamedUserMessageCorrelation {
                method: "provider lifecycle test".to_string(),
                source,
                transport_bytes_written: true,
            })
        }
        Err(crate::incoming_json::DecodeReaderError::Approval { kind, source }) => {
            Err(ManagedBackendError::InvalidApprovalRequest { kind, source })
        }
        Err(crate::incoming_json::DecodeReaderError::Json(source)) => {
            Err(ManagedBackendError::InvalidJsonLine {
                line: crate::incoming_json::redacted_invalid_json(),
                source,
            })
        }
    }
}

struct CountingReader<'a> {
    remaining: &'a [u8],
    consumed: &'a Cell<usize>,
}

impl<'a> CountingReader<'a> {
    const fn new(input: &'a [u8], consumed: &'a Cell<usize>) -> Self {
        Self {
            remaining: input,
            consumed,
        }
    }
}

impl Read for CountingReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = output.len().min(self.remaining.len());
        output[..count].copy_from_slice(&self.remaining[..count]);
        self.remaining = &self.remaining[count..];
        self.consumed.set(self.consumed.get() + count);
        Ok(count)
    }
}

struct SplitReader<'a> {
    first: &'a [u8],
    second: &'a [u8],
}

impl<'a> SplitReader<'a> {
    fn new(input: &'a [u8], split_at: usize) -> Self {
        let (first, second) = input.split_at(split_at);
        Self { first, second }
    }
}

impl Read for SplitReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let reading_first = !self.first.is_empty();
        let remaining = if reading_first {
            self.first
        } else {
            self.second
        };
        let count = output.len().min(remaining.len());
        output[..count].copy_from_slice(&remaining[..count]);
        if reading_first {
            self.first = &self.first[count..];
        } else {
            self.second = &self.second[count..];
        }
        Ok(count)
    }
}

struct TransportLossReader<'a> {
    remaining: &'a [u8],
}

impl Read for TransportLossReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.remaining.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "forced lifecycle transport loss",
            ));
        }
        let count = output.len().min(self.remaining.len());
        output[..count].copy_from_slice(&self.remaining[..count]);
        self.remaining = &self.remaining[count..];
        Ok(count)
    }
}

/// Constructs one real provider fragment for cross-crate lifecycle tests.
#[must_use]
pub const fn provider_observation_fragment(
    context: ProviderValueContext,
    lease: PageLease,
) -> ProviderObservationFragment {
    ProviderObservationFragment::new(context, lease)
}

/// Constructs compact checked submitted-user evidence for cross-crate lifecycle tests.
#[must_use]
pub fn checked_user_message(
    lifecycle: crate::UserMessageEchoLifecycle,
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    item_id: CasItemId,
    timestamp_ms: u64,
    checked_input_items: u64,
) -> crate::CheckedUserMessage {
    crate::CheckedUserMessage::for_lifecycle_test(
        lifecycle,
        thread_id,
        turn_id,
        crate::ItemLifecycleTimestampMs::new(timestamp_ms),
        item_id,
        checked_input_items,
    )
}

/// Constructs a compact approval request for cross-crate lifecycle tests.
#[must_use]
pub fn approval_request(
    kind: ApprovalRequestKind,
    disposition: ApprovalResponseDisposition,
    thread_id: Option<CasThreadId>,
    turn_id: Option<CasTurnId>,
    item_id: Option<CasItemId>,
) -> ApprovalRequest {
    let decoded = ApprovalRequest::decoded(
        ApprovalRequestId::Integer(1),
        kind,
        thread_id,
        turn_id,
        item_id,
    );
    let (request, _responder) = decoded.into_parts();
    request.mark_response_disposition(disposition);
    request
}

/// Constructs one incomplete dynamic-tool call for cross-crate routing tests.
#[must_use]
pub fn building_dynamic_tool_call(
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    call_id: DynamicToolCallId,
) -> crate::DynamicToolCall {
    let (call, _ingress) = crate::DynamicToolCall::decoded(
        crate::DynamicToolCallRequestId::Integer(1),
        thread_id,
        turn_id,
        call_id,
        None,
        DynamicToolName::new("lifecycle_test_tool").expect("test tool name is valid"),
        1,
    );
    call
}

/// Constructs one consumed-capability injection target without a backend response fixture.
#[must_use]
pub fn fresh_idle_thread(thread_id: CasThreadId) -> crate::FreshIdleThread {
    crate::thread_lineage::fresh_idle_thread_for_lifecycle_test(thread_id)
}

/// Constructs compact metadata-only thread-read output for cross-crate lifecycle tests.
#[must_use]
pub fn thread_read_metadata(
    thread_id: CasThreadId,
    status: crate::ThreadStatus,
    model_provider: &str,
    agent_nickname: Option<&str>,
) -> crate::ThreadReadMetadata {
    crate::thread_metadata::thread_read_metadata_for_lifecycle_test(
        thread_id,
        status,
        model_provider,
        agent_nickname,
    )
}

#[derive(Debug)]
pub struct TestSupervisedBackendProcess {
    process: SupervisedBackendProcess,
}

impl TestSupervisedBackendProcess {
    pub fn process_id(&self) -> Option<u32> {
        self.process.process_id()
    }

    pub fn shutdown(
        &mut self,
        grace_timeout: Duration,
        kill_timeout: Duration,
    ) -> Result<(), ManagedBackendError> {
        self.process.shutdown(grace_timeout, kill_timeout)
    }
}

pub fn spawn_sleeping_host_process() -> LifecycleTestResult<TestSupervisedBackendProcess> {
    spawn_host_powershell_script("Start-Sleep -Seconds 60")
}

pub fn spawn_host_powershell_script(
    script: impl AsRef<str>,
) -> LifecycleTestResult<TestSupervisedBackendProcess> {
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-Command", script.as_ref()]);
    spawn_host_command(command)
}

fn spawn_host_command(mut command: Command) -> LifecycleTestResult<TestSupervisedBackendProcess> {
    command.stdin(Stdio::null());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());

    let child = command.spawn()?;
    let process = SupervisedBackendProcess::new(child, "powershell.exe", true, None)?;
    Ok(TestSupervisedBackendProcess { process })
}
