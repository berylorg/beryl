use beryl_backend::{
    BoundedResponseResult, CompatibilityProbe, ForegroundIngressError, JsonRpcErrorVerdict,
    JsonRpcTurnKind,
    lifecycle_test_support::{
        IncomingJsonExpectation, IncomingJsonTestOutcome, IncomingJsonTestResult,
    },
};

use crate::support::{FRAGMENT_SIZES, decode};

#[test]
fn turn_steer_retains_only_the_exact_bounded_turn_identity() {
    let input = r#"{"id":71,"result":{"turnId":"turn-71"}}"#;

    for &fragment_bytes in FRAGMENT_SIZES {
        let result = decode(
            input,
            fragment_bytes,
            IncomingJsonExpectation::TurnSteer { id: 71 },
        );
        let IncomingJsonTestOutcome::Response {
            id,
            result: BoundedResponseResult::TurnSteer(response),
        } = result.outcome
        else {
            panic!("unexpected turn/steer response: {:?}", result.outcome);
        };
        assert_eq!(id, 71);
        assert_eq!(response.turn_id().as_str(), "turn-71");
        assert_eq!(result.expectation_after, IncomingJsonExpectation::Idle);
        assert_eq!(result.consumed_input_bytes, input.len());
        assert!(result.maximum_buffered_input_bytes <= fragment_bytes.max(4));
    }
}

