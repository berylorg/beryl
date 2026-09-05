#[allow(dead_code)]
#[path = "../src/shell/status_line.rs"]
mod status_line;

use beryl_backend::{
    AccountRateLimitsResponse, RateLimitSnapshot, RateLimitWindow, ThreadSessionMetadata,
    ThreadStatus, ThreadTokenUsage, TokenUsageBreakdown, UsageTreeAccountingStatus,
    UsageTreeSelfUsage, UsageTreeSnapshot, UsageTreeTokenUsageBreakdown,
};
use beryl_model::conversation::{
    ConversationThreadId, ConversationThreadTokenUsageSnapshot, ConversationTokenUsageBreakdown,
    ConversationTurnId, RegisteredConversationThread, WorkspaceConversationState,
};
use beryl_model::workspace::WorkspaceId;
use status_line::{
    CancellableActiveTurn, CancellableActiveTurnKind, SelectedRootUsage, StatusLineCellAction,
    StatusLineCellLayout, StatusLineCellValueKind, StatusLineCellValueSegmentKind,
    StatusLineProjection, StatusLineState, ThreadTurnDefaults,
};
use std::collections::BTreeMap;

#[test]
fn status_projection_uses_unknown_fallbacks() {
    let state = StatusLineState::default();

    let projection = state.projection(Some("thread_1"), "Unknown");

    assert_eq!(projection.model, "Unknown");
    assert_eq!(projection.reasoning_effort, "Unknown");
    assert_eq!(
        projection.context_space_left,
        "Unknown I/IC/O: main: —/—/— sub: —/—/—"
    );
    assert_eq!(projection.last_turn_state, "Unknown");
}

