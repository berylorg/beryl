#[allow(dead_code)]
#[path = "../src/shell/status_line.rs"]
mod status_line;

use beryl_backend::{
    ThreadActiveFlags, ThreadSessionMetadata, ThreadStatus, ThreadTokenUsage, TokenUsageBreakdown,
};
use status_line::{
    CancellableActiveTurn, StatusLineCellAction, StatusLineCellValueKind,
    StatusLineCellValueSegmentKind, StatusLineProjection, StatusLineState, StatusLineTurnView,
    ThreadTurnDefaults,
};

#[test]
fn status_projection_uses_unknown_fallbacks() {
    let state = StatusLineState::default();

    let projection = state.projection(Some("thread_1"), "Unknown");

    assert_eq!(projection.model, "Unknown");
    assert_eq!(projection.reasoning_effort, "Unknown");
    assert_eq!(projection.context_space_left, "Unknown");
    assert_eq!(projection.last_turn_state, "Unknown");
    assert_eq!(projection.turn_view, StatusLineTurnView::unknown());
}

#[test]
fn status_projection_uses_session_metadata() {
    let mut state = StatusLineState::default();
    state.set_session_metadata(ThreadSessionMetadata {
        model: Some("gpt-5.4".to_string()),
        model_provider: Some("openai".to_string()),
        reasoning_effort: Some("high".to_string()),
    });

    let projection = state.projection(Some("thread_1"), "working");

    assert_eq!(projection.model, "gpt-5.4");
    assert_eq!(projection.reasoning_effort, "high");
    assert_eq!(projection.last_turn_state, "working");
}

#[test]
fn status_projection_carries_cancellable_active_turn() {
    let state = StatusLineState::default();
    let target = CancellableActiveTurn::ordinary("thread_1", "turn_1");

    let projection = state.projection_with_cancellable_active_turn(
        Some("thread_1"),
        false,
        false,
        "working",
        Some(target.clone()),
    );

    assert!(projection.turn_operation_available());
    assert_eq!(projection.cancellable_active_turn, Some(target));
}

#[test]
fn pending_defaults_overlay_session_metadata_for_selected_thread() {
    let mut state = StatusLineState::default();
    state.set_session_metadata(ThreadSessionMetadata {
        model: Some("gpt-5.4".to_string()),
        model_provider: Some("openai".to_string()),
        reasoning_effort: Some("medium".to_string()),
    });

    assert!(state.set_pending_turn_defaults(
        "thread_1",
        ThreadTurnDefaults::new(Some("gpt-5.5".to_string()), Some("high".to_string())),
    ));

    let selected_projection = state.projection(Some("thread_1"), "Idle");
    assert_eq!(selected_projection.model, "gpt-5.5");
    assert_eq!(selected_projection.reasoning_effort, "high");

    let other_projection = state.projection(Some("thread_2"), "Idle");
    assert_eq!(other_projection.model, "gpt-5.4");
    assert_eq!(other_projection.reasoning_effort, "medium");
}

#[test]
fn pending_turn_options_are_selected_by_thread() {
    let mut state = StatusLineState::default();
    assert!(state.set_pending_turn_defaults(
        "thread_1",
        ThreadTurnDefaults::new(Some("gpt-5.5".to_string()), Some("low".to_string())),
    ));

    let selected_options = state.pending_turn_start_options(Some("thread_1"));
    assert_eq!(selected_options.model(), Some("gpt-5.5"));
    assert_eq!(selected_options.reasoning_effort(), Some("low"));

    let other_options = state.pending_turn_start_options(Some("thread_2"));
    assert_eq!(other_options.model(), None);
    assert_eq!(other_options.reasoning_effort(), None);
}

#[test]
fn effective_turn_context_defaults_include_displayed_model_and_reasoning() {
    let mut state = StatusLineState::default();
    state.set_session_metadata(ThreadSessionMetadata {
        model: Some("gpt-5.4".to_string()),
        model_provider: Some("openai".to_string()),
        reasoning_effort: Some("medium".to_string()),
    });
    assert!(state.set_pending_turn_defaults(
        "thread_1",
        ThreadTurnDefaults::new(Some("gpt-5.5".to_string()), Some("high".to_string())),
    ));
    let selected = state.effective_turn_context_defaults(Some("thread_1"));
    assert_eq!(selected.model(), Some("gpt-5.5"));
    assert_eq!(selected.reasoning_effort(), Some("high"));

    let other = state.effective_turn_context_defaults(Some("thread_2"));
    assert_eq!(other.model(), Some("gpt-5.4"));
    assert_eq!(other.reasoning_effort(), Some("medium"));
}

