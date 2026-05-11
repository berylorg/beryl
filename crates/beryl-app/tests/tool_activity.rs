use std::path::PathBuf;

#[path = "../src/shell/tool_activity.rs"]
mod tool_activity;

use beryl_backend::{
    JsonRpcError, ThreadItem, ThreadReadMetadata, ThreadSessionMetadata, ThreadSummary, TurnInfo,
    TurnStatus, TurnStreamEvent,
};
use beryl_model::workspace::WorkspaceId;
use serde_json::{Value, json};
use tool_activity::{ToolActivityProjection, ToolActivityRowStatus};

#[test]
fn projection_keeps_session_history_sorted_by_running_state_and_start_time() {
    let mut projection = ToolActivityProjection::default();

    projection.apply_stream_event(
        &started("thread_main", "turn_1", mcp_item("mcp_1", "read_file")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_main", "turn_2", command_item("mcp_1")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_child", "turn_1", resource_item("resource_1")),
        None,
    );

    let rows = projection.rows();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].agent_label, "");
    assert_eq!(rows[0].tool_display_value, "file:///workspace/state");
    assert_eq!(rows[0].status, ToolActivityRowStatus::Running);
    assert_eq!(rows[1].tool_display_value, "dir");
    assert_eq!(rows[1].status, ToolActivityRowStatus::Running);
    assert_eq!(rows[2].agent_label, "Main");
    assert_eq!(rows[2].tool_display_value, "read_file");
    assert_eq!(rows[2].status, ToolActivityRowStatus::Running);

    projection.apply_stream_event(
        &completed(
            "thread_main",
            "turn_1",
            mcp_item_with_status("mcp_1", "read_file", "completed"),
        ),
        Some("Main".to_string()),
    );

    let rows = projection.rows();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].tool_display_value, "file:///workspace/state");
    assert_eq!(rows[0].status, ToolActivityRowStatus::Running);
    assert_eq!(rows[1].tool_display_value, "dir");
    assert_eq!(rows[1].status, ToolActivityRowStatus::Running);
    assert_eq!(rows[2].tool_display_value, "read_file");
    assert_eq!(rows[2].status, ToolActivityRowStatus::FinishedOk);
}

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
fn projection_shows_reasoning_lifecycle_and_summary_updates() {
    let mut projection = ToolActivityProjection::default();

    projection.apply_stream_event(
        &reasoning_summary_part_added("thread_main", "turn_1", "reasoning_1", 0),
        Some("Main".to_string()),
    );
    assert_eq!(projection.rows().len(), 1);
    assert_eq!(projection.rows()[0].agent_label, "Main");
    assert_eq!(projection.rows()[0].tool_display_value, "reasoning");
    assert_eq!(projection.rows()[0].status, ToolActivityRowStatus::Running);

    projection.apply_stream_event(
        &reasoning_summary_delta(
            "thread_main",
            "turn_1",
            "reasoning_1",
            0,
            "Checking options",
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &reasoning_summary_delta("thread_main", "turn_1", "reasoning_1", 0, " carefully."),
        Some("Main".to_string()),
    );
    assert_eq!(
        projection.rows()[0].tool_display_value,
        "reasoning: Checking options carefully."
    );

    projection.apply_stream_event(
        &started("thread_main", "turn_2", command_item("cmd_1")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_main", "turn_1", reasoning_item("reasoning_1")),
        Some("Main".to_string()),
    );

    let rows = projection.rows();
    assert_eq!(rows[0].tool_display_value, "dir");
    assert_eq!(
        rows[1].tool_display_value,
        "reasoning: Checking options carefully."
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
fn projection_scopes_visible_rows_to_selected_thread_and_owned_subagents() {
    let mut projection = ToolActivityProjection::default();

    projection.apply_stream_event(
        &started("thread_child_a", "turn_child_a", command_item("cmd_a")),
        Some("Child A".to_string()),
    );
    projection.apply_stream_event(
        &started(
            "thread_parent_a",
            "turn_parent_a",
            collab_spawn_item_for("agent_a", "thread_child_a"),
        ),
        Some("Main A".to_string()),
    );
    projection.apply_stream_event(
        &started(
            "thread_parent_b",
            "turn_parent_b",
            collab_spawn_item_for("agent_b", "thread_child_b"),
        ),
        Some("Main B".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_child_b", "turn_child_b", command_item("cmd_b")),
        Some("Child B".to_string()),
    );
    projection.apply_stream_event(
        &started(
            "thread_other",
            "turn_other",
            mcp_item("mcp_other", "search"),
        ),
        Some("Other".to_string()),
    );

    let parent_a_rows = projection.rows_for_selected_thread(Some("thread_parent_a"));
    assert_eq!(parent_a_rows.len(), 2);
    assert_eq!(parent_a_rows[0].agent_label, "Main A");
    assert_eq!(parent_a_rows[0].tool_display_value, "spawnAgent");
    assert_eq!(parent_a_rows[1].agent_label, "Child A");
    assert_eq!(parent_a_rows[1].tool_display_value, "dir");
    assert_eq!(
        projection.row_count_for_selected_thread(Some("thread_parent_a")),
        2
    );
    let parent_a_window = projection.rows_for_selected_thread_window(Some("thread_parent_a"), 1..8);
    assert_eq!(parent_a_window.len(), 1);
    assert_eq!(parent_a_window[0].0, 1);
    assert_eq!(parent_a_window[0].1.agent_label, "Child A");

    let parent_b_rows = projection.rows_for_selected_thread(Some("thread_parent_b"));
    assert_eq!(parent_b_rows.len(), 2);
    assert_eq!(parent_b_rows[0].agent_label, "Child B");
    assert_eq!(parent_b_rows[0].tool_display_value, "dir");
    assert_eq!(parent_b_rows[1].agent_label, "Main B");
    assert_eq!(parent_b_rows[1].tool_display_value, "spawnAgent");

    let child_a_rows = projection.rows_for_selected_thread(Some("thread_child_a"));
    assert_eq!(child_a_rows.len(), 1);
    assert_eq!(child_a_rows[0].agent_label, "Child A");
    assert_eq!(child_a_rows[0].tool_display_value, "dir");

    assert!(projection.rows_for_selected_thread(None).is_empty());
    assert!(projection.rows_for_selected_thread(Some("")).is_empty());
    assert_eq!(projection.row_count_for_selected_thread(None), 0);
    assert_eq!(projection.row_count_for_selected_thread(Some("")), 0);
    assert!(
        projection
            .rows_for_selected_thread_window(None, 0..2)
            .is_empty()
    );
    assert_eq!(projection.rows().len(), 5);

    let parent_a_rows_after_switch = projection.rows_for_selected_thread(Some("thread_parent_a"));
    assert_eq!(parent_a_rows_after_switch.len(), 2);
    assert_eq!(parent_a_rows_after_switch[1].agent_label, "Child A");
}

#[test]
fn projection_returns_bounded_selected_thread_row_windows() {
    let mut projection = ToolActivityProjection::default();

    for index in 0..12 {
        let command = format!("cmd {index}");
        projection.apply_stream_event(
            &started(
                "thread_main",
                format!("turn_{index}").as_str(),
                command_item_with_command(format!("cmd_{index}").as_str(), &command, "inProgress"),
            ),
            Some("Main".to_string()),
        );
    }
    projection.apply_stream_event(
        &started("thread_other", "turn_other", command_item("cmd_other")),
        Some("Other".to_string()),
    );

    assert_eq!(
        projection.row_count_for_selected_thread(Some("thread_main")),
        12
    );
    assert_eq!(projection.rows().len(), 13);

    let window = projection.rows_for_selected_thread_window(Some("thread_main"), 2..5);
    assert_eq!(window.len(), 3);
    assert_eq!(
        window.iter().map(|(index, _)| *index).collect::<Vec<_>>(),
        vec![2, 3, 4]
    );
    assert_eq!(
        window
            .iter()
            .map(|(_, row)| row.tool_display_value.as_str())
            .collect::<Vec<_>>(),
        vec!["cmd 9", "cmd 8", "cmd 7"]
    );

    let oversized_window = projection.rows_for_selected_thread_window(Some("thread_main"), 10..40);
    assert_eq!(oversized_window.len(), 2);
    assert_eq!(oversized_window[0].0, 10);
    assert_eq!(oversized_window[1].0, 11);
}

#[test]
fn projection_makes_preexisting_child_rows_visible_after_ownership_arrives() {
    let mut projection = ToolActivityProjection::default();

    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("cmd_child")),
        Some("Child".to_string()),
    );
    assert!(
        projection
            .rows_for_selected_thread(Some("thread_parent"))
            .is_empty()
    );

    projection.apply_stream_event(
        &started(
            "thread_parent",
            "turn_parent",
            collab_spawn_item("agent_parent"),
        ),
        Some("Main".to_string()),
    );

    let parent_rows = projection.rows_for_selected_thread(Some("thread_parent"));
    assert_eq!(parent_rows.len(), 2);
    assert_eq!(parent_rows[0].agent_label, "Main");
    assert_eq!(parent_rows[0].tool_display_value, "spawnAgent");
    assert_eq!(parent_rows[1].agent_label, "Child");
    assert_eq!(parent_rows[1].tool_display_value, "dir");
}

#[test]
fn projection_includes_child_reasoning_before_and_after_ownership_arrives() {
    let mut projection = ToolActivityProjection::default();

    projection.apply_stream_event(
        &started(
            "thread_child",
            "turn_child",
            reasoning_item("reasoning_child"),
        ),
        None,
    );
    assert!(
        projection
            .rows_for_selected_thread(Some("thread_parent"))
            .is_empty()
    );

    projection.apply_stream_event(
        &started(
            "thread_parent",
            "turn_parent",
            collab_spawn_item("agent_parent"),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &reasoning_summary_delta(
            "thread_child",
            "turn_child",
            "reasoning_child",
            0,
            "Inspecting child context.",
        ),
        None,
    );
    projection.apply_stream_event(
        &started(
            "thread_child",
            "turn_child_2",
            reasoning_item("reasoning_child_2"),
        ),
        None,
    );

    let parent_rows = projection.rows_for_selected_thread(Some("thread_parent"));
    assert_eq!(parent_rows.len(), 3);
    assert_eq!(parent_rows[0].agent_label, "");
    assert_eq!(parent_rows[0].tool_display_value, "reasoning");
    assert_eq!(parent_rows[1].agent_label, "Main");
    assert_eq!(parent_rows[1].tool_display_value, "spawnAgent");
    assert_eq!(parent_rows[2].agent_label, "");
    assert_eq!(
        parent_rows[2].tool_display_value,
        "reasoning: Inspecting child context."
    );
}

#[test]
fn projection_repairs_child_reasoning_rows_when_nickname_resolves() {
    let mut projection = ToolActivityProjection::default();

    projection.apply_stream_event(
        &started("thread_parent", "turn_parent", collab_spawn_item("agent_1")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started(
            "thread_child",
            "turn_child",
            reasoning_item("reasoning_child"),
        ),
        None,
    );

    let parent_rows = projection.rows_for_selected_thread(Some("thread_parent"));
    assert_eq!(parent_rows[0].agent_label, "");
    assert_eq!(parent_rows[0].tool_display_value, "reasoning");

    projection.apply_stream_event(&thread_started("thread_child", Some("Hooke")), None);

    let parent_rows = projection.rows_for_selected_thread(Some("thread_parent"));
    let reasoning_row = parent_rows
        .iter()
        .find(|row| row.tool_display_value == "reasoning")
        .expect("child reasoning row should remain visible");
    assert_eq!(reasoning_row.agent_label, "Hooke");
}

#[test]
fn projection_keeps_observed_subagent_empty_until_metadata_nickname_resolves() {
    let mut projection = ToolActivityProjection::default();

    projection.apply_stream_event(
        &started("thread_parent", "turn_parent", collab_spawn_item("agent_1")),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("cmd_1")),
        None,
    );

    let parent_rows = projection.rows_for_selected_thread(Some("thread_parent"));
    assert_eq!(parent_rows.len(), 2);
    assert_eq!(parent_rows[0].agent_label, "");
    assert_eq!(parent_rows[0].tool_display_value, "dir");
    assert_eq!(parent_rows[1].agent_label, "Main");
    assert_eq!(parent_rows[1].tool_display_value, "spawnAgent");
    assert_eq!(
        projection.unresolved_subagent_thread_ids(),
        vec!["thread_child".to_string()]
    );

    let mut display_summary = thread_summary("thread_child", None);
    display_summary.name = Some("Research".to_string());
    projection.apply_thread_summary_agent_labels([&display_summary]);

    let child_row = projection
        .rows()
        .iter()
        .find(|row| row.tool_display_value == "dir")
        .expect("child command row should remain visible");
    assert_eq!(child_row.agent_label, "");
    assert_eq!(
        projection.unresolved_subagent_thread_ids(),
        vec!["thread_child".to_string()]
    );

    let nickname_summary = thread_summary("thread_child", Some("Hooke"));
    projection.apply_thread_summary_agent_labels([&nickname_summary]);

    let child_row = projection
        .rows()
        .iter()
        .find(|row| row.tool_display_value == "dir")
        .expect("child command row should remain visible after nickname");
    assert_eq!(child_row.agent_label, "Hooke");
    assert!(projection.unresolved_subagent_thread_ids().is_empty());

    projection.apply_stream_event(
        &started("thread_child", "turn_child_2", mcp_item("mcp_2", "search")),
        None,
    );
    let future_child_row = projection
        .rows()
        .iter()
        .find(|row| row.tool_display_value == "search")
        .expect("future child row should use cached nickname");
    assert_eq!(future_child_row.agent_label, "Hooke");
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
fn projection_uses_subagent_nickname_updates_for_child_thread_rows() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("cmd_1")),
        None,
    );
    assert_eq!(projection.rows()[0].agent_label, "");

    projection.apply_stream_event(&thread_started("thread_child", Some("Hooke")), None);

    let child_row = projection
        .rows()
        .iter()
        .find(|row| row.tool_display_value == "dir")
        .expect("child command row should remain visible");
    assert_eq!(child_row.agent_label, "Hooke");

    projection.apply_stream_event(
        &started("thread_parent", "turn_parent", collab_spawn_item("agent_1")),
        Some("Main".to_string()),
    );

    let child_row = projection
        .rows()
        .iter()
        .find(|row| row.tool_display_value == "dir")
        .expect("child command row should remain visible");
    assert_eq!(child_row.agent_label, "Hooke");

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
        .expect("child command row should remain visible after completion");
    assert_eq!(child_row.agent_label, "Hooke");
    assert_eq!(child_row.status, ToolActivityRowStatus::FinishedOk);

    let parent_row = projection
        .rows()
        .iter()
        .find(|row| row.tool_display_value == "spawnAgent")
        .expect("parent collab row should remain visible");
    assert_eq!(parent_row.agent_label, "Main");
}

#[test]
fn projection_uses_collab_spawn_label_updates_for_child_thread_rows() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("cmd_1")),
        None,
    );
    assert_eq!(projection.rows()[0].agent_label, "");

    projection.apply_stream_event(&agent_label_updated("thread_child", "Gauss"), None);

    let child_row = projection
        .rows()
        .iter()
        .find(|row| row.tool_display_value == "dir")
        .expect("child command row should remain visible");
    assert_eq!(child_row.agent_label, "Gauss");

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
        .expect("child command row should remain visible after completion");
    assert_eq!(child_row.agent_label, "Gauss");
}

