use std::path::PathBuf;

use beryl_backend::{
    AgentMessageItem, CommandExecutionItem, CommandExecutionStatus, FileChangeItem,
    FileUpdateChange, PatchApplyStatus, ProtocolPhase, ReasoningItem, ThreadItem, TurnInfo,
    TurnStatus, TurnStreamEvent, UserInput, UserMessageItem,
};
use gpui::px;

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

        pub(super) fn prepend_history(&mut self, thread_id: &str, turns: Vec<TurnInfo>) -> usize {
            let added = self.details.prepend_thread_history_page(thread_id, turns);
            self.presentation
                .prepend_from_turns(&self.details.turns()[..added]);
            added
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

        pub(super) fn append_live_user_fragment(&mut self, index: usize, text: &str) {
            self.details
                .append_user_input_fragment(index, execution_detail::UserInputFragment::text(text))
                .expect("live turn should exist");
            let turn = self.details.turns()[index].clone();
            self.presentation.replace_turn(index, turn);
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

        pub(super) fn row_identity(&self, index: usize) -> String {
            self.presentation
                .row_identity(index)
                .unwrap()
                .as_str()
                .to_string()
        }

        pub(super) fn row_index_for_identity(&self, identity: &str) -> Option<usize> {
            self.presentation.row_index_for_identity(identity)
        }

        pub(super) fn turn_id_at(&self, index: usize) -> Option<String> {
            self.presentation
                .turn_at(index)
                .and_then(|row| row.turn.turn_id.clone())
        }

        pub(super) fn placeholder_height_at(&self, index: usize) -> Option<Pixels> {
            self.presentation
                .turn_at(index)
                .and_then(|row| row.placeholder_height)
        }

        pub(super) fn is_placeholder_at(&self, index: usize) -> bool {
            self.presentation
                .turn_at(index)
                .is_some_and(|row| row.turn.is_released_history_placeholder())
        }

        pub(super) fn window_turn_ids(&self, range: Range<usize>) -> Vec<String> {
            self.presentation
                .window_for_range(range)
                .rows()
                .iter()
                .map(|row| row.turn.turn_id.clone().unwrap())
                .collect()
        }

        pub(super) fn presentation_len(&self) -> usize {
            self.presentation.len()
        }

        pub(super) fn source_turn_index_at(&self, index: usize) -> Option<usize> {
            self.presentation.source_turn_index_at(index)
        }

        pub(super) fn visible_item_kinds_at(&self, index: usize) -> Vec<String> {
            self.presentation
                .turn_at(index)
                .map(|row| {
                    row.turn
                        .items
                        .iter()
                        .map(|item| match item {
                            execution_detail::ExecutionItem::AgentMessage(item) => {
                                format!("agent:{:?}", item.phase)
                            }
                            execution_detail::ExecutionItem::Reasoning(_) => {
                                "reasoning".to_string()
                            }
                            execution_detail::ExecutionItem::CommandExecution(_) => {
                                "command".to_string()
                            }
                            execution_detail::ExecutionItem::FileChange(_) => {
                                "file-change".to_string()
                            }
                            execution_detail::ExecutionItem::GeneratedImage(_) => {
                                "generated-image".to_string()
                            }
                            execution_detail::ExecutionItem::Generic(item) => {
                                format!("generic:{}", item.item_type)
                            }
                        })
                        .collect()
                })
                .unwrap_or_default()
        }

        pub(super) fn visible_narrative_texts_at(&self, index: usize) -> Vec<String> {
            self.presentation
                .turn_at(index)
                .map(|row| {
                    row.turn
                        .narrative_entries()
                        .iter()
                        .filter_map(|entry| match entry {
                            execution_detail::TurnNarrativeEntry::UserInput { fragment_id } => row
                                .turn
                                .user_input_fragment_by_id(*fragment_id)
                                .map(|(_, fragment)| format!("user: {}", fragment.text)),
                            execution_detail::TurnNarrativeEntry::Item { item_id } => {
                                row.turn.item_by_id(item_id).and_then(|item| match item {
                                    execution_detail::ExecutionItem::AgentMessage(message) => {
                                        Some(format!("assistant: {}", message.text))
                                    }
                                    execution_detail::ExecutionItem::Reasoning(reasoning) => {
                                        Some(format!("reasoning: {}", reasoning.summary.join("")))
                                    }
                                    _ => None,
                                })
                            }
                        })
                        .collect()
                })
                .unwrap_or_default()
        }

        pub(super) fn visible_reasoning_parts_at(
            &self,
            index: usize,
        ) -> Option<(Vec<String>, Vec<String>)> {
            self.presentation.turn_at(index).and_then(|row| {
                row.turn.items.iter().find_map(|item| match item {
                    execution_detail::ExecutionItem::Reasoning(item) => {
                        Some((item.summary.clone(), item.content.clone()))
                    }
                    _ => None,
                })
            })
        }

        pub(super) fn internal_item_kinds_at(&self, index: usize) -> Vec<String> {
            self.details
                .turns()
                .get(index)
                .map(|turn| {
                    turn.items
                        .iter()
                        .map(|item| match item {
                            execution_detail::ExecutionItem::AgentMessage(item) => {
                                format!("agent:{:?}", item.phase)
                            }
                            execution_detail::ExecutionItem::Reasoning(_) => {
                                "reasoning".to_string()
                            }
                            execution_detail::ExecutionItem::CommandExecution(_) => {
                                "command".to_string()
                            }
                            execution_detail::ExecutionItem::FileChange(_) => {
                                "file-change".to_string()
                            }
                            execution_detail::ExecutionItem::GeneratedImage(_) => {
                                "generated-image".to_string()
                            }
                            execution_detail::ExecutionItem::Generic(item) => {
                                format!("generic:{}", item.item_type)
                            }
                        })
                        .collect()
                })
                .unwrap_or_default()
        }

        pub(super) fn internal_command_output_at(&self, index: usize) -> Option<String> {
            self.details.turns().get(index).and_then(|turn| {
                turn.items.iter().find_map(|item| match item {
                    execution_detail::ExecutionItem::CommandExecution(item) => {
                        Some(item.output.clone())
                    }
                    _ => None,
                })
            })
        }

        pub(super) fn latest_user_prompt_anchor(&self) -> Option<(usize, usize, String)> {
            self.presentation.latest_user_prompt_anchor()
        }

        pub(super) fn activity_caret(&self) -> Option<(usize, String)> {
            self.presentation
                .activity_caret_for_source_turn(self.details.working_turn_index())
                .map(|caret| (caret.row_index, caret.row_identity.as_str().to_string()))
        }

        pub(super) fn panel_state_for_range(
            &self,
            range: Range<usize>,
        ) -> transcript_presentation::TranscriptPresentationPanelState {
            self.presentation.panel_state_for_range(range)
        }

        pub(super) fn render_metrics(&self) -> (usize, usize, usize) {
            let metrics = self.presentation.render_metrics();
            (
                metrics.total_turns,
                metrics.total_item_count,
                metrics.total_text_chars,
            )
        }

        pub(super) fn retained_counts(&self) -> (usize, usize, usize) {
            let counts = self.presentation.retained_counts();
            (counts.rows, counts.items, counts.text_bytes)
        }
    }
}

