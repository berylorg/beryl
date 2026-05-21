pub use beryl_app::WorkspaceGraphUpkeepPolicy;

#[allow(dead_code)]
#[path = "../src/graph_upkeep_context.rs"]
mod graph_upkeep_context;
#[allow(dead_code)]
#[path = "../src/shell/status_line.rs"]
mod status_line;

use beryl_backend::TurnStartOptions;
use graph_upkeep_context::{
    compose_hidden_developer_instructions, compose_hidden_developer_instructions_with_contexts,
};
use status_line::ThreadTurnDefaults;

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
fn additional_hidden_contexts_are_between_graph_upkeep_and_global_instructions() {
    let policy =
        WorkspaceGraphUpkeepPolicy::with_instructions(Some("Track decisions.".to_string()));
    let decision_context = "Beryl threaded-decision branch context:\n- Decision checklist item: Pick parser (pick_parser)";
    let global = "Use the operator's global rules.";

    let composed = compose_hidden_developer_instructions_with_contexts(
        Some(&policy),
        [decision_context.to_string()],
        Some(global.to_string()),
    )
    .expect("hidden context should be composed");

    assert_order(&composed, "Track decisions.", decision_context);
    assert_order(&composed, decision_context, global);
}

#[test]
fn disabled_hidden_sections_keep_backend_reset_when_model_is_known() {
    let composed = compose_hidden_developer_instructions(None, None);
    assert_eq!(composed, None);

    let options = status_line::turn_start_options_with_developer_instructions_context(
        TurnStartOptions::default(),
        composed,
        ThreadTurnDefaults::new(Some("gpt-5.5".to_string()), None),
    );

    let context = options
        .developer_instructions_context()
        .expect("known model should keep the hidden reset context");
    assert_eq!(context.developer_instructions(), None);
    assert_eq!(context.model(), "gpt-5.5");
}

#[test]
fn hidden_context_is_omitted_without_effective_model() {
    let policy =
        WorkspaceGraphUpkeepPolicy::with_instructions(Some("Track the active plan.".to_string()));
    let composed = compose_hidden_developer_instructions(Some(&policy), Some("Global".to_string()));
    let stale_options = TurnStartOptions::default().with_developer_instructions_context(
        Some("Old setting".to_string()),
        "gpt-5.4",
        None,
    );

    let options = status_line::turn_start_options_with_developer_instructions_context(
        stale_options,
        composed,
        ThreadTurnDefaults::new(None, Some("high".to_string())),
    );

    assert!(options.developer_instructions_context().is_none());
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

    assert!(queued_options.developer_instructions_context().is_none());
    assert_ne!(first_context, second_context);
    assert!(first_context.contains("Track Phase 1."));
    assert!(second_context.contains("Track Phase 2."));
}

fn assert_order(haystack: &str, before: &str, after: &str) {
    let before_index = haystack.find(before).expect("before text should exist");
    let after_index = haystack.find(after).expect("after text should exist");
    assert!(before_index < after_index);
}
