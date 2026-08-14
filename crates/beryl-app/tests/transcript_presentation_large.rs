use std::path::PathBuf;

use beryl_backend::{
    AgentMessageItem, CommandExecutionItem, CommandExecutionStatus, FileChangeItem,
    FileUpdateChange, ImageGenerationItem, PatchApplyStatus, PatchChangeKind, ThreadItem, TurnInfo,
    TurnStatus, TurnStreamEvent, UserInput, UserMessageItem,
};
use gpui::{Pixels, px};

mod shell {
    #[path = "../../src/shell/execution_detail.rs"]
    mod execution_detail;
    #[path = "../../src/shell/transcript_presentation.rs"]
    mod transcript_presentation;
    #[path = "../../src/shell/transcript_projection.rs"]
    mod transcript_projection;
    #[allow(dead_code)]
    #[path = "../../src/shell/virtual_list/mod.rs"]
    mod virtual_list;

    use std::ops::Range;

    use beryl_backend::{TurnInfo, TurnStreamEvent};
    use gpui::Pixels;

    pub(super) use transcript_presentation::{
        TRANSCRIPT_INITIAL_PRESENTATION_ROWS, TRANSCRIPT_MAX_PRESENTATION_ROWS,
        transcript_frame_preload_range, transcript_frame_presentation_range,
    };
    pub(super) use virtual_list::{
        ListAlignment, ListOffset, ListScrollPosition, ListState, test_support,
    };

    pub(super) struct PresentationHarness {
        details: execution_detail::ExecutionDetailState,
        presentation: transcript_presentation::TranscriptPresentationState,
    }

    impl PresentationHarness {
        pub(super) fn new() -> Self {
            Self {
                details: execution_detail::ExecutionDetailState::default(),
                presentation: transcript_presentation::TranscriptPresentationState::default(),
            }
        }

        pub(super) fn replace_history(&mut self, thread_id: &str, turns: Vec<TurnInfo>) {
            self.details = execution_detail::ExecutionDetailState::default();
            self.details.prepend_thread_history_page(thread_id, turns);
            self.presentation.replace_from_turns(self.details.turns());
        }

        pub(super) fn begin_live_turn(&mut self, prompt: &str) -> usize {
            let index = self.details.begin_turn(prompt.to_string());
            let turn = self.details.turns()[index].clone();
            self.presentation
                .append_turn(index, turn)
                .expect("live prompt should project into a transcript row")
        }

        pub(super) fn apply_stream_event(&mut self, event: TurnStreamEvent) -> Option<usize> {
            let index = self.details.apply_stream_event(event)?;
            let turn = self.details.turns()[index].clone();
            self.presentation.replace_turn(index, turn);
            Some(index)
        }

        pub(super) fn release_range_with_heights(
            &mut self,
            range: Range<usize>,
            heights: &[Pixels],
        ) -> usize {
            let start = range.start;
            let replacements = self.details.release_history_range(range);
            let count = replacements.len();
            for replacement in replacements {
                let height = heights.get(replacement.index - start).copied();
                self.presentation.replace_turn_with_placeholder(
                    replacement.index,
                    replacement.turn,
                    height,
                );
            }
            count
        }

        pub(super) fn panel_state_for_range(
            &self,
            range: Range<usize>,
        ) -> transcript_presentation::TranscriptPresentationPanelState {
            self.presentation.panel_state_for_range(range)
        }

        pub(super) fn turn_count(&self) -> usize {
            self.presentation.len()
        }
    }
}

use shell::{
    ListAlignment, ListOffset, ListScrollPosition, ListState, PresentationHarness,
    TRANSCRIPT_INITIAL_PRESENTATION_ROWS, TRANSCRIPT_MAX_PRESENTATION_ROWS, test_support,
    transcript_frame_preload_range, transcript_frame_presentation_range,
};