use shell::PresentationHarness;

#[test]
fn row_identity_survives_older_history_prepend() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![
            prompt_turn("turn_3", "Prompt 3"),
            prompt_turn("turn_4", "Prompt 4"),
        ],
    );
    let turn_3_identity = harness.row_identity(0);
    let turn_4_identity = harness.row_identity(1);

    assert_eq!(
        harness.prepend_history(
            "thread_a",
            vec![
                prompt_turn("turn_1", "Prompt 1"),
                prompt_turn("turn_2", "Prompt 2")
            ],
        ),
        2
    );

    assert_eq!(harness.row_identity(2), turn_3_identity);
    assert_eq!(harness.row_identity(3), turn_4_identity);
    assert_eq!(
        harness.window_turn_ids(1..4),
        vec!["turn_2", "turn_3", "turn_4"]
    );
}

#[test]
fn row_identity_lookup_tracks_index_after_older_history_prepend() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![
            prompt_turn("turn_3", "Prompt 3"),
            prompt_turn("turn_4", "Prompt 4"),
        ],
    );
    let turn_4_identity = harness.row_identity(1);

    assert_eq!(harness.row_index_for_identity(&turn_4_identity), Some(1));

    harness.prepend_history(
        "thread_a",
        vec![
            prompt_turn("turn_1", "Prompt 1"),
            prompt_turn("turn_2", "Prompt 2"),
        ],
    );

    assert_eq!(harness.row_index_for_identity(&turn_4_identity), Some(3));
    assert_eq!(harness.row_index_for_identity("missing-row"), None);
}

