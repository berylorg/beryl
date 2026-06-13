#![allow(dead_code, private_interfaces)]

use std::time::Duration;

use beryl_backend::{
    AgentMessageItem, ImageGenerationItem, ProtocolPhase, SortDirection, ThreadItem,
    ThreadTurnsListOptions, ThreadTurnsListResponse, TurnInfo, TurnItemsView, TurnStatus,
};

mod shell {
    use std::time::Duration;

    fn elapsed_ms(duration: Duration) -> f64 {
        duration.as_secs_f64() * 1000.0
    }

    #[path = "../../src/shell/execution_detail.rs"]
    pub(super) mod execution_detail;
    #[path = "../../src/shell/transcript_history.rs"]
    pub(super) mod transcript_history;

    #[derive(Debug, PartialEq, Eq)]
    pub(super) struct GeneratedImageSnapshot {
        pub(super) id: String,
        pub(super) status: Option<String>,
        pub(super) revised_prompt: Option<String>,
        pub(super) result_len: Option<usize>,
        pub(super) saved_path: Option<String>,
        pub(super) complete: bool,
    }

    pub(super) struct DetailHarness {
        state: execution_detail::ExecutionDetailState,
    }

    impl DetailHarness {
        pub(super) fn new() -> Self {
            Self {
                state: execution_detail::ExecutionDetailState::default(),
            }
        }

        pub(super) fn prepend(
            &mut self,
            thread_id: &str,
            turns: Vec<beryl_backend::TurnInfo>,
        ) -> usize {
            self.state.prepend_thread_history_page(thread_id, turns)
        }

        pub(super) fn agent_message_texts(&self) -> Vec<Vec<String>> {
            self.state
                .turns()
                .iter()
                .map(|turn| {
                    turn.items
                        .iter()
                        .filter_map(|item| match item {
                            execution_detail::ExecutionItem::AgentMessage(message) => {
                                Some(message.text.clone())
                            }
                            _ => None,
                        })
                        .collect()
                })
                .collect()
        }

        pub(super) fn generated_images(&self) -> Vec<Vec<GeneratedImageSnapshot>> {
            self.state
                .turns()
                .iter()
                .map(|turn| {
                    turn.items
                        .iter()
                        .filter_map(|item| match item {
                            execution_detail::ExecutionItem::GeneratedImage(image) => {
                                Some(GeneratedImageSnapshot {
                                    id: image.id.clone(),
                                    status: image.status.clone(),
                                    revised_prompt: image.revised_prompt.clone(),
                                    result_len: image.result.as_ref().map(|result| result.len()),
                                    saved_path: image.saved_path.clone(),
                                    complete: image.complete,
                                })
                            }
                            _ => None,
                        })
                        .collect()
                })
                .collect()
        }
    }
}

use shell::transcript_history::{
    LoadedTranscriptHistoryPage, THREAD_HISTORY_PAGE_LIMIT, TranscriptHistoryBackend,
    TranscriptHistoryPageRequest, TranscriptHistoryWindow, TranscriptResidencyBudgetReason,
    TranscriptResidencyGrowthStrategy, TranscriptResidencyMeasuredTurnHeight,
    TranscriptResidencyPinKind, TranscriptResidencyPolicy, TranscriptResidencyRequestPriority,
    TranscriptResidencyRetention, TranscriptResidencyStreamedTurnFill,
    TranscriptResidencyTargetInput, TranscriptResidencyTargetPolicy,
    TranscriptResidencyTurnPlanInput, TranscriptResidencyViewport,
    initial_thread_activation_resident_turn_ids, initial_thread_history_page_options,
    initial_thread_resident_page_options, load_thread_resident_history_page,
    loaded_page_from_desc_response, older_thread_history_page_options,
    plan_transcript_residency_target, resident_turn_ids_for_page_window,
    sanitize_loaded_page_for_resident_turn_ids, sanitize_loaded_page_for_turn_admission_plan,
    thread_history_page_options, thread_resident_history_page_options,
    turn_admission_plan_for_page_window,
};

#[test]
fn history_page_options_request_index_only_turns() {
    let initial = initial_thread_history_page_options();
    assert_eq!(initial.items_view, Some(TurnItemsView::NotLoaded));
    assert_eq!(initial.sort_direction, Some(SortDirection::Desc));

    let older = older_thread_history_page_options("older_cursor");
    assert_eq!(older.items_view, Some(TurnItemsView::NotLoaded));
    assert_eq!(older.cursor.as_deref(), Some("older_cursor"));

    let generic = thread_history_page_options(Some("older_cursor"));
    assert_eq!(generic.items_view, Some(TurnItemsView::NotLoaded));
    assert_eq!(generic.cursor.as_deref(), Some("older_cursor"));
}

#[test]
fn initial_resident_page_options_request_full_turns() {
    let initial = initial_thread_resident_page_options();
    assert_eq!(initial.items_view, Some(TurnItemsView::Full));
    assert_eq!(initial.sort_direction, Some(SortDirection::Desc));
    assert_eq!(initial.cursor, None);
}

#[test]
fn thread_resident_history_page_options_request_full_turns() {
    let options = thread_resident_history_page_options(Some("older_cursor"));

    assert_eq!(options.limit, Some(THREAD_HISTORY_PAGE_LIMIT));
    assert_eq!(options.cursor.as_deref(), Some("older_cursor"));
    assert_eq!(options.sort_direction, Some(SortDirection::Desc));
    assert_eq!(options.items_view, Some(TurnItemsView::Full));
}

#[test]
fn resident_history_page_loader_rejects_mixed_items_view_response() {
    let mut backend = FakeHistoryBackend::new(Ok(ThreadTurnsListResponse {
        data: vec![
            turn("turn_2"),
            turn_with_items_view("turn_1", TurnItemsView::NotLoaded),
        ],
        next_cursor: Some("older".to_string()),
        backwards_cursor: Some("newer".to_string()),
    }));

    let error = load_thread_resident_history_page(
        &mut backend,
        "thread_a",
        Some("older_cursor"),
        Duration::from_secs(1),
    )
    .unwrap_err();

    assert_eq!(backend.calls.len(), 1);
    assert_eq!(backend.calls[0].0, "thread_a");
    assert_eq!(
        backend.calls[0].1,
        thread_resident_history_page_options(Some("older_cursor"))
    );
    let message = error.to_string();
    assert!(message.contains("full history request"));
    assert!(message.contains("turn_1"));
    assert!(message.contains("NotLoaded"));
}

#[test]
fn older_history_page_requests_start_only_at_resident_boundary() {
    let page = LoadedTranscriptHistoryPage {
        turns: (0..12)
            .map(|index| turn(&format!("turn_{index}")))
            .collect(),
        older_cursor: Some("older_cursor".to_string()),
        newer_cursor: None,
    };
    let mut window = TranscriptHistoryWindow::from_latest_page(&page);
    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_leading_viewport_margins(1)
            .with_trailing_viewport_margins(1),
    );

    assert_eq!(window.begin_loading_page_for_visible_range(&(4..7)), None);
    assert_eq!(
        window.begin_loading_page_for_visible_range(&(0..2)),
        Some(TranscriptHistoryPageRequest::Older {
            cursor: "older_cursor".to_string(),
        })
    );
}

#[test]
fn index_only_desc_response_is_metadata_not_resident_payload() {
    let page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_3"), turn("turn_2"), turn("turn_1")],
        next_cursor: Some("older".to_string()),
        backwards_cursor: None,
    });

    assert_eq!(turn_ids(&page.turns), vec!["turn_1", "turn_2", "turn_3"]);
    assert!(
        page.turns
            .iter()
            .all(|turn| { turn.items_view == TurnItemsView::NotLoaded && turn.items.is_empty() })
    );

    let window = TranscriptHistoryWindow::from_latest_page(&page);
    let counts = window.residency_retained_counts();
    assert_eq!(counts.index_turns, 3);
    assert_eq!(counts.resident_turns, 0);
    assert_eq!(counts.nonresident_turns, 3);
    assert!(window.resident_turn_ids().is_empty());
}

