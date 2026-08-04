use beryl_backend::{
    ForegroundIngressError,
    lifecycle_test_support::{
        IncomingJsonExpectation, IncomingJsonTestOutcome, IncomingJsonTestResult,
        decode_incoming_json_for_test,
    },
};

pub const FRAGMENT_SIZES: &[usize] = &[1, 2, 3, 7, 31, 257];

pub fn decode(
    input: &str,
    fragment_bytes: usize,
    expectation: IncomingJsonExpectation,
) -> IncomingJsonTestResult {
    decode_incoming_json_for_test(input.as_bytes(), fragment_bytes, expectation)
}

pub fn assert_ingress_error(
    result: IncomingJsonTestResult,
    expected: ForegroundIngressError,
    expectation_after: IncomingJsonExpectation,
    input_bytes: usize,
) {
    match result.outcome {
        IncomingJsonTestOutcome::IngressError(actual) => assert_eq!(actual, expected),
        other => panic!("expected {expected:?}, received {other:?}"),
    }
    assert_eq!(result.expectation_after, expectation_after);
    assert_eq!(result.consumed_input_bytes, input_bytes);
}

pub fn assert_discarded(
    result: IncomingJsonTestResult,
    expectation_after: IncomingJsonExpectation,
    input_bytes: usize,
) {
    assert!(
        matches!(
            result.outcome,
            IncomingJsonTestOutcome::DiscardedNotification
        ),
        "unexpected outcome: {:?}",
        result.outcome
    );
    assert_eq!(result.expectation_after, expectation_after);
    assert_eq!(result.consumed_input_bytes, input_bytes);
}