#[test]
fn latest_user_prompt_anchor_shifts_on_prepend_and_moves_on_append() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![
            prompt_turn("turn_3", "Prompt 3"),
            prompt_turn("turn_4", "Prompt 4"),
        ],
    );
    assert_eq!(
        harness.latest_user_prompt_anchor(),
        Some((1, 0, "Prompt 4".to_string()))
    );

    harness.prepend_history(
        "thread_a",
        vec![
            prompt_turn("turn_1", "Prompt 1"),
            prompt_turn("turn_2", "Prompt 2"),
        ],
    );
    assert_eq!(
        harness.latest_user_prompt_anchor(),
        Some((3, 0, "Prompt 4".to_string()))
    );

    let live_index = harness.begin_live_turn("Live prompt");
    assert_eq!(live_index, 4);
    assert_eq!(
        harness.latest_user_prompt_anchor(),
        Some((4, 0, "Live prompt".to_string()))
    );
}

#[test]
fn multiple_user_fragments_share_one_turn_row_and_anchor_latest_fragment() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![prompt_turn_with_fragments(
            "turn_1",
            &["First fragment", "Second fragment"],
        )],
    );

    assert_eq!(harness.presentation_len(), 1);
    assert_eq!(
        harness.latest_user_prompt_anchor(),
        Some((0, 1, "Second fragment".to_string()))
    );
    assert_eq!(
        harness.render_metrics(),
        (1, 0, "First fragment".len() + "Second fragment".len())
    );
}

#[test]
fn historical_parent_narrative_projection_hides_operational_items_but_keeps_detail_state() {
    let mut harness = PresentationHarness::new();
    harness.replace_history("thread_a", vec![mixed_operational_turn("turn_1")]);

    assert_eq!(harness.presentation_len(), 1);
    assert_eq!(harness.source_turn_index_at(0), Some(0));
    assert_eq!(
        harness.visible_item_kinds_at(0),
        vec![
            "agent:Some(Commentary)".to_string(),
            "reasoning".to_string(),
            "agent:Some(FinalAnswer)".to_string(),
        ]
    );
    assert_eq!(
        harness.visible_reasoning_parts_at(0),
        Some((
            vec!["I inspected the package layout.".to_string()],
            Vec::new()
        ))
    );
    assert_eq!(
        harness.internal_item_kinds_at(0),
        vec![
            "agent:Some(Commentary)".to_string(),
            "command".to_string(),
            "file-change".to_string(),
            "reasoning".to_string(),
            "agent:Some(FinalAnswer)".to_string(),
        ]
    );

    let panel_state = harness.panel_state_for_range(0..1);
    assert!(panel_state.active_nested_code_panel_ids.is_empty());
}

#[test]
fn transcript_render_metrics_count_only_projected_parent_narrative() {
    let mut harness = PresentationHarness::new();
    harness.replace_history("thread_a", vec![mixed_operational_turn("turn_1")]);

    let expected_text_chars = "Explain the workspace".len()
        + "I will inspect the package layout.".len()
        + "I inspected the package layout.".len()
        + "The workspace has a root Cargo package.".len();
    let metrics = harness.render_metrics();

    assert_eq!(metrics, (1, 3, expected_text_chars));
}