#[test]
fn developer_instructions_context_is_added_from_effective_defaults() {
    let options = status_line::turn_start_options_with_developer_instructions_context(
        beryl_backend::TurnStartOptions::default(),
        Some("Use the operator's settings.".to_string()),
        ThreadTurnDefaults::new(Some("gpt-5.5".to_string()), Some("high".to_string())),
    );

    let context = options
        .developer_instructions_context()
        .expect("context should be attached");
    assert_eq!(
        context.developer_instructions(),
        Some("Use the operator's settings.")
    );
    assert_eq!(context.model(), "gpt-5.5");
    assert_eq!(context.reasoning_effort(), Some("high"));
}

#[test]
fn disabled_developer_instructions_context_keeps_hidden_reset() {
    let options = status_line::turn_start_options_with_developer_instructions_context(
        beryl_backend::TurnStartOptions::default(),
        None,
        ThreadTurnDefaults::new(Some("gpt-5.5".to_string()), None),
    );

    let context = options
        .developer_instructions_context()
        .expect("context should be attached");
    assert_eq!(context.developer_instructions(), None);
    assert_eq!(context.model(), "gpt-5.5");
    assert_eq!(context.reasoning_effort(), None);
}

#[test]
fn late_bound_developer_instructions_context_replaces_request_time_context() {
    let request_time_options = beryl_backend::TurnStartOptions::default()
        .with_developer_instructions_context(Some("Old setting".to_string()), "gpt-5.4", None);

    let replacement_start_options =
        status_line::turn_start_options_with_developer_instructions_context(
            request_time_options,
            Some("New setting".to_string()),
            ThreadTurnDefaults::new(Some("gpt-5.5".to_string()), Some("high".to_string())),
        );

    let context = replacement_start_options
        .developer_instructions_context()
        .expect("replacement start should have late-bound context");
    assert_eq!(context.developer_instructions(), Some("New setting"));
    assert_eq!(context.model(), "gpt-5.5");
    assert_eq!(context.reasoning_effort(), Some("high"));
}

#[test]
fn developer_instructions_context_is_omitted_without_effective_model() {
    let stale_options = beryl_backend::TurnStartOptions::default()
        .with_developer_instructions_context(Some("Old setting".to_string()), "gpt-5.4", None);
    let options = status_line::turn_start_options_with_developer_instructions_context(
        stale_options,
        Some("Use the operator's settings.".to_string()),
        ThreadTurnDefaults::new(None, Some("high".to_string())),
    );

    assert!(options.developer_instructions_context().is_none());
}

#[test]
fn pending_defaults_drive_both_display_and_next_turn_options() {
    let mut state = StatusLineState::default();
    assert!(state.set_pending_turn_defaults(
        "thread_1",
        ThreadTurnDefaults::new(Some("gpt-5.5".to_string()), Some("xhigh".to_string())),
    ));

    let projection = state.projection(Some("thread_1"), "ok");
    let options = state.pending_turn_start_options(Some("thread_1"));

    assert_eq!(projection.model, "gpt-5.5");
    assert_eq!(projection.reasoning_effort, "xhigh");
    assert_eq!(options.model(), Some("gpt-5.5"));
    assert_eq!(options.reasoning_effort(), Some("xhigh"));
}

#[test]
fn promotion_displays_effective_defaults_without_resending_pending_options() {
    let mut state = StatusLineState::default();
    state.set_session_metadata(ThreadSessionMetadata {
        model: Some("gpt-5.4".to_string()),
        model_provider: Some("openai".to_string()),
        reasoning_effort: Some("medium".to_string()),
    });
    assert!(state.set_pending_turn_defaults(
        "thread_1",
        ThreadTurnDefaults::new(Some("gpt-5.5".to_string()), Some("high".to_string())),
    ));

    assert!(state.promote_pending_turn_defaults("thread_1"));

    let options = state.pending_turn_start_options(Some("thread_1"));
    assert_eq!(options.model(), None);
    assert_eq!(options.reasoning_effort(), None);

    let projection = state.projection(Some("thread_1"), "working");
    assert_eq!(projection.model, "gpt-5.5");
    assert_eq!(projection.reasoning_effort, "high");
}

