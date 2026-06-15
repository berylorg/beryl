#![allow(dead_code, unused_imports)]

use beryl_backend::{
    CommandExecutionItem, CommandExecutionStatus, ThreadItem, TurnInfo, TurnItemsView, TurnStatus,
    UserInput, UserMessageItem,
};
use gpui::px;

mod shell {
    #[path = "../../src/shell/execution_detail.rs"]
    mod execution_detail;
    #[path = "../../src/shell/transcript_history.rs"]
    mod transcript_history;
    #[path = "../../src/shell/transcript_presentation.rs"]
    mod transcript_presentation;
    #[path = "../../src/shell/transcript_projection.rs"]
    mod transcript_projection;
    #[path = "../../src/shell/transcript_viewport.rs"]
    mod transcript_viewport;
    #[path = "../../src/shell/turn_view.rs"]
    mod turn_view;
    #[allow(dead_code)]
    #[path = "../../src/shell/virtual_list/mod.rs"]
    mod virtual_list;

    use std::ops::Range;

    use beryl_backend::TurnInfo;
    use gpui::{Pixels, px};

    use self::{
        execution_detail::ExecutionDetailState,
        transcript_history::{LoadedTranscriptHistoryPage, TranscriptHistoryWindow},
        transcript_presentation::TranscriptPresentationState,
        transcript_viewport::{
            TranscriptViewportChunkAnchor, TranscriptViewportPlacement, TranscriptViewportState,
            TranscriptViewportTurnAnchor,
        },
        turn_view::TranscriptTurnNumberingSnapshot,
        virtual_list::{ListAlignment, ListOffset, ListScrollPosition, ListState, test_support},
    };

    pub(super) use transcript_presentation::TranscriptPresentationMutation;

    pub(super) struct TurnViewHarness {
        details: ExecutionDetailState,
        history_window: TranscriptHistoryWindow,
        presentation: TranscriptPresentationState,
    }

    impl TurnViewHarness {
        pub(super) fn from_latest_page(
            turns: Vec<TurnInfo>,
            older_cursor: Option<&str>,
            newer_cursor: Option<&str>,
        ) -> Self {
            let history_window = TranscriptHistoryWindow::from_latest_page(&loaded_history_page(
                &turns,
                older_cursor,
                newer_cursor,
            ));
            let mut details = ExecutionDetailState::default();
            details.prepend_thread_history_page("thread_a", turns);
            let mut presentation = TranscriptPresentationState::default();
            presentation.replace_from_turns(details.turns());
            Self {
                details,
                history_window,
                presentation,
            }
        }

        pub(super) fn release_range(
            &mut self,
            range: Range<usize>,
        ) -> Vec<TranscriptPresentationMutation> {
            self.details
                .release_history_range(range)
                .into_iter()
                .map(|replacement| {
                    self.presentation
                        .replace_turn(replacement.index, replacement.turn)
                })
                .collect()
        }

        pub(super) fn restore_history_page(
            &mut self,
            row_start: usize,
            expected_turn_ids: &[String],
            turns: Vec<TurnInfo>,
        ) -> Vec<TranscriptPresentationMutation> {
            self.details
                .restore_history_page("thread_a", row_start, expected_turn_ids, turns)
                .into_iter()
                .map(|replacement| {
                    self.presentation
                        .replace_turn(replacement.index, replacement.turn)
                })
                .collect()
        }

        pub(super) fn presentation_len(&self) -> usize {
            self.presentation.len()
        }

        pub(super) fn snapshot(&self, list_state: &ListState) -> TranscriptTurnNumberingSnapshot {
            self.snapshot_with_viewport(&TranscriptViewportState::default(), list_state)
        }

        pub(super) fn snapshot_with_viewport(
            &self,
            viewport: &TranscriptViewportState,
            list_state: &ListState,
        ) -> TranscriptTurnNumberingSnapshot {
            turn_view::transcript_turn_numbering_snapshot(
                Some("thread_a"),
                &self.details,
                &self.history_window,
                &self.presentation,
                viewport,
                list_state,
            )
        }