#[test]
fn transcript_presentation_retained_counts_match_projected_rows() {
    let mut harness = PresentationHarness::new();
    harness.replace_history("thread_a", vec![mixed_operational_turn("turn_1")]);

    let expected_text_bytes = "Explain the workspace".len()
        + "I will inspect the package layout.".len()
        + "I inspected the package layout.".len()
        + "The workspace has a root Cargo package.".len();

    assert_eq!(harness.retained_counts(), (1, 3, expected_text_bytes));
}

#[test]
fn live_parent_narrative_projection_updates_without_operational_rows() {
    let mut harness = PresentationHarness::new();
    let live_index = harness.begin_live_turn("Inspect the workspace");
    let live_identity = harness.row_identity(live_index);

    harness
        .apply_stream_event(TurnStreamEvent::TurnStarted {
            thread_id: "thread_a".to_string(),
            turn: empty_turn("turn_live", TurnStatus::InProgress),
        })
        .unwrap();
    harness
        .apply_stream_event(TurnStreamEvent::ItemStarted {
            thread_id: "thread_a".to_string(),
            turn_id: "turn_live".to_string(),
            item: ThreadItem::CommandExecution(CommandExecutionItem {
                id: "cmd_live".to_string(),
                command: "cargo nextest run".to_string(),
                cwd: "C:\\repo".to_string(),
                status: CommandExecutionStatus::InProgress,
                process_id: None,
                aggregated_output: None,
                exit_code: None,
                duration_ms: None,
            }),
        })
        .unwrap();
    harness
        .apply_stream_event(TurnStreamEvent::CommandExecutionOutputDelta {
            thread_id: "thread_a".to_string(),
            turn_id: "turn_live".to_string(),
            item_id: "cmd_live".to_string(),
            delta: "running 1 test\n".to_string(),
        })
        .unwrap();
    harness
        .apply_stream_event(TurnStreamEvent::ReasoningSummaryPartAdded {
            thread_id: "thread_a".to_string(),
            turn_id: "turn_live".to_string(),
            item_id: "reason_live".to_string(),
            summary_index: 0,
        })
        .unwrap();
    harness
        .apply_stream_event(TurnStreamEvent::ReasoningSummaryTextDelta {
            thread_id: "thread_a".to_string(),
            turn_id: "turn_live".to_string(),
            item_id: "reason_live".to_string(),
            summary_index: 0,
            delta: "Checked the failing test target.".to_string(),
        })
        .unwrap();
    harness
        .apply_stream_event(TurnStreamEvent::ItemCompleted {
            thread_id: "thread_a".to_string(),
            turn_id: "turn_live".to_string(),
            item: ThreadItem::AgentMessage(AgentMessageItem {
                id: "answer_live".to_string(),
                phase: Some(ProtocolPhase::FinalAnswer),
                text: "The focused test passes.".to_string(),
            }),
        })
        .unwrap();

    assert_eq!(harness.row_identity(live_index), live_identity);
    assert_eq!(
        harness.visible_item_kinds_at(live_index),
        vec![
            "reasoning".to_string(),
            "agent:Some(FinalAnswer)".to_string(),
        ]
    );
    assert_eq!(
        harness.internal_command_output_at(live_index).as_deref(),
        Some("running 1 test\n")
    );

    let panel_state = harness.panel_state_for_range(live_index..live_index + 1);
    assert!(panel_state.active_nested_code_panel_ids.is_empty());
}

