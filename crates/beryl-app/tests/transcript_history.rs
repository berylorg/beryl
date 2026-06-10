#![allow(dead_code, private_interfaces, unused_imports)]

use std::time::Duration;

use beryl_backend::{
    AgentMessageItem, ImageGenerationItem, ProtocolPhase, ThreadItem, ThreadTurnsListOptions,
    ThreadTurnsListResponse, TurnInfo, TurnItemsView, TurnStatus, UserInput, UserMessageItem,
};

mod shell {
    use std::ops::Range;

    #[path = "../../src/shell/execution_detail.rs"]
    mod execution_detail;
    #[path = "../../src/shell/transcript_history.rs"]
    pub(super) mod transcript_history;

    use beryl_backend::TurnInfo;

    pub(super) struct DetailHarness {
        state: execution_detail::ExecutionDetailState,
    }

    #[derive(Debug, PartialEq, Eq)]
    pub(super) struct GeneratedImageSnapshot {
        pub(super) id: String,
        pub(super) status: Option<String>,
        pub(super) revised_prompt: Option<String>,
        pub(super) result_len: Option<usize>,
        pub(super) saved_path: Option<String>,
        pub(super) complete: bool,
    }

    impl DetailHarness {
        pub(super) fn new() -> Self {
            Self {
                state: execution_detail::ExecutionDetailState::default(),
            }
        }

        pub(super) fn prepend(&mut self, thread_id: &str, turns: Vec<TurnInfo>) -> usize {
            self.state.prepend_thread_history_page(thread_id, turns)
        }

        pub(super) fn prepend_partial(&mut self, thread_id: &str, turns: Vec<TurnInfo>) -> usize {
            self.state
                .prepend_thread_history_page_with_image_resolver_and_partial_mode(
                    thread_id,
                    turns,
                    &execution_detail::TranscriptImagePathResolver::default(),
                    true,
                )
                .added_count
        }

        pub(super) fn apply_items(
            &mut self,
            thread_id: &str,
            turn_id: &str,
            items: Vec<beryl_backend::ThreadItem>,
        ) -> bool {
            self.state
                .apply_history_turn_items(
                    thread_id,
                    turn_id,
                    items,
                    &execution_detail::TranscriptImagePathResolver::default(),
                )
                .is_some()
        }

        pub(super) fn release_detail(&mut self, thread_id: &str, turn_id: &str) -> bool {
            self.state
                .release_history_turn_detail(thread_id, turn_id)
                .is_some()
        }

        pub(super) fn release(&mut self, range: Range<usize>) -> usize {
            self.state.release_history_range(range).len()
        }

        pub(super) fn restore(
            &mut self,
            thread_id: &str,
            row_start: usize,
            expected_turn_ids: &[String],
            turns: Vec<TurnInfo>,
        ) -> usize {
            self.state
                .restore_history_page(thread_id, row_start, expected_turn_ids, turns)
                .len()
        }

        pub(super) fn turn_ids(&self) -> Vec<&str> {
            self.state
                .turns()
                .iter()
                .filter_map(|turn| turn.turn_id.as_deref())
                .collect()
        }

        pub(super) fn placeholder_indexes(&self) -> Vec<usize> {
            self.state
                .turns()
                .iter()
                .enumerate()
                .filter_map(|(index, turn)| turn.is_released_history_placeholder().then_some(index))
                .collect()
        }

        pub(super) fn detail_loading_placeholder_indexes(&self) -> Vec<usize> {
            self.state
                .turns()
                .iter()
                .enumerate()
                .filter_map(|(index, turn)| {
                    turn.has_history_detail_loading_placeholder()
                        .then_some(index)
                })
                .collect()
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

        pub(super) fn begin_turn(&mut self, user_input: String) -> usize {
            self.state.begin_turn(user_input)
        }

        pub(super) fn last_user_input(&self) -> &str {
            self.state.turns().last().unwrap().user_input_fragments()[0]
                .text
                .as_str()
        }
    }
}

use shell::DetailHarness;
use shell::GeneratedImageSnapshot;
use shell::transcript_history::{
    THREAD_HISTORY_PAGE_LIMIT, TRANSCRIPT_HISTORY_MAX_RELEASED_PAGES, TranscriptHistoryBackend,
    TranscriptHistoryPageRequest, TranscriptHistoryWindow, TranscriptTurnDetailApplyResult,
    TranscriptTurnDetailCache, TranscriptTurnDetailLoadStart, TranscriptTurnDetailPageLoadError,
    TranscriptTurnDetailPinKind, TranscriptTurnDetailRetention, TranscriptTurnDetailSchedule,
    TranscriptTurnDetailStatus, TranscriptTurnDetailViewportOrder,
    TranscriptTurnDetailViewportPlan, initial_thread_history_page_options,
    load_older_thread_history_page, load_thread_turn_detail_from_history_page,
    loaded_page_from_desc_response, older_thread_history_page_options, thread_history_page_options,
};

#[test]
fn initial_tail_page_is_normalized_to_chronological_turns() {
    let page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_4"), turn("turn_3")],
        next_cursor: Some("older".to_string()),
        backwards_cursor: Some("newer".to_string()),
    });

    assert_eq!(turn_ids(&page.turns), vec!["turn_3", "turn_4"]);
    assert_eq!(page.older_cursor.as_deref(), Some("older"));
    assert_eq!(page.newer_cursor.as_deref(), Some("newer"));
    for turn in &page.turns {
        assert_eq!(turn.items_view, TurnItemsView::NotLoaded);
        assert!(turn.items.is_empty());
    }

    let options = initial_thread_history_page_options();
    assert_eq!(options.limit, Some(THREAD_HISTORY_PAGE_LIMIT));
    assert_eq!(
        options.sort_direction,
        Some(beryl_backend::SortDirection::Desc)
    );
    assert_eq!(options.cursor, None);
    assert_eq!(options.items_view, Some(TurnItemsView::NotLoaded));
}

#[test]
fn history_page_options_always_request_not_loaded_items_view() {
    let initial = initial_thread_history_page_options();
    assert_eq!(initial.limit, Some(THREAD_HISTORY_PAGE_LIMIT));
    assert_eq!(
        initial.sort_direction,
        Some(beryl_backend::SortDirection::Desc)
    );
    assert_eq!(initial.cursor, None);
    assert_eq!(initial.items_view, Some(TurnItemsView::NotLoaded));

    let older = thread_history_page_options(Some("older"));
    assert_eq!(older.cursor.as_deref(), Some("older"));
    assert_eq!(older.items_view, Some(TurnItemsView::NotLoaded));

    let older_helper = older_thread_history_page_options("older");
    assert_eq!(older_helper.cursor.as_deref(), Some("older"));
    assert_eq!(older_helper.items_view, Some(TurnItemsView::NotLoaded));
}

#[test]
fn loaded_pages_drop_returned_full_items_before_retention() {
    let page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_2"), turn("turn_1")],
        next_cursor: Some("older".to_string()),
        backwards_cursor: None,
    });

    assert_eq!(turn_ids(&page.turns), vec!["turn_1", "turn_2"]);
    assert_eq!(page.older_cursor.as_deref(), Some("older"));
    for turn in &page.turns {
        assert_eq!(turn.items_view, TurnItemsView::NotLoaded);
        assert!(turn.items.is_empty());
    }
}

#[test]
fn older_page_request_uses_cursor_and_preserves_chronological_order() {
    let mut backend = FakeHistoryBackend::new(ThreadTurnsListResponse {
        data: vec![turn("turn_2"), turn("turn_1")],
        next_cursor: Some("oldest".to_string()),
        backwards_cursor: None,
    });

    let page =
        load_older_thread_history_page(&mut backend, "thread_a", "older", Duration::from_secs(5))
            .unwrap();

    assert_eq!(backend.calls.len(), 1);
    assert_eq!(backend.calls[0].0, "thread_a");
    assert_eq!(
        backend.calls[0].1,
        older_thread_history_page_options("older")
    );
    assert_eq!(turn_ids(&page.turns), vec!["turn_1", "turn_2"]);
    assert_eq!(page.older_cursor.as_deref(), Some("oldest"));
}

