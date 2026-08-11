use beryl_backend::{
    ForegroundIngressError,
    lifecycle_test_support::{
        IncomingJsonExpectation, IncomingJsonTestOutcome,
        decode_incoming_json_transport_loss_for_test,
    },
};

use crate::support::{FRAGMENT_SIZES, assert_ingress_error, decode};

#[path = "responses/initialize_config.rs"]
mod initialize_config;
#[path = "responses/lineage.rs"]
mod lineage;
#[path = "responses/model_page.rs"]
mod model_page;
#[path = "responses/thread_read.rs"]
mod thread_read;
#[path = "responses/turn_interrupt.rs"]
mod turn_interrupt;
#[path = "responses/turn_start.rs"]
mod turn_start;
#[path = "responses/turn_steer.rs"]
mod turn_steer;

#[test]
fn idle_and_mismatched_success_responses_do_not_publish_results() {
    let input = r#"{"id":18,"result":{"private":"discarded"}}"#;
    assert_ingress_error(
        decode(input, 1, IncomingJsonExpectation::Idle),
        ForegroundIngressError::IdleResponse,
        IncomingJsonExpectation::Idle,
        input.len(),
    );
    assert_ingress_error(
        decode(input, 1, IncomingJsonExpectation::Initialize { id: 17 }),
        ForegroundIngressError::ResponseIdMismatch {
            expected: 17,
            actual: Some(18),
        },
        IncomingJsonExpectation::Poisoned,
        input.len(),
    );
}

#[test]
fn reordered_duplicate_and_trailing_success_discriminants_fail_closed() {
    let cases = [
        r#"{"result":{},"id":17}"#,
        r#"{"id":17,"result":{},"id":17}"#,
        r#"{"id":17,"result":{},"extra":true}"#,
    ];

    for input in cases {
        let result = decode(input, 1, IncomingJsonExpectation::Initialize { id: 17 });
        assert!(matches!(
            result.outcome,
            IncomingJsonTestOutcome::IngressError(
                ForegroundIngressError::Quarantined | ForegroundIngressError::MalformedResponse
            )
        ));
        assert_eq!(result.expectation_after, IncomingJsonExpectation::Poisoned);
        assert_eq!(result.consumed_input_bytes, input.len());
    }
}

#[test]
fn matching_rejection_publishes_only_bounded_error_facts_after_completion() {
    let data = "d".repeat(48 * 1024);
    let input = format!(
        "{{\"error\":{{\"code\":-32001,\"data\":{{\"opaque\":\"{data}\"}},\"message\":\"denied\"}},\"id\":23}}"
    );

    for &fragment_bytes in FRAGMENT_SIZES {
        let result = decode(
            &input,
            fragment_bytes,
            IncomingJsonExpectation::TurnStart { id: 23 },
        );
        match result.outcome {
            IncomingJsonTestOutcome::Rejection {
                code,
                diagnostic,
                diagnostic_was_truncated,
                data_was_present,
                verdict,
            } => {
                assert_eq!(code, -32001);
                assert_eq!(&*diagnostic, "denied");
                assert!(!diagnostic_was_truncated);
                assert!(data_was_present);
                assert_eq!(verdict, None);
            }
            other => panic!("unexpected rejection outcome: {other:?}"),
        }
        assert_eq!(result.expectation_after, IncomingJsonExpectation::Idle);
        assert_eq!(result.consumed_input_bytes, input.len());
    }
}

#[test]
fn diagnostic_limit_is_a_contiguous_unicode_scalar_prefix() {
    let diagnostic = format!("{}\u{1F600}tail", "a".repeat(4_095));
    let input = format!("{{\"error\":{{\"code\":-1,\"message\":\"{diagnostic}\"}},\"id\":9}}");

    for &fragment_bytes in FRAGMENT_SIZES {
        let result = decode(
            &input,
            fragment_bytes,
            IncomingJsonExpectation::Initialize { id: 9 },
        );
        match result.outcome {
            IncomingJsonTestOutcome::Rejection {
                diagnostic,
                diagnostic_was_truncated,
                data_was_present,
                ..
            } => {
                assert_eq!(diagnostic.len(), 4_095);
                assert!(diagnostic.bytes().all(|byte| byte == b'a'));
                assert!(diagnostic_was_truncated);
                assert!(!data_was_present);
            }
            other => panic!("unexpected diagnostic outcome: {other:?}"),
        }
        assert_eq!(result.expectation_after, IncomingJsonExpectation::Idle);
        assert_eq!(result.consumed_input_bytes, input.len());
    }
}

#[test]
fn malformed_and_mismatched_rejections_poison_the_installed_expectation() {
    let cases = [
        r#"{"error":{"code":-1,"message":"denied"},"id":10}"#,
        r#"{"error":{"code":-1},"id":9}"#,
        r#"{"error":{"message":"denied","code":-1},"id":9}"#,
        r#"{"error":{"code":-1,"message":"denied","data":null},"id":9}"#,
        r#"{"error":{"code":-1,"message":"denied"},"id":9,"extra":true}"#,
    ];

    for input in cases {
        let result = decode(input, 1, IncomingJsonExpectation::Initialize { id: 9 });
        assert!(matches!(
            result.outcome,
            IncomingJsonTestOutcome::IngressError(
                ForegroundIngressError::ResponseIdMismatch { .. }
                    | ForegroundIngressError::MalformedResponse
            )
        ));
        assert_eq!(result.expectation_after, IncomingJsonExpectation::Poisoned);
        assert_eq!(result.consumed_input_bytes, input.len());
    }
}

#[test]
fn a_previously_poisoned_expectation_never_recovers() {
    let input = r#"{"id":9,"result":{}}"#;
    let result = decode(input, 1, IncomingJsonExpectation::Poisoned);
    assert!(matches!(
        result.outcome,
        IncomingJsonTestOutcome::IngressError(ForegroundIngressError::IdleResponse)
    ));
    assert_eq!(result.expectation_after, IncomingJsonExpectation::Poisoned);
    assert_eq!(result.consumed_input_bytes, input.len());
}

#[test]
fn transport_loss_and_fatal_interleave_poison_an_installed_expectation() {
    assert_eq!(
        decode_incoming_json_transport_loss_for_test(
            br#"{"id":9,"result":{"unfinished":"#,
            IncomingJsonExpectation::Initialize { id: 9 },
        ),
        IncomingJsonExpectation::Poisoned,
    );

    let approval =
        r#"{"method":"item/permissions/requestApproval","id":1,"params":{"threadId":false}}"#;
    let result = decode(approval, 1, IncomingJsonExpectation::Initialize { id: 9 });
    assert!(matches!(
        result.outcome,
        IncomingJsonTestOutcome::ApprovalError { .. }
    ));
    assert_eq!(result.expectation_after, IncomingJsonExpectation::Poisoned);
    assert!(result.consumed_input_bytes > 0);
    assert!(result.consumed_input_bytes <= approval.len());
}