#[test]
fn unknown_status_projection_uses_context_counter_shape_and_segments() {
    let projection = StatusLineProjection::unknown();

    assert_eq!(
        projection.context_space_left,
        "Unknown I/IC/O: main: —/—/— sub: —/—/—"
    );
    assert_eq!(
        projection
            .context_value_segments
            .iter()
            .map(|segment| (segment.text.as_str(), segment.kind))
            .collect::<Vec<_>>(),
        [
            ("Unknown", StatusLineCellValueSegmentKind::Value),
            ("I/IC/O:", StatusLineCellValueSegmentKind::Label),
            ("main:", StatusLineCellValueSegmentKind::Label),
            ("—/—/—", StatusLineCellValueSegmentKind::Value),
            ("sub:", StatusLineCellValueSegmentKind::Label),
            ("—/—/—", StatusLineCellValueSegmentKind::Value),
        ]
    );
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
fn context_compaction_overrides_selected_turn_state() {
    let mut state = StatusLineState::default();

    assert!(state.begin_context_compaction("thread_1"));

    let selected_projection = state.projection(Some("thread_1"), "ok");
    assert_eq!(selected_projection.last_turn_state, "compacting");

    let other_projection = state.projection(Some("thread_2"), "ok");
    assert_eq!(other_projection.last_turn_state, "ok");
}

#[test]
fn context_compaction_finish_restores_underlying_turn_state() {
    let mut state = StatusLineState::default();

    assert!(state.begin_context_compaction("thread_1"));
    assert!(state.finish_context_compaction("thread_1"));

    let projection = state.projection(Some("thread_1"), "ok");
    assert_eq!(projection.last_turn_state, "ok");
}

#[test]
fn context_compaction_stop_identity_remains_pinned_until_cleanup() {
    let mut state = StatusLineState::default();
    state.begin_context_compaction("thread_1");
    assert!(state.set_context_compaction_turn_id("thread_1", "exact"));
    assert!(!state.set_context_compaction_turn_id("thread_1", "unrelated"));
    assert_eq!(
        state
            .context_compaction_cancellation_target(Some("thread_1"))
            .unwrap()
            .turn_id,
        "exact"
    );
}

#[test]
fn compaction_outcome_projects_until_the_next_ordinary_turn() {
    let mut state = StatusLineState::default();
    state.set_compaction_outcome("thread_1", false);
    assert_eq!(
        state.projection(Some("thread_1"), "ok").last_turn_state,
        "error"
    );
    assert!(
        state
            .context_compaction_cancellation_target(Some("thread_1"))
            .is_none()
    );
    state.clear_compaction_outcome("thread_1");
    assert_eq!(
        state
            .projection(Some("thread_1"), "working")
            .last_turn_state,
        "working"
    );
}

#[test]
fn terminal_compaction_projection_retention_keeps_only_latest_and_preserves_live_identity() {
    let mut state = StatusLineState::default();
    state.begin_context_compaction("live");
    state.set_context_compaction_turn_id("live", "exact");
    for index in 0..100 {
        state.set_compaction_outcome(&format!("thread-{index}"), false);
    }
    for index in 0..99 {
        assert_eq!(
            state
                .projection(Some(&format!("thread-{index}")), "ok")
                .last_turn_state,
            "ok"
        );
    }
    assert_eq!(
        state.projection(Some("thread-99"), "ok").last_turn_state,
        "error"
    );
    assert_eq!(
        state
            .context_compaction_cancellation_target(Some("live"))
            .unwrap()
            .turn_id,
        "exact"
    );
    state.clear_session_metadata();
    assert_eq!(
        state.projection(Some("thread-99"), "ok").last_turn_state,
        "ok"
    );
}

#[test]
fn context_compaction_is_cancellable_only_after_turn_id_is_known() {
    let mut state = StatusLineState::default();

    assert!(state.begin_context_compaction("thread_1"));
    assert_eq!(
        state.context_compaction_cancellation_target(Some("thread_1")),
        None
    );

    assert!(state.set_context_compaction_turn_id("thread_1", "turn_compact"));
    let target = state
        .context_compaction_cancellation_target(Some("thread_1"))
        .unwrap();

    assert_eq!(target.thread_id, "thread_1");
    assert_eq!(target.turn_id, "turn_compact");
    assert_eq!(target.kind, CancellableActiveTurnKind::ContextCompaction);
    assert_eq!(
        state.context_compaction_cancellation_target(Some("thread_2")),
        None
    );
}

#[test]
fn unsupported_compaction_stop_keeps_turn_cell_disabled() {
    let mut state = StatusLineState::default();

    assert!(state.begin_context_compaction("thread_1"));
    let projection = state.projection_with_turn_operations(
        Some("thread_1"),
        false,
        false,
        "working",
        state.context_compaction_cancellation_target(Some("thread_1")),
        None,
    );
    let specs = status_line::status_line_cell_specs(projection, false, false, true);

    assert_eq!(specs[2].value, "compacting");
    assert_eq!(specs[2].action, StatusLineCellAction::None);
    assert!(!specs[2].enabled);

    assert!(state.set_context_compaction_turn_id("thread_1", "turn_compact"));
    let projection = state.projection_with_turn_operations(
        Some("thread_1"),
        false,
        false,
        "working",
        state.context_compaction_cancellation_target(Some("thread_1")),
        None,
    );
    let specs = status_line::status_line_cell_specs(projection, false, false, true);

    assert_eq!(specs[2].action, StatusLineCellAction::TurnOperations);
    assert!(specs[2].enabled);
}

#[test]
fn context_compaction_finish_clears_cancellation_target() {
    let mut state = StatusLineState::default();

    assert!(state.begin_context_compaction("thread_1"));
    assert!(state.set_context_compaction_turn_id("thread_1", "turn_compact"));
    assert!(state.finish_context_compaction("thread_1"));

    assert_eq!(
        state.context_compaction_cancellation_target(Some("thread_1")),
        None
    );
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
fn status_projection_carries_hard_stop_targets_for_selected_turn() {
    let state = StatusLineState::default();
    let target = CancellableActiveTurn::ordinary("thread_1", "turn_1");
    let hard_targets = status_line::SelectedTurnHardStopTargets::new(
        target.clone(),
        vec![beryl_backend::HardStopTarget::turn("thread_1", "turn_1")],
        Vec::new(),
    );

    let projection = state.projection_with_turn_operations(
        Some("thread_1"),
        false,
        false,
        "working",
        Some(target.clone()),
        Some(hard_targets.clone()),
    );

    assert_eq!(projection.cancellable_active_turn, Some(target));
    assert_eq!(projection.hard_stop_targets, Some(hard_targets));
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

    let no_thread_options = state.pending_turn_start_options(None);
    assert_eq!(no_thread_options.model(), None);
    assert_eq!(no_thread_options.reasoning_effort(), None);
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
    assert!(
        state.set_effective_new_thread_defaults(Some(ThreadTurnDefaults::new(
            Some("gpt-5.3-codex".to_string()),
            Some("low".to_string()),
        )))
    );

    let selected = state.effective_turn_context_defaults(Some("thread_1"));
    assert_eq!(selected.model(), Some("gpt-5.5"));
    assert_eq!(selected.reasoning_effort(), Some("high"));

    let other = state.effective_turn_context_defaults(Some("thread_2"));
    assert_eq!(other.model(), Some("gpt-5.4"));
    assert_eq!(other.reasoning_effort(), Some("medium"));

    let new_thread = state.effective_turn_context_defaults(None);
    assert_eq!(new_thread.model(), Some("gpt-5.3-codex"));
    assert_eq!(new_thread.reasoning_effort(), Some("low"));
}

#[test]
fn developer_instructions_are_added_without_effective_defaults() {
    let options = status_line::turn_start_options_with_developer_instructions_context(
        beryl_backend::TurnStartOptions::default(),
        Some("Use the operator's settings.".to_string()),
    );

    assert_eq!(
        options.developer_instructions(),
        Some("Use the operator's settings.")
    );
}

#[test]
fn disabled_developer_instructions_are_omitted() {
    let options = status_line::turn_start_options_with_developer_instructions_context(
        beryl_backend::TurnStartOptions::default(),
        None,
    );

    assert_eq!(options.developer_instructions(), None);
}

#[test]
fn late_bound_developer_instructions_replace_request_time_value() {
    let request_time_options = beryl_backend::TurnStartOptions::default()
        .with_developer_instructions(Some("Old setting".to_string()));

    let replacement_start_options =
        status_line::turn_start_options_with_developer_instructions_context(
            request_time_options,
            Some("New setting".to_string()),
        );

    assert_eq!(
        replacement_start_options.developer_instructions(),
        Some("New setting")
    );
}

#[test]
fn developer_instructions_apply_without_effective_model() {
    let stale_options = beryl_backend::TurnStartOptions::default()
        .with_developer_instructions(Some("Old setting".to_string()));
    let options = status_line::turn_start_options_with_developer_instructions_context(
        stale_options,
        Some("Use the operator's settings.".to_string()),
    );

    assert_eq!(
        options.developer_instructions(),
        Some("Use the operator's settings.")
    );
}

#[test]
fn pending_new_thread_defaults_follow_effective_defaults_until_explicit_selection() {
    let mut state = StatusLineState::default();
    assert!(
        state.set_effective_new_thread_defaults(Some(ThreadTurnDefaults::new(
            Some("gpt-5.4".to_string()),
            Some("medium".to_string()),
        )))
    );

    let first_projection = state.projection(None, "Unknown");
    assert_eq!(first_projection.model, "gpt-5.4");
    assert_eq!(first_projection.reasoning_effort, "medium");
    assert_eq!(state.pending_turn_start_options(None).model(), None);
    assert_eq!(
        state.pending_turn_start_options(None).reasoning_effort(),
        None
    );

    assert!(
        state.set_effective_new_thread_defaults(Some(ThreadTurnDefaults::new(
            Some("gpt-5.5".to_string()),
            Some("high".to_string()),
        )))
    );

    let updated_projection = state.projection(None, "Unknown");
    assert_eq!(updated_projection.model, "gpt-5.5");
    assert_eq!(updated_projection.reasoning_effort, "high");
    assert_eq!(state.pending_turn_start_options(None).model(), None);
    assert_eq!(
        state.pending_turn_start_options(None).reasoning_effort(),
        None
    );
}

#[test]
fn explicit_new_thread_defaults_overlay_effective_defaults_and_drive_first_turn_options() {
    let mut state = StatusLineState::default();
    assert!(
        state.set_effective_new_thread_defaults(Some(ThreadTurnDefaults::new(
            Some("gpt-5.4".to_string()),
            Some("medium".to_string()),
        )))
    );
    assert!(
        state.set_pending_new_thread_defaults(ThreadTurnDefaults::new(
            Some("gpt-5.5".to_string()),
            Some("xhigh".to_string()),
        ))
    );

    let projection = state.projection(None, "Unknown");
    let options = state.pending_turn_start_options(None);

    assert_eq!(projection.model, "gpt-5.5");
    assert_eq!(projection.reasoning_effort, "xhigh");
    assert_eq!(options.model(), Some("gpt-5.5"));
    assert_eq!(options.reasoning_effort(), Some("xhigh"));
}

#[test]
fn explicit_new_thread_defaults_bind_to_created_thread_for_retry_and_promotion() {
    let mut state = StatusLineState::default();
    assert!(
        state.set_pending_new_thread_defaults(ThreadTurnDefaults::new(
            Some("gpt-5.5".to_string()),
            Some("high".to_string()),
        ))
    );

    assert!(state.bind_pending_new_thread_defaults_to_thread("thread_1"));
    assert_eq!(state.pending_turn_start_options(None).model(), None);

    let retry_options = state.pending_turn_start_options(Some("thread_1"));
    assert_eq!(retry_options.model(), Some("gpt-5.5"));
    assert_eq!(retry_options.reasoning_effort(), Some("high"));

    assert!(state.promote_pending_turn_defaults("thread_1"));
    let promoted_options = state.pending_turn_start_options(Some("thread_1"));
    assert_eq!(promoted_options.model(), None);
    assert_eq!(promoted_options.reasoning_effort(), None);

    let projection = state.projection(Some("thread_1"), "working");
    assert_eq!(projection.model, "gpt-5.5");
    assert_eq!(projection.reasoning_effort, "high");
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
fn status_line_model_reasoning_is_available_for_idle_thread_or_new_thread_draft() {
    assert!(status_line::status_line_model_reasoning_available(
        Some("thread_1"),
        Some(&ThreadStatus::Idle),
    ));
    assert!(status_line::status_line_model_reasoning_available(
        None,
        Some(&ThreadStatus::Idle),
    ));
    assert!(!status_line::status_line_model_reasoning_available(
        Some("thread_1"),
        Some(&ThreadStatus::Active {
            active_flags: Vec::new()
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
            active_flags: Vec::new()
        }),
    ));
    assert!(!status_line::status_line_context_operation_available(
        Some("thread_1"),
        None,
    ));
}

#[test]
fn waiting_on_user_input_thread_status_is_interactive() {
    let waiting_on_input: ThreadStatus = serde_json::from_value(serde_json::json!({
        "type": "active",
        "activeFlags": ["waitingOnUserInput"]
    }))
    .unwrap();

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
            model_reasoning_available: true,
            context_operation_available: true,
            cancellable_active_turn: None,
            hard_stop_targets: None,
        },
        true,
        false,
        true,
    );

    assert_eq!(specs.len(), 3);
    assert_eq!(specs[0].label, "M/R");
    assert_eq!(specs[0].value, "gpt-5.5 / high");
    assert_eq!(specs[0].action, StatusLineCellAction::ModelReasoning);
    assert_eq!(specs[0].value_kind, StatusLineCellValueKind::Default);
    assert_eq!(specs[0].layout, StatusLineCellLayout::ContentSized);
    assert!(specs[0].enabled);

    assert_eq!(specs[1].label, "Context");
    assert_eq!(specs[1].value, "42%");
    assert_eq!(specs[1].action, StatusLineCellAction::Context);
    assert!(!specs[1].enabled);
    assert_eq!(specs[1].layout, StatusLineCellLayout::FlexibleContext);

    assert_eq!(specs[2].label, "Turn");
    assert_eq!(specs[2].value, "compacting");
    assert_eq!(specs[2].action, StatusLineCellAction::None);
    assert_eq!(specs[2].value_kind, StatusLineCellValueKind::TurnState);
    assert!(!specs[2].enabled);
    assert_eq!(specs[2].layout, StatusLineCellLayout::Turn);
}

#[test]
fn compact_token_count_formats_base_1000_boundaries_without_overflow() {
    for (value, expected) in [
        (0, "0"),
        (999, "999"),
        (1_000, "1k"),
        (1_450, "1.5k"),
        (999_949, "999.9k"),
        (999_950, "1M"),
        (1_000_000, "1M"),
        (1_050_000, "1.1M"),
        (999_950_000, "1B"),
        (1_000_000_000, "1B"),
        (-1, "0"),
        (i64::MAX, "9223372036.9B"),
    ] {
        assert_eq!(status_line::format_compact_token_count(value), expected);
    }
}

#[test]
fn context_status_appends_selected_root_counters_after_rate_limits_in_order() {
    let mut state = StatusLineState::default();
    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_1".to_string(),
        token_usage_with_totals(250, 1_450, 450, 999_950, Some(1_000)),
    ));
    assert!(state.apply_account_rate_limits(rate_limits(Some((15, 1_440)), Some((55, 10_080)))));

    let projection = state.projection(Some("thread_1"), "ok");
    assert_eq!(
        projection.context_space_left,
        "75% Daily 85% Weekly 45% I/IC/O: main: 1k/450/1M sub: —/—/—"
    );
    let specs = status_line::status_line_cell_specs(projection, true, true, true);
    let segments = &specs[1].value_segments;
    assert_eq!(
        segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect::<Vec<_>>(),
        [
            "75%",
            "Daily",
            "85%",
            "Weekly",
            "45%",
            "I/IC/O:",
            "main:",
            "1k/450/1M",
            "sub:",
            "—/—/—"
        ]
    );
}