#[test]
fn transcript_history_window_tracks_loading_and_cursor_exhaustion() {
    let initial_page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_4"), turn("turn_3")],
        next_cursor: Some("older".to_string()),
        backwards_cursor: None,
    });
    let mut window = TranscriptHistoryWindow::from_latest_page(&initial_page);

    assert!(window.should_request_older(&(0..1)));
    assert_eq!(window.begin_loading_older().as_deref(), Some("older"));
    assert!(window.is_loading_older());
    assert!(!window.should_request_older(&(0..1)));

    let final_page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_2"), turn("turn_1")],
        next_cursor: None,
        backwards_cursor: Some("newer".to_string()),
    });
    window.finish_loading_older_with_added(&final_page, final_page.turns.len());

    assert!(!window.is_loading_older());
    assert!(!window.has_older_pages());
    assert!(!window.should_request_older(&(0..1)));
}

#[test]
fn history_window_reports_current_tail_only_when_latest_page_is_current() {
    let initial_page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_4"), turn("turn_3")],
        next_cursor: Some("older".to_string()),
        backwards_cursor: None,
    });
    let mut window = TranscriptHistoryWindow::from_latest_page(&initial_page);
    assert!(window.current_tail_known());

    assert_eq!(window.begin_loading_older().as_deref(), Some("older"));
    assert!(!window.current_tail_known());

    let older_page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_2"), turn("turn_1")],
        next_cursor: None,
        backwards_cursor: Some("newer".to_string()),
    });
    window.finish_loading_older_with_added(&older_page, older_page.turns.len());
    assert!(window.current_tail_known());

    let stale_latest_page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_2"), turn("turn_1")],
        next_cursor: None,
        backwards_cursor: Some("newer".to_string()),
    });
    let stale_window = TranscriptHistoryWindow::from_latest_page(&stale_latest_page);
    assert!(!stale_window.current_tail_known());
}

#[test]
fn non_advancing_empty_older_page_exhausts_cursor() {
    let initial_page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_2")],
        next_cursor: Some("older".to_string()),
        backwards_cursor: None,
    });
    let mut window = TranscriptHistoryWindow::from_latest_page(&initial_page);
    assert_eq!(window.begin_loading_older().as_deref(), Some("older"));

    let empty_page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: Vec::new(),
        next_cursor: Some("older".to_string()),
        backwards_cursor: None,
    });
    window.finish_loading_older_with_added(&empty_page, 0);

    assert!(!window.has_older_pages());
    assert!(!window.should_request_older(&(0..1)));
}

#[test]
fn history_window_releases_cold_pages_and_requests_released_page_by_cursor() {
    let initial_page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_6"), turn("turn_5")],
        next_cursor: Some("older_4".to_string()),
        backwards_cursor: None,
    });
    let mut window = TranscriptHistoryWindow::from_latest_page(&initial_page);

    assert_eq!(window.begin_loading_older().as_deref(), Some("older_4"));
    let middle_page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_4"), turn("turn_3")],
        next_cursor: Some("older_2".to_string()),
        backwards_cursor: Some("newer_5".to_string()),
    });
    window.finish_loading_older_with_added(&middle_page, middle_page.turns.len());

    assert_eq!(window.begin_loading_older().as_deref(), Some("older_2"));
    let oldest_page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_2"), turn("turn_1")],
        next_cursor: None,
        backwards_cursor: Some("newer_3".to_string()),
    });
    window.finish_loading_older_with_added(&oldest_page, oldest_page.turns.len());
    assert_eq!(window.resident_page_count(), 3);

    let releases = window.release_cold_pages_with_limit(&(100..102), 2);

    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].range, 0..2);
    assert_eq!(window.resident_page_count(), 2);
    assert_eq!(window.released_page_count(), 1);

    let request = window
        .begin_loading_page_for_visible_range(&(0..1))
        .expect("released page should refetch when visible");
    let page_id = match request {
        TranscriptHistoryPageRequest::Released { page_id, cursor } => {
            assert_eq!(cursor.as_deref(), Some("older_2"));
            page_id
        }
        TranscriptHistoryPageRequest::Older { cursor } => {
            panic!("expected released-page refetch, got older cursor {cursor}");
        }
    };
    assert!(
        window
            .begin_loading_page_for_visible_range(&(0..1))
            .is_none()
    );

    let restored = window
        .finish_loading_released_page(page_id, &oldest_page)
        .expect("released page should still be tracked");
    assert_eq!(restored.range, 0..2);
    assert_eq!(restored.turn_ids, vec!["turn_1", "turn_2"]);
    assert_eq!(window.resident_page_count(), 3);
    assert_eq!(window.released_page_count(), 0);
}

#[test]
fn history_window_retained_counts_track_resident_and_released_pages() {
    let initial_page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_6"), turn("turn_5")],
        next_cursor: Some("older_4".to_string()),
        backwards_cursor: None,
    });
    let mut window = TranscriptHistoryWindow::from_latest_page(&initial_page);
    assert_eq!(window.begin_loading_older().as_deref(), Some("older_4"));

    let middle_page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_4"), turn("turn_3")],
        next_cursor: Some("older_2".to_string()),
        backwards_cursor: Some("newer_5".to_string()),
    });
    window.finish_loading_older_with_added(&middle_page, middle_page.turns.len());
    assert_eq!(window.begin_loading_older().as_deref(), Some("older_2"));

    let oldest_page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_2"), turn("turn_1")],
        next_cursor: None,
        backwards_cursor: Some("newer_3".to_string()),
    });
    window.finish_loading_older_with_added(&oldest_page, oldest_page.turns.len());
    window.release_cold_pages_with_limit(&(100..102), 2);

    let counts = window.retained_counts();
    assert_eq!(counts.pages, 3);
    assert_eq!(counts.resident_pages, 2);
    assert_eq!(counts.released_pages, 1);
    assert_eq!(counts.loading_pages, 0);
}

#[test]
fn history_window_caps_released_page_metadata() {
    let initial_page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_latest")],
        next_cursor: Some("older_0".to_string()),
        backwards_cursor: None,
    });
    let mut window = TranscriptHistoryWindow::from_latest_page(&initial_page);

    for index in 0..(TRANSCRIPT_HISTORY_MAX_RELEASED_PAGES + 4) {
        assert_eq!(
            window.begin_loading_older().as_deref(),
            Some(format!("older_{index}").as_str())
        );
        let page = loaded_page_from_desc_response(ThreadTurnsListResponse {
            data: vec![turn(&format!("turn_{index}"))],
            next_cursor: Some(format!("older_{}", index + 1)),
            backwards_cursor: Some(format!("newer_{index}")),
        });
        window.finish_loading_older_with_added(&page, page.turns.len());
    }

    window.release_cold_pages_with_limit(&(10_000..10_001), 1);

    let counts = window.retained_counts();
    assert_eq!(counts.released_pages, TRANSCRIPT_HISTORY_MAX_RELEASED_PAGES);
    assert_eq!(counts.resident_pages, 1);
    assert_eq!(counts.pages, TRANSCRIPT_HISTORY_MAX_RELEASED_PAGES + 1);
}

#[test]
fn prepended_history_pages_merge_before_loaded_turns_and_live_turns_continue_at_tail() {
    let mut state = DetailHarness::new();
    assert_eq!(
        state.prepend("thread_a", vec![turn("turn_3"), turn("turn_4")]),
        2
    );
    assert_eq!(
        state.prepend(
            "thread_a",
            vec![turn("turn_1"), turn("turn_2"), turn("turn_3")]
        ),
        2
    );

    assert_eq!(
        state.turn_ids(),
        vec!["turn_1", "turn_2", "turn_3", "turn_4"]
    );
    assert_eq!(state.begin_turn("live prompt".to_string()), 4);
    assert_eq!(state.last_user_input(), "live prompt");
}

#[test]
fn released_history_page_refetch_restores_only_expected_turns() {
    let mut state = DetailHarness::new();
    assert_eq!(
        state.prepend("thread_a", vec![turn("turn_3"), turn("turn_4")]),
        2
    );
    assert_eq!(
        state.prepend("thread_a", vec![turn("turn_1"), turn("turn_2")]),
        2
    );

    assert_eq!(state.release(0..2), 2);
    assert_eq!(state.placeholder_indexes(), vec![0, 1]);

    let expected_turn_ids = vec!["turn_1".to_string(), "turn_2".to_string()];
    assert_eq!(
        state.restore(
            "thread_a",
            0,
            &expected_turn_ids,
            vec![turn("turn_1"), turn("turn_2"), turn("turn_3")]
        ),
        2
    );

    assert_eq!(
        state.turn_ids(),
        vec!["turn_1", "turn_2", "turn_3", "turn_4"]
    );
    assert!(state.placeholder_indexes().is_empty());
}