#[test]
fn projection_applies_activity_model_metadata_when_nickname_resolves() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started(
            "thread_parent",
            "turn_parent",
            collab_spawn_item_with_runtime_metadata(
                "agent_1",
                &["thread_child"],
                Some("gpt-5.5"),
                None,
            ),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("cmd_1")),
        None,
    );

    let child_row = row_for_activity(&projection, "dir");
    assert_eq!(child_row.agent_label, "");
    let parent_row = row_for_activity(&projection, "spawnAgent");
    assert_eq!(parent_row.agent_label, "Main");

    projection.apply_stream_event(&thread_started("thread_child", Some("Hooke")), None);

    let child_row = row_for_activity(&projection, "dir");
    assert_eq!(child_row.agent_label, "Hooke (gpt-5.5)");
}

#[test]
fn projection_applies_activity_model_and_reasoning_metadata_when_nickname_resolves() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started(
            "thread_parent",
            "turn_parent",
            collab_spawn_item_with_runtime_metadata(
                "agent_1",
                &["thread_child"],
                Some("gpt-5.5"),
                Some("xhigh"),
            ),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("cmd_1")),
        None,
    );

    assert_eq!(row_for_activity(&projection, "dir").agent_label, "");

    projection.apply_stream_event(&thread_started("thread_child", Some("Hooke")), None);

    let child_row = row_for_activity(&projection, "dir");
    assert_eq!(child_row.agent_label, "Hooke (gpt-5.5/xhigh)");
}

