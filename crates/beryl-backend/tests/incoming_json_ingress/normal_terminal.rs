use beryl_backend::{
    ApprovalInterruption, ApprovalOperationCompletion, CodexErrorInfo, ForegroundIngressError,
    ManagedBackendError, NORMAL_TURN_TERMINAL_DIAGNOSTIC_MAX_BYTES, NormalTurnTerminal,
    NormalTurnTerminalStatus, OrderedTurnStreamCompletion, OrderedTurnStreamOperation,
    OrderedTurnStreamProgress, OrderedTurnStreamRejection, OrderedTurnStreamSink,
    OrderedTurnStreamSubmitCause, OrderedTurnStreamSubmitError, TerminalNonSteerableTurnKind,
    lifecycle_test_support::{
        decode_provider_json_at_split_for_test, decode_provider_json_for_test,
        decode_provider_json_without_sink_for_test, decode_provider_transport_loss_for_test,
    },
};

#[path = "normal_terminal/failures.rs"]
mod failures;

#[derive(Debug, Eq, PartialEq)]
struct TerminalTrace {
    thread_id: String,
    turn_id: String,
    status: NormalTurnTerminalStatus,
    message: Option<String>,
    message_truncated: bool,
    error_info: Option<CodexErrorInfo>,
    details: Option<String>,
    details_truncated: bool,
}

#[derive(Clone, Copy)]
enum SinkResponse {
    Applied,
    Reject(OrderedTurnStreamSubmitCause),
    UnexpectedCompletion,
}

struct TerminalSink {
    response: SinkResponse,
    calls: usize,
    traces: Vec<TerminalTrace>,
}

impl TerminalSink {
    const fn applied() -> Self {
        Self {
            response: SinkResponse::Applied,
            calls: 0,
            traces: Vec::new(),
        }
    }

    const fn with_response(response: SinkResponse) -> Self {
        Self {
            response,
            calls: 0,
            traces: Vec::new(),
        }
    }
}

impl OrderedTurnStreamSink for TerminalSink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        self.calls += 1;
        let OrderedTurnStreamOperation::NormalTurnTerminal(terminal) = operation else {
            return Err(OrderedTurnStreamSubmitError::new(
                operation,
                OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::SchemaMismatch),
            ));
        };
        match self.response {
            SinkResponse::Applied => {
                self.traces.push(trace(&terminal));
                Ok(OrderedTurnStreamCompletion::Applied)
            }
            SinkResponse::Reject(cause) => Err(OrderedTurnStreamSubmitError::new(
                OrderedTurnStreamOperation::NormalTurnTerminal(terminal),
                cause,
            )),
            SinkResponse::UnexpectedCompletion => Ok(OrderedTurnStreamCompletion::Approval(
                ApprovalOperationCompletion::Routed {
                    interruption: ApprovalInterruption::NotRequired,
                },
            )),
        }
    }
}

fn trace(terminal: &NormalTurnTerminal) -> TerminalTrace {
    TerminalTrace {
        thread_id: terminal.thread_id().as_str().to_string(),
        turn_id: terminal.turn_id().as_str().to_string(),
        status: terminal.status(),
        message: terminal.error_message().map(str::to_string),
        message_truncated: terminal.error_message_was_truncated(),
        error_info: terminal.codex_error_info().cloned(),
        details: terminal.additional_details().map(str::to_string),
        details_truncated: terminal.additional_details_was_truncated(),
    }
}

fn terminal_json(
    status: &str,
    error: &str,
    started_at: &str,
    completed_at: &str,
    duration_ms: &str,
) -> String {
    format!(
        "{{\"method\":\"turn/completed\",\"params\":{{\"threadId\":\"thread-37\",\"turn\":{{\"id\":\"turn-37\",\"items\":[],\"itemsView\":\"notLoaded\",\"status\":\"{status}\",\"error\":{error},\"startedAt\":{started_at},\"completedAt\":{completed_at},\"durationMs\":{duration_ms}}}}}}}"
    )
}