#[test]
fn skeleton_history_page_turns_render_as_loading_placeholders() {
    let mut state = DetailHarness::new();
    assert_eq!(
        state.prepend_partial(
            "thread_a",
            vec![
                turn_with_items_view("turn_1", TurnItemsView::NotLoaded),
                turn_with_items_view("turn_2", TurnItemsView::NotLoaded),
            ],
        ),
        2
    );

    assert_eq!(state.turn_ids(), vec!["turn_1", "turn_2"]);
    assert_eq!(state.detail_loading_placeholder_indexes(), vec![0, 1]);
    assert_eq!(
        state.agent_message_texts(),
        vec![
            vec!["Loading transcript details...".to_string()],
            vec!["Loading transcript details...".to_string()],
        ]
    );
}

#[test]
fn loaded_turn_detail_replaces_skeleton_and_release_downgrades_again() {
    let mut state = DetailHarness::new();
    assert_eq!(
        state.prepend_partial(
            "thread_a",
            vec![turn_with_items_view("turn_1", TurnItemsView::NotLoaded)],
        ),
        1
    );

    assert!(state.apply_items("thread_a", "turn_1", vec![message_item("loaded_detail")]));
    assert!(state.detail_loading_placeholder_indexes().is_empty());
    assert_eq!(
        state.agent_message_texts(),
        vec![vec!["Answer for loaded_detail".to_string()]]
    );

    assert!(state.release_detail("thread_a", "turn_1"));
    assert_eq!(state.detail_loading_placeholder_indexes(), vec![0]);
    assert_eq!(
        state.agent_message_texts(),
        vec![vec!["Loading transcript details...".to_string()]]
    );
}

#[test]
fn lazy_loaded_generated_image_detail_prefers_saved_path_over_inline_result() {
    let mut state = DetailHarness::new();
    assert_eq!(
        state.prepend_partial(
            "thread_a",
            vec![turn_with_items_view("turn_1", TurnItemsView::NotLoaded)],
        ),
        1
    );

    assert_eq!(state.generated_images(), vec![Vec::new()]);
    assert!(state.apply_items(
        "thread_a",
        "turn_1",
        vec![generated_image_item(
            "image_1",
            "generating",
            Some("inline bytes that must not be retained"),
            Some(r"C:\work\generated\image_1.png"),
        )],
    ));

    assert_eq!(
        state.detail_loading_placeholder_indexes(),
        Vec::<usize>::new()
    );
    assert_eq!(
        state.generated_images(),
        vec![vec![GeneratedImageSnapshot {
            id: "image_1".to_string(),
            status: Some("generating".to_string()),
            revised_prompt: Some("Prompt for image_1".to_string()),
            result_len: None,
            saved_path: Some(r"C:\work\generated\image_1.png".to_string()),
            complete: true,
        }]]
    );
}

#[test]
fn lazy_loaded_inline_generated_image_detail_is_released_with_full_detail() {
    let inline_result = "iVBORw0KGgo=".to_string();
    let mut state = DetailHarness::new();
    assert_eq!(
        state.prepend_partial(
            "thread_a",
            vec![turn_with_items_view("turn_1", TurnItemsView::NotLoaded)],
        ),
        1
    );

    assert!(state.apply_items(
        "thread_a",
        "turn_1",
        vec![generated_image_item(
            "image_inline",
            "completed",
            Some(inline_result.as_str()),
            None,
        )],
    ));
    assert_eq!(
        state.generated_images(),
        vec![vec![GeneratedImageSnapshot {
            id: "image_inline".to_string(),
            status: Some("completed".to_string()),
            revised_prompt: Some("Prompt for image_inline".to_string()),
            result_len: Some(inline_result.len()),
            saved_path: None,
            complete: true,
        }]]
    );

    assert!(state.release_detail("thread_a", "turn_1"));
    assert_eq!(state.detail_loading_placeholder_indexes(), vec![0]);
    assert_eq!(state.generated_images(), vec![Vec::new()]);
}

#[test]
fn lazy_loaded_inline_generated_image_detail_drops_oversized_result() {
    let mut state = DetailHarness::new();
    assert_eq!(
        state.prepend_partial(
            "thread_a",
            vec![turn_with_items_view("turn_1", TurnItemsView::NotLoaded)],
        ),
        1
    );

    let oversized_result = "x".repeat(300 * 1024);
    assert!(state.apply_items(
        "thread_a",
        "turn_1",
        vec![generated_image_item(
            "image_large",
            "completed",
            Some(oversized_result.as_str()),
            None,
        )],
    ));

    assert_eq!(
        state.generated_images(),
        vec![vec![GeneratedImageSnapshot {
            id: "image_large".to_string(),
            status: Some("completed".to_string()),
            revised_prompt: Some("Prompt for image_large".to_string()),
            result_len: None,
            saved_path: None,
            complete: true,
        }]]
    );
}

#[test]
fn lazy_detail_reload_updates_generated_image_with_same_item_id() {
    let mut state = DetailHarness::new();
    assert_eq!(
        state.prepend_partial(
            "thread_a",
            vec![turn_with_items_view("turn_1", TurnItemsView::NotLoaded)],
        ),
        1
    );

    assert!(state.apply_items(
        "thread_a",
        "turn_1",
        vec![generated_image_item(
            "image_1",
            "generating",
            Some("iVBORw0KGgo="),
            None,
        )],
    ));
    assert_eq!(
        state.generated_images(),
        vec![vec![GeneratedImageSnapshot {
            id: "image_1".to_string(),
            status: Some("generating".to_string()),
            revised_prompt: Some("Prompt for image_1".to_string()),
            result_len: Some("iVBORw0KGgo=".len()),
            saved_path: None,
            complete: true,
        }]]
    );

    assert!(state.apply_items(
        "thread_a",
        "turn_1",
        vec![generated_image_item(
            "image_1",
            "completed",
            Some("stale inline bytes"),
            Some(r"C:\work\generated\image_1.png"),
        )],
    ));
    assert_eq!(
        state.generated_images(),
        vec![vec![GeneratedImageSnapshot {
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
fn turn_detail_cache_tracks_skeleton_loading_full_and_failed_states() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    cache.insert_skeleton_from_turn(&turn("turn_1"));

    let counts = cache.retained_counts();
    assert_eq!(counts.skeleton_turns, 1);
    assert_eq!(counts.missing_detail_turns, 1);
    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Missing);

    let ticket = match cache.begin_loading("thread_a", "turn_1").unwrap() {
        TranscriptTurnDetailLoadStart::Started(ticket) => ticket,
        other => panic!("expected started detail load, got {other:?}"),
    };
    assert_eq!(ticket.thread_id(), "thread_a");
    assert_eq!(ticket.turn_id(), "turn_1");
    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Loading);

    assert_eq!(
        cache.finish_loading(&ticket, 1),
        TranscriptTurnDetailApplyResult::Applied
    );
    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Full);
    assert_eq!(cache.full_item_count("turn_1"), Some(1));
    assert!(matches!(
        cache.begin_loading("thread_a", "turn_1").unwrap(),
        TranscriptTurnDetailLoadStart::AlreadyFull
    ));

    cache.insert_skeleton_from_turn(&turn("turn_2"));
    let ticket = cache
        .begin_loading("thread_a", "turn_2")
        .unwrap()
        .ticket()
        .unwrap()
        .clone();
    assert_eq!(
        cache.fail_loading(&ticket),
        TranscriptTurnDetailApplyResult::Applied
    );
    assert_eq!(cache.status("turn_2"), TranscriptTurnDetailStatus::Failed);

    let counts = cache.retained_counts();
    assert_eq!(counts.skeleton_turns, 2);
    assert_eq!(counts.full_detail_turns, 1);
    assert_eq!(counts.failed_detail_turns, 1);
    assert_eq!(counts.retained_item_count, 1);
}

#[test]
fn turn_detail_cache_release_preserves_visible_and_pinned_details() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    for id in ["turn_1", "turn_2", "turn_3"] {
        cache.insert_skeleton_from_turn(&turn(id));
        complete_detail(&mut cache, "thread_a", id, vec![message_item(id)]);
    }

    cache.pin_turn("turn_1", TranscriptTurnDetailPinKind::ActiveContextMenu);
    let retention = TranscriptTurnDetailRetention::from_turn_ids(["turn_2"]);
    let released = cache.release_unretained_details(&retention);

    assert_eq!(released.full_detail_turns, 1);
    assert_eq!(released.retained_item_count, 1);
    assert_eq!(released.released_turn_ids, vec!["turn_3"]);
    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Full);
    assert_eq!(cache.status("turn_2"), TranscriptTurnDetailStatus::Full);
    assert_eq!(cache.status("turn_3"), TranscriptTurnDetailStatus::Missing);
    assert_eq!(cache.retained_counts().pinned_turns, 1);

    cache.unpin_turn("turn_1", TranscriptTurnDetailPinKind::ActiveContextMenu);
    let released = cache.release_unretained_details(&retention);

    assert_eq!(released.full_detail_turns, 1);
    assert_eq!(released.released_turn_ids, vec!["turn_1"]);
    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Missing);
    assert_eq!(cache.status("turn_2"), TranscriptTurnDetailStatus::Full);
    assert_eq!(cache.retained_counts().pinned_turns, 0);
}