#[test]
fn large_synthetic_transcript_frame_prep_is_bounded_across_measured_ranges() {
    let mut harness = PresentationHarness::new();
    harness.replace_history("thread_a", large_mixed_transcript(5_000));
    harness.release_range_with_heights(0..320, &vec![px(72.0); 320]);
    harness.release_range_with_heights(3_000..3_240, &vec![px(96.0); 240]);
    let file_change_index = append_live_file_change_output_turn(&mut harness);
    let turn_count = harness.turn_count();
    let row_heights = vec![px(24.0); turn_count];
    let viewport_height = px(180.0);
    let overdraw = px(60.0);

    let measured_states = [
        (
            "top",
            measured_list_state(
                turn_count,
                ListScrollPosition::Content(ListOffset {
                    item_ix: 0,
                    offset_in_item: px(0.0),
                }),
                viewport_height,
                overdraw,
                &row_heights,
            ),
        ),
        (
            "middle",
            measured_list_state(
                turn_count,
                ListScrollPosition::Content(ListOffset {
                    item_ix: 2_500,
                    offset_in_item: px(0.0),
                }),
                viewport_height,
                overdraw,
                &row_heights,
            ),
        ),
        (
            "tail",
            measured_list_state(
                turn_count,
                ListScrollPosition::Content(ListOffset {
                    item_ix: turn_count - 8,
                    offset_in_item: px(0.0),
                }),
                viewport_height,
                overdraw,
                &row_heights,
            ),
        ),
        (
            "virtual-tail",
            measured_virtual_tail_list_state(
                turn_count,
                viewport_height,
                overdraw,
                &row_heights,
                px(120.0),
            ),
        ),
    ];

    for (label, list_state) in measured_states {
        let production_range = test_support::presentation_range(&list_state);
        let frame_range = transcript_frame_presentation_range(&list_state, turn_count);
        let panel_state = harness.panel_state_for_range(frame_range.clone());

        assert_eq!(
            frame_range, production_range,
            "{label} should use measured production geometry, not fallback"
        );
        assert_eq!(
            panel_state.inspected_row_count,
            frame_range.len(),
            "{label} panel prep should inspect exactly the frame range"
        );
        assert!(
            panel_state.inspected_row_count < TRANSCRIPT_INITIAL_PRESENTATION_ROWS,
            "{label} frame prep should stay tied to viewport geometry"
        );
        assert!(
            panel_state.inspected_row_count <= TRANSCRIPT_MAX_PRESENTATION_ROWS,
            "{label} frame prep should stay below the hard frame cap"
        );
        assert!(
            panel_state.inspected_row_count * 100 < turn_count,
            "{label} frame prep should stay far below total loaded turns"
        );
    }

    let middle_state = harness.panel_state_for_range(2_500..2_510);
    assert!(middle_state.active_nested_code_panel_ids.is_empty());

    let file_change_state = harness.panel_state_for_range(file_change_index..file_change_index + 1);
    assert_eq!(turn_count, 5_001);
    assert_eq!(file_change_state.inspected_row_count, 1);
    assert!(file_change_state.active_nested_code_panel_ids.is_empty());
}

#[test]
fn operational_only_large_history_rows_do_not_contribute_scroll_geometry() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![
            command_only_turn("turn_cmd", "command_hidden"),
            prompt_turn("turn_prompt", "Visible prompt"),
            file_change_only_turn("turn_file", "file_change_hidden"),
            agent_markdown_turn("turn_answer", "Visible answer"),
        ],
    );

    assert_eq!(harness.turn_count(), 2);
    assert_eq!(
        transcript_frame_presentation_range(
            &ListState::new(harness.turn_count(), ListAlignment::Bottom, px(60.0)),
            harness.turn_count()
        ),
        0..2
    );
}

