#[allow(dead_code)]
#[path = "../src/shell/status_line.rs"]
mod status_line;

#[allow(dead_code)]
#[allow(unused_imports)]
#[path = "../src/shell/execution_detail.rs"]
mod execution_detail;

#[allow(dead_code)]
#[path = "../src/shell/pending_turn_input.rs"]
mod pending_turn_input;

#[allow(dead_code)]
#[path = "../src/shell/status_operation_state.rs"]
mod status_operation_state;

use std::time::{Duration, Instant};

use beryl_backend::{
    BackendConfigDefaults, HardStopTarget, HardStopTargetOutcome, ModelInfo, TurnStartOptions,
};
use beryl_model::workspace::WorkspaceId;
use gpui::{Bounds, point, px, size};
use pending_turn_input::PendingTurnInputQueue;
use status_line::{CancellableActiveTurn, SelectedTurnHardStopTargets};
use status_operation_state::{
    HARD_STOP_HOLD_DURATION, HardStopHoldSource, StatusLineOperationKind, StatusLineOperationState,
    StatusModelListCache, reasoning_effort_for_model_selection,
};

#[test]
fn popup_state_tracks_kind_position_and_outside_dismissal() {
    let mut state = StatusLineOperationState::default();

    assert!(!state.is_open());
    state.open(
        StatusLineOperationKind::ModelReasoning,
        point(px(120.0), px(420.0)),
    );

    let open = state.active().unwrap();
    assert_eq!(open.kind(), StatusLineOperationKind::ModelReasoning);
    assert_eq!(open.position(), point(px(120.0), px(420.0)));
    assert!(state.should_dismiss_for_mouse_down(point(px(120.0), px(420.0))));

    state.set_bounds(Some(Bounds::new(
        point(px(100.0), px(380.0)),
        size(px(340.0), px(220.0)),
    )));
    assert!(!state.should_dismiss_for_mouse_down(point(px(140.0), px(420.0))));
    assert!(state.should_dismiss_for_mouse_down(point(px(40.0), px(420.0))));

    state.close();
    assert!(!state.is_open());
    assert!(!state.should_dismiss_for_mouse_down(point(px(40.0), px(420.0))));
}

#[test]
fn turn_stop_request_state_suppresses_duplicate_in_flight_requests() {
    let mut state = StatusLineOperationState::default();
    let target = CancellableActiveTurn::ordinary("thread_1", "turn_1");

    state.open(
        StatusLineOperationKind::TurnOperations,
        point(px(80.0), px(520.0)),
    );
    assert_eq!(
        state.active().unwrap().kind(),
        StatusLineOperationKind::TurnOperations
    );
    assert!(!state.turn_stop_request_in_flight());

    assert!(state.begin_turn_stop_request(target.clone()));
    assert!(state.turn_stop_request_in_flight());
    assert_eq!(state.turn_stop_request_target(), Some(&target));
    assert_eq!(state.turn_stop_request_error(Some(&target)), None);
    assert!(!state.begin_turn_stop_request(CancellableActiveTurn::ordinary("thread_1", "turn_2")));

    assert_eq!(state.finish_turn_stop_request(), Some(target));
    assert!(!state.turn_stop_request_in_flight());
}

#[test]
fn hard_stop_request_state_tracks_targets_failures_and_duplicate_suppression() {
    let mut state = StatusLineOperationState::default();
    let selected_turn = CancellableActiveTurn::ordinary("thread_1", "turn_1");
    let selected_targets = SelectedTurnHardStopTargets::new(
        selected_turn.clone(),
        vec![
            HardStopTarget::turn("thread_1", "turn_1"),
            HardStopTarget::command_execution("proc_1"),
        ],
        Vec::new(),
    );

    assert!(state.begin_hard_stop_request(selected_targets));
    assert!(state.hard_stop_request_in_flight());
    assert_eq!(
        state.hard_stop_request_target().unwrap().selected_turn,
        selected_turn
    );
    assert!(!state.begin_turn_stop_request(CancellableActiveTurn::ordinary("thread_1", "turn_1")));

    let summary = state
        .finish_hard_stop_request(vec![
            HardStopTargetOutcome::Succeeded {
                target: HardStopTarget::turn("thread_1", "turn_1"),
            },
            HardStopTargetOutcome::Failed {
                target: HardStopTarget::command_execution("proc_1"),
                method: "command/exec/terminate",
                message: "already exited".to_string(),
            },
        ])
        .expect("hard-stop request should produce a summary");

    assert!(!state.hard_stop_request_in_flight());
    assert_eq!(summary.target_count, 2);
    assert_eq!(summary.succeeded_count, 1);
    assert_eq!(summary.failures.len(), 1);
    assert_eq!(summary.request_error, None);
    assert_eq!(
        state.hard_stop_request_summary().unwrap().failures[0].method,
        "command/exec/terminate"
    );

    assert!(state.finish_turn_stop_request_for_target("thread_1", "turn_1"));
    assert!(state.hard_stop_request_summary().is_none());
}

