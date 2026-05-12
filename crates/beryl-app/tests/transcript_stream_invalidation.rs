#[path = "../src/shell/transcript_stream_invalidation.rs"]
mod transcript_stream_invalidation;

use beryl_backend::{ThreadStatus, TurnInfo, TurnStatus, TurnStreamEvent};
use transcript_stream_invalidation::{
    TRANSCRIPT_STREAM_INVALIDATION_MAX_THREADS,
    TRANSCRIPT_STREAM_INVALIDATION_MAX_TURNS_PER_THREAD,
    TRANSCRIPT_STREAM_INVALIDATION_MAX_TURNS_TOTAL, TranscriptStreamInvalidations,
};

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

#[test]
fn invalidations_are_bounded_per_thread() {
    let mut invalidations = TranscriptStreamInvalidations::default();
    invalidations.invalidate_turns(
        "thread_a",
        (0..=TRANSCRIPT_STREAM_INVALIDATION_MAX_TURNS_PER_THREAD)
            .map(|index| format!("turn_{index}")),
    );

    assert_eq!(
        invalidations.retained_turn_count_for_test(),
        TRANSCRIPT_STREAM_INVALIDATION_MAX_TURNS_PER_THREAD
    );
    assert!(
        !invalidations.event_targets_invalidated_turn(&TurnStreamEvent::AgentMessageDelta {
            thread_id: "thread_a".to_string(),
            turn_id: "turn_0".to_string(),
            item_id: "message_1".to_string(),
            delta: "old".to_string(),
        })
    );
    assert!(
        invalidations.event_targets_invalidated_turn(&TurnStreamEvent::AgentMessageDelta {
            thread_id: "thread_a".to_string(),
            turn_id: format!("turn_{TRANSCRIPT_STREAM_INVALIDATION_MAX_TURNS_PER_THREAD}"),
            item_id: "message_1".to_string(),
            delta: "new".to_string(),
        })
    );
}

#[test]
fn invalidations_are_bounded_by_thread_and_global_turn_counts() {
    let mut invalidations = TranscriptStreamInvalidations::default();
    for index in 0..=TRANSCRIPT_STREAM_INVALIDATION_MAX_THREADS {
        invalidations.invalidate_turns(&format!("thread_{index}"), [format!("turn_{index}")]);
    }

    assert_eq!(
        invalidations.retained_thread_count_for_test(),
        TRANSCRIPT_STREAM_INVALIDATION_MAX_THREADS
    );
    assert!(
        !invalidations.event_targets_invalidated_turn(&TurnStreamEvent::AgentMessageDelta {
            thread_id: "thread_0".to_string(),
            turn_id: "turn_0".to_string(),
            item_id: "message_1".to_string(),
            delta: "old".to_string(),
        })
    );

    let mut invalidations = TranscriptStreamInvalidations::default();
    for index in 0..=TRANSCRIPT_STREAM_INVALIDATION_MAX_TURNS_TOTAL {
        invalidations
            .invalidate_turns(&format!("thread_{}", index % 16), [format!("turn_{index}")]);
    }
    assert_eq!(
        invalidations.retained_turn_count_for_test(),
        TRANSCRIPT_STREAM_INVALIDATION_MAX_TURNS_TOTAL
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
