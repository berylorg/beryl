use std::path::PathBuf;

#[path = "../src/activity_lifecycle_diagnostics.rs"]
mod activity_lifecycle_diagnostics;
#[path = "../src/activity_presentation_diagnostics.rs"]
mod activity_presentation_diagnostics;
#[path = "../src/shell/tool_activity.rs"]
mod tool_activity;

use beryl_backend::{
    JsonRpcError, ThreadItem, ThreadReadMetadata, ThreadSessionMetadata, ThreadSummary, TurnError,
    TurnInfo, TurnStatus, TurnStreamEvent,
};
use beryl_model::workspace::WorkspaceId;
use serde_json::{Value, json};
use tool_activity::{
    ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET, ACTIVITY_COMPLETED_ROW_BUDGET,
    ACTIVITY_DISPLAY_VALUE_BYTE_LIMIT, ACTIVITY_LABEL_DISPLAY_BYTE_LIMIT,
    ACTIVITY_REASONING_SUMMARY_BYTE_LIMIT, ACTIVITY_SELECTED_COMPLETED_ROW_WINDOW,
    ToolActivityProjection, ToolActivityRowStatus,
};

#[test]
fn projection_classifies_completed_items_from_raw_status() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_main", "turn_1", command_item("cmd_1")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_main", "turn_1", mcp_item("mcp_1", "read_file")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &completed(
            "thread_main",
            "turn_1",
            command_item_with_status("cmd_1", "failed"),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &completed(
            "thread_main",
            "turn_1",
            mcp_item_with_status("mcp_1", "read_file", "completed"),
        ),
        Some("Main".to_string()),
    );

    let rows = projection.rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].tool_display_value, "read_file");
    assert_eq!(rows[0].status, ToolActivityRowStatus::FinishedOk);
    assert_eq!(rows[1].tool_display_value, "dir");
    assert_eq!(rows[1].status, ToolActivityRowStatus::FinishedError);
}

#[test]
fn matching_turn_error_keeps_running_activity_unchanged() {
    for will_retry in [true, false] {
        let mut projection = ToolActivityProjection::default();
        projection.apply_stream_event(
            &started("thread_main", "turn_1", command_item("cmd_1")),
            Some("Main".to_string()),
        );

        assert!(!projection.apply_stream_event(
            &TurnStreamEvent::TurnError {
                thread_id: "thread_main".to_string(),
                turn_id: "turn_1".to_string(),
                error: TurnError {
                    message: "backend reported an error".to_string(),
                    additional_details: None,
                    codex_error_info: None,
                },
                will_retry,
            },
            Some("Main".to_string()),
        ));
        assert_eq!(projection.rows().len(), 1);
        assert_eq!(projection.rows()[0].status, ToolActivityRowStatus::Running);
    }
}

#[test]
fn lifecycle_diagnostics_distinguish_matching_completion_and_completion_without_start() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_main", "turn_1", command_item("matched")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &TurnStreamEvent::ReasoningSummaryTextDelta {
            thread_id: "thread_main".to_string(),
            turn_id: "turn_1".to_string(),
            item_id: "matched".to_string(),
            summary_index: 0,
            delta: "content that must not be retained".to_string(),
        },
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &completed(
            "thread_main",
            "turn_1",
            command_item_with_status("matched", "completed"),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &completed(
            "thread_main",
            "turn_1",
            command_item_with_status("completion_only", "completed"),
        ),
        Some("Main".to_string()),
    );

    let snapshot = projection.lifecycle_diagnostic_snapshot();
    assert_eq!(snapshot.events.len(), 4);
    assert_eq!(snapshot.events[0].kind, "started");
    assert_eq!(snapshot.events[0].projection_outcome, "inserted_running");
    assert_eq!(snapshot.events[0].before_row_status, None);
    assert_eq!(snapshot.events[0].after_row_status, Some("running"));
    assert_eq!(snapshot.events[1].kind, "updated");
    assert_eq!(snapshot.events[1].projection_outcome, "matched_existing");
    assert_eq!(snapshot.events[1].before_row_status, Some("running"));
    assert_eq!(snapshot.events[1].after_row_status, Some("running"));
    assert_eq!(snapshot.events[2].kind, "completed");
    assert_eq!(snapshot.events[2].projection_outcome, "matched_existing");
    assert_eq!(snapshot.events[2].before_row_status, Some("running"));
    assert_eq!(snapshot.events[2].after_row_status, Some("finished_ok"));
    assert_eq!(snapshot.events[3].projection_outcome, "inserted_completed");
    assert_eq!(snapshot.events[3].before_row_status, None);
    assert_eq!(snapshot.events[3].after_row_status, Some("finished_ok"));
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("content that must not be retained")
    );
}

#[test]
fn lifecycle_diagnostics_make_mismatched_item_identity_visible() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_main", "turn_1", command_item("started_item")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &completed(
            "thread_main",
            "turn_1",
            command_item_with_status("different_item", "completed"),
        ),
        Some("Main".to_string()),
    );

    assert_eq!(
        projection
            .rows()
            .iter()
            .find(|row| row.item_id() == "started_item")
            .unwrap()
            .status,
        ToolActivityRowStatus::Running
    );
    assert_eq!(
        projection
            .rows()
            .iter()
            .find(|row| row.item_id() == "different_item")
            .unwrap()
            .status,
        ToolActivityRowStatus::FinishedOk
    );
    let snapshot = projection.lifecycle_diagnostic_snapshot();
    assert_eq!(snapshot.events[1].projection_outcome, "inserted_completed");
    assert_eq!(
        snapshot.events[1].item.value.as_deref(),
        Some("different_item")
    );
}

#[test]
fn projection_row_identity_includes_thread_turn_and_item() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_a", "turn_a", command_item("shared_item")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_b", "turn_b", command_item("shared_item")),
        Some("Main".to_string()),
    );

    let rows = projection.rows();
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0].stable_identity(), rows[1].stable_identity());
}

#[test]
fn projection_row_identity_survives_active_to_finished_resorting() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_main", "turn_1", command_item("first")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_main", "turn_1", command_item("second")),
        Some("Main".to_string()),
    );

    let first_identity = row_for_activity(&projection, "dir")
        .stable_identity()
        .clone();
    projection.apply_stream_event(
        &completed(
            "thread_main",
            "turn_1",
            command_item_with_status("first", "completed"),
        ),
        Some("Main".to_string()),
    );

    let first_row = projection
        .rows()
        .iter()
        .find(|row| row.item_id() == "first")
        .expect("finished activity row should remain visible");
    assert_eq!(first_row.stable_identity(), &first_identity);
    assert_eq!(projection.rows()[0].item_id(), "second");
}

#[test]
fn projection_revision_advances_for_visible_status_and_order_changes() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_main", "turn_1", command_item("first")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_main", "turn_1", command_item("second")),
        Some("Main".to_string()),
    );
    let running_revision = projection.presentation_diagnostic_state();
    let running_window = projection.rows_for_selected_thread_window(Some("thread_main"), 0..2);
    assert_eq!(running_window.len(), 2);
    let first_stable_identity = running_window[0].1.stable_identity().clone();

    projection.apply_stream_event(
        &completed(
            "thread_main",
            "turn_1",
            command_item_with_status("first", "completed"),
        ),
        Some("Main".to_string()),
    );

    let terminal_revision = projection.presentation_diagnostic_state();
    let terminal_window = projection.rows_for_selected_thread_window(Some("thread_main"), 0..2);
    assert!(terminal_revision.revision > running_revision.revision);
    assert!(
        terminal_revision.newest_lifecycle_sequence > running_revision.newest_lifecycle_sequence
    );
    assert_eq!(terminal_revision.running_row_count, 1);
    assert_eq!(terminal_revision.finished_ok_row_count, 1);
    assert_eq!(terminal_window[0].1.item_id(), "second");
    assert_eq!(terminal_window[1].1.item_id(), "first");
    assert_eq!(
        terminal_window[1].1.stable_identity(),
        &first_stable_identity
    );
}

#[test]
fn projection_retained_counts_report_records_rows_and_visible_indexes() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_main", "turn_1", command_item("cmd_1")),
        Some("Main".to_string()),
    );

    let counts = projection.retained_counts();
    assert_eq!(counts.records, 1);
    assert_eq!(counts.rows, 1);
    assert_eq!(counts.visible_thread_indexes, 1);
    assert_eq!(counts.label_count, 0);
    assert_eq!(counts.multi_agent_v2_child_threads, 0);
    assert_eq!(counts.multi_agent_v2_path_associations, 0);
    assert_eq!(counts.visible_thread_index_maps, 1);
    assert!(counts.record_payload_bytes > 0);
    assert!(counts.row_payload_bytes > 0);
    assert_eq!(counts.label_payload_bytes, 0);
    assert!(counts.payload_bytes > 0);
}

#[test]
fn projection_uses_first_non_empty_command_line_as_display_value() {
    assert_eq!(
        projected_command_display_value("\r\n  cargo nextest run -p beryl-app\r\ncargo check"),
        "cargo nextest run -p beryl-app"
    );

    assert_eq!(
        projected_command_display_value(" \r\n\t "),
        "commandExecution"
    );
}

#[test]
fn projection_collapses_windows_powershell_launcher_in_command_display_value() {
    let cases = [
        (
            r#""C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe" -NoProfile -Command Get-Location"#,
            "powershell.exe -NoProfile -Command Get-Location",
        ),
        (
            r#""D:\\Windows.old\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"  -NoProfile"#,
            "powershell.exe  -NoProfile",
        ),
        (
            r"d:\\WINDOWS.old\\System32\\WindowsPowerShell\\v1.0\\PowerShell.EXE -NoLogo",
            "powershell.exe -NoLogo",
        ),
        (
            r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe -NoProfile -Command Get-Location",
            "powershell.exe -NoProfile -Command Get-Location",
        ),
        (
            r#""C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile"#,
            "powershell.exe -NoProfile",
        ),
        (
            r"C:\WINDOWS\system32\WindowsPowerShell\V1.0\PowerShell.EXE -NoLogo",
            "powershell.exe -NoLogo",
        ),
        (
            r"C:\Windows\System32\WindowsPowerShell\v1.0\pwsh.exe -NoProfile",
            r"C:\Windows\System32\WindowsPowerShell\v1.0\pwsh.exe -NoProfile",
        ),
        (
            r"D:\Tools\System32\WindowsPowerShell\v1.0\powershell.exe -NoProfile",
            r"D:\Tools\System32\WindowsPowerShell\v1.0\powershell.exe -NoProfile",
        ),
        (
            r#""C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"#,
            r#""C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"#,
        ),
    ];

    for (command, expected) in cases {
        assert_eq!(projected_command_display_value(command), expected);
    }
}