#[test]
fn token_counters_use_totals_and_remain_independent_from_context_percentage() {
    let mut state = StatusLineState::default();
    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_1".to_string(),
        token_usage_with_totals(250, 100, 175, -4, None),
    ));

    assert_eq!(
        state.projection(Some("thread_1"), "ok").context_space_left,
        "Unknown I/IC/O: main: 0/175/0 sub: —/—/—"
    );
}

#[test]
fn token_counters_subtract_raw_negative_cached_input_before_display_clamping() {
    let mut state = StatusLineState::default();
    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_1".to_string(),
        token_usage_with_totals(0, 100, -50, 0, Some(100)),
    ));

    assert_eq!(
        state.projection(Some("thread_1"), "ok").context_space_left,
        "100% I/IC/O: main: 150/0/0 sub: —/—/—"
    );
}

#[test]
fn token_counters_follow_selected_thread_and_restore_without_draft_inheritance() {
    let mut state = StatusLineState::default();
    assert!(state.apply_token_usage(
        true,
        "thread_a".to_string(),
        "turn_a".to_string(),
        token_usage_with_totals(10, 1_000, 200, 300, Some(100)),
    ));
    assert!(state.apply_token_usage_snapshot(
        true,
        "thread_b".to_string(),
        &token_usage_snapshot("turn_b", 40, Some(100)),
    ));

    assert_eq!(
        state.projection(Some("thread_a"), "ok").context_space_left,
        "90% I/IC/O: main: 800/200/300 sub: —/—/—"
    );
    assert_eq!(
        state.projection(Some("thread_b"), "ok").context_space_left,
        "60% I/IC/O: main: 60/0/11 sub: —/—/—"
    );
    assert_eq!(
        state.projection(None, "ok").context_space_left,
        "Unknown I/IC/O: main: —/—/— sub: —/—/—"
    );
    assert_eq!(
        state.projection(Some("thread_a"), "ok").context_space_left,
        "90% I/IC/O: main: 800/200/300 sub: —/—/—"
    );
}