        pub(super) fn unselected_snapshot(
            &self,
            list_state: &ListState,
        ) -> TranscriptTurnNumberingSnapshot {
            turn_view::transcript_turn_numbering_snapshot(
                None,
                &self.details,
                &self.history_window,
                &self.presentation,
                &TranscriptViewportState::default(),
                list_state,
            )
        }

        pub(super) fn streamed_viewport_for_row(
            &self,
            row_index: usize,
            chunk_index: usize,
        ) -> TranscriptViewportState {
            let row = self
                .presentation
                .turn_at(row_index)
                .expect("test row should exist");
            let chunks = row.model.chunk_presentation().chunks();
            let chunk = chunks
                .get(chunk_index)
                .expect("test chunk should exist for streamed viewport");
            let mut viewport = TranscriptViewportState::default();
            viewport.anchor_streamed(
                TranscriptViewportTurnAnchor::new(
                    row.index,
                    Some(row.identity.as_str().to_string()),
                    row.turn.thread_id.clone(),
                    row.turn.turn_id.clone(),
                ),
                TranscriptViewportChunkAnchor::new(chunk_index, chunk.identity.clone()),
                chunks.len(),
                TranscriptViewportPlacement::Top,
            );
            viewport
        }

        pub(super) fn chunk_count_at(&self, row_index: usize) -> usize {
            self.presentation
                .turn_at(row_index)
                .map(|row| row.model.chunk_presentation().chunks().len())
                .unwrap_or_default()
        }
    }

    fn loaded_history_page(
        turns: &[TurnInfo],
        older_cursor: Option<&str>,
        newer_cursor: Option<&str>,
    ) -> LoadedTranscriptHistoryPage {
        LoadedTranscriptHistoryPage {
            turns: turns.to_vec(),
            older_cursor: older_cursor.map(str::to_string),
            newer_cursor: newer_cursor.map(str::to_string),
        }
    }

    pub(super) fn measured_list_state(
        item_count: usize,
        position: ListScrollPosition,
        viewport_height: Pixels,
        row_heights: &[Pixels],
    ) -> ListState {
        let state = ListState::new(item_count, ListAlignment::Bottom, px(0.0));
        test_support::set_measured_item_heights(&state, row_heights);
        test_support::set_viewport_height(&state, viewport_height);
        state.scroll_to_position(position);
        state
    }

    pub(super) fn unmeasured_list_state(item_count: usize) -> ListState {
        ListState::new(item_count, ListAlignment::Bottom, px(0.0))
    }

    pub(super) fn measured_virtual_tail_list_state(
        item_count: usize,
        viewport_height: Pixels,
        row_heights: &[Pixels],
        offset_from_content_end: Pixels,
    ) -> ListState {
        let state = ListState::new(item_count, ListAlignment::Bottom, px(0.0));
        test_support::set_measured_item_heights(&state, row_heights);
        test_support::set_viewport_height(&state, viewport_height);
        state.set_virtual_trailing_scroll_allowance(offset_from_content_end);
        state.scroll_to_position(ListScrollPosition::VirtualTail {
            offset_from_content_end,
        });
        state
    }

    pub(super) fn content_position(item_ix: usize) -> ListScrollPosition {
        ListScrollPosition::Content(ListOffset {
            item_ix,
            offset_in_item: px(0.0),
        })
    }

    pub(super) fn snapshot_parts(
        snapshot: TranscriptTurnNumberingSnapshot,
    ) -> (Option<usize>, Option<usize>) {
        (snapshot.current(), snapshot.total())
    }
}

#[test]
fn turn_view_current_maps_bottom_visible_row_to_absolute_number() {
    let harness = shell::TurnViewHarness::from_latest_page(
        vec![
            prompt_turn("turn_3", "third"),
            prompt_turn("turn_4", "fourth"),
            prompt_turn("turn_5", "fifth"),
        ],
        None,
        None,
    );
    let list_state = shell::measured_list_state(
        harness.presentation_len(),
        shell::content_position(1),
        px(20.0),
        &[px(20.0), px(20.0), px(20.0)],
    );

    assert_eq!(
        shell::snapshot_parts(harness.snapshot(&list_state)),
        (Some(2), Some(3))
    );
}