#[test]
fn projection_shows_single_relative_file_change_path() {
    assert_eq!(
        projected_file_change_display_value(json!([
            {
                "path": "src/lib.rs",
                "diff": "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,3 @@\n-old\n+new\n+\n",
                "kind": {"type": "update"}
            }
        ])),
        "Patching src/lib.rs, +2 -1"
    );
}

#[test]
fn projection_shows_single_host_absolute_file_change_path_relative_to_root() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");

    assert_eq!(
        projected_file_change_display_value_for_target(
            json!([
                {
                    "path": r"C:\work\beryl\src\lib.rs",
                    "diff": "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,3 @@\n-old\n+new\n+\n",
                    "kind": {"type": "update"}
                }
            ]),
            &execution_target,
        ),
        r"Patching src\lib.rs, +2 -1"
    );
}

#[test]
fn projection_shows_single_wsl_absolute_file_change_path_relative_to_root() {
    let execution_target = WorkspaceId::wsl_linux("Ubuntu", "/home/me/project");

    assert_eq!(
        projected_file_change_display_value_for_target(
            json!([
                {
                    "path": "/home/me/project/src/lib.rs",
                    "diff": "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,3 @@\n-old\n+new\n+\n",
                    "kind": {"type": "update"}
                }
            ]),
            &execution_target,
        ),
        "Patching src/lib.rs, +2 -1"
    );
}

#[test]
fn projection_shows_duplicate_same_file_change_path_with_summed_counts() {
    assert_eq!(
        projected_file_change_display_value(json!([
            {
                "path": "src/lib.rs",
                "diff": "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,3 @@\n-old\n+new\n+\n",
                "kind": {"type": "update"}
            },
            {
                "path": "src/lib.rs",
                "diff": "@@ -8,2 +9,2 @@\n-duplicate\n+duplicate\n",
                "kind": {"type": "update"}
            }
        ])),
        "Patching src/lib.rs, +3 -2"
    );
}

#[test]
fn projection_shows_multi_file_change_patch_summary() {
    assert_eq!(
        projected_file_change_display_value(json!([
            {
                "path": "src/lib.rs",
                "diff": "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,2 @@\n-old\n+new\n",
                "kind": {"type": "update"}
            },
            {
                "path": "src/main.rs",
                "diff": "@@ -4,2 +4,3 @@\n-remove\n+add\n+more\n",
                "kind": {"type": "update"}
            }
        ])),
        "Patching 2 files, +3 -2"
    );
}

#[test]
fn projection_shows_aggregate_for_absolute_file_change_path_outside_root() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");

    assert_eq!(
        projected_file_change_display_value_for_target(
            json!([
                {
                    "path": r"C:\work\other\src\lib.rs",
                    "diff": "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,3 @@\n-old\n+new\n+\n",
                    "kind": {"type": "update"}
                }
            ]),
            &execution_target,
        ),
        "Patching 1 file, +2 -1"
    );
}

#[test]
fn projection_shows_zero_counts_for_empty_file_change() {
    assert_eq!(
        projected_file_change_display_value(json!([])),
        "Patching 0 files, +0 -0"
    );
}

#[test]
fn projection_shows_single_diffless_file_change_path() {
    assert_eq!(
        projected_file_change_display_value(json!([
            {
                "path": "src/lib.rs",
                "diff": "",
                "kind": {"type": "update"}
            }
        ])),
        "Patching src/lib.rs, +0 -0"
    );
}

#[test]
fn projection_refreshes_reasoning_detail_from_completed_summary() {
    let mut projection = ToolActivityProjection::default();

    projection.apply_stream_event(
        &started("thread_main", "turn_1", reasoning_item("reasoning_1")),
        Some("Main".to_string()),
    );
    assert_eq!(projection.rows()[0].tool_display_value, "reasoning");

    projection.apply_stream_event(
        &completed(
            "thread_main",
            "turn_1",
            reasoning_item_with_summary(
                "reasoning_1",
                &[
                    "Checked candidate paths. ",
                    "Selected the direct implementation.",
                ],
            ),
        ),
        Some("Main".to_string()),
    );

    assert_eq!(
        projection.rows()[0].tool_display_value,
        "reasoning: Checked candidate paths. Selected the direct implementation."
    );
    assert_eq!(
        projection.rows()[0].status,
        ToolActivityRowStatus::FinishedOk
    );
}

#[test]
fn projection_ignores_raw_reasoning_text_delta_for_activity_rows() {
    let mut projection = ToolActivityProjection::default();

    let changed = projection.apply_stream_event(
        &TurnStreamEvent::ReasoningTextDelta {
            thread_id: "thread_main".to_string(),
            turn_id: "turn_1".to_string(),
            item_id: "reasoning_1".to_string(),
            content_index: 0,
            delta: "Raw hidden reasoning details.".to_string(),
        },
        Some("Main".to_string()),
    );

    assert!(!changed);
    assert!(projection.rows().is_empty());
}

#[test]
fn projection_finishes_lingering_reasoning_rows() {
    let mut projection = ToolActivityProjection::default();

    projection.apply_stream_event(
        &started("thread_main", "turn_1", reasoning_item("reasoning_1")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &turn_completed_with_status("thread_main", "turn_1", TurnStatus::Interrupted),
        None,
    );

    assert_eq!(projection.rows().len(), 1);
    assert_eq!(projection.rows()[0].tool_display_value, "reasoning");
    assert_eq!(
        projection.rows()[0].status,
        ToolActivityRowStatus::FinishedError
    );
}

#[test]
fn projection_retains_history_and_finishes_lingering_running_rows() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_main", "turn_1", mcp_item("mcp_1", "read_file")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_main", "turn_1", command_item("cmd_1")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_child", "turn_2", mcp_item("mcp_2", "search")),
        Some("Research".to_string()),
    );

    projection.apply_stream_event(
        &turn_completed_with_status("thread_main", "turn_1", TurnStatus::Failed),
        None,
    );
    let rows = projection.rows();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].agent_label, "Research");
    assert_eq!(rows[0].status, ToolActivityRowStatus::Running);
    assert_eq!(rows[1].tool_display_value, "dir");
    assert_eq!(rows[1].status, ToolActivityRowStatus::FinishedError);
    assert_eq!(rows[2].tool_display_value, "read_file");
    assert_eq!(rows[2].status, ToolActivityRowStatus::FinishedError);

    projection.apply_stream_event(
        &started("thread_child", "turn_3", command_item("cmd_2")),
        Some("Research".to_string()),
    );
    projection.apply_stream_event(
        &TurnStreamEvent::ThreadClosed {
            thread_id: "thread_child".to_string(),
        },
        None,
    );
    assert_eq!(projection.rows().len(), 4);
    assert!(
        projection
            .rows()
            .iter()
            .all(|row| row.status != ToolActivityRowStatus::Running)
    );

    projection.apply_stream_event(
        &started("thread_main", "turn_4", command_item("cmd_3")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &TurnStreamEvent::ProtocolError {
            error: JsonRpcError {
                code: -32000,
                message: "stream failed".to_string(),
                data: None,
            },
        },
        None,
    );
    assert_eq!(projection.rows().len(), 5);
    assert_eq!(projection.rows()[0].tool_display_value, "dir");
    assert_eq!(
        projection.rows()[0].status,
        ToolActivityRowStatus::FinishedError
    );
}

#[test]
fn lifecycle_diagnostics_record_turn_thread_and_all_fallback_counts() {
    let mut projection = ToolActivityProjection::default();
    for item_id in ["main_1", "main_2"] {
        projection.apply_stream_event(
            &started("thread_main", "turn_1", command_item(item_id)),
            Some("Main".to_string()),
        );
    }
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("child_1")),
        Some("Child".to_string()),
    );
    projection.apply_stream_event(
        &turn_completed_with_status("thread_main", "turn_1", TurnStatus::Completed),
        None,
    );

    projection.apply_stream_event(
        &started("thread_main", "turn_2", command_item("main_3")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(&thread_archived("thread_main"), None);
    projection.apply_stream_event(
        &TurnStreamEvent::ProtocolError {
            error: JsonRpcError {
                code: -32000,
                message: "content that must not be retained".to_string(),
                data: None,
            },
        },
        None,
    );

    let snapshot = projection.lifecycle_diagnostic_snapshot();
    let fallback_events = snapshot
        .events
        .iter()
        .filter(|event| event.stage == "fallback")
        .collect::<Vec<_>>();
    assert_eq!(fallback_events.len(), 3);
    assert_eq!(fallback_events[0].kind, "turn_completed");
    assert_eq!(fallback_events[0].affected_row_count, 2);
    assert_eq!(fallback_events[1].kind, "thread_archived");
    assert_eq!(fallback_events[1].affected_row_count, 1);
    assert_eq!(fallback_events[2].kind, "protocol_error");
    assert_eq!(fallback_events[2].affected_row_count, 1);
    let serialized = serde_json::to_string(&snapshot).unwrap();
    assert!(!serialized.contains("content that must not be retained"));
}

#[test]
fn lifecycle_diagnostics_record_local_stream_failure_and_clear_with_projection() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("selected_thread", "turn_1", command_item("cmd")),
        Some("Main".to_string()),
    );

    assert!(projection.finish_running_for_thread_stream_failure("selected_thread"));
    let snapshot = projection.lifecycle_diagnostic_snapshot();
    let failure = snapshot.events.last().unwrap();
    assert_eq!(failure.stage, "stream_failure");
    assert_eq!(failure.kind, "local_turn_failure");
    assert_eq!(failure.thread.value.as_deref(), Some("selected_thread"));
    assert_eq!(failure.turn.value, None);
    assert_eq!(failure.affected_row_count, 1);
    assert_eq!(failure.before_row_status, Some("running"));
    assert_eq!(failure.after_row_status, Some("finished_error"));

    assert!(projection.clear_all());
    let snapshot = projection.lifecycle_diagnostic_snapshot();
    assert_eq!(snapshot.retained_count, 0);
    assert_eq!(snapshot.oldest_sequence, None);
    assert_eq!(snapshot.omissions.evicted_event_count, 0);
}