#[test]
fn full_latest_page_is_admitted_as_resident_payload() {
    let page = loaded_history_page(vec![turn("turn_1"), turn("turn_2")], None, None);
    let mut window = TranscriptHistoryWindow::from_latest_page(&page);
    window.bind_residency_to_thread("thread_a");

    let counts = window.residency_retained_counts();
    assert_eq!(counts.index_turns, 2);
    assert_eq!(counts.resident_turns, 2);
    assert_eq!(counts.retained_item_count, 2);
    assert_eq!(window.resident_turn_ids(), vec!["turn_1", "turn_2"]);
    assert!(window.selected_thread_turn_total_is_exact());
}

#[test]
fn initial_activation_admits_tail_window_without_retaining_whole_transport_page() {
    let page = loaded_history_page(
        (0..40)
            .map(|index| turn(&format!("turn_{index}")))
            .collect(),
        Some("older_cursor"),
        None,
    );
    let resident_turn_ids = initial_thread_activation_resident_turn_ids(&page);
    let admitted_page = sanitize_loaded_page_for_resident_turn_ids(&page, resident_turn_ids);
    let window = TranscriptHistoryWindow::from_latest_page(&admitted_page);
    let counts = window.residency_retained_counts();

    assert_eq!(counts.index_turns, 40);
    assert_eq!(counts.resident_turns, 32);
    assert_eq!(counts.nonresident_turns, 8);
    assert_eq!(
        window.resident_turn_ids(),
        (8..40)
            .map(|index| format!("turn_{index}"))
            .collect::<Vec<_>>()
    );
    assert_eq!(admitted_page.turns[0].items_view, TurnItemsView::NotLoaded);
    assert!(admitted_page.turns[0].items.is_empty());
    assert_eq!(admitted_page.turns[39].items_view, TurnItemsView::Full);
    assert!(!admitted_page.turns[39].items.is_empty());
    assert!(window.has_older_pages());
}

#[test]
fn page_window_admission_sanitizes_full_response_outside_target_window() {
    let page = loaded_history_page(
        (0..10)
            .map(|index| turn(&format!("turn_{index}")))
            .collect(),
        None,
        Some("newer_cursor"),
    );
    let resident_turn_ids = resident_turn_ids_for_page_window(
        &page,
        0..1,
        96,
        TranscriptResidencyTargetPolicy::new(),
        Vec::<String>::new(),
    );
    let admitted_page = sanitize_loaded_page_for_resident_turn_ids(&page, resident_turn_ids);
    let window = TranscriptHistoryWindow::from_latest_page(&admitted_page);
    let counts = window.residency_retained_counts();

    assert_eq!(counts.index_turns, 10);
    assert_eq!(counts.resident_turns, 4);
    assert_eq!(counts.nonresident_turns, 6);
    assert_eq!(
        window.resident_turn_ids(),
        (0..4)
            .map(|index| format!("turn_{index}"))
            .collect::<Vec<_>>()
    );
    assert_eq!(admitted_page.turns[3].items_view, TurnItemsView::Full);
    assert_eq!(admitted_page.turns[4].items_view, TurnItemsView::NotLoaded);
    assert!(admitted_page.turns[4].items.is_empty());
}

#[test]
fn page_window_admission_keeps_explicit_pin_outside_visible_target() {
    let page = loaded_history_page(
        (0..10)
            .map(|index| turn(&format!("turn_{index}")))
            .collect(),
        None,
        None,
    );
    let resident_turn_ids = resident_turn_ids_for_page_window(
        &page,
        0..1,
        96,
        TranscriptResidencyTargetPolicy::new(),
        ["turn_9"],
    );
    let admitted_page = sanitize_loaded_page_for_resident_turn_ids(&page, resident_turn_ids);
    let window = TranscriptHistoryWindow::from_latest_page(&admitted_page);

    assert_eq!(
        window.resident_turn_ids(),
        vec![
            "turn_0".to_string(),
            "turn_1".to_string(),
            "turn_2".to_string(),
            "turn_3".to_string(),
            "turn_9".to_string(),
        ]
    );
    assert_eq!(admitted_page.turns[8].items_view, TurnItemsView::NotLoaded);
    assert_eq!(admitted_page.turns[9].items_view, TurnItemsView::Full);
}

#[test]
fn page_window_admission_replaces_required_oversized_turn_with_terminal_marker() {
    let huge_text = "x".repeat(2048);
    let page = loaded_history_page(vec![turn_with_text("turn_huge", &huge_text)], None, None);
    let admission_plan = turn_admission_plan_for_page_window(
        &page,
        0..1,
        96,
        TranscriptResidencyTargetPolicy::new()
            .with_leading_viewport_margins(0)
            .with_trailing_viewport_margins(0)
            .with_max_resident_bytes(256),
        Vec::<String>::new(),
    );

    assert!(admission_plan.resident_turn_ids.is_empty());
    assert_eq!(
        admission_plan.oversized_turn_fallback_ids,
        vec!["turn_huge".to_string()]
    );

    let admitted_page = sanitize_loaded_page_for_turn_admission_plan(&page, &admission_plan);
    let admitted = &admitted_page.turns[0];
    assert_eq!(admitted.id, "turn_huge");
    assert_eq!(admitted.items_view, TurnItemsView::Summary);
    assert_eq!(admitted.items.len(), 1);
    assert!(matches!(
        &admitted.items[0],
        ThreadItem::Generic(item) if item.item_type == "beryl.oversizedTurnFallback"
    ));
    assert!(!format!("{:?}", admitted.items).contains(huge_text.as_str()));

    let window = TranscriptHistoryWindow::from_latest_page(&admitted_page);
    let counts = window.residency_retained_counts();
    assert_eq!(counts.index_turns, 1);
    assert_eq!(counts.resident_turns, 0);
    assert_eq!(counts.nonresident_turns, 1);
    assert_eq!(counts.oversized_fallback_turns, 1);
    assert!(window.resident_turn_ids().is_empty());
}

#[test]
fn page_window_admission_reuses_stable_oversized_marker_after_reload() {
    let visible_turn = turn("turn_visible");
    let pinned_turn = turn_with_text("turn_pinned", &"x".repeat(2048));
    let page = loaded_history_page(vec![visible_turn, pinned_turn], None, None);
    let admission_plan = turn_admission_plan_for_page_window(
        &page,
        0..1,
        96,
        TranscriptResidencyTargetPolicy::new()
            .with_leading_viewport_margins(0)
            .with_trailing_viewport_margins(0)
            .with_max_resident_bytes(4 * 1024),
        ["turn_pinned"],
    );

    assert_eq!(
        admission_plan.resident_turn_ids,
        vec!["turn_visible".to_string()]
    );
    assert_eq!(
        admission_plan.oversized_turn_fallback_ids,
        vec!["turn_pinned".to_string()]
    );

    let first = sanitize_loaded_page_for_turn_admission_plan(&page, &admission_plan);
    let second = sanitize_loaded_page_for_turn_admission_plan(&page, &admission_plan);

    assert_eq!(first.turns[1], second.turns[1]);
    assert_eq!(first.turns[1].items_view, TurnItemsView::Summary);
    assert!(matches!(
        &first.turns[1].items[0],
        ThreadItem::Generic(item) if item.id == "beryl:oversized-turn-fallback:turn_pinned"
    ));
}

#[test]
fn residency_target_plan_does_not_rerequest_indexed_oversized_fallback() {
    let page = loaded_history_page(
        vec![turn_with_text("turn_huge", &"x".repeat(2048))],
        None,
        None,
    );
    let admission_plan = turn_admission_plan_for_page_window(
        &page,
        0..1,
        96,
        TranscriptResidencyTargetPolicy::new()
            .with_leading_viewport_margins(0)
            .with_trailing_viewport_margins(0)
            .with_max_resident_bytes(256),
        Vec::<String>::new(),
    );
    let admitted_page = sanitize_loaded_page_for_turn_admission_plan(&page, &admission_plan);
    let mut window = TranscriptHistoryWindow::from_latest_page(&admitted_page);
    window.bind_residency_to_thread("thread_a");
    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_leading_viewport_margins(0)
            .with_trailing_viewport_margins(0)
            .with_max_resident_bytes(64 * 1024),
    );

    let plan = window.residency_target_plan(
        0..1,
        96,
        Vec::<TranscriptResidencyMeasuredTurnHeight>::new(),
        None,
    );

    assert!(plan.desired_full_turn_ids.is_empty());
    assert_eq!(
        plan.oversized_turn_fallback_ids,
        vec!["turn_huge".to_string()]
    );
    assert!(plan.missing_transport_ranges.is_empty());
    assert!(plan.release_turn_ids.is_empty());
}