#[test]
fn turn_view_snapshot_hides_current_and_total_when_older_pages_may_exist() {
    let harness = shell::TurnViewHarness::from_latest_page(
        vec![
            prompt_turn("turn_3", "third"),
            prompt_turn("turn_4", "fourth"),
            prompt_turn("turn_5", "fifth"),
        ],
        Some("older"),
        None,
    );
    let list_state = shell::measured_list_state(
        harness.presentation_len(),
        shell::content_position(1),
        px(20.0),
        &[px(20.0), px(20.0), px(20.0)],
    );

    assert_eq!(
        shell::snapshot_parts(harness.snapshot(&list_state)),
        (None, None)
    );
}

#[test]
fn turn_view_snapshot_can_show_current_without_total_when_oldest_known_but_tail_stale() {
    let harness = shell::TurnViewHarness::from_latest_page(
        vec![
            prompt_turn("turn_1", "first"),
            prompt_turn("turn_2", "second"),
            prompt_turn("turn_3", "third"),
        ],
        None,
        Some("newer"),
    );
    let list_state = shell::measured_list_state(
        harness.presentation_len(),
        shell::content_position(1),
        px(20.0),
        &[px(20.0), px(20.0), px(20.0)],
    );

    assert_eq!(
        shell::snapshot_parts(harness.snapshot(&list_state)),
        (Some(2), None)
    );
}

#[test]
fn turn_view_current_skips_released_nonresident_source_slots() {
    let mut harness = shell::TurnViewHarness::from_latest_page(
        vec![
            prompt_turn("turn_3", "third"),
            prompt_turn("turn_4", "fourth"),
            prompt_turn("turn_5", "fifth"),
        ],
        None,
        None,
    );
    assert_eq!(
        harness.release_range(1..2),
        vec![shell::TranscriptPresentationMutation::Removed { index: 1, count: 1 }]
    );
    let list_state = shell::measured_list_state(
        harness.presentation_len(),
        shell::content_position(1),
        px(20.0),
        &[px(20.0), px(20.0)],
    );

    assert_eq!(
        shell::snapshot_parts(harness.snapshot(&list_state)),
        (Some(3), Some(3))
    );
}

#[test]
fn turn_view_current_preserves_source_after_full_page_restore() {
    let full_turn = prompt_turn("turn_2", "second");
    let mut harness = shell::TurnViewHarness::from_latest_page(
        vec![
            prompt_turn("turn_1", "first"),
            full_turn.clone(),
            prompt_turn("turn_3", "third"),
        ],
        None,
        None,
    );
    assert_eq!(
        harness.release_range(1..2),
        vec![shell::TranscriptPresentationMutation::Removed { index: 1, count: 1 }]
    );
    let list_state = shell::measured_list_state(
        harness.presentation_len(),
        shell::content_position(1),
        px(20.0),
        &[px(20.0), px(20.0)],
    );

    assert_eq!(
        shell::snapshot_parts(harness.snapshot(&list_state)),
        (Some(3), Some(3))
    );
    assert_eq!(
        harness.restore_history_page(1, &["turn_2".to_string()], vec![full_turn]),
        vec![shell::TranscriptPresentationMutation::Inserted { index: 1, count: 1 }]
    );
    list_state.splice(1..1, 1);

    assert_eq!(
        shell::snapshot_parts(harness.snapshot(&list_state)),
        (Some(3), Some(3))
    );
}