#[test]
fn clear_resets_projection_correlation_and_first_reuse_sequences() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started(
            "selected_thread",
            "turn_before_clear",
            command_item("before_clear"),
        ),
        Some("Main".to_string()),
    );
    let before_clear = projection.presentation_diagnostic_state();
    assert_eq!(before_clear.revision, 1);
    assert_eq!(before_clear.newest_lifecycle_sequence, Some(1));
    assert_eq!(
        projection.lifecycle_diagnostic_snapshot().newest_sequence,
        Some(1)
    );

    assert!(projection.clear_all());
    let cleared = projection.presentation_diagnostic_state();
    assert_eq!(cleared.revision, 0);
    assert_eq!(cleared.newest_lifecycle_sequence, None);
    assert_eq!(cleared.total_row_count, 0);
    assert_eq!(projection.lifecycle_diagnostic_snapshot().retained_count, 0);

    projection.apply_stream_event(
        &started(
            "selected_thread",
            "turn_after_clear",
            command_item("after_clear"),
        ),
        Some("Main".to_string()),
    );
    let reused = projection.presentation_diagnostic_state();
    assert_eq!(reused.revision, 1);
    assert_eq!(reused.newest_lifecycle_sequence, Some(1));
    assert_eq!(
        projection.lifecycle_diagnostic_snapshot().newest_sequence,
        Some(1)
    );
}

#[test]
fn projection_finishes_only_archived_or_deleted_thread_activity() {
    for lifecycle_event in [
        thread_archived as fn(&str) -> TurnStreamEvent,
        thread_deleted,
    ] {
        let mut projection = ToolActivityProjection::default();
        projection.apply_stream_event(
            &started(
                "thread_target",
                "turn_target",
                mcp_item("target", "target_tool"),
            ),
            Some("Target".to_string()),
        );
        projection.apply_stream_event(
            &started(
                "thread_other",
                "turn_other",
                mcp_item("other", "other_tool"),
            ),
            Some("Other".to_string()),
        );

        assert!(projection.apply_stream_event(&lifecycle_event("thread_other"), None));
        assert_eq!(
            row_for_activity(&projection, "target_tool").status,
            ToolActivityRowStatus::Running
        );
        assert_eq!(
            row_for_activity(&projection, "other_tool").status,
            ToolActivityRowStatus::FinishedOk
        );

        assert!(projection.apply_stream_event(&lifecycle_event("thread_target"), None));
        assert_eq!(
            row_for_activity(&projection, "target_tool").status,
            ToolActivityRowStatus::FinishedOk
        );
    }
}

#[test]
fn projection_ignores_same_and_other_thread_unarchive_activity() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started(
            "thread_target",
            "turn_target",
            mcp_item("target", "target_tool"),
        ),
        Some("Target".to_string()),
    );
    projection.apply_stream_event(
        &started(
            "thread_other",
            "turn_other",
            mcp_item("other", "other_tool"),
        ),
        Some("Other".to_string()),
    );

    assert!(!projection.apply_stream_event(&thread_unarchived("thread_other"), None));
    assert!(!projection.apply_stream_event(&thread_unarchived("thread_target"), None));
    assert_eq!(
        row_for_activity(&projection, "target_tool").status,
        ToolActivityRowStatus::Running
    );
    assert_eq!(
        row_for_activity(&projection, "other_tool").status,
        ToolActivityRowStatus::Running
    );
}

#[test]
fn projection_prunes_completed_rows_by_global_row_budget_but_keeps_running() {
    let mut projection = ToolActivityProjection::default();

    for index in 0..ACTIVITY_COMPLETED_ROW_BUDGET + 6 {
        let status = match index {
            value if value == ACTIVITY_COMPLETED_ROW_BUDGET + 3 => "failed",
            value if value == ACTIVITY_COMPLETED_ROW_BUDGET + 4 => "declined",
            _ => "completed",
        };
        add_completed_command(&mut projection, "thread_main", index, status);
    }
    projection.apply_stream_event(
        &started("thread_main", "turn_running", command_item("running")),
        Some("Main".to_string()),
    );

    let counts = projection.retained_counts();
    assert_eq!(counts.records, ACTIVITY_COMPLETED_ROW_BUDGET + 1);
    assert_eq!(counts.rows, ACTIVITY_COMPLETED_ROW_BUDGET + 1);
    assert_eq!(projection.rows()[0].status, ToolActivityRowStatus::Running);
    assert!(
        projection
            .rows()
            .iter()
            .any(|row| row.status == ToolActivityRowStatus::FinishedError)
    );
    assert!(
        projection
            .rows()
            .iter()
            .all(|row| row.tool_display_value != "cmd 0")
    );
}

#[test]
fn projection_prunes_completed_rows_by_display_byte_budget() {
    let mut projection = ToolActivityProjection::default();
    let command = "x".repeat(ACTIVITY_DISPLAY_VALUE_BYTE_LIMIT + 4096);
    let row_count =
        (ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET / ACTIVITY_DISPLAY_VALUE_BYTE_LIMIT) + 20;

    for index in 0..row_count {
        projection.apply_stream_event(
            &completed(
                "thread_main",
                format!("turn_{index}").as_str(),
                command_item_with_command(format!("cmd_{index}").as_str(), &command, "completed"),
            ),
            Some("Main".to_string()),
        );
    }

    let counts = projection.retained_counts();
    assert!(counts.records < row_count);
    assert!(
        counts.records
            <= ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET / ACTIVITY_DISPLAY_VALUE_BYTE_LIMIT
    );
    assert!(
        projection
            .rows()
            .iter()
            .all(|row| row.tool_display_value.len() <= ACTIVITY_DISPLAY_VALUE_BYTE_LIMIT)
    );
}

#[test]
fn projection_protects_latest_completed_window_for_selected_thread() {
    let mut projection = ToolActivityProjection::default();

    for index in 0..ACTIVITY_SELECTED_COMPLETED_ROW_WINDOW {
        add_completed_command(&mut projection, "thread_selected", index, "completed");
    }
    projection.set_selected_thread_id(Some("thread_selected"));
    assert_eq!(
        projection.row_count_for_selected_thread(Some("thread_selected")),
        ACTIVITY_SELECTED_COMPLETED_ROW_WINDOW
    );

    for index in 0..ACTIVITY_COMPLETED_ROW_BUDGET + 100 {
        add_completed_command(&mut projection, "thread_other", index, "completed");
    }

    assert_eq!(
        projection.row_count_for_selected_thread(Some("thread_selected")),
        ACTIVITY_SELECTED_COMPLETED_ROW_WINDOW
    );
    let window = projection.rows_for_selected_thread_window(Some("thread_selected"), 2..5);
    assert_eq!(window.len(), 3);
    assert_eq!(
        window
            .iter()
            .map(|(_, row)| row.agent_label.as_str())
            .collect::<Vec<_>>(),
        vec!["Main", "Main", "Main"]
    );
}

#[test]
fn projection_keeps_retained_spawn_child_ownership_until_child_rows_arrive() {
    let mut projection = ToolActivityProjection::default();
    for index in 0..ACTIVITY_COMPLETED_ROW_BUDGET + 10 {
        add_completed_command(&mut projection, "thread_other", index, "completed");
    }

    projection.apply_stream_event(
        &started("thread_parent", "turn_parent", collab_spawn_item("agent_1")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &turn_completed_with_status("thread_parent", "turn_parent", TurnStatus::Completed),
        None,
    );

    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("cmd_child")),
        None,
    );

    let parent_rows = projection.rows_for_selected_thread(Some("thread_parent"));
    assert!(
        parent_rows
            .iter()
            .any(|row| row.tool_display_value == "dir")
    );
    let counts = projection.retained_counts();
    assert_eq!(counts.parent_thread_links, 1);
    assert_eq!(counts.root_turn_links, 1);
}

#[test]
fn projection_prunes_subagent_maps_when_retained_rows_no_longer_reference_them() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_parent", "turn_parent", collab_spawn_item("agent_1")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("cmd_child")),
        None,
    );
    let metadata = thread_read_metadata("thread_child", None, Some("gpt-5.5"), Some("xhigh"));
    projection.apply_thread_read_metadata([&metadata]);
    projection.apply_stream_event(
        &turn_completed_with_status("thread_parent", "turn_parent", TurnStatus::Completed),
        None,
    );
    projection.apply_stream_event(
        &turn_completed_with_status("thread_child", "turn_child", TurnStatus::Completed),
        None,
    );

    for index in 0..ACTIVITY_COMPLETED_ROW_BUDGET + 10 {
        add_completed_command(&mut projection, "thread_other", index, "completed");
    }

    let counts = projection.retained_counts();
    assert_eq!(counts.parent_thread_links, 0);
    assert_eq!(counts.root_turn_links, 0);
    assert_eq!(counts.subagent_metadata_count, 0);
    assert_eq!(counts.label_count, 0);
    assert!(
        projection
            .rows()
            .iter()
            .all(|row| row.tool_display_value != "spawnAgent" && row.tool_display_value != "dir")
    );
}