#[test]
fn cancellable_turn_target_enables_turn_operations_cell_when_backend_allows_it() {
    let projection = StatusLineProjection {
        model: "gpt-5.5".to_string(),
        reasoning_effort: "high".to_string(),
        context_space_left: "42%".to_string(),
        context_value_segments: Vec::new(),
        last_turn_state: "working".to_string(),
        model_reasoning_available: false,
        context_operation_available: false,
        cancellable_active_turn: Some(CancellableActiveTurn::ordinary("thread_1", "turn_1")),
        hard_stop_targets: None,
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
fn context_percent_uses_selected_thread_last_input_tokens() {
    let mut state = StatusLineState::default();
    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_1".to_string(),
        token_usage(250, 900, Some(1000)),
    ));

    let projection = state.projection(Some("thread_1"), "ok");

    assert_eq!(
        projection.context_space_left,
        "75% I/IC/O: main: 900/0/0 sub: —/—/—"
    );
}

#[test]
fn context_status_appends_available_account_rate_limit_remaining_percentages() {
    let mut state = StatusLineState::default();
    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_1".to_string(),
        token_usage(250, 0, Some(1000)),
    ));
    assert!(state.apply_account_rate_limits(rate_limits(Some((15, 1440)), Some((55, 10080)))));

    let projection = state.projection(Some("thread_1"), "ok");

    assert_eq!(
        projection.context_space_left,
        "75% Daily 85% Weekly 45% I/IC/O: main: 0/0/0 sub: —/—/—"
    );
}

#[test]
fn context_status_exposes_rate_limit_labels_as_value_segments() {
    let mut state = StatusLineState::default();
    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_1".to_string(),
        token_usage(250, 0, Some(1000)),
    ));
    assert!(state.apply_account_rate_limits(rate_limits(Some((15, 1440)), Some((55, 10080)))));

    let projection = state.projection(Some("thread_1"), "ok");
    let specs = status_line::status_line_cell_specs(projection, true, true, true);
    let segments = &specs[1].value_segments;

    assert_eq!(segments.len(), 10);
    assert_eq!(segments[0].kind, StatusLineCellValueSegmentKind::Value);
    assert_eq!(segments[0].text, "75%");
    assert_eq!(segments[1].kind, StatusLineCellValueSegmentKind::Label);
    assert_eq!(segments[1].text, "Daily");
    assert_eq!(segments[2].kind, StatusLineCellValueSegmentKind::Value);
    assert_eq!(segments[2].text, "85%");
    assert_eq!(segments[3].kind, StatusLineCellValueSegmentKind::Label);
    assert_eq!(segments[3].text, "Weekly");
    assert_eq!(segments[4].kind, StatusLineCellValueSegmentKind::Value);
    assert_eq!(segments[4].text, "45%");
    assert_eq!(segments[5].text, "I/IC/O:");
    assert_eq!(segments[5].kind, StatusLineCellValueSegmentKind::Label);
    assert_eq!(segments[6].text, "main:");
    assert_eq!(segments[6].kind, StatusLineCellValueSegmentKind::Label);
    assert_eq!(segments[7].text, "0/0/0");
    assert_eq!(segments[7].kind, StatusLineCellValueSegmentKind::Value);
    assert_eq!(segments[8].text, "sub:");
    assert_eq!(segments[8].kind, StatusLineCellValueSegmentKind::Label);
    assert_eq!(segments[9].text, "—/—/—");
    assert_eq!(segments[9].kind, StatusLineCellValueSegmentKind::Value);
}

#[test]
fn account_rate_limit_read_uses_multi_bucket_view_and_notifications_are_partial() {
    let mut state = StatusLineState::default();
    state.set_session_metadata(ThreadSessionMetadata {
        model: Some("gpt-5.3-codex".to_string()),
        model_provider: Some("openai".to_string()),
        reasoning_effort: Some("medium".to_string()),
    });
    assert!(
        state.replace_account_rate_limits(account_rate_limits_response(
            rate_limits(None, Some((55, 10080))),
            [("codex", rate_limits(Some((15, 1440)), Some((55, 10080))),)],
        ))
    );

    assert_eq!(
        state.projection(Some("thread_1"), "ok").context_space_left,
        "Unknown Daily 85% Weekly 45% I/IC/O: main: —/—/— sub: —/—/—"
    );

    assert!(state.apply_account_rate_limits(rate_limits(None, Some((60, 10080)))));

    assert_eq!(
        state.projection(Some("thread_1"), "ok").context_space_left,
        "Unknown Daily 85% Weekly 40% I/IC/O: main: —/—/— sub: —/—/—"
    );
}

#[test]
fn account_rate_limit_read_selects_main_bucket_and_renders_short_window_label() {
    let mut state = StatusLineState::default();
    state.set_session_metadata(ThreadSessionMetadata {
        model: Some("gpt-5.3-codex".to_string()),
        model_provider: Some("openai".to_string()),
        reasoning_effort: Some("medium".to_string()),
    });

    assert!(
        state.replace_account_rate_limits(account_rate_limits_response(
            rate_limits(None, Some((2, 10080))),
            [
                (
                    "codex",
                    rate_limits_for_limit("codex", "Codex", Some((9, 300)), Some((2, 10080))),
                ),
                (
                    "gpt-5.3-codex-spark",
                    rate_limits_for_limit(
                        "gpt-5.3-codex-spark",
                        "GPT-5.3-Codex-Spark",
                        Some((0, 300)),
                        Some((0, 10080)),
                    ),
                ),
            ],
        ))
    );

    let projection = state.projection(Some("thread_1"), "ok");

    assert_eq!(
        projection.context_space_left,
        "Unknown 5h 91% Weekly 98% I/IC/O: main: —/—/— sub: —/—/—"
    );
}