fn completed_json() -> String {
    terminal_json(
        "completed",
        "null",
        "-9223372036854775808",
        "9223372036854775807",
        "null",
    )
}

fn interrupted_json() -> String {
    terminal_json("interrupted", "null", "null", "-1", "0")
}

fn failed_json(message: &str, error_info: &str, details: Option<&str>) -> String {
    let message = serde_json::to_string(message).unwrap();
    let details = details.map_or_else(
        || "null".to_string(),
        |value| serde_json::to_string(value).unwrap(),
    );
    let error = format!(
        "{{\"message\":{message},\"codexErrorInfo\":{error_info},\"additionalDetails\":{details}}}"
    );
    terminal_json("failed", &error, "1", "2", "-3")
}

fn decode_applied(input: &str, input_buffer_bytes: usize) -> TerminalTrace {
    let mut sink = TerminalSink::applied();
    assert_eq!(
        decode_provider_json_for_test(input.as_bytes(), input_buffer_bytes, &mut sink).unwrap(),
        OrderedTurnStreamProgress::Progress
    );
    assert_eq!(sink.calls, 1);
    assert_eq!(sink.traces.len(), 1);
    sink.traces.pop().unwrap()
}

fn assert_malformed(input: &str) {
    for input_buffer_bytes in [1, 7, input.len().max(1)] {
        let mut sink = TerminalSink::applied();
        let error = decode_provider_json_for_test(input.as_bytes(), input_buffer_bytes, &mut sink)
            .unwrap_err();
        assert!(
            matches!(
                error,
                ManagedBackendError::ForegroundIngress {
                    source: ForegroundIngressError::MalformedNormalTurnTerminal,
                    ..
                }
            ),
            "unexpected error for {input}: {error:?}"
        );
        assert_eq!(sink.calls, 0, "malformed input reached the sink: {input}");
    }
}

fn assert_returned_terminal(
    error: ManagedBackendError,
    expected_cause: OrderedTurnStreamSubmitCause,
) -> TerminalTrace {
    let ManagedBackendError::OrderedTurnStream { source, .. } = error else {
        panic!("unexpected decode error: {error:?}");
    };
    let (operation, cause) = source.into_parts();
    assert_eq!(cause, expected_cause);
    let OrderedTurnStreamOperation::NormalTurnTerminal(terminal) = operation else {
        panic!("sink failure returned the wrong operation");
    };
    trace(&terminal)
}

#[test]
fn completed_and_interrupted_accept_exact_pinned_shape_and_signed_timing_bounds() {
    let completed = decode_applied(&completed_json(), 1);
    assert_eq!(
        completed,
        TerminalTrace {
            thread_id: "thread-37".to_string(),
            turn_id: "turn-37".to_string(),
            status: NormalTurnTerminalStatus::Completed,
            message: None,
            message_truncated: false,
            error_info: None,
            details: None,
            details_truncated: false,
        }
    );

    let interrupted = decode_applied(&interrupted_json(), 2);
    assert_eq!(interrupted.status, NormalTurnTerminalStatus::Interrupted);
    assert_eq!(interrupted.thread_id, "thread-37");
    assert_eq!(interrupted.turn_id, "turn-37");
    assert_eq!(interrupted.message, None);
    assert_eq!(interrupted.error_info, None);
}