#[test]
fn oversized_fallback_release_bypasses_pin_preservation_for_planner_selected_turns() {
    let page = loaded_history_page(
        vec![
            turn_with_text("turn_huge", &"x".repeat(2048)),
            turn("turn_visible"),
        ],
        None,
        None,
    );
    let mut window = TranscriptHistoryWindow::from_latest_page(&page);
    window.bind_residency_to_thread("thread_a");
    window.pin_resident_turn("turn_huge", TranscriptResidencyPinKind::ActiveContextMenu);

    let released =
        window.release_resident_turns_by_id_with_oversized_fallbacks(["turn_huge"], ["turn_huge"]);

    assert_eq!(released.released_turn_ids, vec!["turn_huge"]);
    assert_eq!(window.resident_turn_ids(), vec!["turn_visible"]);
    let counts = window.residency_retained_counts();
    assert_eq!(counts.resident_turns, 1);
    assert_eq!(counts.oversized_fallback_turns, 1);

    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_leading_viewport_margins(0)
            .with_trailing_viewport_margins(0)
            .with_max_resident_bytes(4 * 1024),
    );
    let next_plan = window.residency_target_plan(
        0..1,
        96,
        Vec::<TranscriptResidencyMeasuredTurnHeight>::new(),
        None,
    );
    assert_eq!(
        next_plan.oversized_turn_fallback_ids,
        vec!["turn_huge".to_string()]
    );
    assert!(next_plan.missing_transport_ranges.is_empty());
}

#[test]
fn pinned_resident_turn_survives_release_outside_retention() {
    let page = loaded_history_page(vec![turn("turn_1"), turn("turn_2")], None, None);
    let mut window = TranscriptHistoryWindow::from_latest_page(&page);
    window.bind_residency_to_thread("thread_a");
    window.pin_resident_turn("turn_1", TranscriptResidencyPinKind::ActiveContextMenu);

    let retention = TranscriptResidencyRetention::from_turn_ids(["turn_2"]);
    let released = window.release_unretained_resident_turns(&retention);
    assert!(released.released_turn_ids.is_empty());
    assert_eq!(window.resident_turn_ids(), vec!["turn_1", "turn_2"]);

    window.unpin_resident_turn("turn_1", TranscriptResidencyPinKind::ActiveContextMenu);
    let released = window.release_unretained_resident_turns(&retention);
    assert_eq!(released.released_turn_ids, vec!["turn_1"]);
    assert_eq!(window.resident_turn_ids(), vec!["turn_2"]);
}

#[test]
fn exact_resident_turn_release_preserves_pins_and_page_index_metadata() {
    let page = loaded_history_page(
        vec![turn("turn_0"), turn("turn_1"), turn("turn_2")],
        None,
        None,
    );
    let mut window = TranscriptHistoryWindow::from_latest_page(&page);
    window.bind_residency_to_thread("thread_a");
    window.pin_resident_turn("turn_1", TranscriptResidencyPinKind::ActiveContextMenu);

    let released = window.release_resident_turns_by_id(["turn_0", "turn_1"]);

    assert_eq!(released.released_turn_ids, vec!["turn_0"]);
    assert_eq!(window.resident_turn_ids(), vec!["turn_1", "turn_2"]);
    assert_eq!(window.resident_page_count(), 1);
    assert_eq!(window.indexed_turns().len(), 3);

    window.unpin_resident_turn("turn_1", TranscriptResidencyPinKind::ActiveContextMenu);
    let released = window.release_resident_turns_by_id(["turn_1"]);

    assert_eq!(released.released_turn_ids, vec!["turn_1"]);
    assert_eq!(window.resident_turn_ids(), vec!["turn_2"]);
    assert_eq!(window.resident_page_count(), 1);
    assert_eq!(window.indexed_turns().len(), 3);
}

#[test]
fn policy_budget_shrink_releases_unpinned_resident_turns() {
    let page = loaded_history_page(
        vec![turn("turn_1"), turn("turn_2"), turn("turn_3")],
        None,
        None,
    );
    let mut window = TranscriptHistoryWindow::from_latest_page(&page);
    window.bind_residency_to_thread("thread_a");

    window.set_residency_policy(TranscriptResidencyPolicy::new().with_max_resident_turns(1));

    let counts = window.residency_retained_counts();
    assert_eq!(counts.resident_turns, 1);
    assert_eq!(window.resident_turn_ids(), vec!["turn_3"]);
}

#[test]
fn residency_policy_records_elastic_loading_knobs_without_scheduler_side_effects() {
    let mut window = indexed_window(["turn_1", "turn_2", "turn_3"]);
    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_max_in_flight_requests(3)
            .with_leading_viewport_margins(2)
            .with_trailing_viewport_margins(2)
            .with_request_priority(TranscriptResidencyRequestPriority::NewestFirst),
    );

    let counts = window.residency_retained_counts();

    assert_eq!(counts.index_turns, 3);
    assert_eq!(counts.resident_turns, 0);
    assert_eq!(counts.nonresident_turns, 3);
    assert_eq!(counts.max_in_flight_requests, 3);
    assert_eq!(counts.leading_viewport_margins, 2);
    assert_eq!(counts.trailing_viewport_margins, 2);
    assert_eq!(
        counts.request_priority,
        TranscriptResidencyRequestPriority::NewestFirst
    );
}

#[test]
fn residency_target_planner_expands_window_by_viewport_height_margins() {
    let plan = plan_transcript_residency_target(
        TranscriptResidencyTargetInput::new(
            TranscriptResidencyViewport::new(3..4, 100),
            planner_turns(7, 100, 10, false),
        )
        .with_policy(
            TranscriptResidencyTargetPolicy::new()
                .with_leading_viewport_margins(2)
                .with_trailing_viewport_margins(2),
        ),
    );

    assert_eq!(
        plan.desired_full_turn_ids,
        vec!["turn_3", "turn_2", "turn_4", "turn_1", "turn_5"]
    );
    assert!(plan.diagnostics.viewport_margin_satisfied);
    assert_eq!(plan.missing_transport_ranges, vec![1..6]);
}

#[test]
fn residency_target_planner_uses_streamed_fill_to_stop_before_previous_turns() {
    let mut turns = planner_turns(7, 100, 10, false);
    turns[5] = turns[5]
        .clone()
        .with_streamed_margin_satisfaction(true, false);
    let streamed = plan_transcript_residency_target(
        TranscriptResidencyTargetInput::new(TranscriptResidencyViewport::new(5..6, 100), turns)
            .with_policy(
                TranscriptResidencyTargetPolicy::new()
                    .with_leading_viewport_margins(3)
                    .with_trailing_viewport_margins(0),
            ),
    );

    assert_eq!(streamed.desired_full_turn_ids, vec!["turn_5"]);
    assert_eq!(streamed.missing_transport_ranges, vec![5..6]);
    assert!(streamed.diagnostics.viewport_margin_satisfied);
}

#[test]
fn residency_target_planner_loads_previous_turns_without_streamed_fill() {
    let plan = plan_transcript_residency_target(
        TranscriptResidencyTargetInput::new(
            TranscriptResidencyViewport::new(5..6, 100),
            planner_turns(7, 100, 10, false),
        )
        .with_policy(
            TranscriptResidencyTargetPolicy::new()
                .with_leading_viewport_margins(3)
                .with_trailing_viewport_margins(0),
        ),
    );

    assert_eq!(
        plan.desired_full_turn_ids,
        vec!["turn_5", "turn_4", "turn_3", "turn_2"]
    );
    assert_eq!(plan.missing_transport_ranges, vec![2..6]);
}

#[test]
fn residency_target_planner_retains_loaded_turns_outside_viewport_when_under_budget() {
    let plan = plan_transcript_residency_target(
        TranscriptResidencyTargetInput::new(
            TranscriptResidencyViewport::new(2..3, 100),
            planner_turns(5, 100, 10, true),
        )
        .with_policy(
            TranscriptResidencyTargetPolicy::new()
                .with_leading_viewport_margins(0)
                .with_trailing_viewport_margins(0),
        ),
    );

    assert_eq!(plan.desired_full_turn_ids, vec!["turn_2"]);
    assert!(plan.release_turn_ids.is_empty());
    assert_eq!(
        plan.diagnostics.limiting_reason,
        TranscriptResidencyBudgetReason::None
    );
}