#[test]
fn large_media_history_frame_prep_stays_viewport_windowed() {
    let mut harness = PresentationHarness::new();
    harness.replace_history("thread_a", large_media_transcript(3_000));
    let turn_count = harness.turn_count();
    let row_heights = vec![px(180.0); turn_count];
    let list_state = measured_list_state(
        turn_count,
        ListScrollPosition::Content(ListOffset {
            item_ix: 1_420,
            offset_in_item: px(0.0),
        }),
        px(360.0),
        px(120.0),
        &row_heights,
    );

    let frame_range = transcript_frame_presentation_range(&list_state, turn_count);
    let panel_state = harness.panel_state_for_range(frame_range.clone());

    assert_eq!(turn_count, 3_000);
    assert_eq!(panel_state.inspected_row_count, frame_range.len());
    assert!(
        panel_state.inspected_row_count <= TRANSCRIPT_MAX_PRESENTATION_ROWS,
        "media-heavy frame prep should stay below the hard frame cap"
    );
    assert!(
        panel_state.inspected_row_count * 100 < turn_count,
        "media-heavy frame prep should stay far below total loaded turns"
    );
    assert!(panel_state.active_nested_code_panel_ids.is_empty());
}

#[test]
fn transcript_media_preload_range_uses_half_viewport_margin_without_total_history_scan() {
    let turn_count = 1_000;
    let row_heights = vec![px(100.0); turn_count];
    let list_state = measured_list_state(
        turn_count,
        ListScrollPosition::Content(ListOffset {
            item_ix: 10,
            offset_in_item: px(0.0),
        }),
        px(200.0),
        px(0.0),
        &row_heights,
    );

    let visible = list_state.visible_range();
    let preload = transcript_frame_preload_range(&list_state, turn_count, px(100.0));

    assert_eq!(visible, 10..12);
    assert_eq!(preload, 9..13);
    assert!(preload.len() < turn_count / 100);
}

#[test]
fn transcript_media_preload_range_is_hard_capped() {
    let turn_count = 5_000;
    let row_heights = vec![px(1.0); turn_count];
    let list_state = measured_list_state(
        turn_count,
        ListScrollPosition::Content(ListOffset {
            item_ix: 2_400,
            offset_in_item: px(0.0),
        }),
        px(100.0),
        px(0.0),
        &row_heights,
    );

    let preload = transcript_frame_preload_range(&list_state, turn_count, px(10_000.0));
    let visible = list_state.visible_range();

    assert!(preload.len() <= TRANSCRIPT_MAX_PRESENTATION_ROWS);
    assert!(preload.start <= visible.start);
    assert!(preload.end >= visible.end);
    assert!(preload.len() * 10 < turn_count);
}

#[test]
fn large_synthetic_transcript_frame_prep_uses_fallback_cap_when_geometry_is_unmeasured() {
    let turn_count = 5_000;
    let content_state = ListState::new(turn_count, ListAlignment::Top, px(60.0));
    content_state.scroll_to(ListOffset {
        item_ix: 2_400,
        offset_in_item: px(0.0),
    });

    assert_eq!(
        transcript_frame_presentation_range(&content_state, turn_count),
        2_400..(2_400 + TRANSCRIPT_INITIAL_PRESENTATION_ROWS)
    );

    let bottom_state = ListState::new(turn_count, ListAlignment::Bottom, px(60.0));
    assert_eq!(
        transcript_frame_presentation_range(&bottom_state, turn_count),
        (turn_count - TRANSCRIPT_INITIAL_PRESENTATION_ROWS)..turn_count
    );

    let virtual_tail_state = ListState::new(turn_count, ListAlignment::Bottom, px(60.0));
    virtual_tail_state.set_virtual_trailing_scroll_allowance(px(120.0));
    virtual_tail_state.scroll_to_position(ListScrollPosition::VirtualTail {
        offset_from_content_end: px(80.0),
    });
    assert_eq!(
        transcript_frame_presentation_range(&virtual_tail_state, turn_count),
        (turn_count - TRANSCRIPT_INITIAL_PRESENTATION_ROWS)..turn_count
    );
}

fn prompt_turn(id: &str, prompt: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: vec![ThreadItem::UserMessage(UserMessageItem {
            id: format!("{id}_user"),
            content: vec![UserInput::Text {
                text: prompt.to_string(),
            }],
        })],
        error: None,
    }
}

