#[path = "../src/shell/hard_stop_targets.rs"]
mod hard_stop_targets;
#[path = "../src/shell/status_line.rs"]
mod status_line;

use beryl_backend::{
    HardStopCapabilities, HardStopTarget, ThreadItem, TurnInfo, TurnStatus, TurnStreamEvent,
};
use hard_stop_targets::HardStopTargetProjection;
use serde_json::json;
use status_line::{CancellableActiveTurn, HardStopLimitation};

#[test]
fn projection_includes_selected_parent_subagent_command_and_background_targets() {
    let mut projection = HardStopTargetProjection::default();
    projection.set_capabilities(HardStopCapabilities::new(true, true));

    projection.apply_stream_event(&turn_started("thread_parent", "turn_parent"));
    projection.apply_stream_event(&started(
        "thread_parent",
        "turn_parent",
        collab_spawn_item_for("agent_1", "thread_child"),
    ));
    projection.apply_stream_event(&turn_started("thread_child", "turn_child"));
    projection.apply_stream_event(&started(
        "thread_parent",
        "turn_parent",
        command_item_with_process("cmd_parent", Some("proc_parent")),
    ));
    projection.apply_stream_event(&started(
        "thread_child",
        "turn_child",
        command_item_with_process("cmd_child", Some("proc_child")),
    ));

    let targets = projection
        .selected_turn_targets(Some(&CancellableActiveTurn::ordinary(
            "thread_parent",
            "turn_parent",
        )))
        .expect("selected parent turn should have a hard-stop projection");

    assert_eq!(targets.limitations, Vec::new());
    assert!(
        targets
            .targets
            .contains(&HardStopTarget::turn("thread_parent", "turn_parent"))
    );
    assert!(
        targets
            .targets
            .contains(&HardStopTarget::turn("thread_child", "turn_child"))
    );
    assert!(
        targets
            .targets
            .contains(&HardStopTarget::command_execution("proc_parent"))
    );
    assert!(
        targets
            .targets
            .contains(&HardStopTarget::command_execution("proc_child"))
    );
    assert!(
        targets
            .targets
            .contains(&HardStopTarget::background_terminals("thread_parent"))
    );
    assert!(
        targets
            .targets
            .contains(&HardStopTarget::background_terminals("thread_child"))
    );
}

#[test]
fn projection_omits_subagents_not_owned_by_selected_parent_turn() {
    let mut projection = HardStopTargetProjection::default();
    projection.set_capabilities(HardStopCapabilities::new(true, true));

    projection.apply_stream_event(&turn_started("thread_parent", "turn_parent"));
    projection.apply_stream_event(&started(
        "thread_parent",
        "turn_parent",
        collab_spawn_item_for("agent_owned", "thread_child_owned"),
    ));
    projection.apply_stream_event(&turn_started("thread_child_owned", "turn_child_owned"));
    projection.apply_stream_event(&started(
        "thread_child_owned",
        "turn_child_owned",
        command_item_with_process("cmd_child_owned", Some("proc_child_owned")),
    ));

    projection.apply_stream_event(&started(
        "thread_parent",
        "turn_other",
        collab_spawn_item_for("agent_other", "thread_child_other"),
    ));
    projection.apply_stream_event(&turn_started("thread_child_other", "turn_child_other"));
    projection.apply_stream_event(&started(
        "thread_child_other",
        "turn_child_other",
        command_item_with_process("cmd_child_other", Some("proc_child_other")),
    ));

    let targets = projection
        .selected_turn_targets(Some(&CancellableActiveTurn::ordinary(
            "thread_parent",
            "turn_parent",
        )))
        .expect("selected parent turn should have a hard-stop projection");

    assert!(targets.targets.contains(&HardStopTarget::turn(
        "thread_child_owned",
        "turn_child_owned"
    )));
    assert!(
        targets
            .targets
            .contains(&HardStopTarget::command_execution("proc_child_owned"))
    );
    assert!(!targets.targets.contains(&HardStopTarget::turn(
        "thread_child_other",
        "turn_child_other"
    )));
    assert!(
        !targets
            .targets
            .contains(&HardStopTarget::command_execution("proc_child_other"))
    );
    assert!(
        !targets
            .targets
            .contains(&HardStopTarget::background_terminals("thread_child_other"))
    );
}

