use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
fn app_sources_exclude_removed_materialized_backend_graph() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let forbidden = [
        "parse_turn_stream_event",
        "TurnStreamEvent",
        "ThreadSummary",
        "ThreadItem",
        "AgentMessageItem",
        "ItemDeltaPayload",
        "UserMessageItem",
        "beryl_backend::UserInput",
        "TurnStreamEnvelope",
        "next_turn_stream_event",
        "ToolActivityEvent",
        "apply_tool_activity",
    ];
    let mut offenders = Vec::new();

    for path in rust_files_under(&src_dir) {
        let source = fs::read_to_string(&path).expect("app source should be readable");
        for removed in forbidden {
            if source.contains(removed) {
                let relative = path.strip_prefix(&src_dir).unwrap_or(&path);
                offenders.push(format!("{} contains {removed}", relative.display()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "removed materialized backend graph remains in app source: {offenders:?}"
    );
}

#[test]
fn detached_adapter_files_stay_removed_and_compact_inputs_remain() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for relative in [
        "shell/context_compaction.rs",
        "shell/thread_title/generated.rs",
        "shell/composer_image_label_frontier.rs",
        "shell/turn_steering.rs",
        "shell/turn_input.rs",
        "shell/transcript_images.rs",
        "shell/transcript_images/generated_labels.rs",
        "shell/lifecycle_continuation.rs",
        "shell/hard_stop.rs",
        "shell/hard_stop_targets.rs",
    ] {
        assert!(
            !src_dir.join(relative).exists(),
            "detached whole-value adapter returned at {relative}"
        );
    }

    let activity = fs::read_to_string(src_dir.join("shell/tool_activity.rs"))
        .expect("compact activity projection source should remain");
    assert!(!activity.contains("ToolActivityEvent"));
    assert!(!activity.contains("apply_tool_activity"));
    assert!(activity.contains("pub(super) fn apply_protocol_error("));
    assert!(activity.contains("pub(super) fn apply_thread_read_metadata"));
    assert!(activity.contains("pub(super) fn finish_running_for_turn("));
    assert!(activity.contains("pub(super) fn clear_all("));
    assert!(!activity.contains("apply_stream_event"));
}

#[test]
fn shell_sources_exclude_removed_hard_stop_caller_state_and_render_seams() {
    let shell_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/shell");
    let forbidden = [
        "hard_stop::",
        "mod hard_stop",
        "HardStopTarget",
        "SelectedTurnHardStopTargets",
        "HardStopRequest",
        "HardStopHold",
        "spawn_hard_stop_worker",
        "hard_stop_receiver",
        "hard_stop_targets",
    ];
    let mut offenders = Vec::new();

    for path in rust_files_under(&shell_dir) {
        let source = fs::read_to_string(&path).expect("shell source should be readable");
        for removed in forbidden {
            if source.contains(removed) {
                let relative = path.strip_prefix(&shell_dir).unwrap_or(&path);
                offenders.push(format!("{} contains {removed}", relative.display()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "removed shell-owned hard-stop seams remain in app source: {offenders:?}"
    );
}

fn rust_files_under(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();

    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path).expect("source directory should be readable") {
            let entry = entry.expect("source directory entry should be readable");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    files
}
