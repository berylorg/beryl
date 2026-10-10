use beryl_backend::{
    AccountQuotaObservation, OrderedTurnStreamCompletion, OrderedTurnStreamOperation,
    OrderedTurnStreamSink, OrderedTurnStreamSubmitError,
    lifecycle_test_support::{
        decode_provider_json_at_split_for_test, decode_provider_json_for_test,
    },
};

#[derive(Default)]
struct QuotaSink(usize);

impl OrderedTurnStreamSink for QuotaSink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        assert!(matches!(
            operation,
            OrderedTurnStreamOperation::AccountQuotaObservation(
                AccountQuotaObservation::Unavailable
            )
        ));
        self.0 += 1;
        Ok(OrderedTurnStreamCompletion::Applied)
    }
}

#[test]
fn coincident_model_and_meter_names_cannot_publish_quota_at_any_reader_split() {
    let wire = r#"{"method":"account/rateLimits/updated","params":{"rateLimits":{"limitId":"gpt-5.6","limitName":"gpt-5.6","primary":{"usedPercent":20,"windowDurationMins":300,"resetsAt":null},"secondary":{"usedPercent":30,"windowDurationMins":10080,"resetsAt":0}}},"emittedAtMs":1770000000123}"#;
    for split in 1..wire.len() {
        let mut sink = QuotaSink::default();
        decode_provider_json_at_split_for_test(wire.as_bytes(), split, &mut sink).unwrap();
        assert_eq!(sink.0, 1);
    }
}

#[test]
fn each_sparse_generic_or_malformed_window_snapshot_is_an_ordered_unavailable_observation() {
    let mut sink = QuotaSink::default();
    for snapshot in [
        r#"{}"#,
        r#"{"limitId":"codex","primary":null,"secondary":null}"#,
        r#"{"limitId":"gpt-5.6","primary":{"usedPercent":101,"windowDurationMins":0,"resetsAt":-1}}"#,
        r#"{"limitName":"gpt-5.6","primary":{"usedPercent":20,"windowDurationMins":300}}"#,
    ] {
        let wire = format!(
            r#"{{"method":"account/rateLimits/updated","params":{{"rateLimits":{snapshot}}}}}"#
        );
        decode_provider_json_for_test(wire.as_bytes(), 1, &mut sink).unwrap();
    }
    assert_eq!(sink.0, 4);
}

#[test]
fn unrelated_large_snapshot_facts_are_discarded_without_retention() {
    let fact = "x".repeat(100_000);
    let wire = format!(
        r#"{{"method":"account/rateLimits/updated","params":{{"rateLimits":{{"credits":{{"unrelated":"{fact}"}}}}}}}}"#
    );
    let mut sink = QuotaSink::default();
    decode_provider_json_for_test(wire.as_bytes(), 7, &mut sink).unwrap();
    assert_eq!(sink.0, 1);
}

#[test]
fn invalid_owned_quota_shapes_are_fatal_without_ordered_submission() {
    for snapshot in [
        r#"{"primary":"bad"}"#,
        r#"{"primary":{}}"#,
        r#"{"primary":{"usedPercent":"20"}}"#,
        r#"{"primary":{"usedPercent":null}}"#,
        r#"{"primary":{"usedPercent":20.5}}"#,
        r#"{"primary":{"usedPercent":20,"windowDurationMins":[]}}"#,
        r#"{"secondary":{"usedPercent":20,"resetsAt":false}}"#,
        r#"{"limitId":42}"#,
        r#"{"limitName":{}}"#,
        r#"{"primary":null,"primary":null}"#,
        r#"{"primary":{"usedPercent":20,"usedPercent":30}}"#,
    ] {
        let wire = format!(
            r#"{{"method":"account/rateLimits/updated","params":{{"rateLimits":{snapshot}}},"emittedAtMs":1770000000123}}"#
        );
        for split in 1..wire.len() {
            let mut sink = QuotaSink::default();
            let error = decode_provider_json_at_split_for_test(wire.as_bytes(), split, &mut sink)
                .unwrap_err();
            assert!(
                matches!(
                    error,
                    beryl_backend::ManagedBackendError::ForegroundIngress {
                        source: beryl_backend::ForegroundIngressError::MalformedContextObservation,
                        ..
                    }
                ),
                "{error:?}"
            );
            assert_eq!(sink.0, 0);
        }
    }
}

#[test]
fn unsupported_integer_domains_remain_ordered_unavailable() {
    let mut sink = QuotaSink::default();
    for percent in [
        "-1",
        "101",
        "9223372036854775808",
        "999999999999999999999999999999999999",
    ] {
        let wire = format!(
            r#"{{"method":"account/rateLimits/updated","params":{{"rateLimits":{{"primary":{{"usedPercent":{percent},"windowDurationMins":10080,"resetsAt":-1}},"secondary":{{"usedPercent":20,"windowDurationMins":10080}}}}}},"emittedAtMs":1770000000123}}"#
        );
        decode_provider_json_for_test(wire.as_bytes(), 1, &mut sink).unwrap();
    }
    assert_eq!(sink.0, 4);
}