#[test]
fn residency_target_planner_uses_missing_measurement_fallback_and_required_priority() {
    let plan = plan_transcript_residency_target(
        TranscriptResidencyTargetInput::new(
            TranscriptResidencyViewport::new(2..3, 50),
            (0..6)
                .map(|index| {
                    TranscriptResidencyTurnPlanInput::new(format!("turn_{index}"))
                        .with_source_position(index)
                        .with_estimated_resident_bytes(10)
                })
                .collect(),
        )
        .with_active_turn_id("turn_5")
        .with_pinned_turn_ids(["turn_0", "turn_2"])
        .with_policy(
            TranscriptResidencyTargetPolicy::new()
                .with_default_row_height(50)
                .with_leading_viewport_margins(1)
                .with_trailing_viewport_margins(1),
        ),
    );

    assert_eq!(
        plan.desired_full_turn_ids,
        vec!["turn_2", "turn_5", "turn_0", "turn_1", "turn_3"]
    );
    assert!(plan.diagnostics.viewport_margin_satisfied);
}

#[test]
fn residency_target_planner_shrinks_and_grows_optional_margin_under_byte_budget() {
    let constrained = plan_transcript_residency_target(
        TranscriptResidencyTargetInput::new(
            TranscriptResidencyViewport::new(2..3, 100),
            planner_turns(5, 100, 100, false),
        )
        .with_policy(
            TranscriptResidencyTargetPolicy::new()
                .with_max_resident_bytes(300)
                .with_leading_viewport_margins(2)
                .with_trailing_viewport_margins(2),
        ),
    );

    assert_eq!(
        constrained.desired_full_turn_ids,
        vec!["turn_2", "turn_1", "turn_3"]
    );
    assert!(!constrained.diagnostics.viewport_margin_satisfied);
    assert!(constrained.diagnostics.resident_byte_limit);
    assert_eq!(
        constrained.diagnostics.limiting_reason,
        TranscriptResidencyBudgetReason::ResidentByteLimit
    );

    let grown = plan_transcript_residency_target(
        TranscriptResidencyTargetInput::new(
            TranscriptResidencyViewport::new(2..3, 100),
            planner_turns(5, 100, 100, false),
        )
        .with_policy(
            TranscriptResidencyTargetPolicy::new()
                .with_max_resident_bytes(500)
                .with_leading_viewport_margins(2)
                .with_trailing_viewport_margins(2),
        ),
    );

    assert_eq!(
        grown.desired_full_turn_ids,
        vec!["turn_2", "turn_1", "turn_3", "turn_0", "turn_4"]
    );
    assert!(grown.diagnostics.viewport_margin_satisfied);
    assert!(!grown.diagnostics.resident_byte_limit);
}

#[test]
fn residency_target_planner_reports_required_oversized_fallback_without_retaining_payload() {
    let plan = plan_transcript_residency_target(
        TranscriptResidencyTargetInput::new(
            TranscriptResidencyViewport::new(1..2, 100),
            vec![
                planner_turn("turn_0", 0, 100, 10, false),
                planner_turn("turn_1", 1, 100, 501, true),
                planner_turn("turn_2", 2, 100, 10, false),
            ],
        )
        .with_policy(TranscriptResidencyTargetPolicy::new().with_max_resident_bytes(500)),
    );

    assert_eq!(plan.desired_full_turn_ids, vec!["turn_0", "turn_2"]);
    assert_eq!(plan.oversized_turn_fallback_ids, vec!["turn_1"]);
    assert_eq!(plan.release_turn_ids, vec!["turn_1"]);
    assert!(plan.diagnostics.oversized_turn_fallback);
    assert_eq!(
        plan.diagnostics.limiting_reason,
        TranscriptResidencyBudgetReason::OversizedTurnFallback
    );
}

#[test]
fn residency_target_planner_keeps_oversized_active_turn_out_of_history_release() {
    let plan = plan_transcript_residency_target(
        TranscriptResidencyTargetInput::new(
            TranscriptResidencyViewport::new(0..1, 100),
            vec![
                planner_turn("turn_active", 0, 100, 900, true),
                planner_turn("turn_1", 1, 100, 10, false),
            ],
        )
        .with_active_turn_id("turn_active")
        .with_policy(TranscriptResidencyTargetPolicy::new().with_max_resident_bytes(100)),
    );

    assert!(
        !plan
            .desired_full_turn_ids
            .iter()
            .any(|turn_id| turn_id == "turn_active")
    );
    assert!(plan.oversized_turn_fallback_ids.is_empty());
    assert!(plan.release_turn_ids.is_empty());
    assert!(plan.diagnostics.viewport_margin_satisfied);
    assert!(plan.diagnostics.resident_byte_limit);
    assert_eq!(
        plan.diagnostics.limiting_reason,
        TranscriptResidencyBudgetReason::PinnedResidentOverBudget
    );
}

#[test]
fn residency_target_planner_reports_active_over_budget_when_other_turn_falls_back() {
    let plan = plan_transcript_residency_target(
        TranscriptResidencyTargetInput::new(
            TranscriptResidencyViewport::new(0..1, 100),
            vec![
                planner_turn("turn_active", 0, 100, 900, true),
                planner_turn("turn_huge", 1, 100, 900, true),
            ],
        )
        .with_active_turn_id("turn_active")
        .with_pinned_turn_ids(["turn_huge"])
        .with_policy(TranscriptResidencyTargetPolicy::new().with_max_resident_bytes(100)),
    );

    assert!(plan.desired_full_turn_ids.is_empty());
    assert_eq!(
        plan.oversized_turn_fallback_ids,
        vec!["turn_huge".to_string()]
    );
    assert_eq!(plan.release_turn_ids, vec!["turn_huge".to_string()]);
    assert!(plan.diagnostics.oversized_turn_fallback);
    assert!(plan.diagnostics.resident_byte_limit);
    assert_eq!(
        plan.diagnostics.limiting_reason,
        TranscriptResidencyBudgetReason::PinnedResidentOverBudget
    );
}

#[test]
fn residency_target_planner_keeps_visible_turn_and_falls_back_huge_offscreen_pin() {
    let plan = plan_transcript_residency_target(
        TranscriptResidencyTargetInput::new(
            TranscriptResidencyViewport::new(1..2, 100),
            vec![
                planner_turn("turn_0", 0, 100, 900, true),
                planner_turn("turn_1", 1, 100, 10, false),
                planner_turn("turn_2", 2, 100, 10, false),
            ],
        )
        .with_pinned_turn_ids(["turn_0"])
        .with_policy(TranscriptResidencyTargetPolicy::new().with_max_resident_bytes(100)),
    );

    assert_eq!(plan.desired_full_turn_ids, vec!["turn_1", "turn_2"]);
    assert_eq!(plan.oversized_turn_fallback_ids, vec!["turn_0"]);
    assert_eq!(plan.release_turn_ids, vec!["turn_0"]);
    assert!(plan.diagnostics.oversized_turn_fallback);
}

#[test]
fn residency_target_planner_suppresses_transport_ranges_at_in_flight_limit() {
    let plan = plan_transcript_residency_target(
        TranscriptResidencyTargetInput::new(
            TranscriptResidencyViewport::new(1..2, 100),
            planner_turns(3, 100, 10, false),
        )
        .with_in_flight_requests(1)
        .with_policy(
            TranscriptResidencyTargetPolicy::new()
                .with_max_in_flight_requests(1)
                .with_leading_viewport_margins(1)
                .with_trailing_viewport_margins(1),
        ),
    );

    assert!(plan.missing_transport_ranges.is_empty());
    assert!(plan.diagnostics.in_flight_limit);
    assert_eq!(
        plan.diagnostics.limiting_reason,
        TranscriptResidencyBudgetReason::InFlightRequestLimit
    );
}