#[test]
fn projection_truncates_large_ingress_payloads() {
    let mut projection = ToolActivityProjection::default();
    let long_label = "agent ".to_string() + &"l".repeat(ACTIVITY_LABEL_DISPLAY_BYTE_LIMIT * 2);
    let long_command = "run ".to_string() + &"c".repeat(ACTIVITY_DISPLAY_VALUE_BYTE_LIMIT * 2);
    let long_reasoning =
        "reason ".to_string() + &"r".repeat(ACTIVITY_REASONING_SUMMARY_BYTE_LIMIT * 2);

    projection.apply_stream_event(
        &started(
            "thread_main",
            "turn_command",
            command_item_with_command("cmd_1", &long_command, "inProgress"),
        ),
        Some(long_label),
    );
    projection.apply_stream_event(
        &reasoning_summary_delta(
            "thread_main",
            "turn_reasoning",
            "reasoning_1",
            0,
            &long_reasoning,
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &reasoning_summary_delta(
            "thread_main",
            "turn_reasoning",
            "reasoning_1",
            ACTIVITY_SELECTED_COMPLETED_ROW_WINDOW,
            "ignored high-index summary",
        ),
        Some("Main".to_string()),
    );

    let command_row = projection
        .rows()
        .iter()
        .find(|row| row.tool_display_value.starts_with("run "))
        .expect("command row should be retained");
    assert!(command_row.agent_label.len() <= ACTIVITY_LABEL_DISPLAY_BYTE_LIMIT);
    assert!(command_row.tool_display_value.len() <= ACTIVITY_DISPLAY_VALUE_BYTE_LIMIT);

    let counts = projection.retained_counts();
    assert_eq!(counts.reasoning_summary_parts, 1);
    assert!(counts.reasoning_summary_bytes <= ACTIVITY_REASONING_SUMMARY_BYTE_LIMIT);
}

#[test]
fn projection_ignores_backend_thread_id_fallback_agent_labels() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("cmd_1")),
        Some("thread:thread_child".to_string()),
    );
    assert_eq!(projection.rows()[0].agent_label, "");

    projection.apply_stream_event(
        &agent_label_updated("thread_child", "thread:thread_child"),
        None,
    );
    assert_eq!(projection.rows()[0].agent_label, "");

    projection.apply_stream_event(
        &completed(
            "thread_child",
            "turn_child",
            command_item_with_status("cmd_1", "completed"),
        ),
        Some("thread:thread_child".to_string()),
    );
    let child_row = projection
        .rows()
        .iter()
        .find(|row| row.tool_display_value == "dir")
        .expect("child command row should remain visible");
    assert_eq!(child_row.agent_label, "");
    assert_eq!(child_row.status, ToolActivityRowStatus::FinishedOk);
}

#[test]
fn projection_does_not_suffix_non_subagent_display_labels_with_model_metadata() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_main", "turn_main", command_item("cmd_1")),
        None,
    );

    let mut metadata = thread_read_metadata("thread_main", None, Some("gpt-5.5"), Some("xhigh"));
    metadata.thread.name = Some("Research".to_string());
    projection.apply_thread_read_metadata([&metadata]);

    let row = row_for_activity(&projection, "dir");
    assert_eq!(row.agent_label, "Research");
}

#[test]
fn projection_uses_exact_nested_v2_path_and_exact_metadata_suffix() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item_with_path(
                "subagent_child",
                "interacted",
                Some("thread_child"),
                Some("/root/planner/reviewer"),
            ),
        ),
        Some("Main".to_string()),
    );
    projection.apply_thread_read_metadata([&thread_read_metadata(
        "thread_child",
        None,
        Some("gpt-5.5"),
        Some("xhigh"),
    )]);

    assert_eq!(
        row_for_activity(&projection, "interacted").agent_label,
        "/root/planner/reviewer (gpt-5.5/xhigh)"
    );
}

#[test]
fn projection_uses_subagent_for_pathless_legacy_children() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started(
            "thread_parent",
            "turn_parent",
            collab_spawn_item("spawn_child"),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("cmd_child")),
        None,
    );
    assert_eq!(row_for_activity(&projection, "dir").agent_label, "Subagent");
}

#[test]
fn projection_orders_running_main_then_stable_active_children_and_reactivation() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_parent", "turn_main", command_item("main")),
        Some("Main".to_string()),
    );
    for (child, item) in [("thread_child_a", "child_a"), ("thread_child_b", "child_b")] {
        projection.apply_stream_event(
            &started(
                "thread_parent",
                "turn_main",
                collab_spawn_item_for(format!("spawn_{item}").as_str(), child),
            ),
            Some("Main".to_string()),
        );
        projection.apply_stream_event(
            &started(child, format!("turn_{item}").as_str(), mcp_item(item, item)),
            None,
        );
    }

    let visible = projection.rows_for_selected_thread(Some("thread_parent"));
    assert_eq!(visible[0].agent_label, "Main");
    assert_eq!(
        visible
            .iter()
            .filter(|row| row.agent_label == "Subagent")
            .map(|row| row.tool_display_value.as_str())
            .collect::<Vec<_>>(),
        vec!["child_a", "child_b"]
    );

    projection.apply_stream_event(
        &turn_completed_with_status("thread_child_a", "turn_child_a", TurnStatus::Completed),
        None,
    );
    projection.apply_stream_event(
        &started(
            "thread_child_a",
            "turn_child_a_2",
            mcp_item("child_a_2", "child_a_2"),
        ),
        None,
    );
    assert_eq!(
        projection
            .rows_for_selected_thread(Some("thread_parent"))
            .iter()
            .filter(
                |row| row.agent_label == "Subagent" && row.status == ToolActivityRowStatus::Running
            )
            .map(|row| row.tool_display_value.as_str())
            .collect::<Vec<_>>(),
        vec!["child_b", "child_a_2"]
    );
}

#[test]
fn projection_rebuilds_retained_parent_activity_as_main_after_thread_switch() {
    let mut projection = ToolActivityProjection::default();
    projection.set_selected_thread_id(Some("thread_other"));
    projection.apply_stream_event(
        &started("thread_parent", "turn_parent", command_item("main_command")),
        None,
    );
    projection.apply_stream_event(
        &started(
            "thread_parent",
            "turn_parent",
            collab_spawn_item_for("spawn_child", "thread_child"),
        ),
        None,
    );
    projection.apply_stream_event(
        &started("thread_child", "turn_child", mcp_item("child", "child")),
        None,
    );

    assert!(
        projection
            .rows_for_selected_thread(Some("thread_other"))
            .is_empty()
    );
    projection.set_selected_thread_id(Some("thread_parent"));

    let visible = projection.rows_for_selected_thread(Some("thread_parent"));
    assert_eq!(visible[0].tool_display_value, "dir");
    assert_eq!(visible[0].agent_label, "Main");
    assert_eq!(visible[0].status, ToolActivityRowStatus::Running);
    assert_eq!(visible[1].agent_label, "Main");
    assert_eq!(visible[2].agent_label, "Subagent");
    assert_eq!(visible[2].tool_display_value, "child");
}

#[test]
fn projection_orders_terminal_rows_by_newest_completion_with_key_ties() {
    let mut projection = ToolActivityProjection::default();
    for item in ["a", "b"] {
        projection.apply_stream_event(
            &started("thread_parent", "turn_parent", command_item(item)),
            Some("Main".to_string()),
        );
    }
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            command_item_with_status("a", "completed"),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            command_item_with_status("b", "completed"),
        ),
        Some("Main".to_string()),
    );

    assert_eq!(
        projection
            .rows()
            .iter()
            .map(tool_activity::ToolActivityRow::item_id)
            .collect::<Vec<_>>(),
        vec!["b", "a"]
    );

    let mut tied = ToolActivityProjection::default();
    for item in ["b", "a"] {
        tied.apply_stream_event(
            &started("thread_parent", "turn_parent", command_item(item)),
            Some("Main".to_string()),
        );
    }
    tied.apply_stream_event(
        &turn_completed_with_status("thread_parent", "turn_parent", TurnStatus::Completed),
        None,
    );
    assert_eq!(
        tied.rows()
            .iter()
            .map(tool_activity::ToolActivityRow::item_id)
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
}

#[test]
fn projection_scopes_nested_v2_children_and_labels_operational_and_handoff_rows_by_path() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("child_command")),
        Some("Main".to_string()),
    );
    assert!(
        projection
            .rows_for_selected_thread(Some("thread_parent"))
            .is_empty()
    );

    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item_with_path(
                "child_link",
                "started",
                Some("thread_child"),
                Some("/root/planner"),
            ),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &completed(
            "thread_child",
            "turn_child",
            subagent_activity_item_with_path(
                "grandchild_link",
                "interacted",
                Some("thread_grandchild"),
                Some("/root/planner/reviewer"),
            ),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started(
            "thread_grandchild",
            "turn_grandchild",
            command_item("grandchild_command"),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &completed(
            "thread_child",
            "turn_child",
            final_answer_item("child_handoff", "child handoff"),
        ),
        None,
    );
    projection.apply_stream_event(
        &completed(
            "thread_grandchild",
            "turn_grandchild",
            final_answer_item("grandchild_handoff", "nested handoff text"),
        ),
        None,
    );

    let parent_rows = projection.rows_for_selected_thread(Some("thread_parent"));
    assert_eq!(
        parent_rows
            .iter()
            .filter(|row| row.tool_display_value == "dir")
            .map(|row| row.agent_label.as_str())
            .collect::<Vec<_>>(),
        vec!["/root/planner", "/root/planner/reviewer"]
    );
    assert_eq!(
        parent_rows
            .iter()
            .find(|row| row.item_id() == "child_handoff")
            .map(|row| row.agent_label.as_str()),
        Some("/root/planner")
    );
    assert_eq!(
        parent_rows
            .iter()
            .find(|row| row.item_id() == "grandchild_handoff")
            .map(|row| row.agent_label.as_str()),
        Some("/root/planner/reviewer")
    );
    assert!(
        parent_rows.iter().any(|row| {
            row.tool_display_value == "started" && row.agent_label == "/root/planner"
        })
    );
    assert!(parent_rows.iter().any(|row| {
        row.tool_display_value == "interacted" && row.agent_label == "/root/planner/reviewer"
    }));
    assert_eq!(
        projection
            .rows_for_selected_thread_window(Some("thread_parent"), 1..3)
            .len(),
        2
    );
}

#[test]
fn projection_keeps_v2_agent_paths_exact_for_valid_then_malformed_records() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item_with_path(
                "child_link",
                "started",
                Some("thread_child"),
                Some("/root/first"),
            ),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("child_command")),
        None,
    );
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item_with_path(
                "child_missing_path",
                "interacted",
                Some("thread_child"),
                None,
            ),
        ),
        Some("Main".to_string()),
    );

    assert_eq!(
        row_for_activity(&projection, "started").agent_label,
        "/root/first"
    );
    assert_eq!(row_for_activity(&projection, "interacted").agent_label, "");
    assert_eq!(
        row_for_activity(&projection, "dir").agent_label,
        "/root/first"
    );
}