#[test]
fn turn_view_current_ignores_released_operational_source_slot() {
    let operational = command_only_turn("turn_2");
    let mut harness = shell::TurnViewHarness::from_latest_page(
        vec![
            prompt_turn("turn_1", "first"),
            operational.clone(),
            prompt_turn("turn_3", "third"),
        ],
        None,
        None,
    );
    assert_eq!(harness.presentation_len(), 2);
    assert_eq!(
        harness.release_range(1..2),
        vec![shell::TranscriptPresentationMutation::Unchanged]
    );
    let list_state = shell::measured_list_state(
        harness.presentation_len(),
        shell::content_position(1),
        px(20.0),
        &[px(20.0), px(20.0)],
    );

    assert_eq!(
        shell::snapshot_parts(harness.snapshot(&list_state)),
        (Some(3), Some(3))
    );
    assert_eq!(
        harness.restore_history_page(1, &["turn_2".to_string()], vec![operational]),
        vec![shell::TranscriptPresentationMutation::Unchanged]
    );

    assert_eq!(list_state.item_count(), harness.presentation_len());
    assert_eq!(
        shell::snapshot_parts(harness.snapshot(&list_state)),
        (Some(3), Some(3))
    );
}

#[test]
fn turn_view_current_ignores_released_final_operational_source_slot() {
    let operational = command_only_turn("turn_3");
    let mut harness = shell::TurnViewHarness::from_latest_page(
        vec![
            prompt_turn("turn_1", "first"),
            prompt_turn("turn_2", "second"),
            operational.clone(),
        ],
        None,
        None,
    );
    assert_eq!(harness.presentation_len(), 2);
    assert_eq!(
        harness.release_range(2..3),
        vec![shell::TranscriptPresentationMutation::Unchanged]
    );
    let list_state = shell::measured_list_state(
        harness.presentation_len(),
        shell::content_position(1),
        px(20.0),
        &[px(20.0), px(20.0)],
    );

    assert_eq!(
        shell::snapshot_parts(harness.snapshot(&list_state)),
        (Some(2), Some(3))
    );
    assert_eq!(
        harness.restore_history_page(2, &["turn_3".to_string()], vec![operational]),
        vec![shell::TranscriptPresentationMutation::Unchanged]
    );

    assert_eq!(list_state.item_count(), harness.presentation_len());
    assert_eq!(
        shell::snapshot_parts(harness.snapshot(&list_state)),
        (Some(2), Some(3))
    );
}

#[test]
fn turn_view_current_uses_final_turn_inside_virtual_tail_when_total_exact() {
    let harness = shell::TurnViewHarness::from_latest_page(
        vec![
            prompt_turn("turn_3", "third"),
            prompt_turn("turn_4", "fourth"),
            prompt_turn("turn_5", "fifth"),
        ],
        None,
        None,
    );
    let list_state = shell::measured_virtual_tail_list_state(
        harness.presentation_len(),
        px(40.0),
        &[px(20.0), px(20.0), px(20.0)],
        px(30.0),
    );

    assert_eq!(
        shell::snapshot_parts(harness.snapshot(&list_state)),
        (Some(3), Some(3))
    );
}

#[test]
fn turn_view_current_is_unknown_inside_virtual_tail_when_total_unknown() {
    let harness = shell::TurnViewHarness::from_latest_page(
        vec![
            prompt_turn("turn_3", "third"),
            prompt_turn("turn_4", "fourth"),
            prompt_turn("turn_5", "fifth"),
        ],
        None,
        Some("newer"),
    );
    let list_state = shell::measured_virtual_tail_list_state(
        harness.presentation_len(),
        px(40.0),
        &[px(20.0), px(20.0), px(20.0)],
        px(30.0),
    );

    assert_eq!(
        shell::snapshot_parts(harness.snapshot(&list_state)),
        (None, None)
    );
}

#[test]
fn turn_view_current_is_unknown_for_empty_visible_range_with_exact_total() {
    let harness =
        shell::TurnViewHarness::from_latest_page(vec![prompt_turn("turn_5", "fifth")], None, None);
    let list_state = shell::unmeasured_list_state(harness.presentation_len());

    assert_eq!(
        shell::snapshot_parts(harness.snapshot(&list_state)),
        (None, Some(1))
    );
}

#[test]
fn turn_view_snapshot_treats_exact_zero_as_unknown_parts() {
    let harness = shell::TurnViewHarness::from_latest_page(Vec::new(), None, None);
    let list_state = shell::unmeasured_list_state(0);

    assert_eq!(
        shell::snapshot_parts(harness.snapshot(&list_state)),
        (None, None)
    );
}

