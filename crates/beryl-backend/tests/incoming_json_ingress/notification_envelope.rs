use beryl_backend::{
    OrderedTurnStreamCompletion, OrderedTurnStreamOperation, OrderedTurnStreamSink,
    OrderedTurnStreamSubmitError, lifecycle_test_support::decode_provider_json_at_split_for_test,
};

#[derive(Default)]
struct Sink(usize);
impl OrderedTurnStreamSink for Sink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        assert!(matches!(
            operation,
            OrderedTurnStreamOperation::TurnStarted(_)
                | OrderedTurnStreamOperation::ThreadStatusChanged(_)
                | OrderedTurnStreamOperation::NormalTurnTerminal(_)
        ));
        self.0 += 1;
        Ok(OrderedTurnStreamCompletion::Applied)
    }
}

#[test]
fn ordinary_control_and_terminal_envelopes_accept_pinned_timestamp_at_every_split() {
    for wire in [
        r#"{"method":"turn/started","params":{"threadId":"thread","turn":{"id":"turn","items":[],"itemsView":"notLoaded","status":"inProgress","error":null,"startedAt":1,"completedAt":null,"durationMs":null}},"emittedAtMs":1770000000123}"#,
        r#"{"method":"thread/status/changed","params":{"threadId":"thread","status":{"type":"active","activeFlags":[]}},"emittedAtMs":1770000000123}"#,
        r#"{"method":"turn/completed","params":{"threadId":"thread","turn":{"id":"turn","items":[],"itemsView":"notLoaded","status":"completed","error":null,"startedAt":1,"completedAt":2,"durationMs":1}},"emittedAtMs":1770000000123}"#,
    ] {
        for split in 1..wire.len() {
            let mut sink = Sink::default();
            decode_provider_json_at_split_for_test(wire.as_bytes(), split, &mut sink).unwrap();
            assert_eq!(sink.0, 1);
            for invalid in [
                wire.replace("1770000000123", "\"bad\""),
                wire.replace("1770000000123", "1.5"),
                wire.replace("emittedAtMs", "wrongField"),
            ] {
                let mut sink = Sink::default();
                assert!(
                    decode_provider_json_at_split_for_test(
                        invalid.as_bytes(),
                        split.min(invalid.len() - 1),
                        &mut sink
                    )
                    .is_err()
                );
                assert_eq!(sink.0, 0);
            }
        }
    }
}