#[test]
fn stop_request_state_suppresses_soft_and_hard_duplicates_in_both_directions() {
    let mut state = StatusLineOperationState::default();
    let selected_turn = CancellableActiveTurn::ordinary("thread_1", "turn_1");
    let selected_targets = SelectedTurnHardStopTargets::new(
        selected_turn.clone(),
        vec![HardStopTarget::turn("thread_1", "turn_1")],
        Vec::new(),
    );

    assert!(state.begin_turn_stop_request(selected_turn.clone()));
    assert!(!state.begin_turn_stop_request(selected_turn.clone()));
    assert!(!state.begin_hard_stop_request(selected_targets.clone()));
    assert_eq!(
        state.finish_turn_stop_request(),
        Some(selected_turn.clone())
    );

    assert!(state.begin_hard_stop_request(selected_targets));
    assert!(!state.begin_turn_stop_request(selected_turn));
    assert!(
        !state.begin_hard_stop_request(SelectedTurnHardStopTargets::new(
            CancellableActiveTurn::ordinary("thread_1", "turn_1"),
            vec![HardStopTarget::turn("thread_1", "turn_1")],
            Vec::new(),
        ))
    );
}

#[test]
fn stop_request_state_transitions_preserve_accepted_pending_queue_fragments() {
    let mut state = StatusLineOperationState::default();
    let mut queue = PendingTurnInputQueue::new(
        "thread_1".to_string(),
        WorkspaceId::host_windows("C:\\work\\beryl"),
        true,
        TurnStartOptions::default().with_model("gpt-5.5"),
        7,
        execution_detail::UserInputFragment::text("First queued fragment"),
    );
    queue.append(execution_detail::UserInputFragment::text(
        "Second queued fragment",
    ));
    let expected = queue.clone().into_fragments();
    let selected_turn = CancellableActiveTurn::ordinary("thread_1", "turn_1");

    assert!(state.begin_turn_stop_request(selected_turn.clone()));
    state.fail_turn_stop_request(selected_turn.clone(), "interrupt failed".to_string());
    assert_eq!(queue.clone().into_fragments(), expected);

    let selected_targets = SelectedTurnHardStopTargets::new(
        selected_turn,
        vec![
            HardStopTarget::turn("thread_1", "turn_1"),
            HardStopTarget::command_execution("proc_1"),
        ],
        Vec::new(),
    );
    assert!(state.begin_hard_stop_request(selected_targets));
    state.finish_hard_stop_request(vec![
        HardStopTargetOutcome::Succeeded {
            target: HardStopTarget::turn("thread_1", "turn_1"),
        },
        HardStopTargetOutcome::Succeeded {
            target: HardStopTarget::command_execution("proc_1"),
        },
    ]);

    assert_eq!(queue.into_fragments(), expected);
}

#[test]
fn backend_exit_clears_in_flight_stop_state() {
    let mut state = StatusLineOperationState::default();
    let selected_turn = CancellableActiveTurn::ordinary("thread_1", "turn_1");
    let selected_targets = SelectedTurnHardStopTargets::new(
        selected_turn.clone(),
        vec![HardStopTarget::turn("thread_1", "turn_1")],
        Vec::new(),
    );

    state.open(
        StatusLineOperationKind::TurnOperations,
        point(px(80.0), px(520.0)),
    );
    assert!(state.begin_hard_stop_request(selected_targets));
    assert!(state.clear_stop_requests_for_backend_exit());

    assert!(!state.stop_request_in_flight());
    assert!(!state.hard_stop_hold_active());
    assert!(!state.clear_stop_requests_for_backend_exit());
}

