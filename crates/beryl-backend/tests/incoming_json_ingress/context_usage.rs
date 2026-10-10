use beryl_backend::{
    ForegroundIngressError, ManagedBackendError, OrderedTurnStreamCompletion,
    OrderedTurnStreamOperation, OrderedTurnStreamSink, OrderedTurnStreamSubmitError,
    ThreadContextObservation,
    lifecycle_test_support::{
        decode_provider_json_at_split_for_test, decode_provider_json_for_test,
    },
};

#[derive(Default)]
struct ContextSink(Vec<ThreadContextObservation>);

impl OrderedTurnStreamSink for ContextSink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        let OrderedTurnStreamOperation::ThreadContextObservation(value) = operation else {
            panic!("unexpected ordered operation: {operation:?}");
        };
        self.0.push(value);
        Ok(OrderedTurnStreamCompletion::Applied)
    }
}

fn usage_json(input: &str, window: Option<&str>, cache_write: bool) -> String {
    let cache = if cache_write {
        ",\"cacheWriteInputTokens\":7"
    } else {
        ""
    };
    let breakdown = format!(
        r#"{{"totalTokens":123,"inputTokens":{input},"cachedInputTokens":11{cache},"outputTokens":13,"reasoningOutputTokens":17}}"#
    );
    let window = window.map_or_else(String::new, |value| {
        format!(",\"modelContextWindow\":{value}")
    });
    format!(
        r#"{{"method":"thread/tokenUsage/updated","params":{{"threadId":"exact-thread","turnId":"exact-turn","tokenUsage":{{"total":{breakdown},"last":{breakdown}{window}}}}},"emittedAtMs":1770000000123}}"#
    )
}

#[test]
fn usage_is_exact_at_every_reader_split_and_preserves_independent_counters() {
    let input = usage_json("50", Some("200"), true);
    for split in 1..input.len() {
        let mut sink = ContextSink::default();
        decode_provider_json_at_split_for_test(input.as_bytes(), split, &mut sink).unwrap();
        assert_eq!(sink.0.len(), 1);
        let observation = &sink.0[0];
        assert_eq!(observation.thread_id().as_str(), "exact-thread");
        assert_eq!(observation.turn_id().as_str(), "exact-turn");
        let usage = observation.usage().unwrap();
        assert_eq!(usage.last.total_tokens, 123);
        assert_eq!(usage.last.input_tokens, 50);
        assert_eq!(usage.last.cached_input_tokens, 11);
        assert_eq!(usage.last.cache_write_input_tokens, 7);
        assert_eq!(usage.last.output_tokens, 13);
        assert_eq!(usage.last.reasoning_output_tokens, 17);
        assert_eq!(usage.remaining_percent(), Some(75));
    }
}

#[test]
fn missing_cache_write_uses_only_the_pinned_zero_default() {
    let mut sink = ContextSink::default();
    decode_provider_json_for_test(
        usage_json("50", Some("200"), false).as_bytes(),
        1,
        &mut sink,
    )
    .unwrap();
    assert_eq!(sink.0[0].usage().unwrap().last.cache_write_input_tokens, 0);
}

#[test]
fn missing_null_and_nonpositive_windows_keep_usage_but_make_percentage_unknown() {
    for window in [
        None,
        Some("null"),
        Some("0"),
        Some("-1"),
        Some("-9223372036854775808"),
    ] {
        let mut sink = ContextSink::default();
        decode_provider_json_for_test(usage_json("50", window, true).as_bytes(), 1, &mut sink)
            .unwrap();
        let usage = sink.0[0].usage().unwrap();
        assert_eq!(usage.last.input_tokens, 50);
        assert_eq!(usage.remaining_percent(), None);
    }
}

#[test]
fn unsupported_counters_replace_availability_in_order_without_numeric_truncation() {
    let huge = "9".repeat(20_000);
    let mut sink = ContextSink::default();
    for input in [
        "50",
        "-1",
        "9223372036854775808",
        huge.as_str(),
        "0",
        "9223372036854775807",
    ] {
        decode_provider_json_for_test(
            usage_json(input, Some("9223372036854775807"), true).as_bytes(),
            7,
            &mut sink,
        )
        .unwrap();
    }
    assert!(sink.0[0].usage().is_some());
    for observation in &sink.0[1..4] {
        assert!(observation.usage().is_none());
    }
    assert_eq!(sink.0[4].usage().unwrap().remaining_percent(), Some(100));
    assert_eq!(sink.0[5].usage().unwrap().remaining_percent(), Some(0));
}

#[test]
fn remaining_percentage_uses_last_input_and_clamps_at_zero() {
    let mut sink = ContextSink::default();
    decode_provider_json_for_test(
        usage_json("400", Some("200"), true).as_bytes(),
        1,
        &mut sink,
    )
    .unwrap();
    assert_eq!(sink.0[0].usage().unwrap().remaining_percent(), Some(0));
}

#[test]
fn wrong_shape_order_or_numeric_type_never_reaches_the_sink() {
    let canonical = usage_json("50", Some("200"), true);
    for malformed in [
        canonical.replace("\"threadId\":\"exact-thread\"", "\"threadId\":null"),
        canonical.replace("\"turnId\":\"exact-turn\"", "\"turnId\":\"\""),
        canonical.replace("\"inputTokens\":50", "\"inputTokens\":50.0"),
        canonical.replace("\"inputTokens\":50", "\"inputTokens\":\"50\""),
        canonical.replace(
            "\"totalTokens\":123,\"inputTokens\":50",
            "\"inputTokens\":50,\"totalTokens\":123",
        ),
        canonical.replace("\"modelContextWindow\":200", "\"modelContextWindow\":{}"),
    ] {
        let mut sink = ContextSink::default();
        let error = decode_provider_json_for_test(malformed.as_bytes(), 1, &mut sink).unwrap_err();
        assert!(
            matches!(
                error,
                ManagedBackendError::ForegroundIngress {
                    source: ForegroundIngressError::MalformedContextObservation,
                    ..
                }
            ),
            "unexpected error: {error:?}"
        );
        assert!(sink.0.is_empty());
    }
}