#[test]
fn residency_target_planner_growth_strategy_can_saturate_budget_beyond_fixed_margins() {
    let fixed = plan_transcript_residency_target(
        TranscriptResidencyTargetInput::new(
            TranscriptResidencyViewport::new(2..3, 100),
            planner_turns(6, 100, 10, false),
        )
        .with_policy(
            TranscriptResidencyTargetPolicy::new()
                .with_leading_viewport_margins(0)
                .with_trailing_viewport_margins(0),
        ),
    );
    let saturating = plan_transcript_residency_target(
        TranscriptResidencyTargetInput::new(
            TranscriptResidencyViewport::new(2..3, 100),
            planner_turns(6, 100, 10, false),
        )
        .with_policy(
            TranscriptResidencyTargetPolicy::new()
                .with_leading_viewport_margins(0)
                .with_trailing_viewport_margins(0)
                .with_max_resident_turns(3)
                .with_growth_strategy(TranscriptResidencyGrowthStrategy::SaturateBudget),
        ),
    );

    assert_eq!(fixed.desired_full_turn_ids, vec!["turn_2"]);
    assert_eq!(
        saturating.desired_full_turn_ids,
        vec!["turn_2", "turn_1", "turn_3"]
    );
}

#[test]
fn history_window_residency_target_plan_moves_with_viewport_facts() {
    let mut window = indexed_window(["turn_0", "turn_1", "turn_2", "turn_3", "turn_4", "turn_5"]);
    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_leading_viewport_margins(1)
            .with_trailing_viewport_margins(1),
    );
    let measured = (0..6)
        .map(|source_position| TranscriptResidencyMeasuredTurnHeight {
            source_position,
            measured_height: 50,
        })
        .collect::<Vec<_>>();

    let first = window.residency_target_plan(2..3, 50, measured.clone(), None);
    let second = window.residency_target_plan(4..5, 50, measured, None);

    assert_eq!(
        first.desired_full_turn_ids,
        vec!["turn_2", "turn_1", "turn_3"]
    );
    assert_eq!(first.missing_transport_ranges, vec![1..4]);
    assert_eq!(
        second.desired_full_turn_ids,
        vec!["turn_4", "turn_3", "turn_5"]
    );
    assert_eq!(second.missing_transport_ranges, vec![3..6]);
}

#[test]
fn history_window_bounded_residency_target_plan_uses_source_window_and_required_turns() {
    let mut window = indexed_window([
        "turn_0", "turn_1", "turn_2", "turn_3", "turn_4", "turn_5", "turn_6",
    ]);
    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_leading_viewport_margins(0)
            .with_trailing_viewport_margins(0),
    );

    let bounded = window.residency_target_plan_for_source_window(
        5..6,
        5..6,
        96,
        Vec::<TranscriptResidencyMeasuredTurnHeight>::new(),
        None,
    );

    assert_eq!(bounded.desired_full_turn_ids, vec!["turn_5"]);
    assert_eq!(bounded.missing_transport_ranges, vec![5..6]);

    window.pin_resident_turn("turn_1", TranscriptResidencyPinKind::ActiveContextMenu);
    let with_pin = window.residency_target_plan_for_source_window(
        5..6,
        5..6,
        96,
        Vec::<TranscriptResidencyMeasuredTurnHeight>::new(),
        None,
    );

    assert_eq!(with_pin.desired_full_turn_ids, vec!["turn_5", "turn_1"]);
    assert_eq!(with_pin.missing_transport_ranges, vec![1..2, 5..6]);
}

#[test]
fn history_window_activation_tail_source_planning_reaches_indexed_previous_turns() {
    let page = loaded_history_page(
        (0..10)
            .map(|index| {
                let view = if index == 9 {
                    TurnItemsView::Full
                } else {
                    TurnItemsView::NotLoaded
                };
                turn_with_items_view(&format!("turn_{index}"), view)
            })
            .collect(),
        Some("older_cursor"),
        None,
    );
    let mut window = TranscriptHistoryWindow::from_latest_page(&page);
    window.bind_residency_to_thread("thread_a");
    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_leading_viewport_margins(2)
            .with_trailing_viewport_margins(0),
    );

    let source_planning_range = window.source_planning_range_for_visible_range(9..10, 96);
    let plan = window.residency_target_plan_for_source_window(
        9..10,
        source_planning_range,
        96,
        [TranscriptResidencyMeasuredTurnHeight {
            source_position: 9,
            measured_height: 48,
        }],
        None,
    );

    assert_eq!(
        plan.desired_full_turn_ids,
        vec!["turn_9", "turn_8", "turn_7"]
    );
    assert_eq!(plan.missing_transport_ranges, vec![7..9]);
}

#[test]
fn history_window_streamed_tail_fill_suppresses_indexed_previous_turn_request() {
    let page = loaded_history_page(
        (0..10)
            .map(|index| {
                let view = if index == 9 {
                    TurnItemsView::Full
                } else {
                    TurnItemsView::NotLoaded
                };
                turn_with_items_view(&format!("turn_{index}"), view)
            })
            .collect(),
        Some("older_cursor"),
        None,
    );
    let mut window = TranscriptHistoryWindow::from_latest_page(&page);
    window.bind_residency_to_thread("thread_a");
    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_leading_viewport_margins(2)
            .with_trailing_viewport_margins(0),
    );

    let source_planning_range = window.source_planning_range_for_visible_range(9..10, 96);
    let plan = window.residency_target_plan_for_source_window_with_streamed_fill(
        9..10,
        source_planning_range,
        96,
        Vec::<TranscriptResidencyMeasuredTurnHeight>::new(),
        [TranscriptResidencyStreamedTurnFill {
            source_position: 9,
            leading_margin_satisfied: true,
            trailing_margin_satisfied: false,
        }],
        None,
    );

    assert_eq!(plan.desired_full_turn_ids, vec!["turn_9"]);
    assert!(plan.missing_transport_ranges.is_empty());
    assert!(plan.diagnostics.viewport_margin_satisfied);
}

#[test]
fn history_window_bounded_residency_target_plan_retains_residents_outside_source_window() {
    let page = loaded_history_page(
        (0..5).map(|index| turn(&format!("turn_{index}"))).collect(),
        None,
        None,
    );
    let mut window = TranscriptHistoryWindow::from_latest_page(&page);
    window.bind_residency_to_thread("thread_a");
    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_leading_viewport_margins(1)
            .with_trailing_viewport_margins(1),
    );

    let plan = window.residency_target_plan_for_source_window(
        2..3,
        2..3,
        96,
        Vec::<TranscriptResidencyMeasuredTurnHeight>::new(),
        None,
    );

    assert_eq!(plan.desired_full_turn_ids, vec!["turn_2"]);
    assert!(plan.release_turn_ids.is_empty());
}

#[test]
fn history_window_bounded_residency_target_plan_releases_global_byte_budget_excess() {
    let page = loaded_history_page(
        (0..5).map(|index| turn(&format!("turn_{index}"))).collect(),
        None,
        None,
    );
    let mut window = TranscriptHistoryWindow::from_latest_page(&page);
    window.bind_residency_to_thread("thread_a");
    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_max_resident_bytes(50_000)
            .with_leading_viewport_margins(0)
            .with_trailing_viewport_margins(0),
    );
    assert!(window.update_residency_derived_byte_estimates([("turn_0", 1_000_000usize)]));
    assert_eq!(
        window.residency_retained_counts().budget_reason,
        TranscriptResidencyBudgetReason::ResidentByteLimit
    );

    let plan = window.residency_target_plan_for_source_window(
        4..5,
        4..5,
        96,
        Vec::<TranscriptResidencyMeasuredTurnHeight>::new(),
        None,
    );

    assert_eq!(plan.desired_full_turn_ids, vec!["turn_4"]);
    assert_eq!(plan.release_turn_ids, vec!["turn_0"]);
    assert!(plan.diagnostics.resident_byte_limit);
    assert_eq!(
        plan.diagnostics.limiting_reason,
        TranscriptResidencyBudgetReason::ResidentByteLimit
    );
}