#[test]
fn projection_keeps_activity_metadata_without_model_as_nickname_only() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started(
            "thread_parent",
            "turn_parent",
            collab_spawn_item_with_runtime_metadata(
                "agent_1",
                &["thread_child"],
                None,
                Some("xhigh"),
            ),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("cmd_1")),
        None,
    );
    projection.apply_stream_event(&thread_started("thread_child", Some("Hooke")), None);

    let child_row = row_for_activity(&projection, "dir");
    assert_eq!(child_row.agent_label, "Hooke");
}

#[test]
fn projection_applies_activity_metadata_to_multiple_receiver_threads() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started(
            "thread_parent",
            "turn_parent",
            collab_spawn_item_with_runtime_metadata(
                "agent_1",
                &["thread_child_a", "thread_child_b"],
                Some("gpt-5.5"),
                Some("xhigh"),
            ),
        ),
        Some("Main".to_string()),
    );
    projection.apply_stream_event(
        &started(
            "thread_child_a",
            "turn_child_a",
            command_item_with_command("cmd_a", "whoami", "inProgress"),
        ),
        None,
    );
    projection.apply_stream_event(
        &started(
            "thread_child_b",
            "turn_child_b",
            command_item_with_command("cmd_b", "hostname", "inProgress"),
        ),
        None,
    );

    projection.apply_stream_event(&thread_started("thread_child_a", Some("Hooke")), None);
    projection.apply_stream_event(&thread_started("thread_child_b", Some("Noether")), None);

    let parent_rows = projection.rows_for_selected_thread(Some("thread_parent"));
    let child_a = parent_rows
        .iter()
        .find(|row| row.tool_display_value == "whoami")
        .expect("first child row should remain visible under parent");
    let child_b = parent_rows
        .iter()
        .find(|row| row.tool_display_value == "hostname")
        .expect("second child row should remain visible under parent");
    let parent = parent_rows
        .iter()
        .find(|row| row.tool_display_value == "spawnAgent")
        .expect("parent collab row should remain visible");

    assert_eq!(child_a.agent_label, "Hooke (gpt-5.5/xhigh)");
    assert_eq!(child_b.agent_label, "Noether (gpt-5.5/xhigh)");
    assert_eq!(parent.agent_label, "Main");
}