#[test]
fn live_steering_fragment_presentation_follows_already_visible_assistant_output() {
    let mut harness = PresentationHarness::new();
    let live_index = harness.begin_live_turn("Initial prompt");

    harness
        .apply_stream_event(TurnStreamEvent::TurnStarted {
            thread_id: "thread_a".to_string(),
            turn: empty_turn("turn_live", TurnStatus::InProgress),
        })
        .unwrap();
    harness
        .apply_stream_event(TurnStreamEvent::ItemCompleted {
            thread_id: "thread_a".to_string(),
            turn_id: "turn_live".to_string(),
            item: ThreadItem::AgentMessage(AgentMessageItem {
                id: "assistant_before".to_string(),
                phase: Some(ProtocolPhase::Commentary),
                text: "Already visible assistant output.".to_string(),
            }),
        })
        .unwrap();

    harness.append_live_user_fragment(live_index, "Steered follow-up");
    harness
        .apply_stream_event(TurnStreamEvent::ItemCompleted {
            thread_id: "thread_a".to_string(),
            turn_id: "turn_live".to_string(),
            item: ThreadItem::AgentMessage(AgentMessageItem {
                id: "assistant_after".to_string(),
                phase: Some(ProtocolPhase::FinalAnswer),
                text: "Assistant after steering.".to_string(),
            }),
        })
        .unwrap();

    assert_eq!(
        harness.visible_narrative_texts_at(live_index),
        vec![
            "user: Initial prompt",
            "assistant: Already visible assistant output.",
            "user: Steered follow-up",
            "assistant: Assistant after steering.",
        ]
    );
}

#[test]
fn activity_caret_tracks_working_turn_outside_transcript_metrics() {
    let mut harness = PresentationHarness::new();
    let live_index = harness.begin_live_turn("Inspect the workspace");
    let live_identity = harness.row_identity(live_index);

    assert_eq!(
        harness.activity_caret(),
        Some((live_index, live_identity.clone()))
    );
    assert_eq!(
        harness.render_metrics(),
        (1, 0, "Inspect the workspace".len())
    );

    harness
        .apply_stream_event(TurnStreamEvent::TurnStarted {
            thread_id: "thread_a".to_string(),
            turn: empty_turn("turn_live", TurnStatus::InProgress),
        })
        .unwrap();
    harness
        .apply_stream_event(TurnStreamEvent::ItemStarted {
            thread_id: "thread_a".to_string(),
            turn_id: "turn_live".to_string(),
            item: ThreadItem::CommandExecution(CommandExecutionItem {
                id: "cmd_live".to_string(),
                command: "cargo nextest run".to_string(),
                cwd: "C:\\repo".to_string(),
                status: CommandExecutionStatus::InProgress,
                process_id: None,
                aggregated_output: None,
                exit_code: None,
                duration_ms: None,
            }),
        })
        .unwrap();
    harness
        .apply_stream_event(TurnStreamEvent::CommandExecutionOutputDelta {
            thread_id: "thread_a".to_string(),
            turn_id: "turn_live".to_string(),
            item_id: "cmd_live".to_string(),
            delta: "running 1 test\n".to_string(),
        })
        .unwrap();

    assert_eq!(harness.activity_caret(), Some((live_index, live_identity)));
    assert_eq!(
        harness.visible_item_kinds_at(live_index),
        Vec::<String>::new()
    );
    assert_eq!(
        harness.render_metrics(),
        (1, 0, "Inspect the workspace".len())
    );
    assert_eq!(harness.presentation_len(), 1);
}

#[test]
fn activity_caret_disappears_when_working_turn_finishes() {
    let mut harness = PresentationHarness::new();
    let live_index = harness.begin_live_turn("Summarize the workspace");
    let live_identity = harness.row_identity(live_index);

    harness
        .apply_stream_event(TurnStreamEvent::TurnStarted {
            thread_id: "thread_a".to_string(),
            turn: empty_turn("turn_live", TurnStatus::InProgress),
        })
        .unwrap();

    assert_eq!(
        harness.activity_caret(),
        Some((live_index, live_identity.clone()))
    );

    harness
        .apply_stream_event(TurnStreamEvent::TurnCompleted {
            thread_id: "thread_a".to_string(),
            turn: empty_turn("turn_live", TurnStatus::Completed),
        })
        .unwrap();

    assert_eq!(harness.activity_caret(), None);
    assert_eq!(harness.row_identity(live_index), live_identity);
    assert_eq!(
        harness.render_metrics(),
        (1, 0, "Summarize the workspace".len())
    );
}