#[test]
fn projection_keeps_v2_agent_paths_exact_for_malformed_then_valid_records() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item_with_path(
                "child_missing_path",
                "started",
                Some("thread_child"),
                None,
            ),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("child_command")),
        None,
    );
    assert_eq!(row_for_activity(&projection, "started").agent_label, "");
    assert_eq!(row_for_activity(&projection, "dir").agent_label, "");
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item_with_path(
                "child_valid_path",
                "interacted",
                Some("thread_child"),
                Some("/root/later"),
            ),
        ),
        Some("Main".to_string()),
    );

    assert_eq!(row_for_activity(&projection, "started").agent_label, "");
    assert_eq!(
        row_for_activity(&projection, "dir").agent_label,
        "/root/later"
    );
    assert_eq!(
        row_for_activity(&projection, "interacted").agent_label,
        "/root/later"
    );
}

#[test]
fn projection_retains_distinct_overlimit_v2_agent_paths_exactly() {
    let mut projection = ToolActivityProjection::default();
    let path_a = format!(
        "/root/first/{}",
        "a".repeat(ACTIVITY_LABEL_DISPLAY_BYTE_LIMIT)
    );
    let path_b = format!(
        "/root/second/{}",
        "b".repeat(ACTIVITY_LABEL_DISPLAY_BYTE_LIMIT)
    );

    for (item_id, child_thread_id, agent_path) in [
        ("child_first", "thread_first", path_a.as_str()),
        ("child_second", "thread_second", path_b.as_str()),
    ] {
        projection.apply_stream_event(
            &completed(
                "thread_parent",
                "turn_parent",
                subagent_activity_item_with_path(
                    item_id,
                    "interacted",
                    Some(child_thread_id),
                    Some(agent_path),
                ),
            ),
            Some("Main".to_string()),
        );
    }

    let rows = projection.rows_for_selected_thread(Some("thread_parent"));
    assert_eq!(rows.len(), 2);
    assert_eq!(
        row_for_activity(&projection, "interacted").agent_label,
        path_b
    );
    assert!(rows.iter().any(|row| row.agent_label == path_a));
    assert!(rows.iter().any(|row| row.agent_label == path_b));
    assert!(projection.retained_counts().record_payload_bytes >= path_a.len() + path_b.len());
}

#[test]
fn projection_keeps_multiple_child_rows_in_one_epoch_until_all_finish_then_reactivates() {
    let mut projection = ToolActivityProjection::default();
    for (child, item) in [("thread_a", "a"), ("thread_b", "b")] {
        projection.apply_stream_event(
            &started(
                "thread_parent",
                "turn_parent",
                collab_spawn_item_for(item, child),
            ),
            Some("Main".to_string()),
        );
        projection.apply_stream_event(&started(child, "turn_child", mcp_item(item, item)), None);
    }
    projection.apply_stream_event(
        &started("thread_a", "turn_child", mcp_item("a_second", "a_second")),
        None,
    );
    assert_eq!(
        projection
            .rows_for_selected_thread(Some("thread_parent"))
            .iter()
            .filter(|row| {
                row.agent_label == "Subagent" && row.status == ToolActivityRowStatus::Running
            })
            .map(|row| row.tool_display_value.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "a_second", "b"]
    );
    projection.apply_stream_event(
        &completed(
            "thread_a",
            "turn_child",
            mcp_item_with_status("a", "a", "completed"),
        ),
        None,
    );
    assert_eq!(
        projection
            .rows_for_selected_thread(Some("thread_parent"))
            .iter()
            .filter(|row| {
                row.agent_label == "Subagent" && row.status == ToolActivityRowStatus::Running
            })
            .map(|row| row.tool_display_value.as_str())
            .collect::<Vec<_>>(),
        vec!["a_second", "b"]
    );
    projection.apply_stream_event(
        &completed(
            "thread_a",
            "turn_child",
            mcp_item_with_status("a_second", "a_second", "completed"),
        ),
        None,
    );
    projection.apply_stream_event(
        &started(
            "thread_a",
            "turn_child_2",
            mcp_item("a_reactivated", "a_reactivated"),
        ),
        None,
    );
    assert_eq!(
        projection
            .rows_for_selected_thread(Some("thread_parent"))
            .iter()
            .filter(|row| {
                row.agent_label == "Subagent" && row.status == ToolActivityRowStatus::Running
            })
            .map(|row| row.tool_display_value.as_str())
            .collect::<Vec<_>>(),
        vec!["b", "a_reactivated"]
    );
}

#[test]
fn projection_v2_completed_replay_stays_finished_parent_visible_and_formats_metadata() {
    let mut projection = ToolActivityProjection::default();
    let v2 = completed(
        "thread_parent",
        "turn_parent",
        subagent_activity_item_with_path(
            "child_link",
            "interacted",
            Some("thread_child"),
            Some("/root/research"),
        ),
    );
    projection.apply_stream_event(&v2, Some("Main".to_string()));
    projection.apply_stream_event(&v2, Some("Main".to_string()));
    let row = row_for_activity(&projection, "interacted");
    assert_eq!(row.status, ToolActivityRowStatus::FinishedOk);
    assert_eq!(row.agent_label, "/root/research");
    assert_eq!(
        projection
            .rows_for_selected_thread(Some("thread_parent"))
            .len(),
        1
    );

    projection.apply_thread_read_metadata([&thread_read_metadata(
        "thread_child",
        None,
        Some("gpt-5.5"),
        None,
    )]);
    assert_eq!(
        row_for_activity(&projection, "interacted").agent_label,
        "/root/research (gpt-5.5)"
    );
    projection.apply_thread_read_metadata([&thread_read_metadata(
        "thread_child",
        None,
        None,
        Some("high"),
    )]);
    assert_eq!(
        row_for_activity(&projection, "interacted").agent_label,
        "/root/research (gpt-5.5/high)"
    );
}

#[test]
fn projection_legacy_metadata_suffix_and_per_record_v2_path_retention_are_bounded() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started(
            "thread_parent",
            "turn_parent",
            collab_spawn_item_for("legacy", "thread_legacy"),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started(
            "thread_legacy",
            "turn_legacy",
            command_item("legacy_command"),
        ),
        None,
    );
    projection.apply_thread_read_metadata([&thread_read_metadata(
        "thread_legacy",
        None,
        Some("gpt-5.5"),
        Some("high"),
    )]);
    assert_eq!(
        row_for_activity(&projection, "dir").agent_label,
        "Subagent (gpt-5.5/high)"
    );

    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item_with_path(
                "v2_link",
                "interacted",
                Some("thread_v2"),
                Some("/root/retained"),
            ),
        ),
        Some("Main".to_string()),
    );
    let before = projection.retained_counts();
    assert!(before.payload_bytes >= "/root/retained".len() + "thread_v2".len());
    for index in 0..ACTIVITY_COMPLETED_ROW_BUDGET + 12 {
        add_completed_command(&mut projection, "thread_other", index, "completed");
    }
    let after = projection.retained_counts();
    assert_eq!(after.parent_thread_links, 1);
    assert_eq!(after.root_turn_links, 1);
    assert!(after.payload_bytes < before.payload_bytes + ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET);
    projection.apply_stream_event(
        &turn_completed_with_status("thread_legacy", "turn_legacy", TurnStatus::Completed),
        None,
    );
    projection.apply_stream_event(
        &turn_completed_with_status("thread_parent", "turn_parent", TurnStatus::Completed),
        None,
    );
    for index in 0..ACTIVITY_COMPLETED_ROW_BUDGET + 1 {
        add_completed_command(&mut projection, "thread_post_legacy", index, "completed");
    }
    let reclaimed = projection.retained_counts();
    assert_eq!(reclaimed.parent_thread_links, 0);
    assert_eq!(reclaimed.root_turn_links, 0);
    assert_eq!(reclaimed.subagent_metadata_count, 0);
}

#[test]
fn projection_keeps_malformed_v2_activity_on_the_parent_without_a_child_label() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item("subagent_missing", "interacted", Some("   ")),
        ),
        Some("Main".to_string()),
    );

    let row = row_for_activity(&projection, "interacted");
    assert_eq!(row.agent_label, "Main");
}

#[test]
fn projection_keeps_legacy_collaboration_activity_on_main() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            collab_spawn_item_for("legacy_agent", "thread_child"),
        ),
        Some("Main".to_string()),
    );

    assert_eq!(
        row_for_activity(&projection, "spawnAgent").agent_label,
        "Main"
    );
}

#[test]
fn projection_keeps_pathless_v2_child_labels_empty_but_legacy_children_as_subagent() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item_with_path(
                "v2_missing_path",
                "interacted",
                Some("thread_v2_child"),
                None,
            ),
        ),
        Some("Main".to_string()),
    );
    let v2 = row_for_activity(&projection, "interacted");
    assert_eq!(v2.agent_label, "");
    assert_eq!(v2.status, ToolActivityRowStatus::FinishedOk);
    assert_eq!(
        projection
            .rows_for_selected_thread(Some("thread_parent"))
            .len(),
        1
    );
    projection.apply_stream_event(
        &started(
            "thread_v2_child",
            "turn_v2_child",
            command_item("v2_child_command"),
        ),
        None,
    );
    assert_eq!(row_for_activity(&projection, "dir").agent_label, "");

    projection.apply_stream_event(
        &started(
            "thread_parent",
            "turn_parent",
            collab_spawn_item_for("legacy", "thread_legacy"),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started(
            "thread_legacy",
            "turn_legacy",
            mcp_item("legacy_command", "legacy_tool"),
        ),
        None,
    );
    assert_eq!(
        row_for_activity(&projection, "legacy_tool").agent_label,
        "Subagent"
    );
}

#[test]
fn projection_upgrades_mixed_legacy_child_rows_and_ignores_later_malformed_v2_paths() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started(
            "thread_parent",
            "turn_parent",
            collab_spawn_item_for("legacy_spawn", "thread_child"),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("child_command")),
        None,
    );
    assert_eq!(row_for_activity(&projection, "dir").agent_label, "Subagent");

    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item_with_path(
                "valid_v2",
                "started",
                Some("thread_child"),
                Some("/root/mixed"),
            ),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item_with_path(
                "malformed_v2",
                "interacted",
                Some("thread_child"),
                Some("   "),
            ),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &completed(
            "thread_child",
            "turn_child",
            final_answer_item("child_handoff", "mixed handoff"),
        ),
        None,
    );

    assert_eq!(
        row_for_activity(&projection, "dir").agent_label,
        "/root/mixed"
    );
    assert_eq!(row_for_activity(&projection, "interacted").agent_label, "");
    assert_eq!(
        projection
            .rows()
            .iter()
            .find(|row| row.item_id() == "child_handoff")
            .map(|row| row.agent_label.as_str()),
        Some("/root/mixed")
    );
}

