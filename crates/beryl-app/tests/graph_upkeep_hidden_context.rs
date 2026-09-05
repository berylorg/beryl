pub use beryl_app::WorkspaceGraphUpkeepPolicy;

#[allow(dead_code)]
#[path = "../src/graph_upkeep_context.rs"]
mod graph_upkeep_context;
#[allow(dead_code)]
#[path = "../src/shell/status_line.rs"]
mod status_line;

use beryl_backend::TurnStartOptions;
use graph_upkeep_context::compose_hidden_developer_instructions;

#[test]
fn global_developer_instructions_remain_literal_when_graph_upkeep_is_disabled() {
    let global = "Use the operator's global rules.\nKeep this exact.";

    let composed = compose_hidden_developer_instructions(None, Some(global.to_string()));

    assert_eq!(composed.as_deref(), Some(global));
}

#[test]
fn graph_upkeep_context_precedes_global_developer_instructions() {
    let policy = WorkspaceGraphUpkeepPolicy::with_instructions(Some(
        "Prefer stable feature nodes.\nKeep summaries conservative.".to_string(),
    ));
    let global = "Use the operator's global rules.";

    let composed = compose_hidden_developer_instructions(Some(&policy), Some(global.to_string()))
        .expect("hidden context should be composed");

    assert!(composed.starts_with("Beryl graph upkeep guidance:"));
    let workspace_header = composed
        .find("Workspace graph-upkeep instructions:")
        .expect("workspace graph-upkeep header should be present");
    let workspace_policy = composed
        .find("Prefer stable feature nodes.")
        .expect("workspace graph-upkeep policy should be present");
    let global_index = composed
        .find(global)
        .expect("global developer instructions should be present");
    assert!(workspace_header < workspace_policy);
    assert!(workspace_policy < global_index);
    assert!(composed.ends_with(global));
}

#[test]
fn disabled_hidden_sections_omit_the_turn_scoped_field() {
    let composed = compose_hidden_developer_instructions(None, None);
    assert_eq!(composed, None);

    let options = status_line::turn_start_options_with_developer_instructions_context(
        TurnStartOptions::default(),
        composed,
    );

    assert_eq!(options.developer_instructions(), None);
}

#[test]
fn hidden_context_attaches_without_effective_model() {
    let policy =
        WorkspaceGraphUpkeepPolicy::with_instructions(Some("Track the active plan.".to_string()));
    let composed = compose_hidden_developer_instructions(Some(&policy), Some("Global".to_string()));
    let stale_options =
        TurnStartOptions::default().with_developer_instructions(Some("Old setting".to_string()));

    let options = status_line::turn_start_options_with_developer_instructions_context(
        stale_options,
        composed,
    );

    assert!(
        options
            .developer_instructions()
            .is_some_and(|value| value.contains("Track the active plan."))
    );
}

#[test]
fn graph_upkeep_policy_is_late_bound_for_later_request_assembly() {
    let queued_options = TurnStartOptions::default();
    let first_policy =
        WorkspaceGraphUpkeepPolicy::with_instructions(Some("Track Phase 1.".to_string()));
    let second_policy =
        WorkspaceGraphUpkeepPolicy::with_instructions(Some("Track Phase 2.".to_string()));

    let first_context =
        compose_hidden_developer_instructions(Some(&first_policy), Some("Global".to_string()))
            .expect("first context should be present");
    let second_context =
        compose_hidden_developer_instructions(Some(&second_policy), Some("Global".to_string()))
            .expect("second context should be present");

    assert_eq!(queued_options.developer_instructions(), None);
    assert_ne!(first_context, second_context);
    assert!(first_context.contains("Track Phase 1."));
    assert!(second_context.contains("Track Phase 2."));
}