fn agent_markdown_turn(id: &str, text: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: vec![ThreadItem::AgentMessage(AgentMessageItem {
            id: format!("{id}_agent"),
            text: text.to_string(),
            phase: None,
        })],
        error: None,
    }
}

fn generated_image_turn(id: &str, index: usize, has_result: bool) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: vec![ThreadItem::ImageGeneration(ImageGenerationItem {
            id: format!("{id}_generated_image"),
            status: Some(
                if has_result {
                    "completed"
                } else {
                    "generating"
                }
                .to_string(),
            ),
            revised_prompt: Some(format!("Generated media row {index}")),
            result: has_result.then(|| "iVBORw0KGgo=".to_string()),
            saved_path: Some(format!("C:\\repo\\generated\\image_{index}.png")),
        })],
        error: None,
    }
}

fn prompt_command_turn(
    id: &str,
    prompt: &str,
    item_id: &str,
    command: &str,
    output: &str,
) -> TurnInfo {
    let mut turn = prompt_turn(id, prompt);
    turn.items
        .push(ThreadItem::CommandExecution(CommandExecutionItem {
            id: item_id.to_string(),
            command: command.to_string(),
            cwd: "C:\\repo".to_string(),
            status: CommandExecutionStatus::Completed,
            process_id: None,
            aggregated_output: Some(output.to_string()),
            exit_code: Some(0),
            duration_ms: Some(10),
        }));
    turn
}

fn prompt_file_change_turn(id: &str, prompt: &str, item_id: &str) -> TurnInfo {
    let mut turn = prompt_turn(id, prompt);
    turn.items.push(ThreadItem::FileChange(FileChangeItem {
        id: item_id.to_string(),
        status: PatchApplyStatus::Completed,
        changes: vec![FileUpdateChange {
            path: PathBuf::from(format!("src/{id}.rs")),
            diff: "@@ -1 +1\n-old\n+new\n".to_string(),
            kind: PatchChangeKind::Update { move_path: None },
        }],
    }));
    turn
}

fn command_only_turn(id: &str, item_id: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: vec![ThreadItem::CommandExecution(CommandExecutionItem {
            id: item_id.to_string(),
            command: "cargo nextest".to_string(),
            cwd: "C:\\repo".to_string(),
            status: CommandExecutionStatus::Completed,
            process_id: None,
            aggregated_output: Some("hidden output".to_string()),
            exit_code: Some(0),
            duration_ms: Some(10),
        })],
        error: None,
    }
}

fn file_change_only_turn(id: &str, item_id: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: vec![ThreadItem::FileChange(FileChangeItem {
            id: item_id.to_string(),
            status: PatchApplyStatus::Completed,
            changes: vec![FileUpdateChange {
                path: PathBuf::from(format!("src/{id}.rs")),
                diff: "@@ -1 +1\n-old\n+new\n".to_string(),
                kind: PatchChangeKind::Update { move_path: None },
            }],
        })],
        error: None,
    }
}

fn large_mixed_transcript(count: usize) -> Vec<TurnInfo> {
    (0..count)
        .map(|index| match index % 5 {
            0 => prompt_turn(&format!("turn_{index}"), &format!("Prompt {index}")),
            1 => agent_markdown_turn(
                &format!("turn_{index}"),
                &format!(
                    "# Result {index}\n\n- item {index}.0\n- item {index}.1\n\n```rust\nfn value() -> usize {{ {index} }}\n```"
                ),
            ),
            2 => prompt_command_turn(
                &format!("turn_{index}"),
                &format!("Prompt {index}"),
                &format!("command_{index}"),
                "cargo nextest",
                &format!("test output for turn {index}\nline 2"),
            ),
            3 => prompt_command_turn(
                &format!("turn_{index}"),
                &format!("Prompt {index}"),
                &format!("command_{index}"),
                "cargo fmt",
                "",
            ),
            _ => prompt_file_change_turn(
                &format!("turn_{index}"),
                &format!("Prompt {index}"),
                &format!("file_change_{index}"),
            ),
        })
        .collect()
}