#[test]
fn projection_prunes_completed_child_derived_state_while_main_stays_running() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_parent", "turn_main", command_item("main")),
        Some("Main".to_string()),
    );
    for index in 0..ACTIVITY_COMPLETED_ROW_BUDGET + 12 {
        let child = format!("thread_child_{index}");
        projection.apply_stream_event(
            &completed(
                "thread_parent",
                "turn_main",
                subagent_activity_item_with_path(
                    format!("child_link_{index}").as_str(),
                    "interacted",
                    Some(child.as_str()),
                    Some(format!("/root/{index}").as_str()),
                ),
            ),
            Some("Main".to_string()),
        );
    }

    let counts = projection.retained_counts();
    assert_eq!(counts.records, ACTIVITY_COMPLETED_ROW_BUDGET + 1);
    assert!(counts.parent_thread_links <= ACTIVITY_COMPLETED_ROW_BUDGET);
    assert!(counts.root_turn_links <= ACTIVITY_COMPLETED_ROW_BUDGET);
    assert!(counts.subagent_metadata_count <= ACTIVITY_COMPLETED_ROW_BUDGET);
    assert!(counts.multi_agent_v2_child_threads <= ACTIVITY_COMPLETED_ROW_BUDGET);
    assert!(counts.multi_agent_v2_path_associations <= ACTIVITY_COMPLETED_ROW_BUDGET);
    assert!(counts.payload_bytes < ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET * 2);
    assert_eq!(projection.rows()[0].agent_label, "Main");
    assert_eq!(
        projection
            .rows_for_selected_thread(Some("thread_parent"))
            .first()
            .map(|row| row.agent_label.as_str()),
        Some("Main")
    );
}

#[test]
fn projection_charges_and_reclaims_v2_path_association_around_retained_child_rows() {
    let mut projection = ToolActivityProjection::default();
    let child_thread_id = "thread_child";
    let agent_path = "/root/retained/child";
    projection.apply_stream_event(
        &started("thread_parent", "turn_main", mcp_item("main", "main")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_main",
            subagent_activity_item_with_path(
                "child_link",
                "started",
                Some(child_thread_id),
                Some(agent_path),
            ),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started(child_thread_id, "turn_child", command_item("child_command")),
        None,
    );

    let associated = projection.retained_counts();
    assert_eq!(associated.multi_agent_v2_child_threads, 1);
    assert_eq!(
        associated.multi_agent_v2_child_thread_bytes,
        child_thread_id.len()
    );
    assert_eq!(associated.multi_agent_v2_path_associations, 1);
    assert_eq!(
        associated.multi_agent_v2_path_association_bytes,
        child_thread_id.len() + agent_path.len()
    );
    assert_eq!(row_for_activity(&projection, "dir").agent_label, agent_path);

    for index in 0..ACTIVITY_COMPLETED_ROW_BUDGET + 12 {
        add_completed_command(&mut projection, "thread_other", index, "completed");
    }
    let retained_for_running_child = projection.retained_counts();
    assert_eq!(
        retained_for_running_child.multi_agent_v2_path_associations,
        1
    );
    assert!(
        projection
            .rows()
            .iter()
            .all(|row| row.item_id() != "child_link")
    );
    assert_eq!(row_for_activity(&projection, "dir").agent_label, agent_path);

    projection.apply_stream_event(
        &turn_completed_with_status(child_thread_id, "turn_child", TurnStatus::Completed),
        None,
    );
    for index in ACTIVITY_COMPLETED_ROW_BUDGET + 12..ACTIVITY_COMPLETED_ROW_BUDGET * 2 + 24 {
        add_completed_command(&mut projection, "thread_other", index, "completed");
    }

    let reclaimed = projection.retained_counts();
    assert_eq!(reclaimed.multi_agent_v2_child_threads, 0);
    assert_eq!(reclaimed.multi_agent_v2_child_thread_bytes, 0);
    assert_eq!(reclaimed.multi_agent_v2_path_associations, 0);
    assert_eq!(reclaimed.multi_agent_v2_path_association_bytes, 0);
    assert_eq!(reclaimed.parent_thread_links, 0);
    assert_eq!(reclaimed.root_turn_links, 0);
    assert_eq!(projection.rows()[0].agent_label, "Main");
    assert_eq!(projection.rows()[0].status, ToolActivityRowStatus::Running);
}

#[test]
fn projection_charges_a_shared_v2_path_association_once() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("child_command")),
        None,
    );
    projection.apply_stream_event(
        &started(
            "thread_child",
            "turn_child",
            mcp_item("child_mcp", "child_mcp"),
        ),
        None,
    );
    let agent_path = format!(
        "/root/{}",
        "x".repeat(ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET / 3)
    );
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item_with_path(
                "v2_child_link",
                "started",
                Some("thread_child"),
                Some(agent_path.as_str()),
            ),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &turn_completed_with_status("thread_child", "turn_child", TurnStatus::Completed),
        None,
    );

    let retained = projection.retained_counts();
    assert_eq!(retained.records, 3);
    assert_eq!(retained.multi_agent_v2_child_threads, 1);
    assert_eq!(retained.multi_agent_v2_path_associations, 1);
    assert_eq!(row_for_activity(&projection, "dir").agent_label, agent_path);
    assert_eq!(
        row_for_activity(&projection, "child_mcp").agent_label,
        agent_path
    );
}

#[test]
fn projection_prunes_completed_child_row_with_oversized_v2_path_association() {
    let mut projection = ToolActivityProjection::default();
    let agent_path = format!(
        "/root/{}",
        "x".repeat(ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET)
    );
    projection.apply_stream_event(
        &started("thread_parent", "turn_main", mcp_item("main", "main")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("child_command")),
        None,
    );
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_main",
            subagent_activity_item_with_path(
                "child_link",
                "started",
                Some("thread_child"),
                Some(agent_path.as_str()),
            ),
        ),
        Some("Main".to_string()),
    );

    assert_eq!(
        projection
            .retained_counts()
            .multi_agent_v2_path_associations,
        1
    );
    assert_eq!(row_for_activity(&projection, "dir").agent_label, agent_path);
    assert!(
        projection
            .rows()
            .iter()
            .all(|row| row.item_id() != "child_link")
    );

    projection.apply_stream_event(
        &turn_completed_with_status("thread_child", "turn_child", TurnStatus::Completed),
        None,
    );

    let reclaimed = projection.retained_counts();
    assert_eq!(reclaimed.records, 1);
    assert_eq!(reclaimed.multi_agent_v2_child_threads, 0);
    assert_eq!(reclaimed.multi_agent_v2_path_associations, 0);
    assert_eq!(reclaimed.parent_thread_links, 0);
    assert_eq!(projection.rows()[0].tool_display_value, "main");
    assert_eq!(projection.rows()[0].status, ToolActivityRowStatus::Running);
}

#[test]
fn projection_does_not_retain_v2_identity_for_a_legacy_receiver_reference() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started(
            "thread_parent",
            "turn_parent",
            collab_spawn_item_for("legacy_spawn", "thread_child"),
        ),
        Some("Main".to_string()),
    );
    let agent_path = format!(
        "/root/{}",
        "x".repeat(ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET / 2)
    );
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item_with_path(
                "v2_child_link",
                "interacted",
                Some("thread_child"),
                Some(agent_path.as_str()),
            ),
        ),
        Some("Main".to_string()),
    );

    let retained = projection.retained_counts();
    assert_eq!(retained.records, 1);
    assert_eq!(retained.parent_thread_links, 1);
    assert_eq!(retained.multi_agent_v2_child_threads, 0);
    assert_eq!(retained.multi_agent_v2_child_thread_bytes, 0);
    assert_eq!(retained.multi_agent_v2_path_associations, 0);
    assert_eq!(retained.multi_agent_v2_path_association_bytes, 0);
    assert_eq!(projection.rows()[0].tool_display_value, "spawnAgent");
    assert_eq!(projection.rows()[0].agent_label, "Main");
    assert!(
        projection
            .rows()
            .iter()
            .all(|row| row.item_id() != "v2_child_link")
    );
}

#[test]
fn projection_does_not_retain_v2_identity_for_an_ancestor_reference() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("child_command")),
        None,
    );
    let agent_path = format!(
        "/root/{}",
        "x".repeat(ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET)
    );
    projection.apply_stream_event(
        &completed(
            "thread_root",
            "turn_root",
            subagent_activity_item_with_path(
                "v2_child_link",
                "started",
                Some("thread_child"),
                Some(agent_path.as_str()),
            ),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started(
            "thread_child",
            "turn_child",
            collab_spawn_item_for("legacy_grandchild", "thread_grandchild"),
        ),
        None,
    );
    projection.apply_stream_event(
        &started(
            "thread_grandchild",
            "turn_grandchild",
            command_item("grandchild_command"),
        ),
        None,
    );
    assert_eq!(
        projection
            .retained_counts()
            .multi_agent_v2_path_associations,
        1
    );

    projection.apply_stream_event(
        &turn_completed_with_status("thread_child", "turn_child", TurnStatus::Completed),
        None,
    );

    let retained = projection.retained_counts();
    assert_eq!(retained.records, 1);
    assert_eq!(retained.parent_thread_links, 2);
    assert_eq!(retained.multi_agent_v2_child_threads, 0);
    assert_eq!(retained.multi_agent_v2_path_associations, 0);
    assert_eq!(
        row_for_activity(&projection, "dir").tool_display_value,
        "dir"
    );
    assert_eq!(row_for_activity(&projection, "dir").agent_label, "Subagent");
}

#[test]
fn projection_prunes_v2_ownership_with_its_completed_lifecycle_record() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item("subagent_1", "interacted", Some("thread_child")),
        ),
        Some("Main".to_string()),
    );
    for index in 0..ACTIVITY_COMPLETED_ROW_BUDGET + 10 {
        add_completed_command(&mut projection, "thread_other", index, "completed");
    }

    let counts = projection.retained_counts();
    assert_eq!(counts.parent_thread_links, 0);
    assert_eq!(counts.root_turn_links, 0);
    assert_eq!(counts.subagent_metadata_count, 0);
    assert_eq!(counts.multi_agent_v2_child_threads, 0);
    assert_eq!(counts.multi_agent_v2_path_associations, 0);
    assert_eq!(counts.label_count, 0);
    assert!(
        projection
            .rows()
            .iter()
            .all(|row| row.tool_display_value != "interacted")
    );
}

