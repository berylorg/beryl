use beryl_backend::{
    OrderedTurnStreamProgress, ProviderField, ProviderObservationAbandonReason,
    ProviderObservationSchemaError, ProviderValueContext,
};

use super::assert_schema_error;
use super::support::{SinkOptions, drive_fragmented, fragments_for, lifecycle};

const SENDER: ProviderValueContext =
    ProviderValueContext::Field(ProviderField::CollabSenderThreadId);
const RECEIVER: ProviderValueContext =
    ProviderValueContext::Field(ProviderField::CollabReceiverThreadId);
const SUBAGENT: ProviderValueContext = ProviderValueContext::Field(ProviderField::SubAgentThreadId);

#[test]
fn pinned_thread_id_fields_stream_exact_values_across_splits() {
    let sender = "sender-é🙂";
    let receiver = "receiver-é🙂";
    let agent = "agent-é🙂";
    for split in [1, 2, 3, 7, 31] {
        let collab = drive_fragmented(
            lifecycle(&collab_item(sender, receiver), true),
            SinkOptions::with_page_capacity(5),
            split,
        );
        assert_eq!(collab.outcome.unwrap(), OrderedTurnStreamProgress::Progress);
        assert_eq!(fragments_for(&collab.trace, SENDER), sender.as_bytes());
        assert_eq!(fragments_for(&collab.trace, RECEIVER), receiver.as_bytes());
        assert!(
            collab
                .trace
                .fragments
                .iter()
                .all(|(_, bytes)| std::str::from_utf8(bytes).is_ok())
        );
        assert_eq!(collab.pool.leased, 0);

        let subagent = drive_fragmented(
            lifecycle(&subagent_item(agent), true),
            SinkOptions::with_page_capacity(5),
            split,
        );
        assert_eq!(
            subagent.outcome.unwrap(),
            OrderedTurnStreamProgress::Progress
        );
        assert_eq!(fragments_for(&subagent.trace, SUBAGENT), agent.as_bytes());
        assert_eq!(subagent.pool.leased, 0);
    }
}

#[test]
fn every_pinned_thread_id_location_rejects_invalid_values() {
    let invalid = [
        String::new(),
        " surrounding".to_string(),
        "control\u{1}".to_string(),
        "x".repeat(257),
    ];
    for value in invalid {
        assert_invalid_thread_id(lifecycle(&collab_item(&value, "receiver"), true));
        assert_invalid_thread_id(lifecycle(&collab_item("sender", &value), true));
        assert_invalid_thread_id(lifecycle(&subagent_item(&value), true));
    }
}

fn collab_item(sender: &str, receiver: &str) -> String {
    format!(
        "{{\"type\":\"collabAgentToolCall\",\"id\":\"item_1\",\"tool\":\"spawnAgent\",\"status\":\"completed\",\"senderThreadId\":{},\"receiverThreadIds\":[{}],\"agentsStates\":{{}}}}",
        serde_json::to_string(sender).unwrap(),
        serde_json::to_string(receiver).unwrap(),
    )
}

fn subagent_item(agent: &str) -> String {
    format!(
        "{{\"type\":\"subAgentActivity\",\"id\":\"item_1\",\"kind\":\"started\",\"agentThreadId\":{},\"agentPath\":\"agent\"}}",
        serde_json::to_string(agent).unwrap(),
    )
}

fn assert_invalid_thread_id(message: String) {
    let case = drive_fragmented(message, SinkOptions::with_page_capacity(5), 1);
    assert_schema_error(
        case.outcome.unwrap_err(),
        ProviderObservationSchemaError::InvalidIdentity,
    );
    assert_eq!(
        case.trace.abandons,
        [ProviderObservationAbandonReason::SchemaFailure]
    );
    assert_eq!(case.trace.leased_at_abandon, [0]);
    assert_eq!(case.pool.available, 1);
    assert_eq!(case.pool.leased, 0);
}
