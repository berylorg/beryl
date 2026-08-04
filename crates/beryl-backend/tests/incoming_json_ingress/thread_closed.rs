use beryl_backend::{
    ApprovalInterruption, ApprovalOperationCompletion, ForegroundIngressError, ManagedBackendError,
    OrderedTurnStreamCompletion, OrderedTurnStreamOperation, OrderedTurnStreamProgress,
    OrderedTurnStreamRejection, OrderedTurnStreamSink, OrderedTurnStreamSubmitCause,
    OrderedTurnStreamSubmitError, PROTOCOL_IDENTITY_MAX_BYTES,
    lifecycle_test_support::{
        decode_provider_json_at_split_for_test, decode_provider_json_for_test,
        decode_provider_json_without_sink_for_test, thread_closed_operation,
    },
};
use beryl_model::CasThreadId;

#[derive(Clone, Copy)]
enum SinkResponse {
    Applied,
    Reject(OrderedTurnStreamSubmitCause),
    UnexpectedCompletion,
}

struct CloseSink {
    response: SinkResponse,
    calls: usize,
    thread_ids: Vec<String>,
}

impl CloseSink {
    const fn applied() -> Self {
        Self {
            response: SinkResponse::Applied,
            calls: 0,
            thread_ids: Vec::new(),
        }
    }

    const fn with_response(response: SinkResponse) -> Self {
        Self {
            response,
            calls: 0,
            thread_ids: Vec::new(),
        }
    }
}