#[test]
fn projection_charges_legacy_receiver_identity_bytes_while_main_stays_running() {
    let mut projection = ToolActivityProjection::default();
    projection.set_selected_thread_id(Some("thread_parent"));
    projection.apply_stream_event(
        &started("thread_parent", "turn_parent", command_item("main")),
        Some("Main".to_string()),
    );
    let oversized_receiver_id =
        "child_".to_string() + &"x".repeat(ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET);

    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            collab_spawn_item_for("legacy_spawn", &oversized_receiver_id),
        ),
        Some("Main".to_string()),
    );

    assert_eq!(projection.retained_counts().records, 1);
    assert_eq!(projection.retained_counts().parent_thread_links, 0);
    let visible = projection.rows_for_selected_thread(Some("thread_parent"));
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].agent_label, "Main");
    assert_eq!(visible[0].status, ToolActivityRowStatus::Running);
    assert!(
        visible
            .iter()
            .all(|row| row.tool_display_value != "spawnAgent")
    );
}

#[test]
fn projection_prunes_completed_v2_activity_with_an_oversized_child_identity() {
    let mut projection = ToolActivityProjection::default();
    projection.set_selected_thread_id(Some("thread_parent"));
    let oversized_child_id = "child_".to_string()
        + &"x".repeat(ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET.saturating_add(1));

    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item(
                "subagent_oversized",
                "interacted",
                Some(&oversized_child_id),
            ),
        ),
        Some("Main".to_string()),
    );

    assert!(projection.rows().is_empty());
    assert_eq!(projection.retained_counts().records, 0);
    assert_eq!(projection.retained_counts().parent_thread_links, 0);
}

#[test]
fn projection_keeps_selected_v2_activity_at_the_completed_identity_byte_boundary() {
    let mut projection = ToolActivityProjection::default();
    projection.set_selected_thread_id(Some("thread_parent"));
    let fixed_payload_bytes =
        "Main".len() + "interacted".len() + "main/research".len().saturating_mul(2);
    let child_id_bytes =
        ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET.saturating_sub(fixed_payload_bytes) / 4;
    let child_id = "x".repeat(child_id_bytes);

    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item("subagent_boundary", "interacted", Some(&child_id)),
        ),
        Some("Main".to_string()),
    );

    assert_eq!(projection.retained_counts().records, 1);
    assert_eq!(
        projection.row_count_for_selected_thread(Some("thread_parent")),
        1
    );
}

#[test]
fn projection_applies_selected_v2_identity_budget_newest_first_and_prunes_old_ownership() {
    let mut projection = ToolActivityProjection::default();
    projection.set_selected_thread_id(Some("thread_parent"));
    let fixed_payload_bytes =
        "Main".len() + "interacted".len() + "main/research".len().saturating_mul(2);
    let child_id_bytes =
        ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET.saturating_sub(fixed_payload_bytes) / 4;
    let child_a = "a".repeat(child_id_bytes);
    let child_b = "b".repeat(child_id_bytes);

    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item("subagent_old", "started", Some(&child_a)),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item("subagent_new", "interacted", Some(&child_b)),
        ),
        Some("Main".to_string()),
    );

    assert_eq!(projection.retained_counts().records, 1);
    assert_eq!(projection.retained_counts().parent_thread_links, 1);
    assert_eq!(projection.retained_counts().label_count, 0);
    assert_eq!(
        projection
            .rows_for_selected_thread(Some("thread_parent"))
            .iter()
            .map(|row| row.tool_display_value.as_str())
            .collect::<Vec<_>>(),
        vec!["interacted"]
    );
}

#[test]
fn diagnostic_sample_projects_recognized_v2_kinds_with_immediate_parent_identity() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item("item_child", "started", Some("thread_child")),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &completed(
            "thread_child",
            "turn_child",
            subagent_activity_item("item_grandchild", "interrupted", Some("thread_grandchild")),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item("item_invalid_kind", "other", Some("thread_ignored")),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            collab_spawn_item_for("legacy_item", "thread_legacy"),
        ),
        Some("Main".to_string()),
    );

    let sample = projection.multi_agent_v2_activity_diagnostic_sample(Some("thread_parent"));

    assert!(!sample.truncated);
    assert_eq!(sample.records.len(), 2);
    assert_eq!(sample.records[0].parent_thread_id, "thread_child");
    assert_eq!(sample.records[0].parent_turn_id, "turn_child");
    assert_eq!(sample.records[0].parent_item_id, "item_grandchild");
    assert_eq!(
        sample.records[0].child_thread_id.as_deref(),
        Some("thread_grandchild")
    );
    assert_eq!(sample.records[0].lifecycle_kind, "interrupted");
    assert_eq!(sample.records[0].row_status, "finished_ok");
    assert_eq!(sample.records[1].parent_thread_id, "thread_parent");
    assert_eq!(sample.records[1].lifecycle_kind, "started");
    assert!(
        projection
            .multi_agent_v2_activity_diagnostic_sample(None)
            .records
            .is_empty()
    );
}

#[test]
fn diagnostic_sample_marks_invalid_identity_omission_and_item_cap_truncation() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item("", "interacted", Some("thread_child")),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item(&"x".repeat(513), "interacted", Some("thread_child")),
        ),
        Some("Main".to_string()),
    );
    for index in 0..65 {
        projection.apply_stream_event(
            &completed(
                "thread_parent",
                "turn_parent",
                subagent_activity_item(
                    format!("item_{index:03}").as_str(),
                    "interacted",
                    Some("thread_child"),
                ),
            ),
            Some("Main".to_string()),
        );
    }

    let sample = projection.multi_agent_v2_activity_diagnostic_sample(Some("thread_parent"));

    assert_eq!(sample.records.len(), 64);
    assert!(sample.truncated);
    assert_eq!(sample.records[0].parent_item_id, "item_064");
    assert_eq!(sample.records[63].parent_item_id, "item_001");
}

#[test]
fn diagnostic_sample_stops_at_aggregate_identity_budget_without_skipping() {
    let mut projection = ToolActivityProjection::default();
    let thread_id = "t".repeat(512);
    let turn_id = "u".repeat(512);
    projection.set_selected_thread_id(Some(&thread_id));
    for index in 0..64 {
        projection.apply_stream_event(
            &completed(
                &thread_id,
                &turn_id,
                subagent_activity_item(
                    format!("{index:03}_{}", "i".repeat(507)).as_str(),
                    "interacted",
                    None,
                ),
            ),
            Some("Main".to_string()),
        );
    }

    let sample = projection.multi_agent_v2_activity_diagnostic_sample(Some(&thread_id));

    assert_eq!(sample.records.len(), 42);
    assert!(sample.truncated);
    assert_eq!(&sample.records.last().unwrap().parent_item_id[..3], "022");
}

#[test]
fn diagnostic_sample_reports_finished_error_and_not_applicable_without_child_identity() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &completed(
            "thread_parent",
            "turn_parent",
            subagent_activity_item_with_status("item_error", "interacted", None, "failed"),
        ),
        Some("Main".to_string()),
    );

    let sample = projection.multi_agent_v2_activity_diagnostic_sample(Some("thread_parent"));

    assert_eq!(sample.records.len(), 1);
    assert_eq!(sample.records[0].row_status, "finished_error");
    assert_eq!(sample.records[0].child_thread_id, None);
}

#[test]
fn diagnostic_sample_omits_overbound_parent_thread_and_turn_without_item_cap() {
    for (thread_id, turn_id) in [
        ("t".repeat(512) + " ", "turn_parent".to_string()),
        ("thread_parent".to_string(), "u".repeat(513)),
    ] {
        let mut projection = ToolActivityProjection::default();
        projection.set_selected_thread_id(Some(&thread_id));
        projection.apply_stream_event(
            &completed(
                &thread_id,
                &turn_id,
                subagent_activity_item("item_activity", "interacted", None),
            ),
            Some("Main".to_string()),
        );

        let sample = projection.multi_agent_v2_activity_diagnostic_sample(Some(&thread_id));

        assert!(sample.records.is_empty());
        assert!(sample.truncated);
    }
}

#[test]
fn diagnostic_sample_omits_blank_and_overbound_parent_item_without_item_cap() {
    for item_id in [String::from("   "), "i".repeat(513)] {
        let mut projection = ToolActivityProjection::default();
        projection.set_selected_thread_id(Some("thread_parent"));
        projection.apply_stream_event(
            &completed(
                "thread_parent",
                "turn_parent",
                subagent_activity_item(&item_id, "interacted", None),
            ),
            Some("Main".to_string()),
        );

        let sample = projection.multi_agent_v2_activity_diagnostic_sample(Some("thread_parent"));

        assert!(sample.records.is_empty());
        assert!(sample.truncated);
    }
}

#[test]
fn diagnostic_sample_preserves_whitespace_identity_and_omits_overbound_child() {
    let mut projection = ToolActivityProjection::default();
    let parent_thread_id = "thread_parent ";
    let child_thread_id = "thread_child ";
    projection.set_selected_thread_id(Some(parent_thread_id));
    projection.apply_stream_event(
        &completed(
            parent_thread_id,
            "turn_parent ",
            subagent_activity_item("item_activity ", "started", Some(child_thread_id)),
        ),
        Some("Main".to_string()),
    );

    let exact = projection.multi_agent_v2_activity_diagnostic_sample(Some(parent_thread_id));
    assert_eq!(exact.records.len(), 1);
    assert_eq!(exact.records[0].parent_thread_id, parent_thread_id);
    assert_eq!(exact.records[0].parent_turn_id, "turn_parent ");
    assert_eq!(exact.records[0].parent_item_id, "item_activity ");
    assert_eq!(
        exact.records[0].child_thread_id.as_deref(),
        Some(child_thread_id)
    );

    let oversized_child_id = "c".repeat(512) + " ";
    projection.apply_stream_event(
        &completed(
            parent_thread_id,
            "turn_parent ",
            subagent_activity_item(
                "item_oversized_child",
                "interacted",
                Some(&oversized_child_id),
            ),
        ),
        Some("Main".to_string()),
    );
    let bounded = projection.multi_agent_v2_activity_diagnostic_sample(Some(parent_thread_id));
    assert_eq!(bounded.records.len(), 1);
    assert!(bounded.truncated);
}