#[test]
fn session_metadata_for_thread_replaces_promoted_defaults() {
    let mut state = StatusLineState::default();
    assert!(state.set_pending_turn_defaults(
        "thread_1",
        ThreadTurnDefaults::new(Some("gpt-5.5".to_string()), Some("high".to_string())),
    ));
    assert!(state.promote_pending_turn_defaults("thread_1"));

    state.set_session_metadata_for_thread(
        Some("thread_1"),
        ThreadSessionMetadata {
            model: Some("gpt-5.6".to_string()),
            model_provider: Some("openai".to_string()),
            reasoning_effort: Some("low".to_string()),
        },
    );

    let projection = state.projection(Some("thread_1"), "Idle");
    assert_eq!(projection.model, "gpt-5.6");
    assert_eq!(projection.reasoning_effort, "low");
}

#[test]
fn pending_defaults_are_preserved_until_start_success_promotes_them() {
    let mut state = StatusLineState::default();
    assert!(state.set_pending_turn_defaults(
        "thread_1",
        ThreadTurnDefaults::new(Some("gpt-5.5".to_string()), Some("medium".to_string())),
    ));

    let options = state.pending_turn_start_options(Some("thread_1"));
    assert_eq!(options.model(), Some("gpt-5.5"));
    assert_eq!(options.reasoning_effort(), Some("medium"));
}

#[test]
fn status_projection_carries_operation_availability() {
    let state = StatusLineState::default();

    let unavailable =
        state.projection_with_operation_availability(Some("thread_1"), false, false, "Idle");
    assert!(!unavailable.model_reasoning_available);
    assert!(!unavailable.context_operation_available);

    let available =
        state.projection_with_operation_availability(Some("thread_1"), true, true, "Idle");
    assert!(available.model_reasoning_available);
    assert!(available.context_operation_available);
}

#[test]
fn status_line_model_reasoning_is_available_only_for_an_idle_selected_thread() {
    assert!(status_line::status_line_model_reasoning_available(
        Some("thread_1"),
        Some(&ThreadStatus::Idle),
    ));
    assert!(!status_line::status_line_model_reasoning_available(
        Some("thread_1"),
        Some(&ThreadStatus::Active {
            active_flags: ThreadActiveFlags::empty(),
        }),
    ));
    assert!(!status_line::status_line_model_reasoning_available(
        Some("thread_1"),
        None,
    ));
}

#[test]
fn status_line_context_operations_require_selected_idle_thread() {
    assert!(status_line::status_line_context_operation_available(
        Some("thread_1"),
        Some(&ThreadStatus::Idle),
    ));
    assert!(!status_line::status_line_context_operation_available(
        None,
        Some(&ThreadStatus::Idle),
    ));
    assert!(!status_line::status_line_context_operation_available(
        Some("thread_1"),
        Some(&ThreadStatus::Active {
            active_flags: ThreadActiveFlags::empty(),
        }),
    ));
    assert!(!status_line::status_line_context_operation_available(
        Some("thread_1"),
        None,
    ));
}

#[test]
fn waiting_on_user_input_thread_status_is_interactive() {
    let waiting_on_input = ThreadStatus::active(ThreadActiveFlags::new(false, true));

    assert!(status_line::status_line_model_reasoning_available(
        Some("thread_1"),
        Some(&waiting_on_input),
    ));
    assert!(status_line::status_line_context_operation_available(
        Some("thread_1"),
        Some(&waiting_on_input),
    ));
}

#[test]
fn completed_idle_selected_thread_enables_interactive_status_cells() {
    let state = StatusLineState::default();
    let projection = state.projection_with_operation_availability(
        Some("thread_1"),
        status_line::status_line_model_reasoning_available(
            Some("thread_1"),
            Some(&ThreadStatus::Idle),
        ),
        status_line::status_line_context_operation_available(
            Some("thread_1"),
            Some(&ThreadStatus::Idle),
        ),
        "ok",
    );
    let specs = status_line::status_line_cell_specs(projection, true, true, true);

    assert!(specs[0].enabled);
    assert!(specs[1].enabled);
    assert_eq!(specs[2].value, "ok");
    assert!(!specs[2].enabled);
}