#[test]
fn failed_accepts_every_closed_unit_codex_error_variant() {
    let cases = [
        (
            "contextWindowExceeded",
            CodexErrorInfo::ContextWindowExceeded,
        ),
        (
            "sessionBudgetExceeded",
            CodexErrorInfo::SessionBudgetExceeded,
        ),
        ("usageLimitExceeded", CodexErrorInfo::UsageLimitExceeded),
        ("serverOverloaded", CodexErrorInfo::ServerOverloaded),
        ("cyberPolicy", CodexErrorInfo::CyberPolicy),
        ("internalServerError", CodexErrorInfo::InternalServerError),
        ("unauthorized", CodexErrorInfo::Unauthorized),
        ("badRequest", CodexErrorInfo::BadRequest),
        ("threadRollbackFailed", CodexErrorInfo::ThreadRollbackFailed),
        ("sandboxError", CodexErrorInfo::SandboxError),
        ("other", CodexErrorInfo::Other),
    ];

    for (wire, expected) in cases {
        let error_info = format!("\"{wire}\"");
        let terminal = decode_applied(
            &failed_json("failed message", &error_info, Some("detail")),
            1,
        );
        assert_eq!(terminal.status, NormalTurnTerminalStatus::Failed);
        assert_eq!(terminal.message.as_deref(), Some("failed message"));
        assert_eq!(terminal.error_info, Some(expected));
        assert_eq!(terminal.details.as_deref(), Some("detail"));
        assert!(!terminal.message_truncated);
        assert!(!terminal.details_truncated);
    }

    let null_info = decode_applied(&failed_json("m", "null", None), 1);
    assert_eq!(null_info.error_info, None);
    assert_eq!(null_info.details, None);
}

#[test]
fn failed_accepts_every_closed_object_codex_error_variant() {
    let cases = [
        (
            r#"{"httpConnectionFailed":{"httpStatusCode":null}}"#,
            CodexErrorInfo::HttpConnectionFailed {
                http_status_code: None,
            },
        ),
        (
            r#"{"responseStreamConnectionFailed":{"httpStatusCode":0}}"#,
            CodexErrorInfo::ResponseStreamConnectionFailed {
                http_status_code: Some(0),
            },
        ),
        (
            r#"{"responseStreamDisconnected":{"httpStatusCode":65535}}"#,
            CodexErrorInfo::ResponseStreamDisconnected {
                http_status_code: Some(u16::MAX),
            },
        ),
        (
            r#"{"responseTooManyFailedAttempts":{"httpStatusCode":503}}"#,
            CodexErrorInfo::ResponseTooManyFailedAttempts {
                http_status_code: Some(503),
            },
        ),
        (
            r#"{"activeTurnNotSteerable":{"turnKind":"review"}}"#,
            CodexErrorInfo::ActiveTurnNotSteerable {
                turn_kind: TerminalNonSteerableTurnKind::Review,
            },
        ),
        (
            r#"{"activeTurnNotSteerable":{"turnKind":"compact"}}"#,
            CodexErrorInfo::ActiveTurnNotSteerable {
                turn_kind: TerminalNonSteerableTurnKind::Compact,
            },
        ),
    ];

    for (wire, expected) in cases {
        let terminal = decode_applied(&failed_json("object", wire, None), 3);
        assert_eq!(terminal.error_info, Some(expected));
    }
}

#[test]
fn every_reader_split_and_one_byte_input_buffer_submit_exactly_once() {
    let input = failed_json(
        "split évidence",
        r#"{"activeTurnNotSteerable":{"turnKind":"compact"}}"#,
        Some("tail"),
    );

    for split_at in 1..input.len() {
        let mut sink = TerminalSink::applied();
        assert_eq!(
            decode_provider_json_at_split_for_test(input.as_bytes(), split_at, &mut sink).unwrap(),
            OrderedTurnStreamProgress::Progress,
            "reader split {split_at} failed"
        );
        assert_eq!(
            sink.calls, 1,
            "reader split {split_at} submitted incorrectly"
        );
        assert_eq!(sink.traces.len(), 1);
    }

    let one_byte = decode_applied(&input, 1);
    assert_eq!(one_byte.message.as_deref(), Some("split évidence"));
    assert_eq!(one_byte.details.as_deref(), Some("tail"));
}

