use std::time::Duration;

use beryl_app::compaction_diagnostics::{
    COMPACTION_DIAGNOSTIC_CAPACITY, COMPACTION_DIAGNOSTIC_IDENTITY_FIELD_BYTE_LIMIT,
    CompactionDiagnosticCategory, CompactionDiagnosticEventData, CompactionDiagnosticOutcome,
    CompactionDiagnosticStage, CompactionDiagnosticStart, CompactionDiagnostics,
};
use beryl_app::{READ_COMPACTION_DIAGNOSTICS_TOOL, beryl_diagnostic_dynamic_tool_specs};
fn start<'a>(generation: u64, workspace: Option<&'a str>) -> CompactionDiagnosticStart<'a> {
    CompactionDiagnosticStart {
        local_generation: generation,
        workspace_identity: workspace,
        runtime_alias: Some("runtime-1"),
        thread_identity: Some("thread-1"),
        warning_threshold: Duration::from_secs(180),
    }
}

fn data<'a>(turn: Option<&'a str>, elapsed_seconds: u64) -> CompactionDiagnosticEventData<'a> {
    CompactionDiagnosticEventData {
        turn_identity: turn,
        elapsed: Duration::from_secs(elapsed_seconds),
        last_event_age: Some(Duration::from_secs(1)),
    }
}

#[test]
fn collector_retains_bound_identity_and_content_free_closed_categories() {
    let diagnostics = CompactionDiagnostics::default();
    let handle = diagnostics.begin(start(7, Some("workspace-opaque")));
    handle.bind_operation(Some("operation-uuid"), Some("observation-session-uuid"));
    handle.bind_turn(Some("turn-uuid"));
    handle.record(
        CompactionDiagnosticStage::Lifecycle,
        CompactionDiagnosticCategory::ItemCompleted,
        data(Some("turn-uuid"), 4),
    );

    let value = serde_json::to_value(diagnostics.snapshot()).unwrap();
    assert_eq!(value["summary"]["localGeneration"], 7);
    assert_eq!(value["summary"]["operation"]["value"], "operation-uuid");
    assert_eq!(value["events"][1]["stage"], "lifecycle");
    assert_eq!(value["events"][1]["category"], "item_completed");
    assert_eq!(value["events"][1]["turn"]["value"], "turn-uuid");
    assert!(!serde_json::to_string(&value).unwrap().contains("C:\\"));
}

#[test]
fn collector_bounds_events_and_marks_invalid_identities_without_substitution() {
    let diagnostics = CompactionDiagnostics::default();
    let over_bound = "x".repeat(COMPACTION_DIAGNOSTIC_IDENTITY_FIELD_BYTE_LIMIT + 1);
    let handle = diagnostics.begin(start(1, Some(&over_bound)));
    for index in 0..(COMPACTION_DIAGNOSTIC_CAPACITY + 3) {
        handle.record(
            CompactionDiagnosticStage::Warning,
            CompactionDiagnosticCategory::ThresholdReached,
            data(Some("  "), index as u64),
        );
    }

    let value = serde_json::to_value(diagnostics.snapshot()).unwrap();
    assert_eq!(value["retainedCount"], COMPACTION_DIAGNOSTIC_CAPACITY);
    assert!(value["omissions"]["evictedEventCount"].as_u64().unwrap() > 0);
    assert_eq!(value["summary"]["workspace"]["validity"], "over_bound");
    assert!(value["summary"]["workspace"].get("value").is_none());
    assert!(
        value["omissions"]["blankIdentityFieldCount"]
            .as_u64()
            .unwrap()
            > 0
    );
}

#[test]
fn older_cleanup_cannot_replace_newer_operation_summary() {
    let diagnostics = CompactionDiagnostics::default();
    let older = diagnostics.begin(start(1, Some("old")));
    let newer = diagnostics.begin(start(2, Some("new")));
    older.finish(CompactionDiagnosticOutcome::Cancelled, data(None, 9));
    newer.set_outcome(CompactionDiagnosticOutcome::Pending);

    let value = serde_json::to_value(diagnostics.snapshot()).unwrap();
    assert_eq!(value["summary"]["localGeneration"], 2);
    assert_eq!(value["summary"]["workspace"]["value"], "new");
    assert!(
        value["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["localGeneration"] == 1 && event["stage"] == "cleanup")
    );
}

#[test]
fn newer_live_summary_resets_activity_age_and_ignores_late_older_cleanup() {
    let diagnostics = CompactionDiagnostics::default();
    let older = diagnostics.begin(start(1, Some("old")));
    older.record(
        CompactionDiagnosticStage::Lifecycle,
        CompactionDiagnosticCategory::ItemCompleted,
        data(Some("turn-old"), 4),
    );
    let newer = diagnostics.begin(start(2, Some("new")));
    newer.set_outcome(CompactionDiagnosticOutcome::Unconfirmed);
    older.finish(CompactionDiagnosticOutcome::Cancelled, data(None, 9));

    let value = serde_json::to_value(diagnostics.snapshot()).unwrap();
    assert_eq!(value["summary"]["localGeneration"], 2);
    assert_eq!(value["summary"]["outcome"], "unconfirmed");
    assert!(value["summary"]["elapsedMicros"].as_u64().is_some());
    assert!(value["summary"].get("lastEventAgeMicros").is_none());
}

#[test]
fn aggregate_identity_budget_evicts_events_before_the_event_count_cap() {
    let diagnostics = CompactionDiagnostics::default();
    let identity = "x".repeat(COMPACTION_DIAGNOSTIC_IDENTITY_FIELD_BYTE_LIMIT);
    let handle = diagnostics.begin(CompactionDiagnosticStart {
        local_generation: 3,
        workspace_identity: Some(&identity),
        runtime_alias: Some(&identity),
        thread_identity: Some(&identity),
        warning_threshold: Duration::from_secs(180),
    });
    handle.bind_operation(Some(&identity), Some(&identity));
    handle.bind_turn(Some(&identity));
    for index in 0..COMPACTION_DIAGNOSTIC_CAPACITY {
        handle.record(
            CompactionDiagnosticStage::Lifecycle,
            CompactionDiagnosticCategory::ItemCompleted,
            data(Some(&identity), index as u64),
        );
    }

    let value = serde_json::to_value(diagnostics.snapshot()).unwrap();
    assert!(value["retainedCount"].as_u64().unwrap() < COMPACTION_DIAGNOSTIC_CAPACITY as u64);
    assert!(value["retainedIdentityBytes"].as_u64().unwrap() <= 128 * 1024);
    assert!(value["omissions"]["evictedEventCount"].as_u64().unwrap() > 0);
}

#[test]
fn read_tool_remains_bounded_and_read_only() {
    let spec = beryl_diagnostic_dynamic_tool_specs()
        .into_iter()
        .find(|spec| spec.name == READ_COMPACTION_DIAGNOSTICS_TOOL)
        .expect("compaction diagnostics tool must be registered");
    assert_eq!(spec.input_schema["additionalProperties"], false);
    assert_eq!(spec.input_schema["properties"]["limit"]["default"], 64);
    assert_eq!(
        spec.input_schema["properties"]["afterSequence"]["minimum"],
        0
    );
}
