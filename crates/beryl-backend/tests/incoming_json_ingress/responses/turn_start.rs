use beryl_backend::{
    BoundedResponseResult, ForegroundIngressError, TurnStatus,
    lifecycle_test_support::{
        IncomingJsonExpectation, IncomingJsonTestOutcome, IncomingJsonTestResult,
    },
};

use crate::support::{FRAGMENT_SIZES, decode};

#[test]
fn turn_start_retains_only_bounded_identity_and_closed_status() {
    let input = r#"{"id":17,"result":{"turn":{"id":"turn-17","items":[],"status":"inProgress"}}}"#;

    for &fragment_bytes in FRAGMENT_SIZES {
        assert_turn_start(
            decode(
                input,
                fragment_bytes,
                IncomingJsonExpectation::TurnStart { id: 17 },
            ),
            17,
            "turn-17",
            TurnStatus::InProgress,
            input.len(),
            fragment_bytes,
        );
    }
}

#[test]
fn turn_start_structurally_discards_each_items_value_shape_with_bounded_buffering() {
    let large = "x".repeat(128 * 1024);
    let items = [
        "null".to_owned(),
        "true".to_owned(),
        "-1234567890.125e42".to_owned(),
        format!(r#""{large}""#),
        format!(r#"[null,{{"payload":"{large}"}},[true,false]]"#),
        format!(r#"{{"nested":[{{"payload":"{large}"}}]}}"#),
    ];

    for (index, items) in items.iter().enumerate() {
        let id = 30 + u64::try_from(index).unwrap();
        let turn_id = format!("turn-{id}");
        let input = format!(
            r#"{{"id":{id},"result":{{"turn":{{"id":"{turn_id}","items":{items},"status":"inProgress"}}}}}}"#,
        );
        for &fragment_bytes in FRAGMENT_SIZES {
            assert_turn_start(
                decode(
                    &input,
                    fragment_bytes,
                    IncomingJsonExpectation::TurnStart { id },
                ),
                id,
                &turn_id,
                TurnStatus::InProgress,
                input.len(),
                fragment_bytes,
            );
        }
    }
}

#[test]
fn malformed_or_order_incompatible_turn_start_is_consumed_without_publication() {
    let oversized_id = "i".repeat(257);
    let cases = [
        r#"{"id":47,"result":[]}"#.to_owned(),
        r#"{"id":47,"result":{}}"#.to_owned(),
        r#"{"id":47,"result":{"turn":[]}}"#.to_owned(),
        r#"{"id":47,"result":{"turn":{"items":[],"status":"inProgress"}}}"#.to_owned(),
        r#"{"id":47,"result":{"turn":{"id":"turn-47","status":"inProgress"}}}"#.to_owned(),
        r#"{"id":47,"result":{"turn":{"id":"turn-47","items":[]}}}"#.to_owned(),
        r#"{"id":47,"result":{"turn":{"items":[],"id":"turn-47","status":"inProgress"}}}"#
            .to_owned(),
        r#"{"id":47,"result":{"turn":{"id":"turn-47","status":"inProgress","items":[]}}}"#
            .to_owned(),
        r#"{"id":47,"result":{"turn":{"id":"turn-47","id":"turn-47","items":[],"status":"inProgress"}}}"#
            .to_owned(),
        r#"{"id":47,"result":{"turn":{"id":"turn-47","items":[],"items":[],"status":"inProgress"}}}"#
            .to_owned(),
        r#"{"id":47,"result":{"turn":{"id":"turn-47","items":[],"status":"inProgress","status":"inProgress"}}}"#
            .to_owned(),
        r#"{"id":47,"result":{"turn":{"id":false,"items":[],"status":"inProgress"}}}"#
            .to_owned(),
        r#"{"id":47,"result":{"turn":{"id":"turn-47","items":[],"status":false}}}"#
            .to_owned(),
        r#"{"id":47,"result":{"turn":{"id":"turn-47","items":[],"status":"future"}}}"#
            .to_owned(),
        format!(
            r#"{{"id":47,"result":{{"turn":{{"id":"{oversized_id}","items":[],"status":"inProgress"}}}}}}"#,
        ),
        r#"{"id":47,"result":{"turn":{"id":"turn-47","items":[],"status":"inProgress"},"turn":{"id":"turn-47","items":[],"status":"inProgress"}}}"#
            .to_owned(),
    ];

    for input in cases {
        let result = decode(&input, 1, IncomingJsonExpectation::TurnStart { id: 47 });
        assert!(matches!(
            result.outcome,
            IncomingJsonTestOutcome::IngressError(ForegroundIngressError::MalformedResponse)
        ));
        assert_eq!(result.expectation_after, IncomingJsonExpectation::Poisoned);
        assert_eq!(result.consumed_input_bytes, input.len());
    }
}

fn assert_turn_start(
    result: IncomingJsonTestResult,
    expected_response_id: u64,
    expected_turn_id: &str,
    expected_status: TurnStatus,
    input_bytes: usize,
    fragment_bytes: usize,
) {
    let IncomingJsonTestOutcome::Response {
        id,
        result: BoundedResponseResult::TurnStart(response),
    } = result.outcome
    else {
        panic!(
            "unexpected turn/start response outcome: {:?}",
            result.outcome
        );
    };
    assert_eq!(id, expected_response_id);
    assert_eq!(response.turn_id().as_str(), expected_turn_id);
    assert_eq!(response.status(), &expected_status);
    assert_eq!(result.expectation_after, IncomingJsonExpectation::Idle);
    assert_eq!(result.consumed_input_bytes, input_bytes);
    assert!(result.maximum_buffered_input_bytes <= fragment_bytes.max(4));
}
