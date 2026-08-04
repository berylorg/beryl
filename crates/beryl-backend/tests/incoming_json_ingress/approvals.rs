use beryl_backend::{
    ApprovalRequestKind, ApprovalRequestSchemaError,
    lifecycle_test_support::{IncomingJsonExpectation, IncomingJsonTestOutcome},
};

use crate::support::{FRAGMENT_SIZES, decode};

#[test]
fn canonical_approval_is_normalized_only_after_complete_fragmented_input() {
    let input = r#"{"method":"item/permissions/requestApproval","id":17,"params":{"threadId":"thread_1","turnId":"turn_1","permissions":{"network":{"enabled":true}}}}"#;

    for &fragment_bytes in FRAGMENT_SIZES {
        let result = decode(input, fragment_bytes, IncomingJsonExpectation::Idle);
        let IncomingJsonTestOutcome::Approval {
            request,
            responder_matches,
            maximum_buffered_input_bytes,
        } = result.outcome
        else {
            panic!("expected compact approval");
        };
        assert_eq!(request.kind(), ApprovalRequestKind::Permissions);
        assert_eq!(request.request_id().as_i64(), Some(17));
        assert_eq!(
            request.thread_id().map(|identity| identity.as_str()),
            Some("thread_1"),
        );
        assert_eq!(
            request.turn_id().map(|identity| identity.as_str()),
            Some("turn_1"),
        );
        assert_eq!(request.item_id(), None);
        assert!(responder_matches);
        assert!(maximum_buffered_input_bytes <= fragment_bytes.max(1));
        assert_eq!(result.expectation_after, IncomingJsonExpectation::Idle);
        assert_eq!(result.consumed_input_bytes, input.len());
    }
}

#[test]
fn arbitrary_approval_payload_is_discarded_without_entering_compact_request_state() {
    let opaque = "x".repeat(64 * 1024);
    let input = format!(
        "{{\"method\":\"item/commandExecution/requestApproval\",\"id\":1,\"params\":{{\"threadId\":\"thread_1\",\"command\":\"{opaque}\"}}}}",
    );
    let result = decode(&input, 31, IncomingJsonExpectation::Idle);
    let IncomingJsonTestOutcome::Approval {
        request,
        maximum_buffered_input_bytes,
        ..
    } = result.outcome
    else {
        panic!("expected compact approval");
    };
    assert_eq!(request.kind(), ApprovalRequestKind::CommandExecution);
    assert!(maximum_buffered_input_bytes <= 31);
    assert_eq!(result.consumed_input_bytes, input.len());
}

#[test]
fn malformed_known_approval_is_typed_and_connection_terminal() {
    let input = r#"{"method":"item/fileChange/requestApproval","id":1}"#;
    let result = decode(input, 2, IncomingJsonExpectation::Initialize { id: 9 });
    assert!(matches!(
        result.outcome,
        IncomingJsonTestOutcome::ApprovalError {
            kind: ApprovalRequestKind::FileChange,
            source: ApprovalRequestSchemaError::MissingParams,
        }
    ));
    assert_eq!(result.expectation_after, IncomingJsonExpectation::Poisoned);
    assert_eq!(result.consumed_input_bytes, input.len());
}