#[test]
fn projection_keeps_context_compaction_hard_stop_thread_local() {
    let mut projection = HardStopTargetProjection::default();
    projection.set_capabilities(HardStopCapabilities::new(true, true));

    projection.apply_stream_event(&turn_started("thread_parent", "turn_compact"));
    projection.apply_stream_event(&started(
        "thread_parent",
        "turn_compact",
        collab_spawn_item_for("agent_child", "thread_child"),
    ));
    projection.apply_stream_event(&turn_started("thread_child", "turn_child"));
    projection.apply_stream_event(&started(
        "thread_parent",
        "turn_compact",
        command_item_with_process("cmd_parent", Some("proc_parent")),
    ));
    projection.apply_stream_event(&started(
        "thread_child",
        "turn_child",
        command_item_with_process("cmd_child", Some("proc_child")),
    ));

    let targets = projection
        .selected_turn_targets(Some(&CancellableActiveTurn::context_compaction(
            "thread_parent",
            "turn_compact",
        )))
        .expect("selected compaction turn should have a hard-stop projection");

    assert!(
        targets
            .targets
            .contains(&HardStopTarget::turn("thread_parent", "turn_compact"))
    );
    assert!(
        targets
            .targets
            .contains(&HardStopTarget::command_execution("proc_parent"))
    );
    assert!(
        targets
            .targets
            .contains(&HardStopTarget::background_terminals("thread_parent"))
    );
    assert!(
        !targets
            .targets
            .contains(&HardStopTarget::turn("thread_child", "turn_child"))
    );
    assert!(
        !targets
            .targets
            .contains(&HardStopTarget::command_execution("proc_child"))
    );
    assert!(
        !targets
            .targets
            .contains(&HardStopTarget::background_terminals("thread_child"))
    );
}

#[test]
fn projection_reports_unsupported_and_missing_handles_as_limitations() {
    let mut projection = HardStopTargetProjection::default();
    projection.apply_stream_event(&turn_started("thread_parent", "turn_parent"));
    projection.apply_stream_event(&started(
        "thread_parent",
        "turn_parent",
        command_item_with_process("cmd_known", Some("proc_known")),
    ));
    projection.apply_stream_event(&started(
        "thread_parent",
        "turn_parent",
        command_item_with_process("cmd_missing", None),
    ));

    let targets = projection
        .selected_turn_targets(Some(&CancellableActiveTurn::ordinary(
            "thread_parent",
            "turn_parent",
        )))
        .expect("selected parent turn should have a hard-stop projection");

    assert_eq!(
        targets.targets,
        vec![HardStopTarget::turn("thread_parent", "turn_parent")]
    );
    assert!(targets.limitations.contains(
        &HardStopLimitation::CommandExecutionTerminateUnsupported {
            process_id: "proc_known".to_string()
        }
    ));
    assert!(targets.limitations.contains(
        &HardStopLimitation::CommandExecutionProcessHandleUnavailable {
            thread_id: "thread_parent".to_string(),
            turn_id: "turn_parent".to_string(),
            item_id: "cmd_missing".to_string(),
        }
    ));
    assert!(targets.limitations.contains(
        &HardStopLimitation::BackgroundTerminalCleanupUnsupported {
            thread_id: "thread_parent".to_string()
        }
    ));
}

#[test]
fn projection_removes_completed_command_execution_process_handles() {
    let mut projection = HardStopTargetProjection::default();
    projection.set_capabilities(HardStopCapabilities::new(true, false));
    projection.apply_stream_event(&turn_started("thread_parent", "turn_parent"));
    projection.apply_stream_event(&started(
        "thread_parent",
        "turn_parent",
        command_item_with_process("cmd_1", Some("proc_1")),
    ));
    projection.apply_stream_event(&completed(
        "thread_parent",
        "turn_parent",
        command_item_with_process("cmd_1", Some("proc_1")),
    ));

    let targets = projection
        .selected_turn_targets(Some(&CancellableActiveTurn::ordinary(
            "thread_parent",
            "turn_parent",
        )))
        .expect("selected parent turn should have a hard-stop projection");

    assert!(
        !targets
            .targets
            .contains(&HardStopTarget::command_execution("proc_1"))
    );
}

fn started(thread_id: &str, turn_id: &str, item: ThreadItem) -> TurnStreamEvent {
    TurnStreamEvent::ItemStarted {
        thread_id: thread_id.to_string(),
        turn_id: turn_id.to_string(),
        item,
    }
}

fn completed(thread_id: &str, turn_id: &str, item: ThreadItem) -> TurnStreamEvent {
    TurnStreamEvent::ItemCompleted {
        thread_id: thread_id.to_string(),
        turn_id: turn_id.to_string(),
        item,
    }
}

fn turn_started(thread_id: &str, turn_id: &str) -> TurnStreamEvent {
    TurnStreamEvent::TurnStarted {
        thread_id: thread_id.to_string(),
        turn: TurnInfo {
            id: turn_id.to_string(),
            status: TurnStatus::InProgress,
            items: Vec::new(),
            error: None,
        },
    }
}

fn command_item_with_process(item_id: &str, process_id: Option<&str>) -> ThreadItem {
    let mut item = json!({
        "id": item_id,
        "type": "commandExecution",
        "command": "dir",
        "cwd": "C:/work/beryl",
        "status": "inProgress"
    });
    if let Some(process_id) = process_id {
        item["processId"] = json!(process_id);
    }
    serde_json::from_value(item).unwrap()
}

fn collab_spawn_item_for(item_id: &str, receiver_thread_id: &str) -> ThreadItem {
    serde_json::from_value(json!({
        "id": item_id,
        "type": "collabAgentToolCall",
        "agentsStates": {
            receiver_thread_id: {"status": "running", "message": null}
        },
        "receiverThreadIds": [receiver_thread_id],
        "senderThreadId": "thread_parent",
        "status": "inProgress",
        "tool": "spawnAgent"
    }))
    .unwrap()
}
