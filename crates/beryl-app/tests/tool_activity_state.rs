#![allow(dead_code)]

#[path = "../src/shell/tool_activity.rs"]
mod tool_activity;

use tool_activity::{ToolActivityProjection, ToolActivityRowStatus};

#[test]
fn retained_activity_state_compiles_without_an_event_ingress() {
    let mut projection = ToolActivityProjection::default();

    assert!(projection.rows().is_empty());
    assert_eq!(projection.row_count_for_selected_thread(None), 0);
    assert_eq!(projection.retained_counts().payload_bytes, 0);
    assert!(projection.set_selected_thread_id(Some("thread-1")));
    assert!(!projection.set_selected_thread_id(Some("thread-1")));
    assert!(!projection.finish_running_for_turn(
        "thread-1",
        "turn-1",
        ToolActivityRowStatus::FinishedError,
    ));
    assert!(!projection.apply_protocol_error());
    assert!(projection.clear_all());
    assert!(!projection.clear_all());
}