#[test]
fn account_rate_limit_read_selects_spark_bucket_for_spark_model() {
    let mut state = StatusLineState::default();
    state.set_session_metadata(ThreadSessionMetadata {
        model: Some("gpt-5.3-codex-spark".to_string()),
        model_provider: Some("openai".to_string()),
        reasoning_effort: Some("medium".to_string()),
    });

    assert!(
        state.replace_account_rate_limits(account_rate_limits_response(
            rate_limits(None, Some((2, 10080))),
            [
                (
                    "codex",
                    rate_limits_for_limit("codex", "Codex", Some((9, 300)), Some((2, 10080))),
                ),
                (
                    "gpt-5.3-codex-spark",
                    rate_limits_for_limit(
                        "gpt-5.3-codex-spark",
                        "GPT-5.3-Codex-Spark",
                        Some((25, 300)),
                        Some((30, 10080)),
                    ),
                ),
            ],
        ))
    );

    let projection = state.projection(Some("thread_1"), "ok");

    assert_eq!(
        projection.context_space_left,
        "Unknown 5h 75% Weekly 70% I/IC/O: main: —/—/— sub: —/—/—"
    );
}

#[test]
fn account_rate_limit_segments_are_partial_and_independent_from_context_usage() {
    let mut state = StatusLineState::default();
    assert!(state.apply_account_rate_limits(rate_limits(Some((15, 1440)), None)));

    let projection = state.projection(Some("thread_1"), "ok");

    assert_eq!(
        projection.context_space_left,
        "Unknown Daily 85% I/IC/O: main: —/—/— sub: —/—/—"
    );
}

#[test]
fn account_rate_limit_remaining_clamps_used_percent_and_requires_known_window() {
    let mut state = StatusLineState::default();
    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_1".to_string(),
        token_usage(0, 0, Some(1000)),
    ));
    assert!(state.apply_account_rate_limits(RateLimitSnapshot {
        limit_id: None,
        limit_name: None,
        primary: Some(rate_limit_window(-5, Some(1440))),
        secondary: Some(rate_limit_window(120, None)),
    }));

    let projection = state.projection(Some("thread_1"), "ok");

    assert_eq!(
        projection.context_space_left,
        "100% Daily 100% I/IC/O: main: 0/0/0 sub: —/—/—"
    );
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
    assert_eq!(
        projection.context_space_left,
        "Unknown I/IC/O: main: —/—/— sub: —/—/—"
    );
}

#[test]
fn complete_usage_tree_renders_root_and_authoritative_descendant_totals() {
    let mut state = StatusLineState::default();
    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_legacy".to_string(),
        token_usage_with_totals(250, 1_000, 200, 300, Some(1_000)),
    ));
    let tree = usage_tree_snapshot("thread_1", 1, UsageTreeAccountingStatus::Complete, 400);

    assert!(state.apply_usage_tree_snapshot(Some("thread_1"), tree.clone()));
    assert_eq!(
        state.selected_root_usage(Some("thread_1")),
        SelectedRootUsage::Complete(&tree)
    );
    assert_eq!(
        state.projection(Some("thread_1"), "ok").context_space_left,
        "60% I/IC/O: main: 480/20/10 sub: 0/20/10"
    );
}

#[test]
fn unsupported_complete_usage_tree_falls_back_to_legacy_main_totals() {
    let mut state = StatusLineState::default();
    let legacy = token_usage_with_totals(250, 1_000, 200, 300, Some(1_000));
    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_legacy".to_string(),
        legacy.clone(),
    ));
    let mut unsupported_tree =
        usage_tree_snapshot("thread_1", 1, UsageTreeAccountingStatus::Complete, 400);
    unsupported_tree.schema_version = 2;
    assert!(state.apply_usage_tree_snapshot(Some("thread_1"), unsupported_tree));

    assert_eq!(
        state.selected_root_usage(Some("thread_1")),
        SelectedRootUsage::LegacyFallback(&legacy)
    );
    assert_eq!(
        state.projection(Some("thread_1"), "ok").context_space_left,
        "60% I/IC/O: main: 800/200/300 sub: —/—/—"
    );
}

#[test]
fn complete_usage_tree_renders_zero_descendants_and_clamps_each_group() {
    let mut state = StatusLineState::default();
    let mut tree = usage_tree_snapshot("thread_1", 1, UsageTreeAccountingStatus::Complete, 250);
    tree.self_usage.total.input_tokens = -20;
    tree.self_usage.total.cached_input_tokens = 150;
    tree.self_usage.total.output_tokens = -4;
    tree.descendants_total.input_tokens = 100;
    tree.descendants_total.cached_input_tokens = 200;
    tree.descendants_total.output_tokens = -10;
    assert!(state.apply_usage_tree_snapshot(Some("thread_1"), tree));

    assert_eq!(
        state.projection(Some("thread_1"), "ok").context_space_left,
        "75% I/IC/O: main: 0/150/0 sub: 0/200/0"
    );

    let mut zero_descendants =
        usage_tree_snapshot("thread_1", 2, UsageTreeAccountingStatus::Complete, 250);
    zero_descendants.descendants_total.input_tokens = 0;
    zero_descendants.descendants_total.cached_input_tokens = 0;
    zero_descendants.descendants_total.output_tokens = 0;
    assert!(state.apply_usage_tree_snapshot(Some("thread_1"), zero_descendants));

    assert_eq!(
        state.projection(Some("thread_1"), "ok").context_space_left,
        "75% I/IC/O: main: 330/20/10 sub: 0/0/0"
    );
}