#[test]
fn history_window_bounded_residency_target_plan_releases_global_turn_budget_excess() {
    let latest = loaded_history_page(
        vec![turn("turn_2"), turn("turn_3")],
        Some("older_cursor"),
        None,
    );
    let older = loaded_history_page(vec![turn("turn_0"), turn("turn_1")], None, Some("newer"));
    let mut window = TranscriptHistoryWindow::from_latest_page(&latest);
    window.bind_residency_to_thread("thread_a");
    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_max_resident_turns(3)
            .with_leading_viewport_margins(0)
            .with_trailing_viewport_margins(0),
    );
    assert_eq!(window.resident_turn_ids(), vec!["turn_2", "turn_3"]);
    window.finish_loading_older_with_turn_ids(
        &older,
        vec!["turn_0".to_string(), "turn_1".to_string()],
    );
    assert_eq!(
        window.residency_retained_counts().budget_reason,
        TranscriptResidencyBudgetReason::ResidentTurnLimit
    );

    let plan = window.residency_target_plan_for_source_window(
        3..4,
        3..4,
        96,
        Vec::<TranscriptResidencyMeasuredTurnHeight>::new(),
        None,
    );

    assert_eq!(plan.desired_full_turn_ids, vec!["turn_3"]);
    assert_eq!(plan.release_turn_ids, vec!["turn_0"]);
    assert!(plan.diagnostics.resident_turn_limit);
    assert_eq!(
        plan.diagnostics.limiting_reason,
        TranscriptResidencyBudgetReason::ResidentTurnLimit
    );
}

#[test]
fn history_window_residency_target_plan_does_not_release_under_budget() {
    let page = loaded_history_page(
        (0..5).map(|index| turn(&format!("turn_{index}"))).collect(),
        None,
        None,
    );
    let mut window = TranscriptHistoryWindow::from_latest_page(&page);
    window.bind_residency_to_thread("thread_a");
    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_leading_viewport_margins(0)
            .with_trailing_viewport_margins(0),
    );

    let plan = window.residency_target_plan(
        2..3,
        96,
        Vec::<TranscriptResidencyMeasuredTurnHeight>::new(),
        None,
    );

    assert_eq!(plan.desired_full_turn_ids, vec!["turn_2"]);
    assert!(plan.release_turn_ids.is_empty());
}

#[test]
fn planner_missing_ranges_restore_released_page_before_requesting_older() {
    let latest = loaded_history_page(
        vec![turn("turn_2"), turn("turn_3")],
        Some("older_cursor"),
        None,
    );
    let older = loaded_history_page(vec![turn("turn_0"), turn("turn_1")], None, Some("newer"));
    let mut window = TranscriptHistoryWindow::from_latest_page(&latest);
    window.bind_residency_to_thread("thread_a");
    assert_eq!(
        window.begin_loading_older().as_deref(),
        Some("older_cursor")
    );
    window.finish_loading_older_with_turn_ids(
        &older,
        vec!["turn_0".to_string(), "turn_1".to_string()],
    );
    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_max_resident_pages(1)
            .with_leading_viewport_margins(0)
            .with_trailing_viewport_margins(0)
            .with_cold_release_hysteresis_viewports(0)
            .with_minimum_restore_margin_rows(0),
    );
    let releases = window.release_cold_pages(&(2..4));
    let page_id = releases[0].page_id;
    let plan = window.residency_target_plan(
        0..1,
        96,
        Vec::<TranscriptResidencyMeasuredTurnHeight>::new(),
        None,
    );

    assert_eq!(plan.missing_transport_ranges, vec![0..1]);
    assert_eq!(
        window.begin_loading_page_for_residency_target_plan(&plan, &(0..1)),
        Some(TranscriptHistoryPageRequest::Released {
            page_id,
            cursor: Some("older_cursor".to_string()),
        })
    );
}

#[test]
fn planner_missing_ranges_reload_partially_resident_indexed_page() {
    let page = loaded_history_page(
        (0..6).map(|index| turn(&format!("turn_{index}"))).collect(),
        Some("older_cursor"),
        None,
    );
    let admission_plan = turn_admission_plan_for_page_window(
        &page,
        5..6,
        96,
        TranscriptResidencyTargetPolicy::new()
            .with_leading_viewport_margins(0)
            .with_trailing_viewport_margins(0),
        Vec::<String>::new(),
    );
    let admitted_page = sanitize_loaded_page_for_turn_admission_plan(&page, &admission_plan);
    let mut window = TranscriptHistoryWindow::from_latest_page(&admitted_page);
    window.bind_residency_to_thread("thread_a");
    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_leading_viewport_margins(0)
            .with_trailing_viewport_margins(0),
    );
    let plan = window.residency_target_plan(
        2..3,
        96,
        Vec::<TranscriptResidencyMeasuredTurnHeight>::new(),
        None,
    );

    assert_eq!(window.resident_turn_ids(), vec!["turn_5"]);
    assert_eq!(plan.missing_transport_ranges, vec![2..3]);
    assert!(matches!(
        window.begin_loading_page_for_residency_target_plan(&plan, &(2..3)),
        Some(TranscriptHistoryPageRequest::Indexed { cursor: None, .. })
    ));
}

#[test]
fn page_residency_allows_one_history_page_request_in_flight() {
    let page = loaded_history_page(
        vec![turn("turn_2"), turn("turn_3")],
        Some("older_cursor"),
        None,
    );
    let mut window = TranscriptHistoryWindow::from_latest_page(&page);
    window.bind_residency_to_thread("thread_a");

    assert_eq!(
        window.begin_loading_page_for_visible_range(&(0..1)),
        Some(TranscriptHistoryPageRequest::Older {
            cursor: "older_cursor".to_string(),
        })
    );
    assert_eq!(window.retained_counts().loading_pages, 1);
    let counts = window.residency_retained_counts();
    assert_eq!(counts.in_flight_requests, 1);
    assert_eq!(
        counts.budget_reason,
        TranscriptResidencyBudgetReason::InFlightRequestLimit
    );

    assert_eq!(window.begin_loading_page_for_visible_range(&(0..1)), None);

    window.fail_loading_older();
    assert_eq!(window.residency_retained_counts().in_flight_requests, 0);
    assert_eq!(
        window.begin_loading_page_for_visible_range(&(0..1)),
        Some(TranscriptHistoryPageRequest::Older {
            cursor: "older_cursor".to_string(),
        })
    );
}

#[test]
fn history_page_request_identity_matches_only_active_loading_page() {
    let latest = loaded_history_page(
        vec![turn("turn_2"), turn("turn_3")],
        Some("older_cursor"),
        None,
    );
    let older = loaded_history_page(vec![turn("turn_0"), turn("turn_1")], None, Some("newer"));
    let mut window = TranscriptHistoryWindow::from_latest_page(&latest);
    window.bind_residency_to_thread("thread_a");

    let older_request = window
        .begin_loading_page_for_visible_range(&(0..1))
        .expect("older page should start loading");
    assert!(window.loading_page_matches_request(&older_request));
    assert!(
        !window.loading_page_matches_request(&TranscriptHistoryPageRequest::Older {
            cursor: "different_cursor".to_string(),
        })
    );
    window.finish_loading_older_with_turn_ids(
        &older,
        vec!["turn_0".to_string(), "turn_1".to_string()],
    );
    assert!(!window.loading_page_matches_request(&older_request));

    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_max_resident_pages(1)
            .with_leading_viewport_margins(0)
            .with_trailing_viewport_margins(0)
            .with_cold_release_hysteresis_viewports(0)
            .with_minimum_restore_margin_rows(0),
    );
    let releases = window.release_cold_pages(&(2..4));
    let released_request = window
        .begin_loading_page_for_visible_range(&(0..1))
        .expect("released page should start restoring");
    assert!(window.loading_page_matches_request(&released_request));
    assert!(
        !window.loading_page_matches_request(&TranscriptHistoryPageRequest::Released {
            page_id: releases[0].page_id,
            cursor: Some("mismatched cursor must not match".to_string()),
        })
    );
    window.fail_loading_older();
    assert!(!window.loading_page_matches_request(&released_request));
}

#[test]
fn pinned_resident_turn_can_exceed_byte_budget_with_diagnostic_reason() {
    let page = loaded_history_page(
        vec![turn_with_text("turn_1", &"x".repeat(4096)), turn("turn_2")],
        None,
        None,
    );
    let mut window = TranscriptHistoryWindow::from_latest_page(&page);
    window.bind_residency_to_thread("thread_a");
    window.pin_resident_turn("turn_1", TranscriptResidencyPinKind::ActiveContextMenu);

    window.set_residency_policy(TranscriptResidencyPolicy::new().with_max_resident_bytes(0));

    let counts = window.residency_retained_counts();
    assert_eq!(window.resident_turn_ids(), vec!["turn_1"]);
    assert_eq!(counts.resident_turns, 1);
    assert_eq!(
        counts.budget_reason,
        TranscriptResidencyBudgetReason::PinnedResidentOverBudget
    );
}