#[test]
fn turn_detail_cache_rejects_stale_loads_after_release_or_thread_switch() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    cache.insert_skeleton_from_turn(&turn("turn_1"));
    let ticket = cache
        .begin_loading("thread_a", "turn_1")
        .unwrap()
        .ticket()
        .unwrap()
        .clone();

    let released = cache.release_unretained_details(&TranscriptTurnDetailRetention::new());
    assert_eq!(released.loading_detail_turns, 1);
    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Missing);
    assert!(!cache.should_start_loading(&ticket));
    assert_eq!(
        cache.skip_loading(&ticket),
        TranscriptTurnDetailApplyResult::Stale
    );
    assert_eq!(
        cache.finish_loading(&ticket, 1),
        TranscriptTurnDetailApplyResult::Stale
    );
    assert_eq!(cache.full_item_count("turn_1"), None);

    let ticket = cache
        .begin_loading("thread_a", "turn_1")
        .unwrap()
        .ticket()
        .unwrap()
        .clone();
    cache.reset_for_thread("thread_b");
    assert_eq!(
        cache.finish_loading(&ticket, 1),
        TranscriptTurnDetailApplyResult::Stale
    );
    assert!(!cache.should_start_loading(&ticket));
    assert_eq!(
        cache.skip_loading(&ticket),
        TranscriptTurnDetailApplyResult::Stale
    );
    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Missing);
    assert_eq!(cache.thread_id(), Some("thread_b"));
}

#[test]
fn turn_detail_cache_skips_current_queued_ticket_without_failure() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    cache.insert_skeleton_from_turn(&turn("turn_1"));
    let ticket = cache
        .begin_loading("thread_a", "turn_1")
        .unwrap()
        .ticket()
        .unwrap()
        .clone();

    assert!(cache.should_start_loading(&ticket));
    assert_eq!(
        cache.skip_loading(&ticket),
        TranscriptTurnDetailApplyResult::Applied
    );

    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Missing);
    assert!(!cache.should_start_loading(&ticket));
    assert_eq!(
        cache.finish_loading(&ticket, 1),
        TranscriptTurnDetailApplyResult::Stale
    );
    assert_eq!(
        cache.fail_loading(&ticket),
        TranscriptTurnDetailApplyResult::Stale
    );
    assert_eq!(cache.full_item_count("turn_1"), None);
}

#[test]
fn turn_detail_cache_keeps_pinned_loading_ticket_current_outside_viewport() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    cache.insert_skeleton_from_turn(&turn("turn_1"));
    let ticket = cache
        .begin_loading("thread_a", "turn_1")
        .unwrap()
        .ticket()
        .unwrap()
        .clone();
    cache.pin_turn("turn_1", TranscriptTurnDetailPinKind::ActiveContextMenu);

    let released = cache.release_unretained_details(&TranscriptTurnDetailRetention::new());

    assert_eq!(released.loading_detail_turns, 0);
    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Loading);
    assert!(cache.should_start_loading(&ticket));
}

#[test]
fn turn_detail_cache_retained_item_count_is_independent_of_skeleton_count() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    let turn_ids = (0..100)
        .map(|index| format!("turn_{index}"))
        .collect::<Vec<_>>();
    for turn_id in &turn_ids {
        cache.insert_skeleton_from_turn(&turn(turn_id));
    }
    for turn_id in ["turn_20", "turn_21", "turn_22"] {
        complete_detail(&mut cache, "thread_a", turn_id, vec![message_item(turn_id)]);
    }

    let counts = cache.retained_counts();
    assert_eq!(counts.skeleton_turns, 100);
    assert_eq!(counts.full_detail_turns, 3);
    assert_eq!(counts.retained_item_count, 3);

    let retention = TranscriptTurnDetailRetention::from_visible_range(&turn_ids, 21..22, 0);
    assert_eq!(retention.len(), 1);
    let released = cache.release_unretained_details(&retention);

    assert_eq!(released.full_detail_turns, 2);
    let counts = cache.retained_counts();
    assert_eq!(counts.skeleton_turns, 100);
    assert_eq!(counts.full_detail_turns, 1);
    assert_eq!(counts.retained_item_count, 1);
    assert_eq!(cache.status("turn_21"), TranscriptTurnDetailStatus::Full);
    assert_eq!(cache.status("turn_20"), TranscriptTurnDetailStatus::Missing);
}

#[test]
fn turn_detail_cache_prunes_skeletons_for_released_history_pages() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    let turns = vec![
        turn_with_items_view("turn_1", TurnItemsView::NotLoaded),
        turn_with_items_view("turn_2", TurnItemsView::NotLoaded),
        turn_with_items_view("turn_3", TurnItemsView::NotLoaded),
        turn_with_items_view("turn_4", TurnItemsView::NotLoaded),
    ];
    cache.insert_skeletons_from_history_page(&turns[0..2], Some("older_cursor"));
    cache.insert_skeletons_from_history_page(&turns[2..4], None);

    assert_eq!(cache.retained_counts().skeleton_turns, 4);

    let pruned = cache.prune_skeletons_to_protected_turns(["turn_3", "turn_4"]);

    assert_eq!(pruned, 2);
    assert_eq!(cache.retained_counts().skeleton_turns, 2);
    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Missing);
    assert_eq!(cache.status("turn_2"), TranscriptTurnDetailStatus::Missing);

    let schedule = schedule_required_for_test(&mut cache, "thread_a", ["turn_1", "turn_3"]);

    assert_eq!(ticket_turn_ids(&schedule.requested_tickets), vec!["turn_3"]);
}

#[test]
fn turn_detail_cache_prune_preserves_active_and_pinned_skeletons() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    for turn_id in ["turn_1", "turn_2", "turn_3", "turn_4", "turn_5"] {
        cache.insert_skeleton_from_turn(&turn_with_items_view(turn_id, TurnItemsView::NotLoaded));
    }
    let loading_ticket = cache
        .begin_loading("thread_a", "turn_1")
        .unwrap()
        .ticket()
        .unwrap()
        .clone();
    complete_detail(
        &mut cache,
        "thread_a",
        "turn_2",
        vec![message_item("turn_2")],
    );
    let failed_ticket = cache
        .begin_loading("thread_a", "turn_3")
        .unwrap()
        .ticket()
        .unwrap()
        .clone();
    assert_eq!(
        cache.fail_loading(&failed_ticket),
        TranscriptTurnDetailApplyResult::Applied
    );
    cache.pin_turn("turn_4", TranscriptTurnDetailPinKind::ActiveContextMenu);

    let pruned = cache.prune_skeletons_to_protected_turns(std::iter::empty::<&str>());

    assert_eq!(pruned, 1);
    assert_eq!(cache.retained_counts().skeleton_turns, 4);
    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Loading);
    assert_eq!(cache.status("turn_2"), TranscriptTurnDetailStatus::Full);
    assert_eq!(cache.status("turn_3"), TranscriptTurnDetailStatus::Failed);
    assert_eq!(cache.status("turn_4"), TranscriptTurnDetailStatus::Missing);
    assert!(cache.should_start_loading(&loading_ticket));

    let released = cache.release_unretained_details(&TranscriptTurnDetailRetention::new());

    assert_eq!(released.loading_detail_turns, 1);
    assert_eq!(released.full_detail_turns, 1);
    assert_eq!(released.failed_detail_turns, 1);

    let pruned = cache.prune_skeletons_to_protected_turns(std::iter::empty::<&str>());

    assert_eq!(pruned, 3);
    assert_eq!(cache.retained_counts().skeleton_turns, 1);
    assert_eq!(cache.retained_counts().pinned_turns, 1);

    cache.unpin_turn("turn_4", TranscriptTurnDetailPinKind::ActiveContextMenu);
    let pruned = cache.prune_skeletons_to_protected_turns(std::iter::empty::<&str>());

    assert_eq!(pruned, 1);
    assert_eq!(cache.retained_counts().skeleton_turns, 0);
}