#[test]
fn turn_view_snapshot_is_unknown_without_selected_thread() {
    let harness =
        shell::TurnViewHarness::from_latest_page(vec![prompt_turn("turn_1", "first")], None, None);
    let list_state = shell::measured_list_state(
        harness.presentation_len(),
        shell::content_position(0),
        px(20.0),
        &[px(20.0)],
    );

    assert_eq!(
        shell::snapshot_parts(harness.unselected_snapshot(&list_state)),
        (None, None)
    );
}

#[test]
fn turn_view_current_does_not_promote_hidden_operational_tail_without_virtual_space() {
    let harness = shell::TurnViewHarness::from_latest_page(
        vec![prompt_turn("turn_1", "first"), command_only_turn("turn_2")],
        None,
        None,
    );
    let list_state = shell::measured_list_state(
        harness.presentation_len(),
        shell::content_position(0),
        px(20.0),
        &[px(20.0)],
    );

    assert_eq!(
        shell::snapshot_parts(harness.snapshot(&list_state)),
        (Some(1), Some(2))
    );
}

#[test]
fn turn_view_virtual_tail_can_report_hidden_operational_final_turn_when_total_exact() {
    let harness = shell::TurnViewHarness::from_latest_page(
        vec![prompt_turn("turn_1", "first"), command_only_turn("turn_2")],
        None,
        None,
    );
    let list_state = shell::measured_virtual_tail_list_state(
        harness.presentation_len(),
        px(40.0),
        &[px(20.0)],
        px(30.0),
    );

    assert_eq!(
        shell::snapshot_parts(harness.snapshot(&list_state)),
        (Some(2), Some(2))
    );
}

#[test]
fn turn_view_source_index_counts_hidden_operational_prefix_turns() {
    let harness = shell::TurnViewHarness::from_latest_page(
        vec![command_only_turn("turn_1"), prompt_turn("turn_2", "second")],
        None,
        None,
    );
    let list_state = shell::measured_list_state(
        harness.presentation_len(),
        shell::content_position(0),
        px(20.0),
        &[px(20.0)],
    );

    assert_eq!(
        shell::snapshot_parts(harness.snapshot(&list_state)),
        (Some(2), Some(2))
    );
}

#[test]
fn turn_view_current_uses_streamed_anchor_turn_not_visible_chunk_or_adjacent_row() {
    let huge_prompt = (0..80)
        .map(|index| format!("Huge prompt paragraph {index}."))
        .collect::<Vec<_>>()
        .join("\n\n");
    let harness = shell::TurnViewHarness::from_latest_page(
        vec![
            prompt_turn("turn_1", "first"),
            prompt_turn("turn_2", &huge_prompt),
            prompt_turn("turn_3", "third"),
        ],
        None,
        None,
    );
    assert!(harness.chunk_count_at(1) > 1);
    let viewport = harness.streamed_viewport_for_row(1, 1);
    let list_state = shell::measured_list_state(
        harness.presentation_len(),
        shell::content_position(1),
        px(40.0),
        &[px(20.0), px(20.0), px(20.0)],
    );

    assert_eq!(
        shell::snapshot_parts(harness.snapshot_with_viewport(&viewport, &list_state)),
        (Some(2), Some(3))
    );
}

fn prompt_turn(id: &str, prompt: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items_view: TurnItemsView::Full,
        items: vec![ThreadItem::UserMessage(UserMessageItem {
            id: format!("{id}_user"),
            content: vec![UserInput::Text {
                text: prompt.to_string(),
            }],
        })],
        error: None,
    }
}

fn command_only_turn(id: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items_view: TurnItemsView::Full,
        items: vec![ThreadItem::CommandExecution(CommandExecutionItem {
            id: format!("{id}_command"),
            command: "cargo metadata".to_string(),
            cwd: "C:\\repo".to_string(),
            status: CommandExecutionStatus::Completed,
            process_id: None,
            aggregated_output: Some("{}".to_string()),
            exit_code: Some(0),
            duration_ms: Some(10),
        })],
        error: None,
    }
}