#[test]
fn residency_retained_counts_split_payload_and_derived_bytes() {
    let page = loaded_history_page(
        vec![turn_with_text("turn_1", "plain **markdown**")],
        None,
        None,
    );
    let mut window = TranscriptHistoryWindow::from_latest_page(&page);
    window.bind_residency_to_thread("thread_a");
    let initial = window.residency_retained_counts();

    assert!(initial.resident_payload_bytes > 0);
    assert!(initial.resident_derived_bytes > 0);
    assert_eq!(
        initial.resident_bytes,
        initial
            .resident_payload_bytes
            .saturating_add(initial.resident_derived_bytes)
    );

    assert!(window.update_residency_derived_byte_estimates([("turn_1", 777usize)]));
    let updated = window.residency_retained_counts();
    assert_eq!(updated.resident_derived_bytes, 777);
    assert_eq!(
        updated.resident_bytes,
        updated
            .resident_payload_bytes
            .saturating_add(updated.resident_derived_bytes)
    );
}

#[test]
fn residency_target_plan_uses_updated_derived_byte_estimates() {
    let page = loaded_history_page(
        vec![turn("turn_0"), turn("turn_1"), turn("turn_2")],
        None,
        None,
    );
    let mut window = TranscriptHistoryWindow::from_latest_page(&page);
    window.bind_residency_to_thread("thread_a");
    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_max_resident_bytes(50_000)
            .with_leading_viewport_margins(0)
            .with_trailing_viewport_margins(3),
    );

    assert!(window.update_residency_derived_byte_estimates([("turn_1", 1_000_000usize)]));
    let plan = window.residency_target_plan(0..1, 96, Vec::new(), None);

    assert!(plan.desired_full_turn_ids.contains(&"turn_0".to_string()));
    assert!(plan.release_turn_ids.contains(&"turn_1".to_string()));
    assert_eq!(
        plan.diagnostics.limiting_reason,
        TranscriptResidencyBudgetReason::ResidentByteLimit
    );
}

#[test]
fn saved_path_generated_image_estimate_does_not_count_inline_result_bytes() {
    let huge_result = "x".repeat(2_000_000);
    let page = loaded_history_page(
        vec![turn_with_items(
            "turn_image",
            vec![generated_image_item(
                "image_1",
                "completed",
                Some(huge_result.as_str()),
                Some(r"C:\work\generated\image_1.png"),
            )],
        )],
        None,
        None,
    );

    let admission_plan = turn_admission_plan_for_page_window(
        &page,
        0..1,
        96,
        TranscriptResidencyTargetPolicy::new()
            .with_leading_viewport_margins(0)
            .with_trailing_viewport_margins(0)
            .with_max_resident_bytes(1_500_000),
        Vec::<String>::new(),
    );

    assert_eq!(
        admission_plan.resident_turn_ids,
        vec!["turn_image".to_string()]
    );
    assert!(admission_plan.oversized_turn_fallback_ids.is_empty());
}

#[test]
fn policy_release_hysteresis_keeps_nearby_resident_pages_until_margin_tightens() {
    let latest = loaded_history_page(
        vec![turn("turn_2"), turn("turn_3")],
        Some("older_cursor"),
        None,
    );
    let older = loaded_history_page(vec![turn("turn_0"), turn("turn_1")], None, Some("newer"));
    let mut window = TranscriptHistoryWindow::from_latest_page(&latest);
    window.bind_residency_to_thread("thread_a");
    assert_eq!(
        window.begin_loading_older().as_deref(),
        Some("older_cursor")
    );
    window.finish_loading_older_with_turn_ids(
        &older,
        vec!["turn_0".to_string(), "turn_1".to_string()],
    );
    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_max_resident_pages(1)
            .with_cold_release_hysteresis_viewports(1),
    );

    assert!(window.release_cold_pages(&(1..3)).is_empty());

    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_max_resident_pages(1)
            .with_leading_viewport_margins(0)
            .with_trailing_viewport_margins(0)
            .with_cold_release_hysteresis_viewports(0)
            .with_minimum_restore_margin_rows(0),
    );
    let releases = window.release_cold_pages(&(2..4));

    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].range, 0..2);
    assert_eq!(window.resident_page_count(), 1);
}

#[test]
fn released_history_page_restores_full_resident_turns_without_placeholder_rows() {
    let latest = loaded_history_page(
        vec![turn("turn_2"), turn("turn_3")],
        Some("older_cursor"),
        None,
    );
    let older = loaded_history_page(vec![turn("turn_0"), turn("turn_1")], None, Some("newer"));
    let mut window = TranscriptHistoryWindow::from_latest_page(&latest);
    window.bind_residency_to_thread("thread_a");
    assert_eq!(
        window.begin_loading_older().as_deref(),
        Some("older_cursor")
    );
    window.finish_loading_older_with_turn_ids(
        &older,
        vec!["turn_0".to_string(), "turn_1".to_string()],
    );
    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_max_resident_pages(1)
            .with_leading_viewport_margins(0)
            .with_trailing_viewport_margins(0)
            .with_cold_release_hysteresis_viewports(0)
            .with_minimum_restore_margin_rows(0),
    );
    let releases = window.release_cold_pages(&(2..4));
    let page_id = releases[0].page_id;

    assert_eq!(window.resident_turn_ids(), vec!["turn_2", "turn_3"]);
    assert_eq!(
        window.begin_loading_page_for_visible_range(&(0..1)),
        Some(TranscriptHistoryPageRequest::Released {
            page_id,
            cursor: Some("older_cursor".to_string()),
        })
    );
    let restored = window
        .finish_loading_released_page(page_id, &older)
        .expect("released page should be restored by id");

    assert_eq!(restored.range, 0..2);
    assert_eq!(restored.turn_ids, vec!["turn_0", "turn_1"]);
    assert_eq!(
        window.resident_turn_ids(),
        vec!["turn_0", "turn_1", "turn_2", "turn_3"]
    );
    assert_eq!(window.residency_retained_counts().nonresident_turns, 0);
}

#[test]
fn residency_stats_track_release_restore_pins_and_reset() {
    let latest = loaded_history_page(
        vec![turn("turn_2"), turn("turn_3")],
        Some("older_cursor"),
        None,
    );
    let older = loaded_history_page(vec![turn("turn_0"), turn("turn_1")], None, Some("newer"));
    let mut window = TranscriptHistoryWindow::from_latest_page(&latest);
    window.bind_residency_to_thread("thread_a");
    assert_eq!(
        window.begin_loading_older().as_deref(),
        Some("older_cursor")
    );
    window.finish_loading_older_with_turn_ids(
        &older,
        vec!["turn_0".to_string(), "turn_1".to_string()],
    );

    let counts = window.residency_retained_counts();
    assert_eq!(counts.index_turns, 4);
    assert_eq!(counts.resident_turns, 4);
    assert_eq!(counts.nonresident_turns, 0);
    assert_eq!(counts.retained_item_count, 4);
    assert_eq!(counts.in_flight_requests, 0);

    window.pin_resident_turn("turn_0", TranscriptResidencyPinKind::ActiveContextMenu);
    assert_eq!(window.residency_retained_counts().pinned_turns, 1);
    window.unpin_resident_turn("turn_0", TranscriptResidencyPinKind::ActiveContextMenu);
    assert_eq!(window.residency_retained_counts().pinned_turns, 0);

    window.set_residency_policy(
        TranscriptResidencyPolicy::new()
            .with_max_resident_pages(1)
            .with_leading_viewport_margins(0)
            .with_trailing_viewport_margins(0)
            .with_cold_release_hysteresis_viewports(0)
            .with_minimum_restore_margin_rows(0),
    );
    let releases = window.release_cold_pages(&(2..4));
    let page_id = releases[0].page_id;
    let counts = window.residency_retained_counts();
    assert_eq!(counts.index_turns, 4);
    assert_eq!(counts.resident_turns, 2);
    assert_eq!(counts.nonresident_turns, 2);
    assert_eq!(counts.retained_item_count, 2);

    assert_eq!(
        window.begin_loading_page_for_visible_range(&(0..1)),
        Some(TranscriptHistoryPageRequest::Released {
            page_id,
            cursor: Some("older_cursor".to_string()),
        })
    );
    assert_eq!(window.residency_retained_counts().in_flight_requests, 1);
    window
        .finish_loading_released_page(page_id, &older)
        .expect("released page should restore");
    let counts = window.residency_retained_counts();
    assert_eq!(counts.index_turns, 4);
    assert_eq!(counts.resident_turns, 4);
    assert_eq!(counts.nonresident_turns, 0);
    assert_eq!(counts.retained_item_count, 4);
    assert_eq!(counts.in_flight_requests, 0);

    window.clear_residency();
    let counts = window.residency_retained_counts();
    assert_eq!(counts.index_turns, 0);
    assert_eq!(counts.resident_turns, 0);
    assert_eq!(counts.nonresident_turns, 0);
    assert_eq!(counts.retained_item_count, 0);
}