#[test]
fn legacy_partial_tree_is_retained_but_preserves_independent_legacy_fallback() {
    let mut state = StatusLineState::default();
    let legacy = token_usage_with_totals(250, 1_000, 200, 300, Some(1_000));
    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_legacy".to_string(),
        legacy.clone(),
    ));
    assert!(state.apply_usage_tree_snapshot(
        Some("thread_1"),
        usage_tree_snapshot("thread_1", 7, UsageTreeAccountingStatus::LegacyPartial, 900,),
    ));

    assert_eq!(
        state.selected_root_usage(Some("thread_1")),
        SelectedRootUsage::LegacyFallback(&legacy)
    );
    assert_eq!(
        state.selected_root_usage(Some("thread_2")),
        SelectedRootUsage::Unavailable
    );

    let mut partial_only = StatusLineState::default();
    assert!(partial_only.apply_usage_tree_snapshot(
        Some("thread_1"),
        usage_tree_snapshot("thread_1", 7, UsageTreeAccountingStatus::LegacyPartial, 900,),
    ));
    assert_eq!(
        partial_only.selected_root_usage(Some("thread_1")),
        SelectedRootUsage::Unavailable
    );
    assert_eq!(
        partial_only
            .projection(Some("thread_1"), "ok")
            .context_space_left,
        "10% I/IC/O: main: —/—/— sub: —/—/—"
    );
}

#[test]
fn newer_partial_revision_replaces_complete_tree_but_not_legacy_fallback() {
    let mut state = StatusLineState::default();
    let legacy = token_usage_with_totals(250, 1_000, 200, 300, Some(1_000));
    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_legacy".to_string(),
        legacy.clone(),
    ));
    assert!(state.apply_usage_tree_snapshot(
        Some("thread_1"),
        usage_tree_snapshot("thread_1", 5, UsageTreeAccountingStatus::Complete, 500),
    ));
    assert!(state.apply_usage_tree_snapshot(
        Some("thread_1"),
        usage_tree_snapshot("thread_1", 6, UsageTreeAccountingStatus::LegacyPartial, 600),
    ));

    assert_eq!(
        state.selected_root_usage(Some("thread_1")),
        SelectedRootUsage::LegacyFallback(&legacy)
    );
    assert_eq!(
        state.projection(Some("thread_1"), "ok").context_space_left,
        "40% I/IC/O: main: 800/200/300 sub: —/—/—"
    );
    assert!(!state.apply_usage_tree_snapshot(
        Some("thread_1"),
        usage_tree_snapshot("thread_1", 5, UsageTreeAccountingStatus::Complete, 500),
    ));
}

#[test]
fn retained_tree_without_a_usable_context_window_does_not_mix_with_legacy_context() {
    let mut state = StatusLineState::default();
    let legacy = token_usage_with_totals(250, 1_000, 200, 300, Some(1_000));
    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_legacy".to_string(),
        legacy,
    ));

    let mut missing_window =
        usage_tree_snapshot("thread_1", 1, UsageTreeAccountingStatus::LegacyPartial, 900);
    missing_window.self_usage.model_context_window = None;
    assert!(state.apply_usage_tree_snapshot(Some("thread_1"), missing_window));
    assert_eq!(
        state.projection(Some("thread_1"), "ok").context_space_left,
        "Unknown I/IC/O: main: 800/200/300 sub: —/—/—"
    );

    let mut nonpositive_window =
        usage_tree_snapshot("thread_1", 2, UsageTreeAccountingStatus::LegacyPartial, 900);
    nonpositive_window.self_usage.model_context_window = Some(0);
    assert!(state.apply_usage_tree_snapshot(Some("thread_1"), nonpositive_window));
    assert_eq!(
        state.projection(Some("thread_1"), "ok").context_space_left,
        "Unknown I/IC/O: main: 800/200/300 sub: —/—/—"
    );
}

#[test]
fn usage_tree_guard_accepts_only_newer_exact_selected_root_snapshots() {
    let mut state = StatusLineState::default();
    let root_a_v2 = usage_tree_snapshot("root_a", 2, UsageTreeAccountingStatus::Complete, 200);
    let root_a_v3 = usage_tree_snapshot("root_a", 3, UsageTreeAccountingStatus::Complete, 300);

    assert!(!state.apply_usage_tree_snapshot(None, root_a_v2.clone()));
    assert!(!state.apply_usage_tree_snapshot(Some("root_b"), root_a_v2.clone()));
    assert!(state.apply_usage_tree_snapshot(Some("root_a"), root_a_v2.clone()));
    assert!(!state.apply_usage_tree_snapshot(Some("root_a"), root_a_v2.clone()));
    assert!(!state.apply_usage_tree_snapshot(
        Some("root_a"),
        usage_tree_snapshot("root_a", 1, UsageTreeAccountingStatus::Complete, 100)
    ));
    assert!(state.apply_usage_tree_snapshot(Some("root_a"), root_a_v3.clone()));
    assert_eq!(
        state.selected_root_usage(Some("root_a")),
        SelectedRootUsage::Complete(&root_a_v3)
    );
}

#[test]
fn usage_tree_selection_switch_evicts_prior_root_and_accepts_new_selected_read() {
    let mut state = StatusLineState::default();
    let root_a = usage_tree_snapshot("root_a", 4, UsageTreeAccountingStatus::Complete, 400);
    let root_b = usage_tree_snapshot("root_b", 2, UsageTreeAccountingStatus::Complete, 200);
    assert!(state.apply_usage_tree_snapshot(Some("root_a"), root_a.clone()));

    state.clear_usage_tree_snapshot_unless_root(Some("root_a"));
    assert_eq!(
        state.selected_root_usage(Some("root_a")),
        SelectedRootUsage::Complete(&root_a)
    );
    state.clear_usage_tree_snapshot_unless_root(Some("root_b"));
    assert_eq!(
        state.selected_root_usage(Some("root_a")),
        SelectedRootUsage::Unavailable
    );
    assert!(state.apply_usage_tree_snapshot(Some("root_b"), root_b.clone()));

    assert!(!state.apply_usage_tree_snapshot(
        Some("root_b"),
        usage_tree_snapshot("root_a", 5, UsageTreeAccountingStatus::Complete, 500)
    ));
    assert_eq!(
        state.selected_root_usage(Some("root_b")),
        SelectedRootUsage::Complete(&root_b)
    );
    state.clear_usage_tree_snapshot_unless_root(Some("root_a"));
    assert_eq!(
        state.selected_root_usage(Some("root_a")),
        SelectedRootUsage::Unavailable
    );
    assert!(state.apply_usage_tree_snapshot(
        Some("root_a"),
        usage_tree_snapshot("root_a", 1, UsageTreeAccountingStatus::Complete, 100)
    ));
    assert_eq!(
        state.selected_root_usage(None),
        SelectedRootUsage::Unavailable
    );
}

