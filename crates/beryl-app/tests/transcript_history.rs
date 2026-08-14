#![allow(dead_code, private_interfaces, unused_imports)]

use std::time::Duration;

use beryl_backend::{
    AgentMessageItem, ProtocolPhase, ThreadItem, ThreadTurnsListOptions, ThreadTurnsListResponse,
    TurnInfo, TurnStatus,
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

    impl DetailHarness {
        pub(super) fn new() -> Self {
            Self {
                state: execution_detail::ExecutionDetailState::default(),
            }
        }

        pub(super) fn prepend(&mut self, thread_id: &str, turns: Vec<TurnInfo>) -> usize {
            self.state.prepend_thread_history_page(thread_id, turns)
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
use shell::transcript_history::{
    THREAD_HISTORY_PAGE_LIMIT, TRANSCRIPT_HISTORY_MAX_RELEASED_PAGES, TranscriptHistoryBackend,
    TranscriptHistoryPageRequest, TranscriptHistoryWindow, initial_thread_history_page_options,
    load_older_thread_history_page, loaded_page_from_desc_response,
    older_thread_history_page_options,
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

    let options = initial_thread_history_page_options();
    assert_eq!(options.limit, Some(THREAD_HISTORY_PAGE_LIMIT));
    assert_eq!(
        options.sort_direction,
        Some(beryl_backend::SortDirection::Desc)
    );
    assert_eq!(options.cursor, None);
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

fn turn(id: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: vec![ThreadItem::AgentMessage(AgentMessageItem {
            id: format!("{id}_message"),
            phase: Some(ProtocolPhase::FinalAnswer),
            text: format!("Answer for {id}"),
        })],
        error: None,
    }
}