fn large_media_transcript(count: usize) -> Vec<TurnInfo> {
    (0..count)
        .map(|index| match index % 4 {
            0 => agent_markdown_turn(
                &format!("turn_{index}"),
                &format!(
                    "Before ![cat {index}](images/cat_{index}.png) ![hat {index}](images/hat_{index}.png) after"
                ),
            ),
            1 => generated_image_turn(&format!("turn_{index}"), index, true),
            2 => agent_markdown_turn(
                &format!("turn_{index}"),
                &format!("Fallback ![svg {index}](images/vector_{index}.svg) done"),
            ),
            _ => generated_image_turn(&format!("turn_{index}"), index, false),
        })
        .collect()
}

fn append_live_file_change_output_turn(harness: &mut PresentationHarness) -> usize {
    let index = harness.begin_live_turn("Apply patch");
    let thread_id = "thread_a".to_string();
    let turn_id = "turn_file_change_live".to_string();
    let item_id = "file_change_live".to_string();
    let item = ThreadItem::FileChange(FileChangeItem {
        id: item_id.clone(),
        status: PatchApplyStatus::InProgress,
        changes: vec![FileUpdateChange {
            path: PathBuf::from("src/live.rs"),
            diff: "@@ -1 +1\n-before\n+after\n".to_string(),
            kind: PatchChangeKind::Update { move_path: None },
        }],
    });

    harness
        .apply_stream_event(TurnStreamEvent::TurnStarted {
            thread_id: thread_id.clone(),
            turn: empty_turn(&turn_id, TurnStatus::InProgress),
        })
        .unwrap();
    harness
        .apply_stream_event(TurnStreamEvent::ItemStarted {
            thread_id: thread_id.clone(),
            turn_id: turn_id.clone(),
            item: item.clone(),
        })
        .unwrap();
    harness
        .apply_stream_event(TurnStreamEvent::FileChangeOutputDelta {
            thread_id: thread_id.clone(),
            turn_id: turn_id.clone(),
            item_id: item_id.clone(),
            delta: "applying patch\npatch applied\n".to_string(),
        })
        .unwrap();
    harness
        .apply_stream_event(TurnStreamEvent::ItemCompleted {
            thread_id: thread_id.clone(),
            turn_id: turn_id.clone(),
            item,
        })
        .unwrap();
    harness
        .apply_stream_event(TurnStreamEvent::TurnCompleted {
            thread_id,
            turn: empty_turn(&turn_id, TurnStatus::Completed),
        })
        .unwrap();

    index
}

fn measured_list_state(
    turn_count: usize,
    position: ListScrollPosition,
    viewport_height: Pixels,
    overdraw: Pixels,
    row_heights: &[Pixels],
) -> ListState {
    let state = ListState::new(turn_count, ListAlignment::Top, overdraw);
    test_support::set_measured_item_heights(&state, row_heights);
    test_support::set_viewport_height(&state, viewport_height);
    state.scroll_to_position(position);
    state
}

fn measured_virtual_tail_list_state(
    turn_count: usize,
    viewport_height: Pixels,
    overdraw: Pixels,
    row_heights: &[Pixels],
    offset_from_content_end: Pixels,
) -> ListState {
    let state = ListState::new(turn_count, ListAlignment::Bottom, overdraw);
    test_support::set_measured_item_heights(&state, row_heights);
    test_support::set_viewport_height(&state, viewport_height);
    state.set_virtual_trailing_scroll_allowance(offset_from_content_end);
    state.scroll_to_position(ListScrollPosition::VirtualTail {
        offset_from_content_end,
    });
    state
}

fn empty_turn(id: &str, status: TurnStatus) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status,
        items: Vec::new(),
        error: None,
    }
}