#[test]
fn restored_history_page_recreates_pruned_detail_skeleton_locators() {
    let initial_page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_6"), turn("turn_5")],
        next_cursor: Some("older_4".to_string()),
        backwards_cursor: None,
    });
    let mut window = TranscriptHistoryWindow::from_latest_page(&initial_page);
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    cache.insert_skeletons_from_history_page(&initial_page.turns, None);

    assert_eq!(window.begin_loading_older().as_deref(), Some("older_4"));
    let middle_page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_4"), turn("turn_3")],
        next_cursor: Some("older_2".to_string()),
        backwards_cursor: Some("newer_5".to_string()),
    });
    window.finish_loading_older_with_added(&middle_page, middle_page.turns.len());
    cache.insert_skeletons_from_history_page(&middle_page.turns, Some("older_4"));

    assert_eq!(window.begin_loading_older().as_deref(), Some("older_2"));
    let oldest_page = loaded_page_from_desc_response(ThreadTurnsListResponse {
        data: vec![turn("turn_2"), turn("turn_1")],
        next_cursor: None,
        backwards_cursor: Some("newer_3".to_string()),
    });
    window.finish_loading_older_with_added(&oldest_page, oldest_page.turns.len());
    cache.insert_skeletons_from_history_page(&oldest_page.turns, Some("older_2"));

    let releases = window.release_cold_pages_with_limit(&(100..102), 2);
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].range, 0..2);

    let pruned = cache.prune_skeletons_to_protected_turns(window.resident_turn_ids());

    assert_eq!(pruned, 2);
    assert_eq!(cache.retained_counts().skeleton_turns, 4);

    let request = window
        .begin_loading_page_for_visible_range(&(0..1))
        .expect("released page should refetch when visible");
    let (page_id, cursor) = match request {
        TranscriptHistoryPageRequest::Released { page_id, cursor } => (page_id, cursor),
        TranscriptHistoryPageRequest::Older { cursor } => {
            panic!("expected released-page refetch, got older cursor {cursor}");
        }
    };
    assert_eq!(cursor.as_deref(), Some("older_2"));

    let restored = window
        .finish_loading_released_page(page_id, &oldest_page)
        .expect("released page should still be tracked");
    assert_eq!(restored.turn_ids, vec!["turn_1", "turn_2"]);
    cache.insert_skeletons_from_history_page(&oldest_page.turns, cursor.as_deref());

    assert_eq!(cache.retained_counts().skeleton_turns, 6);

    let schedule = schedule_required_for_test(&mut cache, "thread_a", ["turn_1"]);
    let locator = schedule.requested_tickets[0]
        .page_locator()
        .expect("restored skeleton should carry page metadata");

    assert_eq!(locator.cursor(), Some("older_2"));
    assert_eq!(locator.limit(), 2);
}

#[test]
fn turn_detail_cache_replaces_named_ui_pins() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    cache.replace_pins(
        TranscriptTurnDetailPinKind::ActiveContextMenu,
        ["turn_1", "turn_2"],
    );
    cache.replace_pins(TranscriptTurnDetailPinKind::EditTarget, ["turn_2"]);
    cache.replace_pins(TranscriptTurnDetailPinKind::MediaActionTarget, ["turn_3"]);
    cache.replace_pins(TranscriptTurnDetailPinKind::ActiveTurn, ["turn_4"]);
    assert_eq!(cache.retained_counts().pinned_turns, 4);

    cache.replace_pins(TranscriptTurnDetailPinKind::ActiveContextMenu, ["turn_2"]);
    assert_eq!(cache.retained_counts().pinned_turns, 3);

    cache.replace_pins(
        TranscriptTurnDetailPinKind::ActiveTurn,
        std::iter::empty::<&str>(),
    );
    assert_eq!(cache.retained_counts().pinned_turns, 2);
    cache.unpin_turn("turn_3", TranscriptTurnDetailPinKind::MediaActionTarget);
    assert_eq!(cache.retained_counts().pinned_turns, 1);
}

#[test]
fn turn_detail_scheduler_requests_only_missing_non_full_required_turns() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    cache.insert_skeleton_from_turn(&turn_with_items_view("turn_1", TurnItemsView::NotLoaded));
    cache.insert_skeleton_from_turn(&turn_with_items_view("turn_2", TurnItemsView::Full));
    cache.insert_skeleton_from_turn(&turn_with_items_view("turn_3", TurnItemsView::NotLoaded));
    complete_detail(
        &mut cache,
        "thread_a",
        "turn_3",
        vec![message_item("turn_3")],
    );

    let schedule = schedule_required_for_test(
        &mut cache,
        "thread_a",
        ["turn_1", "turn_2", "turn_3", "turn_1"],
    );

    assert_eq!(schedule.retained_turns, 3);
    assert_eq!(schedule.released.full_detail_turns, 0);
    assert_eq!(ticket_turn_ids(&schedule.requested_tickets), vec!["turn_1"]);
    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Loading);
    assert_eq!(cache.status("turn_2"), TranscriptTurnDetailStatus::Missing);
    assert_eq!(cache.status("turn_3"), TranscriptTurnDetailStatus::Full);

    let repeated = schedule_required_for_test(&mut cache, "thread_a", ["turn_1"]);
    assert!(repeated.requested_tickets.is_empty());
}

#[test]
fn turn_detail_scheduler_tail_micro_batch_requests_newest_visible_skeleton_first() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    let turns = (1..=6)
        .map(|index| turn_with_items_view(&format!("turn_{index}"), TurnItemsView::NotLoaded))
        .collect::<Vec<_>>();
    cache.insert_skeletons_from_history_page(&turns, Some("tail_cursor"));

    let schedule = schedule_viewport_for_test(
        &mut cache,
        "thread_a",
        ["turn_3", "turn_4", "turn_5", "turn_6"],
        ["turn_1", "turn_2", "turn_3", "turn_4", "turn_5", "turn_6"],
        TranscriptTurnDetailViewportOrder::NewestFirst,
        1,
    );

    assert_eq!(schedule.retained_turns, 6);
    assert_eq!(ticket_turn_ids(&schedule.requested_tickets), vec!["turn_6"]);
    assert_eq!(cache.status("turn_6"), TranscriptTurnDetailStatus::Loading);
    assert_eq!(cache.status("turn_5"), TranscriptTurnDetailStatus::Missing);
    let locator = schedule.requested_tickets[0]
        .page_locator()
        .expect("tail turn should carry history-page locator");
    assert_eq!(locator.cursor(), Some("tail_cursor"));
    assert_eq!(locator.limit(), 1);
}

#[test]
fn turn_detail_scheduler_micro_batch_does_not_enqueue_every_missing_visible_turn() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    let turn_ids = (1..=10)
        .map(|index| format!("turn_{index}"))
        .collect::<Vec<_>>();
    for turn_id in &turn_ids {
        cache.insert_skeleton_from_turn(&turn_with_items_view(turn_id, TurnItemsView::NotLoaded));
    }

    let schedule = schedule_viewport_for_test(
        &mut cache,
        "thread_a",
        &turn_ids,
        &turn_ids,
        TranscriptTurnDetailViewportOrder::NewestFirst,
        1,
    );

    assert_eq!(schedule.retained_turns, 10);
    assert_eq!(
        ticket_turn_ids(&schedule.requested_tickets),
        vec!["turn_10"]
    );
    assert_eq!(cache.status("turn_10"), TranscriptTurnDetailStatus::Loading);
    assert_eq!(cache.status("turn_9"), TranscriptTurnDetailStatus::Missing);
}