#[test]
fn hard_stop_hold_tracks_progress_and_cancels_early_release() {
    let mut state = StatusLineOperationState::default();
    let target = CancellableActiveTurn::ordinary("thread_1", "turn_1");
    let now = Instant::now();

    state.open(
        StatusLineOperationKind::TurnOperations,
        point(px(80.0), px(520.0)),
    );
    assert!(state.begin_hard_stop_hold(target.clone(), HardStopHoldSource::Pointer, now,));
    assert!(
        state
            .hard_stop_hold_progress_for_target(&target, now)
            .is_some()
    );

    let progress = state
        .hard_stop_hold_progress_for_target(&target, now + Duration::from_millis(1500))
        .unwrap();
    assert!((0.45..0.55).contains(&progress));

    assert!(state.cancel_hard_stop_hold_source(HardStopHoldSource::Pointer));
    assert!(!state.hard_stop_hold_active());
}

#[test]
fn hard_stop_hold_completes_once_after_required_duration() {
    let mut state = StatusLineOperationState::default();
    let target = CancellableActiveTurn::ordinary("thread_1", "turn_1");
    let now = Instant::now();

    state.open(
        StatusLineOperationKind::TurnOperations,
        point(px(80.0), px(520.0)),
    );
    assert!(state.begin_hard_stop_hold(target.clone(), HardStopHoldSource::Keyboard, now,));
    assert_eq!(
        state.complete_hard_stop_hold_if_ready(now + HARD_STOP_HOLD_DURATION),
        Some(target)
    );
    assert_eq!(
        state.complete_hard_stop_hold_if_ready(now + HARD_STOP_HOLD_DURATION),
        None
    );
}

#[test]
fn hard_stop_hold_cancels_when_active_target_changes_or_popup_closes() {
    let mut state = StatusLineOperationState::default();
    let target = CancellableActiveTurn::ordinary("thread_1", "turn_1");
    let other_target = CancellableActiveTurn::ordinary("thread_1", "turn_2");
    let now = Instant::now();

    state.open(
        StatusLineOperationKind::TurnOperations,
        point(px(80.0), px(520.0)),
    );
    assert!(state.begin_hard_stop_hold(target.clone(), HardStopHoldSource::Pointer, now,));
    assert!(state.cancel_hard_stop_hold_for_target_change(Some(&other_target)));
    assert!(!state.hard_stop_hold_active());

    assert!(state.begin_hard_stop_hold(target, HardStopHoldSource::Pointer, now));
    state.close();
    assert!(!state.hard_stop_hold_active());
}

#[test]
fn turn_stop_request_state_tracks_failures_by_target_and_clears_on_completion() {
    let mut state = StatusLineOperationState::default();
    let target = CancellableActiveTurn::ordinary("thread_1", "turn_1");
    let other_target = CancellableActiveTurn::ordinary("thread_1", "turn_2");

    assert!(state.begin_turn_stop_request(target.clone()));
    assert_eq!(
        state.fail_turn_stop_request(target.clone(), "interrupt failed".to_string()),
        Some(target.clone())
    );
    assert!(!state.turn_stop_request_in_flight());
    assert_eq!(
        state.turn_stop_request_error(Some(&target)),
        Some("interrupt failed")
    );
    assert_eq!(state.turn_stop_request_error(Some(&other_target)), None);

    assert!(state.finish_turn_stop_request_for_target("thread_1", "turn_1"));
    assert_eq!(state.turn_stop_request_error(Some(&target)), None);
}

#[test]
fn model_list_cache_tracks_load_success_failure_and_lookup_aliases() {
    let mut cache = StatusModelListCache::default();

    assert!(cache.should_load());
    cache.begin_loading();
    assert!(cache.loading());
    assert!(!cache.should_load());

    cache.finish_failed("model/list failed".to_string());
    assert!(!cache.loading());
    assert_eq!(cache.last_error(), Some("model/list failed"));
    assert!(cache.should_load());

    cache.begin_loading();
    cache.finish_loaded(vec![model("gpt-5.5-id", "gpt-5.5", "GPT-5.5")]);
    assert!(!cache.loading());
    assert_eq!(cache.last_error(), None);
    assert!(!cache.should_load());
    assert!(cache.find_model("gpt-5.5-id").is_some());
    assert!(cache.find_model("gpt-5.5").is_some());
    assert!(cache.find_model("GPT-5.5").is_some());
    assert!(cache.find_model("gpt-4.9").is_none());
}