#[test]
fn activity_caret_does_not_create_operational_placeholder_row() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![command_turn_with_status(
            "turn_cmd",
            "command_a",
            "cargo nextest",
            "running",
            TurnStatus::InProgress,
        )],
    );

    assert_eq!(harness.presentation_len(), 0);
    assert_eq!(harness.activity_caret(), None);
    assert_eq!(harness.render_metrics(), (0, 0, 0));
}

#[test]
fn operational_only_history_turns_do_not_create_presentation_rows() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![
            command_turn("turn_cmd", "command_a", "cargo nextest", "ok"),
            prompt_turn("turn_prompt", "Prompt 1"),
        ],
    );

    assert_eq!(harness.presentation_len(), 1);
    assert_eq!(harness.source_turn_index_at(0), Some(1));
    assert_eq!(harness.window_turn_ids(0..1), vec!["turn_prompt"]);
    assert_eq!(
        harness.latest_user_prompt_anchor(),
        Some((0, 0, "Prompt 1".to_string()))
    );
}

#[test]
fn hidden_operational_turns_do_not_allocate_ephemeral_row_identity() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![command_turn("turn_cmd", "command_a", "cargo nextest", "ok")],
    );

    assert_eq!(harness.presentation_len(), 0);

    harness.begin_live_turn("Live prompt");

    assert_eq!(harness.presentation_len(), 1);
    assert_eq!(harness.row_identity(0), "ephemeral-turn:0");
}

#[test]
fn released_history_rows_keep_identity_and_placeholder_geometry() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![
            prompt_turn("turn_1", "Prompt 1"),
            prompt_turn("turn_2", "Prompt 2"),
            prompt_turn("turn_3", "Prompt 3"),
        ],
    );
    let turn_1_identity = harness.row_identity(0);
    let turn_2_identity = harness.row_identity(1);
    let turn_3_identity = harness.row_identity(2);

    assert_eq!(
        harness.release_range_with_heights(0..2, &[px(120.0), px(160.0)]),
        2
    );

    assert_eq!(harness.row_identity(0), turn_1_identity);
    assert_eq!(harness.row_identity(1), turn_2_identity);
    assert_eq!(harness.row_identity(2), turn_3_identity);
    assert!(harness.is_placeholder_at(0));
    assert!(harness.is_placeholder_at(1));
    assert!(!harness.is_placeholder_at(2));
    assert_eq!(harness.turn_id_at(0).as_deref(), Some("turn_1"));
    assert_eq!(harness.placeholder_height_at(0), Some(px(120.0)));
    assert_eq!(harness.placeholder_height_at(1), Some(px(160.0)));
    assert_eq!(
        harness.latest_user_prompt_anchor(),
        Some((2, 0, "Prompt 3".to_string()))
    );
}

#[test]
fn live_turn_row_identity_survives_turn_id_materialization() {
    let mut harness = PresentationHarness::new();
    harness.replace_history("thread_a", vec![prompt_turn("turn_1", "Prompt 1")]);
    let live_index = harness.begin_live_turn("Live prompt");
    let live_identity = harness.row_identity(live_index);

    let updated_index = harness
        .apply_stream_event(TurnStreamEvent::TurnStarted {
            thread_id: "thread_a".to_string(),
            turn: empty_turn("turn_live", TurnStatus::InProgress),
        })
        .unwrap();

    assert_eq!(updated_index, live_index);
    assert_eq!(harness.row_identity(live_index), live_identity);
    assert_eq!(harness.turn_id_at(live_index).as_deref(), Some("turn_live"));
}