#[test]
fn no_incomplete_prefix_reaches_the_ordered_sink() {
    let input = failed_json("prefix", r#""serverOverloaded""#, Some("detail"));
    for prefix_bytes in 1..input.len() {
        let mut sink = TerminalSink::applied();
        let result = decode_provider_transport_loss_for_test(
            &input.as_bytes()[..prefix_bytes],
            1,
            &mut sink,
        );
        assert!(
            result.is_err(),
            "prefix {prefix_bytes} unexpectedly decoded"
        );
        assert_eq!(sink.calls, 0, "prefix {prefix_bytes} reached the sink");
    }
}

#[test]
fn diagnostic_projection_uses_one_utf8_safe_aggregate_bound() {
    let message = "a".repeat(NORMAL_TURN_TERMINAL_DIAGNOSTIC_MAX_BYTES - 6);
    let details = "é".repeat(10);
    let terminal = decode_applied(&failed_json(&message, "null", Some(&details)), 1);

    assert_eq!(terminal.message.as_deref(), Some(message.as_str()));
    assert!(!terminal.message_truncated);
    assert_eq!(terminal.details.as_deref(), Some("ééé"));
    assert!(terminal.details_truncated);
    assert_eq!(
        terminal.message.as_ref().unwrap().len() + terminal.details.as_ref().unwrap().len(),
        NORMAL_TURN_TERMINAL_DIAGNOSTIC_MAX_BYTES
    );
}

#[test]
fn a_multibyte_message_truncates_only_at_a_utf8_boundary() {
    let message = "é".repeat(NORMAL_TURN_TERMINAL_DIAGNOSTIC_MAX_BYTES / 2 + 1);
    let terminal = decode_applied(&failed_json(&message, "null", None), 3);

    let retained = terminal.message.unwrap();
    assert_eq!(retained.len(), NORMAL_TURN_TERMINAL_DIAGNOSTIC_MAX_BYTES);
    assert_eq!(
        retained,
        "é".repeat(NORMAL_TURN_TERMINAL_DIAGNOSTIC_MAX_BYTES / 2)
    );
    assert!(terminal.message_truncated);
    assert!(!terminal.details_truncated);
}

#[test]
fn exact_route_turn_shape_and_status_fail_closed() {
    let canonical = completed_json();
    let too_long_id = "x".repeat(beryl_backend::PROTOCOL_IDENTITY_MAX_BYTES + 1);
    let cases = [
        canonical.replacen(
            "\"method\":\"turn/completed\",\"params\"",
            "\"method\":\"turn/completed\",\"extra\":null,\"params\"",
            1,
        ),
        canonical.replacen(
            "\"threadId\":\"thread-37\",\"turn\"",
            "\"threadId\":\"thread-37\",\"threadId\":\"other\",\"turn\"",
            1,
        ),
        canonical.replacen(
            "\"threadId\":\"thread-37\",\"turn\"",
            "\"threadId\":\"thread-37\",\"extra\":0,\"turn\"",
            1,
        ),
        canonical.replacen("\"threadId\":\"thread-37\",", "", 1),
        canonical.replacen("\"threadId\":\"thread-37\"", "\"threadId\":\"\"", 1),
        canonical.replacen(
            "\"threadId\":\"thread-37\"",
            &format!("\"threadId\":\"{too_long_id}\""),
            1,
        ),
        canonical.replacen("\"id\":\"turn-37\"", "\"id\":7", 1),
        canonical.replacen(
            "\"id\":\"turn-37\",\"items\":[]",
            "\"items\":[],\"id\":\"turn-37\"",
            1,
        ),
        canonical.replacen("\"items\":[]", "\"items\":[{}]", 1),
        canonical.replacen("\"itemsView\":\"notLoaded\"", "\"itemsView\":\"loaded\"", 1),
        canonical.replacen(
            "\"items\":[],\"itemsView\":\"notLoaded\"",
            "\"itemsView\":\"notLoaded\",\"items\":[]",
            1,
        ),
        canonical.replacen("\"status\":\"completed\"", "\"status\":\"inProgress\"", 1),
        canonical.replacen(
            "\"status\":\"completed\"",
            "\"status\":\"completed\",\"status\":\"completed\"",
            1,
        ),
        canonical.replacen(",\"error\":null", "", 1),
        canonical.replacen(
            "\"durationMs\":null}",
            "\"durationMs\":null,\"extra\":0}",
            1,
        ),
    ];

    for input in cases {
        assert_malformed(&input);
    }
}