#[test]
fn turn_detail_scheduler_next_pass_uses_recomputed_viewport() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    let turn_ids = (1..=5)
        .map(|index| format!("turn_{index}"))
        .collect::<Vec<_>>();
    for turn_id in &turn_ids {
        cache.insert_skeleton_from_turn(&turn_with_items_view(turn_id, TurnItemsView::NotLoaded));
    }
    let first = schedule_viewport_for_test(
        &mut cache,
        "thread_a",
        &turn_ids,
        &turn_ids,
        TranscriptTurnDetailViewportOrder::NewestFirst,
        1,
    );
    assert_eq!(ticket_turn_ids(&first.requested_tickets), vec!["turn_5"]);
    assert_eq!(
        cache.finish_loading(&first.requested_tickets[0], 1),
        TranscriptTurnDetailApplyResult::Applied
    );

    let second = schedule_viewport_for_test(
        &mut cache,
        "thread_a",
        ["turn_5"],
        ["turn_5"],
        TranscriptTurnDetailViewportOrder::NewestFirst,
        1,
    );

    assert!(second.requested_tickets.is_empty());
    assert_eq!(cache.status("turn_5"), TranscriptTurnDetailStatus::Full);
    assert_eq!(cache.status("turn_4"), TranscriptTurnDetailStatus::Missing);
}

#[test]
fn turn_detail_scheduler_empty_priority_still_retains_current_view_detail() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    for turn_id in ["turn_1", "turn_2"] {
        cache.insert_skeleton_from_turn(&turn_with_items_view(turn_id, TurnItemsView::NotLoaded));
    }

    let first = schedule_viewport_for_test(
        &mut cache,
        "thread_a",
        ["turn_2"],
        ["turn_1", "turn_2"],
        TranscriptTurnDetailViewportOrder::NewestFirst,
        1,
    );
    assert_eq!(ticket_turn_ids(&first.requested_tickets), vec!["turn_2"]);
    assert_eq!(
        cache.finish_loading(&first.requested_tickets[0], 10),
        TranscriptTurnDetailApplyResult::Applied
    );

    let second = schedule_viewport_for_test(
        &mut cache,
        "thread_a",
        std::iter::empty::<&str>(),
        ["turn_2"],
        TranscriptTurnDetailViewportOrder::NewestFirst,
        1,
    );

    assert!(second.requested_tickets.is_empty());
    assert_eq!(second.retained_turns, 1);
    assert_eq!(second.released.full_detail_turns, 0);
    assert_eq!(cache.status("turn_2"), TranscriptTurnDetailStatus::Full);
}

#[test]
fn turn_detail_scheduler_priority_only_plan_does_not_request_retained_overscan() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    for turn_id in ["turn_1", "turn_2", "turn_3"] {
        cache.insert_skeleton_from_turn(&turn_with_items_view(turn_id, TurnItemsView::NotLoaded));
    }

    let first = schedule_priority_for_test(
        &mut cache,
        "thread_a",
        ["turn_3"],
        ["turn_1", "turn_2", "turn_3"],
        TranscriptTurnDetailViewportOrder::NewestFirst,
        1,
    );
    assert_eq!(ticket_turn_ids(&first.requested_tickets), vec!["turn_3"]);
    assert_eq!(
        cache.finish_loading(&first.requested_tickets[0], 1),
        TranscriptTurnDetailApplyResult::Applied
    );

    let second = schedule_priority_for_test(
        &mut cache,
        "thread_a",
        ["turn_3"],
        ["turn_1", "turn_2", "turn_3"],
        TranscriptTurnDetailViewportOrder::NewestFirst,
        1,
    );

    assert!(second.requested_tickets.is_empty());
    assert_eq!(cache.status("turn_3"), TranscriptTurnDetailStatus::Full);
    assert_eq!(cache.status("turn_2"), TranscriptTurnDetailStatus::Missing);
    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Missing);
}

#[test]
fn turn_detail_scheduler_retains_pinned_nonviewport_detail_without_promoting_it() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    for turn_id in ["turn_1", "turn_2", "turn_3", "turn_4"] {
        cache.insert_skeleton_from_turn(&turn_with_items_view(turn_id, TurnItemsView::NotLoaded));
    }
    complete_detail(
        &mut cache,
        "thread_a",
        "turn_1",
        vec![message_item("turn_1")],
    );
    cache.pin_turn("turn_1", TranscriptTurnDetailPinKind::ActiveContextMenu);

    let schedule = schedule_viewport_for_test(
        &mut cache,
        "thread_a",
        ["turn_3", "turn_4"],
        ["turn_3", "turn_4"],
        TranscriptTurnDetailViewportOrder::NewestFirst,
        1,
    );

    assert_eq!(ticket_turn_ids(&schedule.requested_tickets), vec!["turn_4"]);
    assert_eq!(schedule.released.full_detail_turns, 0);
    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Full);
    assert_eq!(cache.status("turn_4"), TranscriptTurnDetailStatus::Loading);
}

#[test]
fn turn_detail_scheduler_zero_batch_releases_stale_loading_without_new_request() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    for turn_id in ["turn_1", "turn_2", "turn_3"] {
        cache.insert_skeleton_from_turn(&turn_with_items_view(turn_id, TurnItemsView::NotLoaded));
    }
    let first = schedule_viewport_for_test(
        &mut cache,
        "thread_a",
        ["turn_2", "turn_3"],
        ["turn_2", "turn_3"],
        TranscriptTurnDetailViewportOrder::NewestFirst,
        1,
    );
    assert_eq!(ticket_turn_ids(&first.requested_tickets), vec!["turn_3"]);

    let second = schedule_viewport_for_test(
        &mut cache,
        "thread_a",
        ["turn_1"],
        ["turn_1"],
        TranscriptTurnDetailViewportOrder::OldestFirst,
        0,
    );

    assert!(second.requested_tickets.is_empty());
    assert_eq!(second.released.loading_detail_turns, 1);
    assert_eq!(cache.status("turn_3"), TranscriptTurnDetailStatus::Missing);
    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Missing);
}

#[test]
fn turn_detail_scheduler_ignores_required_turns_without_history_skeleton() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");

    let schedule = schedule_required_for_test(&mut cache, "thread_a", ["live_turn"]);

    assert_eq!(schedule.retained_turns, 1);
    assert!(schedule.requested_tickets.is_empty());
    assert_eq!(
        cache.status("live_turn"),
        TranscriptTurnDetailStatus::Missing
    );
}

#[test]
fn turn_detail_scheduler_releases_out_of_window_details_and_keeps_required() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    for turn_id in ["turn_1", "turn_2", "turn_3"] {
        cache.insert_skeleton_from_turn(&turn_with_items_view(turn_id, TurnItemsView::NotLoaded));
    }
    complete_detail(
        &mut cache,
        "thread_a",
        "turn_1",
        vec![message_item("turn_1")],
    );
    complete_detail(
        &mut cache,
        "thread_a",
        "turn_2",
        vec![message_item("turn_2")],
    );

    let schedule = schedule_required_for_test(&mut cache, "thread_a", ["turn_2", "turn_3"]);

    assert_eq!(schedule.retained_turns, 2);
    assert_eq!(schedule.released.full_detail_turns, 1);
    assert_eq!(schedule.released.retained_item_count, 1);
    assert_eq!(ticket_turn_ids(&schedule.requested_tickets), vec!["turn_3"]);
    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Missing);
    assert_eq!(cache.status("turn_2"), TranscriptTurnDetailStatus::Full);
    assert_eq!(cache.status("turn_3"), TranscriptTurnDetailStatus::Loading);
}

#[test]
fn turn_detail_scheduler_does_not_retry_failed_detail_each_frame() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    cache.insert_skeleton_from_turn(&turn_with_items_view("turn_1", TurnItemsView::NotLoaded));
    let ticket = cache
        .begin_loading("thread_a", "turn_1")
        .unwrap()
        .ticket()
        .unwrap()
        .clone();
    assert_eq!(
        cache.fail_loading(&ticket),
        TranscriptTurnDetailApplyResult::Applied
    );

    let schedule = schedule_required_for_test(&mut cache, "thread_a", ["turn_1"]);

    assert!(schedule.requested_tickets.is_empty());
    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Failed);
}

#[test]
fn turn_detail_tickets_carry_minimal_history_page_locator() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    let turns = vec![
        turn_with_items_view("turn_1", TurnItemsView::NotLoaded),
        turn_with_items_view("turn_2", TurnItemsView::NotLoaded),
        turn_with_items_view("turn_3", TurnItemsView::NotLoaded),
    ];
    cache.insert_skeletons_from_history_page(&turns, Some("older_cursor"));

    let schedule = schedule_required_for_test(&mut cache, "thread_a", ["turn_1", "turn_3"]);

    assert_eq!(ticket_turn_ids(&schedule.requested_tickets), vec!["turn_1"]);
    let first_locator = schedule.requested_tickets[0]
        .page_locator()
        .expect("first page turn should carry a history-page locator");
    assert_eq!(first_locator.cursor(), Some("older_cursor"));
    assert_eq!(first_locator.limit(), 3);
    assert_eq!(
        ticket_coalesced_turn_ids(&schedule.requested_tickets[0]),
        vec!["turn_1", "turn_3"]
    );
}