#[test]
fn diagnostic_sample_enforces_utf8_identity_byte_boundary_without_normalization() {
    let at_limit = "\u{00e9}".repeat(256);
    let over_limit = format!("{at_limit}x");
    let mut projection = ToolActivityProjection::default();
    projection.set_selected_thread_id(Some(&at_limit));
    projection.apply_stream_event(
        &completed(
            &at_limit,
            "turn_parent",
            subagent_activity_item("item_boundary", "interacted", None),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &completed(
            &at_limit,
            "turn_parent",
            subagent_activity_item(&over_limit, "interacted", None),
        ),
        Some("Main".to_string()),
    );

    let sample = projection.multi_agent_v2_activity_diagnostic_sample(Some(&at_limit));

    assert_eq!(sample.records.len(), 1);
    assert_eq!(sample.records[0].parent_thread_id, at_limit);
    assert!(sample.truncated);
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

fn reasoning_summary_part_added(
    thread_id: &str,
    turn_id: &str,
    item_id: &str,
    summary_index: usize,
) -> TurnStreamEvent {
    TurnStreamEvent::ReasoningSummaryPartAdded {
        thread_id: thread_id.to_string(),
        turn_id: turn_id.to_string(),
        item_id: item_id.to_string(),
        summary_index,
    }
}

fn reasoning_summary_delta(
    thread_id: &str,
    turn_id: &str,
    item_id: &str,
    summary_index: usize,
    delta: &str,
) -> TurnStreamEvent {
    TurnStreamEvent::ReasoningSummaryTextDelta {
        thread_id: thread_id.to_string(),
        turn_id: turn_id.to_string(),
        item_id: item_id.to_string(),
        summary_index,
        delta: delta.to_string(),
    }
}

fn turn_completed_with_status(
    thread_id: &str,
    turn_id: &str,
    status: TurnStatus,
) -> TurnStreamEvent {
    TurnStreamEvent::TurnCompleted {
        thread_id: thread_id.to_string(),
        turn: TurnInfo {
            id: turn_id.to_string(),
            status,
            items: Vec::new(),
            error: None,
        },
    }
}

fn agent_label_updated(thread_id: &str, label: &str) -> TurnStreamEvent {
    TurnStreamEvent::AgentLabelUpdated {
        thread_id: thread_id.to_string(),
        label: label.to_string(),
    }
}

fn thread_archived(thread_id: &str) -> TurnStreamEvent {
    TurnStreamEvent::ThreadArchived {
        thread_id: thread_id.to_string(),
    }
}

fn thread_unarchived(thread_id: &str) -> TurnStreamEvent {
    TurnStreamEvent::ThreadUnarchived {
        thread_id: thread_id.to_string(),
    }
}

fn thread_deleted(thread_id: &str) -> TurnStreamEvent {
    TurnStreamEvent::ThreadDeleted {
        thread_id: thread_id.to_string(),
    }
}

fn thread_summary(thread_id: &str, agent_nickname: Option<&str>) -> ThreadSummary {
    ThreadSummary {
        id: thread_id.to_string(),
        forked_from_id: None,
        cwd: PathBuf::from("C:/work/beryl"),
        preview: format!("{thread_id} preview"),
        name: None,
        agent_nickname: agent_nickname.map(str::to_string),
        path: None,
        created_at: 1,
        updated_at: 2,
        model_provider: "openai".to_string(),
        ephemeral: false,
    }
}

fn thread_read_metadata(
    thread_id: &str,
    agent_nickname: Option<&str>,
    model: Option<&str>,
    reasoning_effort: Option<&str>,
) -> ThreadReadMetadata {
    ThreadReadMetadata {
        thread: thread_summary(thread_id, agent_nickname),
        session_metadata: ThreadSessionMetadata {
            model: model.map(str::to_string),
            model_provider: None,
            reasoning_effort: reasoning_effort.map(str::to_string),
        },
    }
}

fn observed_child_projection() -> ToolActivityProjection {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_parent", "turn_parent", collab_spawn_item("agent_1")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("cmd_1")),
        None,
    );
    projection
}

fn row_for_activity<'a>(
    projection: &'a ToolActivityProjection,
    tool_display_value: &str,
) -> &'a tool_activity::ToolActivityRow {
    projection
        .rows()
        .iter()
        .find(|row| row.tool_display_value == tool_display_value)
        .expect("activity row should be visible")
}

fn add_completed_command(
    projection: &mut ToolActivityProjection,
    thread_id: &str,
    index: usize,
    status: &str,
) {
    projection.apply_stream_event(
        &completed(
            thread_id,
            format!("turn_{index}").as_str(),
            command_item_with_command(
                format!("cmd_{index}").as_str(),
                &format!("cmd {index}"),
                status,
            ),
        ),
        Some("Main".to_string()),
    );
}

fn mcp_item(item_id: &str, tool: &str) -> ThreadItem {
    mcp_item_with_status(item_id, tool, "inProgress")
}

fn mcp_item_with_status(item_id: &str, tool: &str, status: &str) -> ThreadItem {
    serde_json::from_value(json!({
        "id": item_id,
        "type": "mcpToolCall",
        "server": "filesystem",
        "tool": tool,
        "arguments": {},
        "status": status
    }))
    .unwrap()
}

fn command_item(item_id: &str) -> ThreadItem {
    command_item_with_status(item_id, "inProgress")
}

fn command_item_with_status(item_id: &str, status: &str) -> ThreadItem {
    command_item_with_command(item_id, "dir", status)
}

fn command_item_with_command(item_id: &str, command: &str, status: &str) -> ThreadItem {
    serde_json::from_value(json!({
        "id": item_id,
        "type": "commandExecution",
        "command": command,
        "cwd": "C:/work/beryl",
        "status": status
    }))
    .unwrap()
}

fn final_answer_item(item_id: &str, text: &str) -> ThreadItem {
    serde_json::from_value(json!({
        "id": item_id,
        "type": "agentMessage",
        "phase": "final_answer",
        "text": text
    }))
    .unwrap()
}

fn projected_command_display_value(command: &str) -> String {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started(
            "thread_main",
            "turn_1",
            command_item_with_command("cmd_1", command, "inProgress"),
        ),
        Some("Main".to_string()),
    );
    projection.rows()[0].tool_display_value.clone()
}

fn file_change_item(item_id: &str, changes: Value) -> ThreadItem {
    serde_json::from_value(json!({
        "id": item_id,
        "type": "fileChange",
        "changes": changes,
        "status": "inProgress"
    }))
    .unwrap()
}

fn projected_file_change_display_value(changes: Value) -> String {
    projected_file_change_display_value_for_execution_target(changes, None)
}

fn projected_file_change_display_value_for_target(
    changes: Value,
    execution_target: &WorkspaceId,
) -> String {
    projected_file_change_display_value_for_execution_target(changes, Some(execution_target))
}

fn projected_file_change_display_value_for_execution_target(
    changes: Value,
    execution_target: Option<&WorkspaceId>,
) -> String {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event_with_execution_target(
        &started(
            "thread_main",
            "turn_1",
            file_change_item("patch_1", changes),
        ),
        Some("Main".to_string()),
        execution_target,
    );
    projection.rows()[0].tool_display_value.clone()
}

fn reasoning_item(item_id: &str) -> ThreadItem {
    reasoning_item_with_summary(item_id, &[])
}

fn reasoning_item_with_summary(item_id: &str, summary: &[&str]) -> ThreadItem {
    serde_json::from_value(json!({
        "id": item_id,
        "type": "reasoning",
        "content": ["Raw hidden reasoning details."],
        "summary": summary
    }))
    .unwrap()
}

fn resource_item(item_id: &str) -> ThreadItem {
    serde_json::from_value(json!({
        "id": item_id,
        "type": "mcpToolCall",
        "server": "workspace",
        "arguments": {},
        "mcpAppResourceUri": "file:///workspace/state",
        "status": "inProgress"
    }))
    .unwrap()
}

fn collab_spawn_item(item_id: &str) -> ThreadItem {
    collab_spawn_item_for(item_id, "thread_child")
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

fn collab_spawn_item_with_runtime_metadata(
    item_id: &str,
    receiver_thread_ids: &[&str],
    model: Option<&str>,
    reasoning_effort: Option<&str>,
) -> ThreadItem {
    let mut item = json!({
        "id": item_id,
        "type": "collabAgentToolCall",
        "agentsStates": {},
        "receiverThreadIds": receiver_thread_ids,
        "senderThreadId": "thread_parent",
        "status": "inProgress",
        "tool": "spawnAgent"
    });
    if let Some(model) = model {
        item["model"] = json!(model);
    }
    if let Some(reasoning_effort) = reasoning_effort {
        item["reasoningEffort"] = json!(reasoning_effort);
    }
    serde_json::from_value(item).unwrap()
}

fn subagent_activity_item(item_id: &str, kind: &str, agent_thread_id: Option<&str>) -> ThreadItem {
    subagent_activity_item_with_path_and_status(
        item_id,
        kind,
        agent_thread_id,
        Some("main/research"),
        "completed",
    )
}

fn subagent_activity_item_with_status(
    item_id: &str,
    kind: &str,
    agent_thread_id: Option<&str>,
    status: &str,
) -> ThreadItem {
    subagent_activity_item_with_path_and_status(
        item_id,
        kind,
        agent_thread_id,
        Some("main/research"),
        status,
    )
}

fn subagent_activity_item_with_path(
    item_id: &str,
    kind: &str,
    agent_thread_id: Option<&str>,
    agent_path: Option<&str>,
) -> ThreadItem {
    subagent_activity_item_with_path_and_status(
        item_id,
        kind,
        agent_thread_id,
        agent_path,
        "completed",
    )
}

fn subagent_activity_item_with_path_and_status(
    item_id: &str,
    kind: &str,
    agent_thread_id: Option<&str>,
    agent_path: Option<&str>,
    status: &str,
) -> ThreadItem {
    let mut item = json!({
        "id": item_id,
        "type": "subAgentActivity",
        "kind": kind,
        "status": status,
    });
    if let Some(agent_thread_id) = agent_thread_id {
        item["agentThreadId"] = json!(agent_thread_id);
    }
    if let Some(agent_path) = agent_path {
        item["agentPath"] = json!(agent_path);
    }
    serde_json::from_value(item).unwrap()
}