#[test]
fn projection_repairs_child_rows_from_thread_summary_agent_nicknames() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("cmd_1")),
        None,
    );
    assert_eq!(projection.rows()[0].agent_label, "");

    let child_summary = thread_summary("thread_child", Some("Hooke"));
    projection.apply_thread_summary_agent_labels([&child_summary]);

    let child_row = projection
        .rows()
        .iter()
        .find(|row| row.tool_display_value == "dir")
        .expect("child command row should remain visible");
    assert_eq!(child_row.agent_label, "Hooke");

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
        .expect("child command row should remain visible after completion");
    assert_eq!(child_row.agent_label, "Hooke");
}

#[test]
fn projection_formats_subagent_read_metadata_nickname_without_model() {
    let mut projection = observed_child_projection();

    let metadata = thread_read_metadata("thread_child", Some("Hooke"), None, None);
    projection.apply_thread_read_metadata([&metadata]);

    let child_row = row_for_activity(&projection, "dir");
    assert_eq!(child_row.agent_label, "Hooke");
    assert!(projection.subagent_metadata_resolution_targets().is_empty());
}

#[test]
fn projection_requests_runtime_metadata_for_already_named_subagent_once() {
    let mut projection = observed_child_projection();
    projection.apply_stream_event(&thread_started("thread_child", Some("Hooke")), None);

    let targets = projection.subagent_metadata_resolution_targets();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].thread_id, "thread_child");
    assert!(!targets[0].requires_nickname);

    let metadata = thread_read_metadata("thread_child", None, None, None);
    projection.apply_thread_read_metadata([&metadata]);

    assert!(projection.subagent_metadata_resolution_targets().is_empty());
    let child_row = row_for_activity(&projection, "dir");
    assert_eq!(child_row.agent_label, "Hooke");
}

