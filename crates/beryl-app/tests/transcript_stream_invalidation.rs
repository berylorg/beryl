#[path = "../src/shell/transcript_stream_invalidation.rs"]
mod transcript_stream_invalidation;

use beryl_backend::{ThreadStatus, TurnInfo, TurnStatus, TurnStreamEvent};
use transcript_stream_invalidation::TranscriptStreamInvalidations;

#[test]
fn invalidated_discarded_turn_events_are_filtered_by_thread_and_turn_identity() {
    let mut invalidations = TranscriptStreamInvalidations::default();
    invalidations.invalidate_turns("thread_a", ["turn_2".to_string(), "turn_3".to_string()]);

    assert!(
        invalidations.event_targets_invalidated_turn(&TurnStreamEvent::AgentMessageDelta {
            thread_id: "thread_a".to_string(),
            turn_id: "turn_2".to_string(),
            item_id: "message_1".to_string(),
            delta: "late output".to_string(),
        })
    );
    assert!(
        invalidations.event_targets_invalidated_turn(&TurnStreamEvent::TurnCompleted {
            thread_id: "thread_a".to_string(),
            turn: turn("turn_3"),
        })
    );
    assert!(
        !invalidations.event_targets_invalidated_turn(&TurnStreamEvent::AgentMessageDelta {
            thread_id: "thread_b".to_string(),
            turn_id: "turn_2".to_string(),
            item_id: "message_1".to_string(),
            delta: "other thread".to_string(),
        })
    );
    assert!(
        !invalidations.event_targets_invalidated_turn(&TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_a".to_string(),
            status: ThreadStatus::Idle,
        },)
    );

    invalidations.clear();
    assert!(
        !invalidations.event_targets_invalidated_turn(&TurnStreamEvent::AgentMessageDelta {
            thread_id: "thread_a".to_string(),
            turn_id: "turn_2".to_string(),
            item_id: "message_1".to_string(),
            delta: "after reset".to_string(),
        })
    );
}

fn turn(id: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: Vec::new(),
        error: None,
    }
}