#[test]
fn status_line_cell_specs_cover_three_cells_and_disabled_interactions() {
    let specs = status_line::status_line_cell_specs(
        StatusLineProjection {
            model: "gpt-5.5".to_string(),
            reasoning_effort: "high".to_string(),
            context_space_left: "42%".to_string(),
            context_value_segments: Vec::new(),
            last_turn_state: "compacting".to_string(),
            turn_view: StatusLineTurnView::unknown(),
            model_reasoning_available: true,
            context_operation_available: true,
            cancellable_active_turn: None,
        },
        true,
        false,
        true,
    );

    assert_eq!(specs.len(), 3);
    assert_eq!(specs[0].label, "Model / Reasoning");
    assert_eq!(specs[0].value, "gpt-5.5 / high");
    assert_eq!(specs[0].action, StatusLineCellAction::ModelReasoning);
    assert_eq!(specs[0].value_kind, StatusLineCellValueKind::Default);
    assert!(specs[0].enabled);

    assert_eq!(specs[1].label, "Context");
    assert_eq!(specs[1].value, "42%");
    assert_eq!(specs[1].action, StatusLineCellAction::Context);
    assert!(!specs[1].enabled);

    assert_eq!(specs[2].label, "Turn");
    assert_eq!(specs[2].value, "compacting");
    assert_eq!(specs[2].action, StatusLineCellAction::None);
    assert_eq!(specs[2].value_kind, StatusLineCellValueKind::TurnState);
    assert!(!specs[2].enabled);
    assert_eq!(specs[2].value_segments.len(), 3);
    assert_eq!(
        specs[2].value_segments[0].kind,
        StatusLineCellValueSegmentKind::Value
    );
    assert_eq!(specs[2].value_segments[0].text, "compacting");
    assert_eq!(
        specs[2].value_segments[1].kind,
        StatusLineCellValueSegmentKind::Label
    );
    assert_eq!(specs[2].value_segments[1].text, "View");
    assert_eq!(
        specs[2].value_segments[2].kind,
        StatusLineCellValueSegmentKind::SecondaryValue
    );
    assert_eq!(specs[2].value_segments[2].text, "-/-");
}

#[test]
fn status_line_turn_view_count_formats_known_and_unknown_sides() {
    let unknown_specs =
        status_line::status_line_cell_specs(StatusLineProjection::unknown(), false, false, false);
    assert_eq!(unknown_specs[2].value_segments[2].text, "-/-");

    let total_only_specs = status_line::status_line_cell_specs(
        StatusLineProjection::unknown().with_turn_view(StatusLineTurnView::new(None, Some(5))),
        false,
        false,
        false,
    );
    assert_eq!(total_only_specs[2].value_segments[2].text, "-/5");

    let known_specs = status_line::status_line_cell_specs(
        StatusLineProjection::unknown().with_turn_view(StatusLineTurnView::new(Some(5), Some(5))),
        false,
        false,
        false,
    );
    assert_eq!(known_specs[2].value_segments[2].text, "5/5");
}

#[test]
fn status_line_turn_view_count_treats_zero_as_unknown() {
    let specs = status_line::status_line_cell_specs(
        StatusLineProjection::unknown().with_turn_view(StatusLineTurnView::new(Some(0), Some(0))),
        false,
        false,
        false,
    );

    assert_eq!(specs[2].value_segments[2].text, "-/-");
}

#[test]
fn cancellable_turn_target_enables_turn_operations_cell_when_backend_allows_it() {
    let projection = StatusLineProjection {
        model: "gpt-5.5".to_string(),
        reasoning_effort: "high".to_string(),
        context_space_left: "42%".to_string(),
        context_value_segments: Vec::new(),
        last_turn_state: "working".to_string(),
        turn_view: StatusLineTurnView::unknown(),
        model_reasoning_available: false,
        context_operation_available: false,
        cancellable_active_turn: Some(CancellableActiveTurn::ordinary("thread_1", "turn_1")),
    };

    let disabled_specs =
        status_line::status_line_cell_specs(projection.clone(), false, false, false);
    assert_eq!(
        disabled_specs[2].action,
        StatusLineCellAction::TurnOperations
    );
    assert!(!disabled_specs[2].enabled);

    let enabled_specs = status_line::status_line_cell_specs(projection, false, false, true);
    assert_eq!(
        enabled_specs[2].action,
        StatusLineCellAction::TurnOperations
    );
    assert!(enabled_specs[2].enabled);
}

