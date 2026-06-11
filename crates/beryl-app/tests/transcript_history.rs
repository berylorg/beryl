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
    TranscriptResidencyPinKind, TranscriptResidencyPolicy, TranscriptResidencyRequestPriority,
    TranscriptResidencyRetention, initial_thread_history_page_options,
    initial_thread_resident_page_options, load_thread_resident_history_page,
    loaded_page_from_desc_response, older_thread_history_page_options, thread_history_page_options,
    thread_resident_history_page_options,
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