#[test]
fn turn_detail_cache_coalesces_prefix_response_for_retained_siblings() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    let turns = vec![
        turn_with_items_view("turn_1", TurnItemsView::NotLoaded),
        turn_with_items_view("turn_2", TurnItemsView::NotLoaded),
        turn_with_items_view("turn_3", TurnItemsView::NotLoaded),
    ];
    cache.insert_skeletons_from_history_page(&turns, Some("older_cursor"));

    let schedule = schedule_viewport_for_test(
        &mut cache,
        "thread_a",
        ["turn_1", "turn_2", "turn_3"],
        ["turn_1", "turn_2", "turn_3"],
        TranscriptTurnDetailViewportOrder::OldestFirst,
        1,
    );

    assert_eq!(ticket_turn_ids(&schedule.requested_tickets), vec!["turn_1"]);
    let ticket = &schedule.requested_tickets[0];
    assert_eq!(
        ticket_coalesced_turn_ids(ticket),
        vec!["turn_1", "turn_2", "turn_3"]
    );
    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Loading);
    assert_eq!(cache.status("turn_2"), TranscriptTurnDetailStatus::Loading);
    assert_eq!(cache.status("turn_3"), TranscriptTurnDetailStatus::Loading);

    assert_eq!(
        cache.finish_coalesced_loading(ticket, "turn_1", 1),
        TranscriptTurnDetailApplyResult::Applied
    );
    assert_eq!(
        cache.finish_coalesced_loading(ticket, "turn_2", 2),
        TranscriptTurnDetailApplyResult::Applied
    );
    assert_eq!(
        cache.finish_coalesced_loading(ticket, "turn_3", 3),
        TranscriptTurnDetailApplyResult::Applied
    );

    assert_eq!(cache.full_item_count("turn_1"), Some(1));
    assert_eq!(cache.full_item_count("turn_2"), Some(2));
    assert_eq!(cache.full_item_count("turn_3"), Some(3));
    assert_eq!(cache.retained_counts().retained_item_count, 6);
}

#[test]
fn turn_detail_cache_drops_coalesced_siblings_released_before_response() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    let turns = vec![
        turn_with_items_view("turn_1", TurnItemsView::NotLoaded),
        turn_with_items_view("turn_2", TurnItemsView::NotLoaded),
        turn_with_items_view("turn_3", TurnItemsView::NotLoaded),
    ];
    cache.insert_skeletons_from_history_page(&turns, Some("older_cursor"));
    let schedule = schedule_viewport_for_test(
        &mut cache,
        "thread_a",
        ["turn_1", "turn_2", "turn_3"],
        ["turn_1", "turn_2", "turn_3"],
        TranscriptTurnDetailViewportOrder::OldestFirst,
        1,
    );
    let ticket = &schedule.requested_tickets[0];

    let released =
        cache.release_unretained_details(&TranscriptTurnDetailRetention::from_turn_ids(["turn_1"]));

    assert_eq!(released.loading_detail_turns, 2);
    assert_eq!(
        cache.current_loading_coalesced_turn_ids(ticket),
        vec!["turn_1".to_string()]
    );
    assert_eq!(
        cache.finish_coalesced_loading(ticket, "turn_1", 1),
        TranscriptTurnDetailApplyResult::Applied
    );
    assert_eq!(
        cache.finish_coalesced_loading(ticket, "turn_2", 1),
        TranscriptTurnDetailApplyResult::Stale
    );
    assert_eq!(
        cache.finish_coalesced_loading(ticket, "turn_3", 1),
        TranscriptTurnDetailApplyResult::Stale
    );
    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Full);
    assert_eq!(cache.status("turn_2"), TranscriptTurnDetailStatus::Missing);
    assert_eq!(cache.status("turn_3"), TranscriptTurnDetailStatus::Missing);
    assert_eq!(cache.retained_counts().retained_item_count, 1);
}

#[test]
fn turn_detail_cache_clears_missing_coalesced_response_sibling() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    let turns = vec![
        turn_with_items_view("turn_1", TurnItemsView::NotLoaded),
        turn_with_items_view("turn_2", TurnItemsView::NotLoaded),
    ];
    cache.insert_skeletons_from_history_page(&turns, Some("older_cursor"));
    let schedule = schedule_viewport_for_test(
        &mut cache,
        "thread_a",
        ["turn_1", "turn_2"],
        ["turn_1", "turn_2"],
        TranscriptTurnDetailViewportOrder::OldestFirst,
        1,
    );
    let ticket = &schedule.requested_tickets[0];

    assert_eq!(
        cache.finish_coalesced_loading(ticket, "turn_1", 1),
        TranscriptTurnDetailApplyResult::Applied
    );
    assert_eq!(
        cache.skip_coalesced_loading(ticket, "turn_2"),
        TranscriptTurnDetailApplyResult::Applied
    );

    assert_eq!(cache.status("turn_1"), TranscriptTurnDetailStatus::Full);
    assert_eq!(cache.status("turn_2"), TranscriptTurnDetailStatus::Missing);
}

#[test]
fn coalesced_off_window_generated_image_detail_is_not_applied_or_retained() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    let skeletons = vec![
        turn_with_items_view("turn_1", TurnItemsView::NotLoaded),
        turn_with_items_view("turn_2", TurnItemsView::NotLoaded),
    ];
    cache.insert_skeletons_from_history_page(&skeletons, Some("older_cursor"));
    let mut state = DetailHarness::new();
    assert_eq!(state.prepend_partial("thread_a", skeletons), 2);
    let schedule = schedule_viewport_for_test(
        &mut cache,
        "thread_a",
        ["turn_1", "turn_2"],
        ["turn_1", "turn_2"],
        TranscriptTurnDetailViewportOrder::OldestFirst,
        1,
    );
    let ticket = &schedule.requested_tickets[0];

    cache.release_unretained_details(&TranscriptTurnDetailRetention::from_turn_ids(["turn_1"]));
    let returned_turns = vec![
        turn_with_items("turn_1", vec![message_item("turn_1")]),
        turn_with_items(
            "turn_2",
            vec![generated_image_item(
                "image_2",
                "completed",
                Some("inline bytes that must not be retained"),
                Some(r"C:\work\generated\image_2.png"),
            )],
        ),
    ];
    let resolver_turn_ids = cache.current_loading_coalesced_turn_ids(ticket);
    let resolver_turns = returned_turns
        .into_iter()
        .filter(|turn| resolver_turn_ids.contains(&turn.id))
        .collect::<Vec<_>>();

    assert_eq!(turn_ids(&resolver_turns), vec!["turn_1"]);
    for turn in resolver_turns {
        let turn_id = turn.id;
        let items = turn.items;
        if cache.finish_coalesced_loading(ticket, &turn_id, items.len())
            == TranscriptTurnDetailApplyResult::Applied
        {
            assert!(state.apply_items("thread_a", &turn_id, items));
        }
    }

    assert_eq!(cache.retained_counts().retained_item_count, 1);
    assert_eq!(state.detail_loading_placeholder_indexes(), vec![1]);
    assert_eq!(state.generated_images(), vec![Vec::new(), Vec::new()]);
}

#[test]
fn coalesced_off_window_local_image_detail_is_not_selected_for_resolution() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    let skeletons = vec![
        turn_with_items_view("turn_1", TurnItemsView::NotLoaded),
        turn_with_items_view("turn_2", TurnItemsView::NotLoaded),
    ];
    cache.insert_skeletons_from_history_page(&skeletons, Some("older_cursor"));
    let schedule = schedule_viewport_for_test(
        &mut cache,
        "thread_a",
        ["turn_1", "turn_2"],
        ["turn_1", "turn_2"],
        TranscriptTurnDetailViewportOrder::OldestFirst,
        1,
    );
    let ticket = &schedule.requested_tickets[0];

    cache.release_unretained_details(&TranscriptTurnDetailRetention::from_turn_ids(["turn_1"]));
    let returned_turns = vec![
        turn_with_items("turn_1", vec![message_item("turn_1")]),
        turn_with_items(
            "turn_2",
            vec![local_image_user_message(
                "offscreen_user_image",
                "/tmp/offscreen-image.png",
            )],
        ),
    ];
    let resolver_turn_ids = cache.current_loading_coalesced_turn_ids(ticket);
    let resolver_turns = returned_turns
        .into_iter()
        .filter(|turn| resolver_turn_ids.contains(&turn.id))
        .collect::<Vec<_>>();

    assert_eq!(turn_ids(&resolver_turns), vec!["turn_1"]);
    assert!(local_image_paths(&resolver_turns).is_empty());
}

