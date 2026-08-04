use beryl_backend::{
    ForegroundIngressError,
    lifecycle_test_support::{IncomingJsonExpectation, IncomingJsonTestOutcome},
};

use crate::support::{FRAGMENT_SIZES, assert_discarded, assert_ingress_error, decode};

#[test]
fn unknown_and_no_owner_notifications_discard_across_fragmentation() {
    let huge = "x".repeat(64 * 1024);
    let unknown = format!(
        "{{\"method\":\"future/notification\",\"params\":{{\"nested\":[{{\"huge\":\"{huge}\"}}]}}}}"
    );
    let no_owner = r#"{"method":"thread/name/updated","params":{"threadId":"t","name":"n"}}"#;

    for &fragment_bytes in FRAGMENT_SIZES {
        assert_discarded(
            decode(&unknown, fragment_bytes, IncomingJsonExpectation::Idle),
            IncomingJsonExpectation::Idle,
            unknown.len(),
        );
        assert_discarded(
            decode(no_owner, fragment_bytes, IncomingJsonExpectation::Idle),
            IncomingJsonExpectation::Idle,
            no_owner.len(),
        );
    }
}

#[test]
fn structural_discard_does_not_consume_an_installed_expectation() {
    let input = r#"{"method":"future/notification","params":{"ignored":true}}"#;
    let expectation = IncomingJsonExpectation::Initialize { id: 41 };
    assert_discarded(decode(input, 1, expectation), expectation, input.len());
}

#[test]
fn known_unrestored_control_is_unavailable_after_full_discard() {
    let huge = "y".repeat(32 * 1024);
    let input =
        format!("{{\"method\":\"thread/started\",\"params\":{{\"unretained\":\"{huge}\"}}}}");

    for &fragment_bytes in FRAGMENT_SIZES {
        assert_ingress_error(
            decode(&input, fragment_bytes, IncomingJsonExpectation::Idle),
            ForegroundIngressError::KnownControlUnavailable,
            IncomingJsonExpectation::Idle,
            input.len(),
        );
    }
}

#[test]
fn unsupported_server_request_is_consumed_before_connection_failure() {
    let huge = "z".repeat(48 * 1024);
    let input = format!(
        "{{\"method\":\"future/request\",\"params\":{{\"unretained\":\"{huge}\"}},\"id\":91}}"
    );

    for &fragment_bytes in FRAGMENT_SIZES {
        assert_ingress_error(
            decode(&input, fragment_bytes, IncomingJsonExpectation::Idle),
            ForegroundIngressError::UnsupportedServerRequest,
            IncomingJsonExpectation::Idle,
            input.len(),
        );
    }
}

#[test]
fn ambiguous_id_first_prefix_enters_irreversible_quarantine() {
    let input = r#"{"id":7,"method":"future/request","params":{"ignored":true}}"#;
    assert_ingress_error(
        decode(input, 1, IncomingJsonExpectation::Initialize { id: 7 }),
        ForegroundIngressError::Quarantined,
        IncomingJsonExpectation::Poisoned,
        input.len(),
    );
}

#[test]
fn classification_prefix_pressure_quarantines_without_raw_capture() {
    let long_name = "n".repeat(1_100);
    let input = format!(
        "{{\"{long_name}\":{{\"secret\":\"{}\"}}}}",
        "q".repeat(24_000)
    );

    for &fragment_bytes in FRAGMENT_SIZES {
        let result = decode(&input, fragment_bytes, IncomingJsonExpectation::Idle);
        assert_ingress_error(
            result,
            ForegroundIngressError::Quarantined,
            IncomingJsonExpectation::Idle,
            input.len(),
        );
    }
}

#[test]
fn malformed_json_never_surfaces_retained_content() {
    let input = r#"{"method":"future/notification","params":{"secret":"unterminated"}"#;
    let result = decode(input, 3, IncomingJsonExpectation::Idle);
    assert!(matches!(
        result.outcome,
        IncomingJsonTestOutcome::OtherError
    ));
    assert_eq!(result.expectation_after, IncomingJsonExpectation::Idle);
}