impl OrderedTurnStreamSink for CloseSink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        self.calls += 1;
        let OrderedTurnStreamOperation::ThreadClosed(closed) = operation else {
            return Err(OrderedTurnStreamSubmitError::new(
                operation,
                OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::SchemaMismatch),
            ));
        };
        match self.response {
            SinkResponse::Applied => {
                self.thread_ids
                    .push(closed.thread_id().as_str().to_string());
                Ok(OrderedTurnStreamCompletion::Applied)
            }
            SinkResponse::Reject(cause) => Err(OrderedTurnStreamSubmitError::new(
                OrderedTurnStreamOperation::ThreadClosed(closed),
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

fn close_json(thread_id: &str) -> String {
    format!(r#"{{"method":"thread/closed","params":{{"threadId":"{thread_id}"}}}}"#)
}

fn assert_malformed(input: &str) {
    for input_buffer_bytes in [1, 7, input.len().max(1)] {
        let mut sink = CloseSink::applied();
        let error = decode_provider_json_for_test(input.as_bytes(), input_buffer_bytes, &mut sink)
            .unwrap_err();
        assert!(
            matches!(
                error,
                ManagedBackendError::ForegroundIngress {
                    source: ForegroundIngressError::MalformedThreadClosed,
                    ..
                }
            ),
            "unexpected error for {input}: {error:?}"
        );
        assert_eq!(sink.calls, 0, "malformed close reached the sink: {input}");
    }
}

fn returned_close(
    error: ManagedBackendError,
    expected_cause: OrderedTurnStreamSubmitCause,
) -> String {
    let ManagedBackendError::OrderedTurnStream { source, .. } = error else {
        panic!("unexpected decode error: {error:?}");
    };
    let (operation, cause) = source.into_parts();
    assert_eq!(cause, expected_cause);
    let OrderedTurnStreamOperation::ThreadClosed(closed) = operation else {
        panic!("sink failure returned the wrong operation");
    };
    closed.thread_id().as_str().to_string()
}

#[test]
fn lifecycle_test_helper_constructs_the_exact_ordered_close() {
    let operation = thread_closed_operation(CasThreadId::new("thread-helper").unwrap());
    let OrderedTurnStreamOperation::ThreadClosed(closed) = operation else {
        panic!("helper returned the wrong ordered operation");
    };
    assert_eq!(closed.thread_id().as_str(), "thread-helper");
}

#[test]
fn valid_close_is_incremental_and_applied_in_source_order() {
    let mut sink = CloseSink::applied();
    for thread_id in ["thread-first", "thread-second", "thread-third"] {
        assert_eq!(
            decode_provider_json_for_test(close_json(thread_id).as_bytes(), 1, &mut sink).unwrap(),
            OrderedTurnStreamProgress::Progress
        );
    }

    assert_eq!(sink.calls, 3);
    assert_eq!(
        sink.thread_ids,
        ["thread-first", "thread-second", "thread-third"]
    );

    let input = close_json("thread-split");
    for split_at in 1..input.len() {
        let mut split_sink = CloseSink::applied();
        assert_eq!(
            decode_provider_json_at_split_for_test(input.as_bytes(), split_at, &mut split_sink)
                .unwrap(),
            OrderedTurnStreamProgress::Progress,
            "reader split {split_at} failed"
        );
        assert_eq!(split_sink.calls, 1);
        assert_eq!(split_sink.thread_ids, ["thread-split"]);
    }
}

#[test]
fn exact_field_order_and_shape_are_required() {
    let canonical = close_json("thread-close");
    let cases = [
        canonical.replacen(
            "\"method\":\"thread/closed\",\"params\"",
            "\"method\":\"thread/closed\",\"extra\":null,\"params\"",
            1,
        ),
        canonical.replacen(
            "\"threadId\":\"thread-close\"",
            "\"extra\":null,\"threadId\":\"thread-close\"",
            1,
        ),
        canonical.replacen(
            "\"threadId\":\"thread-close\"",
            "\"threadId\":\"thread-close\",\"extra\":null",
            1,
        ),
        r#"{"method":"thread/closed","params":[]}"#.to_string(),
        canonical.replacen("\"threadId\":\"thread-close\"", "\"threadId\":7", 1),
        canonical.replacen("\"threadId\":\"thread-close\"", "", 1),
        canonical.replacen("\"threadId\":\"thread-close\"", "\"threadId\":\"\"", 1),
        canonical.replacen("}}", "},\"extra\":null}", 1),
    ];

    for input in cases {
        assert_malformed(&input);
    }
}

#[test]
fn identity_uses_the_existing_fixed_protocol_bound() {
    let maximum = "m".repeat(PROTOCOL_IDENTITY_MAX_BYTES);
    let mut sink = CloseSink::applied();
    assert_eq!(
        decode_provider_json_for_test(close_json(&maximum).as_bytes(), 1, &mut sink).unwrap(),
        OrderedTurnStreamProgress::Progress
    );
    assert_eq!(sink.thread_ids, [maximum]);

    let oversized = "x".repeat(PROTOCOL_IDENTITY_MAX_BYTES + 1);
    assert_malformed(&close_json(&oversized));
}

#[test]
fn sink_rejection_returns_exact_close_ownership() {
    let cause = OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::StagingConflict);
    let mut sink = CloseSink::with_response(SinkResponse::Reject(cause));
    let error = decode_provider_json_for_test(close_json("thread-owned").as_bytes(), 3, &mut sink)
        .unwrap_err();

    assert_eq!(sink.calls, 1);
    assert_eq!(returned_close(error, cause), "thread-owned");
}

#[test]
fn classifier_targets_close_instead_of_discarding_it_as_unavailable() {
    let error =
        decode_provider_json_without_sink_for_test(close_json("thread-unbound").as_bytes(), 1)
            .unwrap_err();

    assert_eq!(
        returned_close(error, OrderedTurnStreamSubmitCause::Unavailable),
        "thread-unbound"
    );

    let reordered = r#"{"params":{"threadId":"thread-unbound"},"method":"thread/closed"}"#;
    let mut sink = CloseSink::applied();
    let error = decode_provider_json_for_test(reordered.as_bytes(), 1, &mut sink).unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::ForegroundIngress {
            source: ForegroundIngressError::Quarantined,
            ..
        }
    ));
    assert_eq!(sink.calls, 0);
}

#[test]
fn applied_is_the_only_valid_sink_acknowledgement() {
    let mut sink = CloseSink::with_response(SinkResponse::UnexpectedCompletion);
    let error = decode_provider_json_for_test(close_json("thread-ack").as_bytes(), 1, &mut sink)
        .unwrap_err();

    assert!(matches!(
        error,
        ManagedBackendError::OrderedTurnStreamUnexpectedCompletion { .. }
    ));
    assert_eq!(sink.calls, 1);
}