#[test]
fn model_selection_keeps_supported_reasoning_or_uses_model_default() {
    let mut model = model("gpt-5.5-id", "gpt-5.5", "GPT-5.5");
    model.supported_reasoning_efforts =
        vec!["low".to_string(), "medium".to_string(), "high".to_string()];
    model.default_reasoning_effort = Some("medium".to_string());

    assert_eq!(
        reasoning_effort_for_model_selection(&model, Some("high")).as_deref(),
        Some("high")
    );
    assert_eq!(
        reasoning_effort_for_model_selection(&model, Some("xhigh")).as_deref(),
        Some("medium")
    );

    model.default_reasoning_effort = Some("xhigh".to_string());
    assert_eq!(
        reasoning_effort_for_model_selection(&model, Some("xhigh")).as_deref(),
        Some("low")
    );

    model.supported_reasoning_efforts.clear();
    assert_eq!(reasoning_effort_for_model_selection(&model, None), None);
}

#[test]
fn model_list_cache_uses_config_defaults_for_effective_new_thread_reasoning() {
    let mut hidden_default = model("hidden-id", "gpt-hidden", "Hidden");
    hidden_default.hidden = true;
    hidden_default.is_default = true;
    hidden_default.supported_reasoning_efforts =
        vec!["low".to_string(), "medium".to_string(), "high".to_string()];
    hidden_default.default_reasoning_effort = Some("medium".to_string());

    let mut visible = model("visible-id", "gpt-visible", "Visible");
    visible.supported_reasoning_efforts = vec!["low".to_string()];
    visible.default_reasoning_effort = Some("low".to_string());

    let mut cache = StatusModelListCache::default();
    cache.finish_loaded_with_config(
        vec![visible, hidden_default],
        BackendConfigDefaults {
            model: Some("gpt-5.5".to_string()),
            model_reasoning_effort: Some("xhigh".to_string()),
        },
    );

    let defaults = cache.effective_default_turn_defaults().unwrap();
    assert_eq!(defaults.model(), Some("gpt-5.5"));
    assert_eq!(defaults.reasoning_effort(), Some("xhigh"));
}

#[test]
fn model_list_cache_uses_model_list_only_for_model_fallback() {
    let mut hidden = model("hidden-id", "gpt-hidden", "Hidden");
    hidden.hidden = true;

    let mut visible = model("visible-id", "gpt-visible", "Visible");
    visible.supported_reasoning_efforts = vec!["high".to_string()];
    visible.default_reasoning_effort = Some("high".to_string());

    let mut cache = StatusModelListCache::default();
    cache.finish_loaded(vec![hidden, visible]);

    let defaults = cache.effective_default_turn_defaults().unwrap();
    assert_eq!(defaults.model(), Some("gpt-visible"));
    assert_eq!(defaults.reasoning_effort(), None);
}

#[test]
fn model_list_cache_keeps_reasoning_unknown_when_config_reasoning_is_absent() {
    let mut default_model = model("default-id", "gpt-default", "Default");
    default_model.is_default = true;
    default_model.supported_reasoning_efforts = vec!["medium".to_string(), "high".to_string()];
    default_model.default_reasoning_effort = Some("medium".to_string());

    let mut cache = StatusModelListCache::default();
    cache.finish_loaded_with_config(
        vec![default_model],
        BackendConfigDefaults {
            model: Some("gpt-5.5".to_string()),
            model_reasoning_effort: None,
        },
    );

    let defaults = cache.effective_default_turn_defaults().unwrap();
    assert_eq!(defaults.model(), Some("gpt-5.5"));
    assert_eq!(defaults.reasoning_effort(), None);
}

fn model(id: &str, model: &str, display_name: &str) -> ModelInfo {
    ModelInfo {
        id: id.to_string(),
        model: model.to_string(),
        display_name: display_name.to_string(),
        description: None,
        hidden: false,
        supported_reasoning_efforts: Vec::new(),
        default_reasoning_effort: None,
        input_modalities: Vec::new(),
        supports_personality: false,
        is_default: false,
    }
}