#[test]
fn non_owned_active_turn_state_does_not_enable_turn_operations_cell() {
    let projection = StatusLineProjection {
        model: "gpt-5.5".to_string(),
        reasoning_effort: "high".to_string(),
        context_space_left: "42%".to_string(),
        context_value_segments: Vec::new(),
        last_turn_state: "active".to_string(),
        turn_view: StatusLineTurnView::unknown(),
        model_reasoning_available: false,
        context_operation_available: false,
        cancellable_active_turn: None,
    };

    let specs = status_line::status_line_cell_specs(projection, false, false, true);

    assert_eq!(specs[2].value, "active");
    assert_eq!(specs[2].action, StatusLineCellAction::None);
    assert!(!specs[2].enabled);
}

#[test]
fn known_turn_view_does_not_enable_turn_operations_cell_without_stop_target() {
    let projection = StatusLineProjection {
        model: "gpt-5.5".to_string(),
        reasoning_effort: "high".to_string(),
        context_space_left: "42%".to_string(),
        context_value_segments: Vec::new(),
        last_turn_state: "ok".to_string(),
        turn_view: StatusLineTurnView::new(Some(5), Some(5)),
        model_reasoning_available: false,
        context_operation_available: false,
        cancellable_active_turn: None,
    };

    let specs = status_line::status_line_cell_specs(projection, false, false, true);

    assert_eq!(specs[2].value_segments[2].text, "5/5");
    assert_eq!(specs[2].action, StatusLineCellAction::None);
    assert!(!specs[2].enabled);
}

#[test]
fn context_percent_uses_selected_thread_last_input_tokens() {
    let mut state = StatusLineState::default();
    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_1".to_string(),
        token_usage(250, 900, Some(1000)),
    ));

    let projection = state.projection(Some("thread_1"), "ok");

    assert_eq!(projection.context_space_left, "75%");
}

#[test]
fn token_usage_for_unknown_thread_is_ignored() {
    let mut state = StatusLineState::default();

    assert!(!state.apply_token_usage(
        false,
        "thread_2".to_string(),
        "turn_1".to_string(),
        token_usage(250, 0, Some(1000)),
    ));

    assert_eq!(state.cached_thread_count(), 0);

    let projection = state.projection(Some("thread_2"), "ok");
    assert_eq!(projection.context_space_left, "Unknown");
}

#[test]
fn cached_token_usage_is_selected_by_thread() {
    let mut state = StatusLineState::default();

    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_1".to_string(),
        token_usage(250, 0, Some(1000)),
    ));
    assert!(state.apply_token_usage(
        true,
        "thread_2".to_string(),
        "turn_2".to_string(),
        token_usage(100, 0, Some(1000)),
    ));

    assert_eq!(
        state.projection(Some("thread_1"), "ok").context_space_left,
        "75%"
    );
    assert_eq!(
        state.projection(Some("thread_2"), "ok").context_space_left,
        "90%"
    );
}

#[test]
fn cached_token_usage_survives_switching_away_and_back() {
    let mut state = StatusLineState::default();

    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_1".to_string(),
        token_usage(250, 0, Some(1000)),
    ));

    assert_eq!(
        state.projection(Some("thread_2"), "ok").context_space_left,
        "Unknown"
    );
    assert_eq!(
        state.projection(Some("thread_1"), "ok").context_space_left,
        "75%"
    );
}

#[test]
fn missing_token_usage_keeps_context_unknown() {
    let state = StatusLineState::default();

    assert_eq!(
        state
            .projection(Some("thread_1"), "Idle")
            .context_space_left,
        "Unknown"
    );
}

#[test]
fn non_positive_context_window_is_unknown() {
    let mut state = StatusLineState::default();
    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_1".to_string(),
        token_usage(250, 0, Some(0)),
    ));

    let projection = state.projection(Some("thread_1"), "ok");
    assert_eq!(projection.context_space_left, "Unknown");
}

#[test]
fn missing_context_window_is_unknown() {
    let mut state = StatusLineState::default();
    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_1".to_string(),
        token_usage(250, 0, None),
    ));

    let projection = state.projection(Some("thread_1"), "ok");
    assert_eq!(projection.context_space_left, "Unknown");
}

fn token_usage(
    last_input_tokens: i64,
    total_input_tokens: i64,
    model_context_window: Option<i64>,
) -> ThreadTokenUsage {
    ThreadTokenUsage {
        last: TokenUsageBreakdown {
            input_tokens: last_input_tokens,
            ..TokenUsageBreakdown::default()
        },
        total: TokenUsageBreakdown {
            input_tokens: total_input_tokens,
            ..TokenUsageBreakdown::default()
        },
        model_context_window,
    }
}