#[test]
fn load_thread_turn_detail_from_history_page_uses_cursor_limit_and_full_items_view() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    let turns = vec![
        turn_with_items_view("turn_1", TurnItemsView::NotLoaded),
        turn_with_items_view("turn_2", TurnItemsView::NotLoaded),
        turn_with_items_view("turn_3", TurnItemsView::NotLoaded),
    ];
    cache.insert_skeletons_from_history_page(&turns, Some("older_cursor"));
    let schedule = schedule_required_for_test(&mut cache, "thread_a", ["turn_2"]);
    let page_locator = schedule.requested_tickets[0]
        .page_locator()
        .expect("scheduled ticket should carry page metadata");

    let mut backend = FakeHistoryBackend::new(ThreadTurnsListResponse {
        data: vec![turn("turn_3"), turn("turn_2")],
        next_cursor: Some("older_next".to_string()),
        backwards_cursor: Some("newer_previous".to_string()),
    });

    let detail_load = load_thread_turn_detail_from_history_page(
        &mut backend,
        "thread_a",
        "turn_2",
        page_locator,
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(turn_ids(&detail_load.turns), vec!["turn_2", "turn_3"]);
    assert_eq!(
        item_ids(&detail_load.turns[0].items),
        vec!["turn_2_message"]
    );
    assert_eq!(detail_load.returned_turn_count, 2);
    assert_eq!(backend.calls.len(), 1);
    assert_eq!(backend.calls[0].0, "thread_a");
    assert_eq!(
        backend.calls[0].1,
        ThreadTurnsListOptions::page(2)
            .with_sort_direction(beryl_backend::SortDirection::Desc)
            .with_items_view(TurnItemsView::Full)
            .with_cursor("older_cursor")
    );
}

#[test]
fn history_page_detail_load_reports_missing_requested_turn() {
    let mut cache = TranscriptTurnDetailCache::default();
    cache.reset_for_thread("thread_a");
    let turns = vec![
        turn_with_items_view("turn_1", TurnItemsView::NotLoaded),
        turn_with_items_view("turn_2", TurnItemsView::NotLoaded),
    ];
    cache.insert_skeletons_from_history_page(&turns, None);
    let schedule = schedule_required_for_test(&mut cache, "thread_a", ["turn_1"]);
    let page_locator = schedule.requested_tickets[0]
        .page_locator()
        .expect("scheduled ticket should carry page metadata");
    let mut backend = FakeHistoryBackend::new(ThreadTurnsListResponse {
        data: vec![turn("turn_2")],
        next_cursor: None,
        backwards_cursor: None,
    });

    let error = load_thread_turn_detail_from_history_page(
        &mut backend,
        "thread_a",
        "turn_1",
        page_locator,
        Duration::from_secs(5),
    )
    .unwrap_err();

    match error {
        TranscriptTurnDetailPageLoadError::MissingTurn {
            turn_id,
            cursor,
            limit,
            returned_turn_count,
            ..
        } => {
            assert_eq!(turn_id, "turn_1");
            assert_eq!(cursor, None);
            assert_eq!(limit, 2);
            assert_eq!(returned_turn_count, 1);
        }
        other => panic!("expected missing-turn detail load error, got {other:?}"),
    }
}

struct FakeHistoryBackend {
    response: ThreadTurnsListResponse,
    calls: Vec<(String, ThreadTurnsListOptions)>,
}

impl FakeHistoryBackend {
    fn new(response: ThreadTurnsListResponse) -> Self {
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
        _: Duration,
    ) -> Result<ThreadTurnsListResponse, Self::Error> {
        self.calls.push((thread_id.to_string(), options.clone()));
        Ok(self.response.clone())
    }
}

fn turn_ids(turns: &[TurnInfo]) -> Vec<&str> {
    turns.iter().map(|turn| turn.id.as_str()).collect()
}

fn schedule_required_for_test<I, S>(
    cache: &mut TranscriptTurnDetailCache,
    thread_id: &str,
    turn_ids: I,
) -> TranscriptTurnDetailSchedule
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let turn_ids = turn_ids
        .into_iter()
        .map(|turn_id| turn_id.as_ref().to_string())
        .collect::<Vec<_>>();
    schedule_viewport_for_test(
        cache,
        thread_id,
        &turn_ids,
        &turn_ids,
        TranscriptTurnDetailViewportOrder::OldestFirst,
        usize::MAX,
    )
}

fn schedule_viewport_for_test<I, S, J, T>(
    cache: &mut TranscriptTurnDetailCache,
    thread_id: &str,
    visible_turn_ids: I,
    retained_turn_ids: J,
    order: TranscriptTurnDetailViewportOrder,
    max_requested_tickets: usize,
) -> TranscriptTurnDetailSchedule
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
    J: IntoIterator<Item = T>,
    T: AsRef<str>,
{
    let plan = TranscriptTurnDetailViewportPlan::from_visible_and_retained(
        visible_turn_ids,
        retained_turn_ids,
        order,
    );
    cache.schedule_viewport_full_details(thread_id, plan, max_requested_tickets)
}

fn schedule_priority_for_test<I, S, J, T>(
    cache: &mut TranscriptTurnDetailCache,
    thread_id: &str,
    priority_turn_ids: I,
    retained_turn_ids: J,
    order: TranscriptTurnDetailViewportOrder,
    max_requested_tickets: usize,
) -> TranscriptTurnDetailSchedule
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
    J: IntoIterator<Item = T>,
    T: AsRef<str>,
{
    let plan = TranscriptTurnDetailViewportPlan::from_priority_and_retained(
        priority_turn_ids,
        retained_turn_ids,
        order,
    );
    cache.schedule_viewport_full_details(thread_id, plan, max_requested_tickets)
}

fn ticket_turn_ids(
    tickets: &[shell::transcript_history::TranscriptTurnDetailLoadTicket],
) -> Vec<&str> {
    tickets.iter().map(|ticket| ticket.turn_id()).collect()
}

fn ticket_coalesced_turn_ids(
    ticket: &shell::transcript_history::TranscriptTurnDetailLoadTicket,
) -> Vec<&str> {
    ticket
        .coalesced_turn_ids()
        .iter()
        .map(String::as_str)
        .collect()
}

fn item_ids(items: &[ThreadItem]) -> Vec<&str> {
    items.iter().map(ThreadItem::id).collect()
}

fn complete_detail(
    cache: &mut TranscriptTurnDetailCache,
    thread_id: &str,
    turn_id: &str,
    items: Vec<ThreadItem>,
) {
    let ticket = cache
        .begin_loading(thread_id, turn_id)
        .expect("detail load should start for selected thread")
        .ticket()
        .expect("detail load should have a ticket")
        .clone();
    assert_eq!(
        cache.finish_loading(&ticket, items.len()),
        TranscriptTurnDetailApplyResult::Applied
    );
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

fn local_image_user_message(id: &str, path: &str) -> ThreadItem {
    ThreadItem::UserMessage(UserMessageItem {
        id: id.to_string(),
        content: vec![UserInput::LocalImage {
            path: path.to_string(),
        }],
    })
}

fn local_image_paths(turns: &[TurnInfo]) -> Vec<&str> {
    turns
        .iter()
        .flat_map(|turn| turn.items.iter())
        .filter_map(|item| match item {
            ThreadItem::UserMessage(message) => Some(message),
            _ => None,
        })
        .flat_map(|message| message.content.iter())
        .filter_map(|input| match input {
            UserInput::LocalImage { path } => Some(path.as_str()),
            _ => None,
        })
        .collect()
}

fn turn(id: &str) -> TurnInfo {
    turn_with_items_view(id, TurnItemsView::Full)
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
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items_view,
        items: vec![message_item(id)],
        error: None,
    }
}