#[test]
fn pending_new_thread_draft_clears_selected_usage_tree() {
    let mut state = StatusLineState::default();
    assert!(state.apply_usage_tree_snapshot(
        Some("root_a"),
        usage_tree_snapshot("root_a", 1, UsageTreeAccountingStatus::Complete, 400)
    ));

    state.clear_usage_tree_snapshot_unless_root(None);

    assert_eq!(
        state.selected_root_usage(Some("root_a")),
        SelectedRootUsage::Unavailable
    );
    assert_eq!(
        state.selected_root_usage(None),
        SelectedRootUsage::Unavailable
    );
    assert!(!state.apply_usage_tree_snapshot(
        None,
        usage_tree_snapshot("root_a", 2, UsageTreeAccountingStatus::Complete, 500)
    ));
    assert_eq!(
        state.projection(None, "ok").context_space_left,
        "Unknown I/IC/O: main: —/—/— sub: —/—/—"
    );
}

#[test]
fn unavailable_usage_tree_read_uses_only_exact_legacy_root_usage() {
    let mut state = StatusLineState::default();
    assert!(state.apply_token_usage(
        true,
        "root_a".to_string(),
        "turn_a".to_string(),
        token_usage_with_totals(250, 1_000, 200, 300, Some(1_000)),
    ));
    assert!(state.apply_usage_tree_snapshot(
        Some("root_a"),
        usage_tree_snapshot("root_a", 1, UsageTreeAccountingStatus::Complete, 400)
    ));

    state.clear_usage_tree_snapshot_unless_root(Some("root_b"));

    assert_eq!(
        state.projection(Some("root_a"), "ok").context_space_left,
        "75% I/IC/O: main: 800/200/300 sub: —/—/—"
    );
    assert_eq!(
        state.projection(Some("root_b"), "ok").context_space_left,
        "Unknown I/IC/O: main: —/—/— sub: —/—/—"
    );
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
        "75% I/IC/O: main: 0/0/0 sub: —/—/—"
    );
    assert_eq!(
        state.projection(Some("thread_2"), "ok").context_space_left,
        "90% I/IC/O: main: 0/0/0 sub: —/—/—"
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
        "Unknown I/IC/O: main: —/—/— sub: —/—/—"
    );
    assert_eq!(
        state.projection(Some("thread_1"), "ok").context_space_left,
        "75% I/IC/O: main: 0/0/0 sub: —/—/—"
    );
}

#[test]
fn durable_snapshot_hydrates_context_for_selected_thread() {
    let mut state = StatusLineState::default();

    assert!(state.apply_token_usage_snapshot(
        true,
        "thread_1".to_string(),
        &token_usage_snapshot("turn_1", 50, Some(200)),
    ));

    assert_eq!(
        state
            .projection(Some("thread_1"), "Idle")
            .context_space_left,
        "75% I/IC/O: main: 70/0/11 sub: —/—/—"
    );
}

#[test]
fn durable_snapshot_cache_is_selected_by_thread_after_switching() {
    let mut state = StatusLineState::default();

    state.apply_token_usage_snapshot(
        true,
        "thread_a".to_string(),
        &token_usage_snapshot("turn_a", 50, Some(200)),
    );
    state.apply_token_usage_snapshot(
        true,
        "thread_b".to_string(),
        &token_usage_snapshot("turn_b", 40, Some(100)),
    );

    assert_eq!(
        state
            .projection(Some("thread_a"), "Idle")
            .context_space_left,
        "75% I/IC/O: main: 70/0/11 sub: —/—/—"
    );
    assert_eq!(
        state
            .projection(Some("thread_b"), "Idle")
            .context_space_left,
        "60% I/IC/O: main: 60/0/11 sub: —/—/—"
    );
    assert_eq!(
        state
            .projection(Some("thread_a"), "Idle")
            .context_space_left,
        "75% I/IC/O: main: 70/0/11 sub: —/—/—"
    );
}

#[test]
fn missing_durable_snapshot_keeps_context_unknown_after_restart_style_hydration() {
    let state = StatusLineState::default();

    assert_eq!(
        state
            .projection(Some("thread_1"), "Idle")
            .context_space_left,
        "Unknown I/IC/O: main: —/—/— sub: —/—/—"
    );
}

#[test]
fn durable_snapshot_for_unknown_thread_is_ignored() {
    let mut state = StatusLineState::default();

    assert!(!state.apply_token_usage_snapshot(
        false,
        "thread_1".to_string(),
        &token_usage_snapshot("turn_1", 50, Some(200)),
    ));

    assert_eq!(state.cached_thread_count(), 0);
    assert_eq!(
        state
            .projection(Some("thread_1"), "Idle")
            .context_space_left,
        "Unknown I/IC/O: main: —/—/— sub: —/—/—"
    );
}

#[test]
fn durable_snapshot_missing_context_window_is_unknown() {
    let mut state = StatusLineState::default();

    assert!(state.apply_token_usage_snapshot(
        true,
        "thread_1".to_string(),
        &token_usage_snapshot("turn_1", 50, None),
    ));

    assert_eq!(
        state
            .projection(Some("thread_1"), "Idle")
            .context_space_left,
        "Unknown I/IC/O: main: 70/0/11 sub: —/—/—"
    );
}

#[test]
fn durable_snapshot_non_positive_context_window_is_unknown() {
    let mut state = StatusLineState::default();

    assert!(state.apply_token_usage_snapshot(
        true,
        "thread_1".to_string(),
        &token_usage_snapshot("turn_1", 50, Some(0)),
    ));

    assert_eq!(
        state
            .projection(Some("thread_1"), "Idle")
            .context_space_left,
        "Unknown I/IC/O: main: 70/0/11 sub: —/—/—"
    );
}