#[test]
fn command_items_do_not_register_nested_code_panels() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![prompt_command_turn(
            "turn_1",
            "Prompt 1",
            "command_a",
            "cargo nextest",
            "ok",
        )],
    );
    let panel_state = harness.panel_state_for_range(0..1);

    assert!(panel_state.active_nested_code_panel_ids.is_empty());
    assert_eq!(harness.internal_item_kinds_at(0), vec!["command"]);
}

#[test]
fn panel_state_for_range_is_bounded_to_requested_rows() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        (0..1_000)
            .map(|index| {
                prompt_command_turn(
                    &format!("turn_{index}"),
                    &format!("Prompt {index}"),
                    &format!("command_{index}"),
                    "cargo nextest",
                    "ok",
                )
            })
            .collect(),
    );
    let panel_state = harness.panel_state_for_range(500..502);

    assert_eq!(panel_state.inspected_row_count, 2);
    assert!(panel_state.active_nested_code_panel_ids.is_empty());
    assert_eq!(harness.presentation_len(), 1_000);
}

fn prompt_turn(id: &str, prompt: &str) -> TurnInfo {
    prompt_turn_with_fragments(id, &[prompt])
}

fn prompt_turn_with_fragments(id: &str, prompts: &[&str]) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: vec![ThreadItem::UserMessage(UserMessageItem {
            id: format!("{id}_user"),
            content: prompts
                .iter()
                .map(|prompt| UserInput::Text {
                    text: (*prompt).to_string(),
                })
                .collect(),
        })],
        error: None,
    }
}

fn mixed_operational_turn(id: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items: vec![
            ThreadItem::UserMessage(UserMessageItem {
                id: format!("{id}_user"),
                content: vec![UserInput::Text {
                    text: "Explain the workspace".to_string(),
                }],
            }),
            ThreadItem::AgentMessage(AgentMessageItem {
                id: format!("{id}_commentary"),
                phase: Some(ProtocolPhase::Commentary),
                text: "I will inspect the package layout.".to_string(),
            }),
            ThreadItem::CommandExecution(CommandExecutionItem {
                id: format!("{id}_command"),
                command: "cargo metadata".to_string(),
                cwd: "C:\\repo".to_string(),
                status: CommandExecutionStatus::Completed,
                process_id: None,
                aggregated_output: Some("{\"packages\":[]}".to_string()),
                exit_code: Some(0),
                duration_ms: Some(10),
            }),
            ThreadItem::FileChange(FileChangeItem {
                id: format!("{id}_file_change"),
                status: PatchApplyStatus::Completed,
                changes: vec![FileUpdateChange {
                    path: PathBuf::from("src/lib.rs"),
                    diff: "+pub fn marker() {}".to_string(),
                    kind: beryl_backend::PatchChangeKind::Update { move_path: None },
                }],
            }),
            ThreadItem::Reasoning(ReasoningItem {
                id: format!("{id}_reasoning"),
                summary: vec!["I inspected the package layout.".to_string()],
                content: vec!["Raw hidden reasoning details.".to_string()],
            }),
            ThreadItem::AgentMessage(AgentMessageItem {
                id: format!("{id}_answer"),
                phase: Some(ProtocolPhase::FinalAnswer),
                text: "The workspace has a root Cargo package.".to_string(),
            }),
        ],
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

fn command_turn(id: &str, item_id: &str, command: &str, output: &str) -> TurnInfo {
    command_turn_with_status(id, item_id, command, output, TurnStatus::Completed)
}

fn command_turn_with_status(
    id: &str,
    item_id: &str,
    command: &str,
    output: &str,
    status: TurnStatus,
) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status,
        items: vec![ThreadItem::CommandExecution(CommandExecutionItem {
            id: item_id.to_string(),
            command: command.to_string(),
            cwd: "C:\\repo".to_string(),
            status: CommandExecutionStatus::Completed,
            process_id: None,
            aggregated_output: Some(output.to_string()),
            exit_code: Some(0),
            duration_ms: Some(10),
        })],
        error: None,
    }
}

fn empty_turn(id: &str, status: TurnStatus) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status,
        items: Vec::new(),
        error: None,
    }
}
