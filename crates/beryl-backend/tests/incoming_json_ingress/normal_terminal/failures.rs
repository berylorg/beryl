use super::*;

#[test]
fn status_error_consistency_and_exact_error_shape_fail_closed() {
    let completed_with_error = terminal_json(
        "completed",
        r#"{"message":"m","codexErrorInfo":null,"additionalDetails":null}"#,
        "null",
        "null",
        "null",
    );
    let interrupted_with_error =
        completed_with_error.replacen("\"status\":\"completed\"", "\"status\":\"interrupted\"", 1);
    let cases = [
        completed_with_error,
        interrupted_with_error,
        terminal_json("failed", "null", "null", "null", "null"),
        failed_json("m", r#""unknown""#, None),
        failed_json("m", r#"{"httpConnectionFailed":{}}"#, None),
        failed_json(
            "m",
            r#"{"httpConnectionFailed":{"httpStatusCode":1,"extra":0}}"#,
            None,
        ),
        failed_json(
            "m",
            r#"{"activeTurnNotSteerable":{"httpStatusCode":1}}"#,
            None,
        ),
        failed_json(
            "m",
            r#"{"activeTurnNotSteerable":{"turnKind":"other"}}"#,
            None,
        ),
    ];

    for input in cases {
        assert_malformed(&input);
    }
}

#[test]
fn error_field_order_duplicates_and_wrong_scalar_types_fail_closed() {
    let exact = failed_json("m", "null", Some("d"));
    let cases = [
        exact.replacen(
            "\"message\":\"m\",\"codexErrorInfo\":null",
            "\"codexErrorInfo\":null,\"message\":\"m\"",
            1,
        ),
        exact.replacen(
            "\"message\":\"m\"",
            "\"message\":\"m\",\"message\":\"other\"",
            1,
        ),
        exact.replacen("\"message\":\"m\",", "", 1),
        exact.replacen(",\"codexErrorInfo\":null", "", 1),
        exact.replacen(",\"additionalDetails\":\"d\"", "", 1),
        exact.replacen(
            "\"additionalDetails\":\"d\"",
            "\"additionalDetails\":\"d\",\"extra\":null",
            1,
        ),
        exact.replacen("\"message\":\"m\"", "\"message\":7", 1),
        exact.replacen(
            "\"additionalDetails\":\"d\"",
            "\"additionalDetails\":false",
            1,
        ),
        failed_json(
            "m",
            r#"{"httpConnectionFailed":{"httpStatusCode":-1}}"#,
            None,
        ),
        failed_json(
            "m",
            r#"{"httpConnectionFailed":{"httpStatusCode":65536}}"#,
            None,
        ),
        failed_json(
            "m",
            r#"{"httpConnectionFailed":{"httpStatusCode":1.0}}"#,
            None,
        ),
        failed_json(
            "m",
            r#"{"responseStreamDisconnected":{"httpStatusCode":"503"}}"#,
            None,
        ),
    ];

    for input in cases {
        assert_malformed(&input);
    }
}

#[test]
fn timing_fields_are_mandatory_nullable_signed_i64_values_in_exact_order() {
    let canonical = completed_json();
    let cases = [
        terminal_json("completed", "null", "9223372036854775808", "null", "null"),
        terminal_json("completed", "null", "-9223372036854775809", "null", "null"),
        terminal_json("completed", "null", "1.0", "null", "null"),
        terminal_json("completed", "null", "null", "1e3", "null"),
        terminal_json("completed", "null", "null", "null", "\"0\""),
        canonical.replacen(",\"startedAt\":-9223372036854775808", "", 1),
        canonical.replacen(",\"completedAt\":9223372036854775807", "", 1),
        canonical.replacen(",\"durationMs\":null", "", 1),
        canonical.replacen(
            "\"startedAt\":-9223372036854775808,\"completedAt\":9223372036854775807",
            "\"completedAt\":9223372036854775807,\"startedAt\":-9223372036854775808",
            1,
        ),
    ];

    for input in cases {
        assert_malformed(&input);
    }
}

#[test]
fn unavailable_and_rejecting_sinks_return_the_exact_terminal_operation() {
    let input = failed_json("owned", r#""badRequest""#, Some("returned"));
    let unavailable = decode_provider_json_without_sink_for_test(input.as_bytes(), 1).unwrap_err();
    let unavailable_trace =
        assert_returned_terminal(unavailable, OrderedTurnStreamSubmitCause::Unavailable);
    assert_eq!(unavailable_trace.message.as_deref(), Some("owned"));
    assert_eq!(unavailable_trace.details.as_deref(), Some("returned"));
    assert_eq!(
        unavailable_trace.error_info,
        Some(CodexErrorInfo::BadRequest)
    );

    let cause = OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::StagingConflict);
    let mut sink = TerminalSink::with_response(SinkResponse::Reject(cause));
    let rejected = decode_provider_json_for_test(input.as_bytes(), 3, &mut sink).unwrap_err();
    assert_eq!(sink.calls, 1);
    let rejected_trace = assert_returned_terminal(rejected, cause);
    assert_eq!(rejected_trace, unavailable_trace);
}

#[test]
fn a_non_applied_sink_completion_fails_closed() {
    let input = completed_json();
    let mut sink = TerminalSink::with_response(SinkResponse::UnexpectedCompletion);
    let error = decode_provider_json_for_test(input.as_bytes(), 1, &mut sink).unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::OrderedTurnStreamUnexpectedCompletion { .. }
    ));
    assert_eq!(sink.calls, 1);
}
