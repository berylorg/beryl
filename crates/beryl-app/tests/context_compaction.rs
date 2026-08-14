#[path = "../src/shell/context_compaction.rs"]
mod context_compaction;

use beryl_backend::{ThreadItem, ThreadStatus, TurnInfo, TurnStatus, TurnStreamEvent};
use context_compaction::ContextCompactionStreamState;
use serde_json::json;

#[test]
fn idle_before_compaction_activity_does_not_finish() {
    let mut state = ContextCompactionStreamState::default();

    let finished = state.observe(
        "thread_1",
        &TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_1".to_string(),
            status: ThreadStatus::Idle,
        },
    );

    assert!(!finished);
}

#[test]
fn deferred_startup_idle_then_compaction_activity_then_idle_finishes() {
    let mut state = ContextCompactionStreamState::default();

    assert!(!state.observe(
        "thread_1",
        &TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_1".to_string(),
            status: ThreadStatus::Idle,
        },
    ));

    assert!(!state.observe(
        "thread_1",
        &TurnStreamEvent::ItemStarted {
            thread_id: "thread_1".to_string(),
            turn_id: "turn_compact".to_string(),
            item: context_compaction_item(),
        },
    ));

    assert!(state.observe(
        "thread_1",
        &TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_1".to_string(),
            status: ThreadStatus::Idle,
        },
    ));
}

#[test]
fn compaction_item_then_idle_finishes() {
    let mut state = ContextCompactionStreamState::default();

    assert!(!state.observe(
        "thread_1",
        &TurnStreamEvent::ItemStarted {
            thread_id: "thread_1".to_string(),
            turn_id: "turn_1".to_string(),
            item: context_compaction_item(),
        },
    ));
    assert_eq!(state.active_turn_id(), Some("turn_1"));

    assert!(state.observe(
        "thread_1",
        &TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_1".to_string(),
            status: ThreadStatus::Idle,
        },
    ));
}

#[test]
fn active_thread_status_marks_compaction_without_interruptible_turn_id() {
    let mut state = ContextCompactionStreamState::default();

    assert!(!state.observe(
        "thread_1",
        &TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_1".to_string(),
            status: ThreadStatus::Active {
                active_flags: Vec::new(),
            },
        },
    ));

    assert_eq!(state.active_turn_id(), None);
}

#[test]
fn active_after_compaction_start_then_idle_finishes() {
    let mut state = ContextCompactionStreamState::default();

    assert!(!state.observe(
        "thread_1",
        &TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_1".to_string(),
            status: ThreadStatus::Active {
                active_flags: Vec::new(),
            },
        },
    ));

    assert!(state.observe(
        "thread_1",
        &TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_1".to_string(),
            status: ThreadStatus::Idle,
        },
    ));
}

#[test]
fn deferred_idle_then_active_then_idle_finishes() {
    let mut state = ContextCompactionStreamState::default();

    assert!(!state.observe(
        "thread_1",
        &TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_1".to_string(),
            status: ThreadStatus::Idle,
        },
    ));

    assert!(!state.observe(
        "thread_1",
        &TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_1".to_string(),
            status: ThreadStatus::Active {
                active_flags: Vec::new(),
            },
        },
    ));

    assert!(state.observe(
        "thread_1",
        &TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_1".to_string(),
            status: ThreadStatus::Idle,
        },
    ));
}

#[test]
fn other_thread_compaction_activity_does_not_finish_selected_thread() {
    let mut state = ContextCompactionStreamState::default();

    assert!(!state.observe(
        "thread_1",
        &TurnStreamEvent::ItemStarted {
            thread_id: "thread_2".to_string(),
            turn_id: "turn_1".to_string(),
            item: context_compaction_item(),
        },
    ));

    assert!(!state.observe(
        "thread_1",
        &TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_1".to_string(),
            status: ThreadStatus::Idle,
        },
    ));
}

#[test]
fn other_thread_active_does_not_finish_selected_thread() {
    let mut state = ContextCompactionStreamState::default();

    assert!(!state.observe(
        "thread_1",
        &TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_2".to_string(),
            status: ThreadStatus::Active {
                active_flags: Vec::new(),
            },
        },
    ));

    assert!(!state.observe(
        "thread_1",
        &TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_1".to_string(),
            status: ThreadStatus::Idle,
        },
    ));
}

#[test]
fn turn_with_compaction_item_then_idle_finishes() {
    let mut state = ContextCompactionStreamState::default();

    assert!(!state.observe(
        "thread_1",
        &TurnStreamEvent::TurnCompleted {
            thread_id: "thread_1".to_string(),
            turn: TurnInfo {
                id: "turn_1".to_string(),
                status: TurnStatus::Completed,
                items: vec![context_compaction_item()],
                error: None,
            },
        },
    ));

    assert!(state.observe(
        "thread_1",
        &TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_1".to_string(),
            status: ThreadStatus::Idle,
        },
    ));
}

fn context_compaction_item() -> ThreadItem {
    serde_json::from_value(json!({
        "id": "item_compact",
        "type": "contextCompaction"
    }))
    .unwrap()
}