#[test]
fn malformed_turn_steer_success_is_consumed_without_publication() {
    let oversized = "x".repeat(257);
    let cases = [
        r#"{"id":72,"result":[]}"#.to_owned(),
        r#"{"id":72,"result":{}}"#.to_owned(),
        r#"{"id":72,"result":{"turnId":7}}"#.to_owned(),
        r#"{"id":72,"result":{"turnId":""}}"#.to_owned(),
        r#"{"id":72,"result":{"other":"turn-72"}}"#.to_owned(),
        r#"{"id":72,"result":{"turnId":"turn-72","extra":null}}"#.to_owned(),
        r#"{"id":72,"result":{"turnId":"turn-72","turnId":"turn-72"}}"#.to_owned(),
        format!(r#"{{"id":72,"result":{{"turnId":"{oversized}"}}}}"#),
    ];

    for input in cases {
        let result = decode(&input, 1, IncomingJsonExpectation::TurnSteer { id: 72 });
        assert!(matches!(
            result.outcome,
            IncomingJsonTestOutcome::IngressError(ForegroundIngressError::MalformedResponse)
        ));
        assert_eq!(result.expectation_after, IncomingJsonExpectation::Poisoned);
        assert_eq!(result.consumed_input_bytes, input.len());
    }
}

#[test]
fn exact_active_turn_not_steerable_data_produces_a_closed_verdict() {
    let cases = [
        (
            "review",
            "null",
            JsonRpcErrorVerdict::ActiveTurnNotSteerable {
                turn_kind: JsonRpcTurnKind::Review,
            },
        ),
        (
            "compact",
            r#""compact detail""#,
            JsonRpcErrorVerdict::ActiveTurnNotSteerable {
                turn_kind: JsonRpcTurnKind::Compact,
            },
        ),
    ];

    for (turn_kind, details, expected_verdict) in cases {
        let data = exact_non_steerable_data(turn_kind, details);
        let input = rejection_json(-32_600, Some(&data), "outer diagnostic", 73);
        for &fragment_bytes in FRAGMENT_SIZES {
            assert_rejection(
                decode(
                    &input,
                    fragment_bytes,
                    IncomingJsonExpectation::TurnSteer { id: 73 },
                ),
                -32_600,
                "outer diagnostic",
                true,
                Some(expected_verdict),
                input.len(),
            );
        }
    }
}

#[test]
fn unmatched_valid_data_remains_an_exact_rejection_without_a_verdict() {
    let exact = exact_non_steerable_data("review", "null");
    let cases = [
        "null".to_owned(),
        "{}".to_owned(),
        r#"{"message":false,"codexErrorInfo":{"activeTurnNotSteerable":{"turnKind":"review"}},"additionalDetails":null}"#
            .to_owned(),
        r#"{"codexErrorInfo":{"activeTurnNotSteerable":{"turnKind":"review"}},"message":"m","additionalDetails":null}"#
            .to_owned(),
        r#"{"message":"m","codexErrorInfo":null,"additionalDetails":null}"#.to_owned(),
        r#"{"message":"m","codexErrorInfo":{"activeTurnNotSteerable":{"turnKind":"other"}},"additionalDetails":null}"#
            .to_owned(),
        r#"{"message":"m","codexErrorInfo":{"activeTurnNotSteerable":{"turnKind":"review","extra":null}},"additionalDetails":null}"#
            .to_owned(),
        r#"{"message":"m","codexErrorInfo":{"activeTurnNotSteerable":{"turnKind":"review"}},"additionalDetails":false}"#
            .to_owned(),
        r#"{"message":"m","codexErrorInfo":{"activeTurnNotSteerable":{"turnKind":"review"}}}"#
            .to_owned(),
        r#"{"message":"m","codexErrorInfo":{"activeTurnNotSteerable":{"turnKind":"review"}},"additionalDetails":null,"extra":null}"#
            .to_owned(),
        exact.replacen(
            r#""message":"steering denied""#,
            r#""message":"steering denied","message":"duplicate""#,
            1,
        ),
    ];

    for data in cases {
        let input = rejection_json(-32_600, Some(&data), "denied", 74);
        for &fragment_bytes in FRAGMENT_SIZES {
            assert_rejection(
                decode(
                    &input,
                    fragment_bytes,
                    IncomingJsonExpectation::TurnSteer { id: 74 },
                ),
                -32_600,
                "denied",
                true,
                None,
                input.len(),
            );
        }
    }
}

#[test]
fn verdict_requires_the_exact_family_code_and_structured_data() {
    let data = exact_non_steerable_data("compact", "null");

    let wrong_code = rejection_json(-32_001, Some(&data), "denied", 75);
    assert_rejection(
        decode(
            &wrong_code,
            1,
            IncomingJsonExpectation::TurnSteer { id: 75 },
        ),
        -32_001,
        "denied",
        true,
        None,
        wrong_code.len(),
    );

    let no_data = rejection_json(-32_600, None, "activeTurnNotSteerable review", 76);
    assert_rejection(
        decode(&no_data, 1, IncomingJsonExpectation::TurnSteer { id: 76 }),
        -32_600,
        "activeTurnNotSteerable review",
        false,
        None,
        no_data.len(),
    );

    let other_family = rejection_json(-32_600, Some(&data), "denied", 77);
    assert_rejection(
        decode(
            &other_family,
            1,
            IncomingJsonExpectation::TurnStart { id: 77 },
        ),
        -32_600,
        "denied",
        true,
        None,
        other_family.len(),
    );

    let compatibility = rejection_json(-32_600, Some(&data), "denied", 78);
    assert_rejection(
        decode(
            &compatibility,
            1,
            IncomingJsonExpectation::Compatibility {
                id: 78,
                probe: CompatibilityProbe::TurnSteer,
            },
        ),
        -32_600,
        "denied",
        true,
        None,
        compatibility.len(),
    );
}

fn exact_non_steerable_data(turn_kind: &str, details: &str) -> String {
    format!(
        r#"{{"message":"steering denied","codexErrorInfo":{{"activeTurnNotSteerable":{{"turnKind":"{turn_kind}"}}}},"additionalDetails":{details}}}"#
    )
}

fn rejection_json(code: i64, data: Option<&str>, diagnostic: &str, id: u64) -> String {
    let data = data.map_or_else(String::new, |data| format!(r#","data":{data}"#));
    format!(r#"{{"error":{{"code":{code}{data},"message":"{diagnostic}"}},"id":{id}}}"#)
}

fn assert_rejection(
    result: IncomingJsonTestResult,
    expected_code: i64,
    expected_diagnostic: &str,
    expected_data: bool,
    expected_verdict: Option<JsonRpcErrorVerdict>,
    input_bytes: usize,
) {
    let IncomingJsonTestOutcome::Rejection {
        code,
        diagnostic,
        diagnostic_was_truncated,
        data_was_present,
        verdict,
    } = result.outcome
    else {
        panic!("unexpected rejection outcome: {:?}", result.outcome);
    };
    assert_eq!(code, expected_code);
    assert_eq!(&*diagnostic, expected_diagnostic);
    assert!(!diagnostic_was_truncated);
    assert_eq!(data_was_present, expected_data);
    assert_eq!(verdict, expected_verdict);
    assert_eq!(result.expectation_after, IncomingJsonExpectation::Idle);
    assert_eq!(result.consumed_input_bytes, input_bytes);
}
