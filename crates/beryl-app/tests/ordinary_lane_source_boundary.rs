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
fn production_sources_exclude_removed_hard_stop_and_coarse_cleanup_surfaces() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for relative in [
        "cas_projection/stop/hard.rs",
        "shell/hard_stop.rs",
        "shell/hard_stop_targets.rs",
    ] {
        assert!(
            !src_dir.join(relative).exists(),
            "removed hard-stop source returned at {relative}"
        );
    }

    let forbidden = [
        "hard_stop::",
        "mod hard_stop",
        "mod hard;",
        "hard::HardStop",
        "HardStopRunOwner",
        "HardStopAttachment",
        "HardStopAdmission",
        "HardStopCoordinatorState",
        "HardStopActivityState",
        "HardStopSlot",
        "HardStopElection",
        "HardStopLimitation",
        "HardStopTargetKind",
        "HardStopTargetDisposition",
        "HardStopTargetResult",
        "BoundedHardStopResult",
        "StopDispatchSettlement::HardStop",
        "HardStopRunningProvenNondispatch",
        "attach_hard_stop",
        "begin_hard_run",
        "settle_primary_and_begin_hard_run",
        "begin_racing_proven_nondispatch_hard_run",
        "dispatch_exact_hard_stop",
        "dispatch_hard_stop_owner",
        "finish_authorized_hard_stop",
        "finish_unavailable_hard_stop",
        "finish_hard_without_run",
        "consume_hard_slot",
        "wait_for_finalization_release",
        "hard_activity",
        "hard_wake",
        "PublishedHardStopActivity",
        "PublishedHardStopActivityEffect",
        "BoundPublishedHardStopActivity",
        "hard_stop_activity_transition",
        "record_published_activity",
        "clear_published_activity",
        "PersistentFailureStopEvidence",
        "persistent_failure_evidence",
        "permits_volatile_interrupt",
        "ExactHardStopLimitation",
        "CoarseThreadCleanup",
        "admits_exact_thread_background_terminals_cleanup",
        "clean_exact_thread_background_terminals",
        "HardStopTarget",
        "SelectedTurnHardStopTargets",
        "HardStopRequest",
        "HardStopHold",
        "spawn_hard_stop_worker",
        "hard_stop_receiver",
        "hard_stop_targets",
    ];
    let mut offenders = Vec::new();

    for path in rust_files_under(&src_dir) {
        let source = fs::read_to_string(&path).expect("production source should be readable");
        for removed in forbidden {
            if source.contains(removed) {
                let relative = path.strip_prefix(&src_dir).unwrap_or(&path);
                offenders.push(format!("{} contains {removed}", relative.display()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "removed hard-stop or coarse-cleanup surfaces remain in app production source: {offenders:?}"
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