#[test]
fn projection_formats_subagent_read_metadata_with_model() {
    let mut projection = observed_child_projection();

    let metadata = thread_read_metadata("thread_child", Some("Hooke"), Some("gpt-5.5"), None);
    projection.apply_thread_read_metadata([&metadata]);

    let child_row = row_for_activity(&projection, "dir");
    assert_eq!(child_row.agent_label, "Hooke (gpt-5.5)");
}

#[test]
fn projection_formats_subagent_read_metadata_with_model_and_reasoning() {
    let mut projection = observed_child_projection();

    let metadata = thread_read_metadata(
        "thread_child",
        Some("Hooke"),
        Some("gpt-5.5"),
        Some("xhigh"),
    );
    projection.apply_thread_read_metadata([&metadata]);

    let child_row = row_for_activity(&projection, "dir");
    assert_eq!(child_row.agent_label, "Hooke (gpt-5.5/xhigh)");
}

#[test]
fn projection_keeps_subagent_read_metadata_unresolved_when_nickname_is_absent() {
    let mut projection = observed_child_projection();

    let metadata = thread_read_metadata("thread_child", None, Some("gpt-5.5"), Some("xhigh"));
    projection.apply_thread_read_metadata([&metadata]);

    let child_row = row_for_activity(&projection, "dir");
    assert_eq!(child_row.agent_label, "");
    assert_eq!(
        projection.unresolved_subagent_thread_ids(),
        vec!["thread_child".to_string()]
    );
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
fn projection_uses_thread_summary_display_label_for_non_subagent_when_nickname_is_absent() {
    let mut projection = ToolActivityProjection::default();
    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("cmd_1")),
        None,
    );

    let mut child_summary = thread_summary("thread_child", None);
    child_summary.name = Some("Research".to_string());
    projection.apply_thread_summary_agent_labels([&child_summary]);

    let child_row = projection
        .rows()
        .iter()
        .find(|row| row.tool_display_value == "dir")
        .expect("child command row should remain visible");
    assert_eq!(child_row.agent_label, "Research");

    let child_summary = thread_summary("thread_child", Some("Hooke"));
    projection.apply_thread_summary_agent_labels([&child_summary]);
    let child_row = projection
        .rows()
        .iter()
        .find(|row| row.tool_display_value == "dir")
        .expect("child command row should remain visible after nickname");
    assert_eq!(child_row.agent_label, "Hooke");

    let mut later_display_summary = thread_summary("thread_child", None);
    later_display_summary.name = Some("Renamed Thread".to_string());
    projection.apply_thread_summary_agent_labels([&later_display_summary]);
    let child_row = projection
        .rows()
        .iter()
        .find(|row| row.tool_display_value == "dir")
        .expect("child command row should remain visible after display update");
    assert_eq!(child_row.agent_label, "Hooke");
}