#[test]
fn empty_history_window_can_rebuild_residency_stats_from_thread_turns() {
    let window = TranscriptHistoryWindow::from_turns(&[
        turn("turn_1"),
        turn_with_items_view("turn_2", TurnItemsView::NotLoaded),
    ]);

    let counts = window.residency_retained_counts();
    assert_eq!(counts.index_turns, 2);
    assert_eq!(counts.resident_turns, 1);
    assert_eq!(counts.nonresident_turns, 1);
    assert_eq!(counts.retained_item_count, 1);
}

#[test]
fn not_loaded_history_turns_do_not_render_loading_messages() {
    let mut details = shell::DetailHarness::new();
    details.prepend(
        "thread_a",
        vec![
            turn_with_items_view("turn_1", TurnItemsView::NotLoaded),
            turn_with_items_view("turn_2", TurnItemsView::NotLoaded),
        ],
    );

    assert_eq!(
        details.agent_message_texts(),
        vec![Vec::<String>::new(), Vec::new()]
    );
}

#[test]
fn generated_image_history_prefers_saved_path_over_inline_result() {
    let mut details = shell::DetailHarness::new();
    details.prepend(
        "thread_a",
        vec![turn_with_items(
            "turn_1",
            vec![generated_image_item(
                "image_1",
                "completed",
                Some("inline bytes that should not stay resident"),
                Some(r"C:\work\generated\image_1.png"),
            )],
        )],
    );

    assert_eq!(
        details.generated_images(),
        vec![vec![shell::GeneratedImageSnapshot {
            id: "image_1".to_string(),
            status: Some("completed".to_string()),
            revised_prompt: Some("Prompt for image_1".to_string()),
            result_len: None,
            saved_path: Some(r"C:\work\generated\image_1.png".to_string()),
            complete: true,
        }]]
    );
}

#[test]
fn generated_image_history_drops_oversized_inline_result() {
    let mut details = shell::DetailHarness::new();
    let oversized_inline = "x".repeat(256 * 1024 + 1);
    details.prepend(
        "thread_a",
        vec![turn_with_items(
            "turn_1",
            vec![generated_image_item(
                "image_inline",
                "completed",
                Some(oversized_inline.as_str()),
                None,
            )],
        )],
    );

    assert_eq!(
        details.generated_images(),
        vec![vec![shell::GeneratedImageSnapshot {
            id: "image_inline".to_string(),
            status: Some("completed".to_string()),
            revised_prompt: Some("Prompt for image_inline".to_string()),
            result_len: None,
            saved_path: None,
            complete: true,
        }]]
    );
}

#[test]
fn generated_image_history_retains_small_inline_result() {
    let mut details = shell::DetailHarness::new();
    let inline = "iVBORw0KGgo=";
    details.prepend(
        "thread_a",
        vec![turn_with_items(
            "turn_1",
            vec![generated_image_item(
                "image_inline",
                "completed",
                Some(inline),
                None,
            )],
        )],
    );

    assert_eq!(
        details.generated_images(),
        vec![vec![shell::GeneratedImageSnapshot {
            id: "image_inline".to_string(),
            status: Some("completed".to_string()),
            revised_prompt: Some("Prompt for image_inline".to_string()),
            result_len: Some(inline.len()),
            saved_path: None,
            complete: true,
        }]]
    );
}

fn indexed_window<const N: usize>(turn_ids: [&str; N]) -> TranscriptHistoryWindow {
    let page = loaded_history_page(
        turn_ids
            .into_iter()
            .map(|turn_id| turn_with_items_view(turn_id, TurnItemsView::NotLoaded))
            .collect(),
        None,
        None,
    );
    let mut window = TranscriptHistoryWindow::from_latest_page(&page);
    window.bind_residency_to_thread("thread_a");
    window
}

fn planner_turns(
    count: usize,
    height: usize,
    estimated_resident_bytes: usize,
    resident: bool,
) -> Vec<TranscriptResidencyTurnPlanInput> {
    (0..count)
        .map(|index| {
            planner_turn(
                &format!("turn_{index}"),
                index,
                height,
                estimated_resident_bytes,
                resident,
            )
        })
        .collect()
}

fn planner_turn(
    turn_id: &str,
    source_position: usize,
    height: usize,
    estimated_resident_bytes: usize,
    resident: bool,
) -> TranscriptResidencyTurnPlanInput {
    TranscriptResidencyTurnPlanInput::new(turn_id)
        .with_source_position(source_position)
        .with_measured_height(height)
        .with_estimated_resident_bytes(estimated_resident_bytes)
        .with_resident(resident)
}

struct FakeHistoryBackend {
    response: Result<ThreadTurnsListResponse, String>,
    calls: Vec<(String, ThreadTurnsListOptions)>,
}

impl FakeHistoryBackend {
    fn new(response: Result<ThreadTurnsListResponse, String>) -> Self {
        Self {
            response,
            calls: Vec::new(),
        }
    }
}

impl TranscriptHistoryBackend for FakeHistoryBackend {
    type Error = String;

    fn list_thread_turns(
        &mut self,
        thread_id: &str,
        options: &ThreadTurnsListOptions,
        _timeout: Duration,
    ) -> Result<ThreadTurnsListResponse, Self::Error> {
        self.calls.push((thread_id.to_string(), options.clone()));
        self.response.clone()
    }
}

fn loaded_history_page(
    turns: Vec<TurnInfo>,
    older_cursor: Option<&str>,
    newer_cursor: Option<&str>,
) -> LoadedTranscriptHistoryPage {
    LoadedTranscriptHistoryPage {
        turns,
        older_cursor: older_cursor.map(str::to_string),
        newer_cursor: newer_cursor.map(str::to_string),
    }
}

fn turn_ids(turns: &[TurnInfo]) -> Vec<&str> {
    turns.iter().map(|turn| turn.id.as_str()).collect()
}

fn turn(id: &str) -> TurnInfo {
    turn_with_items_view(id, TurnItemsView::Full)
}

fn turn_with_text(id: &str, text: &str) -> TurnInfo {
    turn_with_items(
        id,
        vec![ThreadItem::AgentMessage(AgentMessageItem {
            id: format!("{id}_message"),
            phase: Some(ProtocolPhase::FinalAnswer),
            text: text.to_string(),
        })],
    )
}

fn turn_with_items(id: &str, items: Vec<ThreadItem>) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items_view: TurnItemsView::Full,
        items,
        error: None,
    }
}

fn turn_with_items_view(id: &str, items_view: TurnItemsView) -> TurnInfo {
    let items = if items_view == TurnItemsView::Full {
        vec![message_item(id)]
    } else {
        Vec::new()
    };
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items_view,
        items,
        error: None,
    }
}

fn message_item(id: &str) -> ThreadItem {
    ThreadItem::AgentMessage(AgentMessageItem {
        id: format!("{id}_message"),
        phase: Some(ProtocolPhase::FinalAnswer),
        text: format!("Answer for {id}"),
    })
}

fn generated_image_item(
    id: &str,
    status: &str,
    result: Option<&str>,
    saved_path: Option<&str>,
) -> ThreadItem {
    ThreadItem::ImageGeneration(ImageGenerationItem {
        id: id.to_string(),
        status: Some(status.to_string()),
        revised_prompt: Some(format!("Prompt for {id}")),
        result: result.map(str::to_string),
        saved_path: saved_path.map(str::to_string),
    })
}