#[test]
fn durable_snapshot_does_not_overwrite_newer_notification_cache() {
    let mut state = StatusLineState::default();

    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_live".to_string(),
        token_usage(20, 0, Some(100)),
    ));
    assert!(!state.apply_token_usage_snapshot(
        true,
        "thread_1".to_string(),
        &token_usage_snapshot("turn_durable", 80, Some(100)),
    ));

    assert_eq!(
        state
            .projection(Some("thread_1"), "Idle")
            .context_space_left,
        "80% I/IC/O: main: 0/0/0 sub: —/—/—"
    );
}

#[test]
fn restart_style_hydration_reads_workspace_conversation_state_snapshots() {
    let mut state = StatusLineState::default();
    let workspace_state =
        workspace_state_with_snapshot("thread_1", token_usage_snapshot("turn_1", 50, Some(200)));

    assert!(
        state.hydrate_token_usage_snapshots(&workspace_state, |thread_id| {
            thread_id == "thread_1"
        })
    );

    assert_eq!(
        state
            .projection(Some("thread_1"), "Idle")
            .context_space_left,
        "75% I/IC/O: main: 70/0/11 sub: —/—/—"
    );
}

#[test]
fn new_thread_projection_does_not_consume_cached_usage() {
    let mut state = StatusLineState::default();

    assert!(state.apply_token_usage(
        true,
        "thread_1".to_string(),
        "turn_1".to_string(),
        token_usage(250, 0, Some(1000)),
    ));

    assert_eq!(
        state.projection(None, "ok").context_space_left,
        "Unknown I/IC/O: main: —/—/— sub: —/—/—"
    );
    assert_eq!(
        state.projection(Some("thread_1"), "ok").context_space_left,
        "75% I/IC/O: main: 0/0/0 sub: —/—/—"
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
    assert_eq!(
        projection.context_space_left,
        "Unknown I/IC/O: main: 0/0/0 sub: —/—/—"
    );
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
    assert_eq!(
        projection.context_space_left,
        "Unknown I/IC/O: main: 0/0/0 sub: —/—/—"
    );
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

fn token_usage_with_totals(
    last_input_tokens: i64,
    total_input_tokens: i64,
    total_cached_input_tokens: i64,
    total_output_tokens: i64,
    model_context_window: Option<i64>,
) -> ThreadTokenUsage {
    ThreadTokenUsage {
        last: TokenUsageBreakdown {
            input_tokens: last_input_tokens,
            ..TokenUsageBreakdown::default()
        },
        total: TokenUsageBreakdown {
            input_tokens: total_input_tokens,
            cached_input_tokens: total_cached_input_tokens,
            output_tokens: total_output_tokens,
            ..TokenUsageBreakdown::default()
        },
        model_context_window,
    }
}

fn rate_limits(primary: Option<(i32, i64)>, secondary: Option<(i32, i64)>) -> RateLimitSnapshot {
    RateLimitSnapshot {
        limit_id: None,
        limit_name: None,
        primary: primary.map(|(used_percent, window_duration_mins)| {
            rate_limit_window(used_percent, Some(window_duration_mins))
        }),
        secondary: secondary.map(|(used_percent, window_duration_mins)| {
            rate_limit_window(used_percent, Some(window_duration_mins))
        }),
    }
}

fn rate_limits_for_limit(
    limit_id: &str,
    limit_name: &str,
    primary: Option<(i32, i64)>,
    secondary: Option<(i32, i64)>,
) -> RateLimitSnapshot {
    RateLimitSnapshot {
        limit_id: Some(limit_id.to_string()),
        limit_name: Some(limit_name.to_string()),
        primary: primary.map(|(used_percent, window_duration_mins)| {
            rate_limit_window(used_percent, Some(window_duration_mins))
        }),
        secondary: secondary.map(|(used_percent, window_duration_mins)| {
            rate_limit_window(used_percent, Some(window_duration_mins))
        }),
    }
}

fn account_rate_limits_response(
    rate_limits: RateLimitSnapshot,
    rate_limits_by_limit_id: impl IntoIterator<Item = (&'static str, RateLimitSnapshot)>,
) -> AccountRateLimitsResponse {
    AccountRateLimitsResponse {
        rate_limits,
        rate_limits_by_limit_id: Some(
            rate_limits_by_limit_id
                .into_iter()
                .map(|(limit_id, snapshot)| (limit_id.to_string(), snapshot))
                .collect::<BTreeMap<_, _>>(),
        ),
    }
}

fn rate_limit_window(used_percent: i32, window_duration_mins: Option<i64>) -> RateLimitWindow {
    RateLimitWindow {
        used_percent,
        window_duration_mins,
        resets_at: None,
    }
}

fn token_usage_snapshot(
    turn_id: &str,
    input_tokens: i64,
    model_context_window: Option<i64>,
) -> ConversationThreadTokenUsageSnapshot {
    ConversationThreadTokenUsageSnapshot::new(
        ConversationTurnId::new(turn_id),
        ConversationTokenUsageBreakdown::new(0, input_tokens, 5, 7, input_tokens + 12),
        ConversationTokenUsageBreakdown::new(0, input_tokens + 20, 11, 13, input_tokens + 44),
        model_context_window,
        42,
    )
}

fn usage_tree_snapshot(
    root_thread_id: &str,
    revision: u64,
    accounting_status: UsageTreeAccountingStatus,
    last_input_tokens: i64,
) -> UsageTreeSnapshot {
    let breakdown = |input_tokens| UsageTreeTokenUsageBreakdown {
        total_tokens: input_tokens + 35,
        input_tokens,
        cached_input_tokens: 20,
        cache_write_input_tokens: 3,
        output_tokens: 10,
        reasoning_output_tokens: 5,
    };
    UsageTreeSnapshot {
        schema_version: 1,
        root_thread_id: root_thread_id.to_string(),
        revision,
        self_usage: UsageTreeSelfUsage {
            total: breakdown(last_input_tokens + 100),
            last: breakdown(last_input_tokens),
            model_context_window: Some(1_000),
        },
        descendants_total: breakdown(0),
        tree_total: breakdown(last_input_tokens + 100),
        accounting_status,
    }
}

fn workspace_state_with_snapshot(
    thread_id: &str,
    snapshot: ConversationThreadTokenUsageSnapshot,
) -> WorkspaceConversationState {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new(thread_id);
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target,
        "Preview",
        None,
        1,
        2,
    ));
    workspace_state
        .record_thread_token_usage_snapshot(&thread_id, snapshot)
        .unwrap();
    workspace_state
}