#[test]
fn projection_keeps_thread_metadata_nickname_above_later_activity_labels() {
    let mut projection = ToolActivityProjection::default();

    projection.apply_stream_event(
        &started("thread_child", "turn_child", command_item("cmd_1")),
        None,
    );
    let child_summary = thread_summary("thread_child", Some("Hooke"));
    projection.apply_thread_summary_agent_labels([&child_summary]);

    projection.apply_stream_event(
        &started(
            "thread_parent",
            "turn_parent",
            collab_spawn_item_with_agent_label("agent_1", "thread_child", "thread:thread_child"),
        ),
        Some("Main".to_string()),
    );

    let child_row = projection
        .rows()
        .iter()
        .find(|row| row.tool_display_value == "dir")
        .expect("child command row should remain visible");
    assert_eq!(child_row.agent_label, "Hooke");
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

fn thread_started(thread_id: &str, agent_nickname: Option<&str>) -> TurnStreamEvent {
    TurnStreamEvent::ThreadStarted {
        thread: thread_summary(thread_id, agent_nickname),
    }
}

fn agent_label_updated(thread_id: &str, label: &str) -> TurnStreamEvent {
    TurnStreamEvent::AgentLabelUpdated {
        thread_id: thread_id.to_string(),
        label: label.to_string(),
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

fn collab_spawn_item_with_agent_label(
    item_id: &str,
    receiver_thread_id: &str,
    label: &str,
) -> ThreadItem {
    serde_json::from_value(json!({
        "id": item_id,
        "type": "collabAgentToolCall",
        "agentsStates": {
            receiver_thread_id: {"agentNickname": label}
        },
        "receiverThreadIds": [receiver_thread_id],
        "senderThreadId": "thread_parent",
        "status": "inProgress",
        "tool": "spawnAgent"
    }))
    .unwrap()
}
