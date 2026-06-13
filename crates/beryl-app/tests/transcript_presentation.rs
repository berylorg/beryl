use std::{collections::BTreeMap, path::PathBuf};

use beryl_backend::{
    AgentMessageItem, CommandExecutionItem, CommandExecutionStatus, FileChangeItem,
    FileUpdateChange, ImageGenerationItem, PatchApplyStatus, ProtocolPhase, ReasoningItem,
    ThreadItem, TurnInfo, TurnStatus, TurnStreamEvent, UserInput, UserMessageItem,
};
use gpui::px;

mod shell {
    #[path = "../../src/shell/execution_detail.rs"]
    mod execution_detail;
    #[path = "../../src/shell/transcript_media.rs"]
    mod transcript_media;
    #[path = "../../src/shell/transcript_presentability.rs"]
    mod transcript_presentability;
    #[path = "../../src/shell/transcript_presentation.rs"]
    mod transcript_presentation;
    #[path = "../../src/shell/transcript_projection.rs"]
    mod transcript_projection;
    mod transcript_history {
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub(crate) struct TranscriptHistoryPageRequest {
            pub(crate) page_id: String,
        }
    }
    #[path = "../../src/shell/transcript_media_admission.rs"]
    mod transcript_media_admission;
    #[path = "../../src/shell/transcript_prepublication_preparation.rs"]
    mod transcript_prepublication_preparation;
    #[allow(dead_code)]
    #[path = "../../src/shell/virtual_list/mod.rs"]
    mod virtual_list;

    use std::ops::Range;

    use beryl_backend::{TurnInfo, TurnStreamEvent};
    use gpui::{Pixels, px};

    pub(super) use transcript_media::TranscriptMediaLoadOutcome;
    pub(super) use transcript_media_admission::{
        SourceBackedUploadAdmissionDecision, TranscriptMediaAdmissionSummary,
        TranscriptMediaAdmissionTarget, note_source_backed_upload_admission,
    };
    pub(super) use transcript_prepublication_preparation::{
        TranscriptPrepublicationPreparationBudget, TranscriptPrepublicationPreparationDriver,
        TranscriptPrepublicationPreparationLayout,
    };
    pub(super) use transcript_presentability::{
        TranscriptCompletedMediaReadiness, TranscriptMediaPresentability,
        TranscriptMediaReadinessKey, TranscriptMediaRequestedRenderSize,
        TranscriptMediaTerminalFallback, TranscriptPresentabilitySummary,
        TranscriptRowPresentabilityContext,
    };
    pub(super) use transcript_presentation::TranscriptPresentationMutation;
    pub(super) use transcript_presentation::TranscriptRowChunkMeasurementKey;
    pub(super) use transcript_presentation::TranscriptRowChunkOwner;
    pub(super) use transcript_presentation::TranscriptRowMeasurementDisplayState;
    pub(super) use transcript_presentation::transcript_row_chunk_render_window;
    pub(super) use transcript_presentation::{
        TranscriptRowIdentity, TranscriptRowPresentationRevision,
    };

    #[allow(dead_code)]
    struct StagedSelectedThreadActivationForTest;

    #[allow(dead_code)]
    struct StagedTranscriptResidencyPageAdmissionForTest;

    #[allow(dead_code)]
    pub(super) struct ConversationSurfaceState {
        staged_selected_thread_activation: Option<StagedSelectedThreadActivationForTest>,
        staged_transcript_residency_page: Option<StagedTranscriptResidencyPageAdmissionForTest>,
    }

    impl StagedSelectedThreadActivationForTest {
        fn media_admission_request(
            &self,
        ) -> transcript_media_admission::TranscriptMediaAdmissionRequest {
            unreachable!("test staging stub should not build an admission request")
        }

        fn media_admission_target_matches(
            &self,
            _target: &transcript_media_admission::TranscriptMediaAdmissionTarget,
        ) -> bool {
            false
        }

        fn note_media_admission_summary(
            &mut self,
            summary: transcript_media_admission::TranscriptMediaAdmissionSummary,
        ) -> transcript_media_admission::TranscriptMediaAdmissionSummary {
            summary
        }

        fn prepublication_preparation_request(
            &self,
            _layout: transcript_prepublication_preparation::TranscriptPrepublicationPreparationLayout,
        ) -> transcript_prepublication_preparation::TranscriptPrepublicationPreparationRequest
        {
            unreachable!("test staging stub should not build a preparation request")
        }

        fn prepublication_preparation_target_matches(
            &self,
            _target: &transcript_media_admission::TranscriptMediaAdmissionTarget,
        ) -> bool {
            false
        }

        fn note_prepublication_preparation_summary(
            &mut self,
            summary: transcript_prepublication_preparation::TranscriptPrepublicationPreparationSummary,
        ) -> transcript_prepublication_preparation::TranscriptPrepublicationPreparationSummary
        {
            summary
        }
    }

    impl StagedTranscriptResidencyPageAdmissionForTest {
        fn media_admission_request(
            &self,
        ) -> transcript_media_admission::TranscriptMediaAdmissionRequest {
            unreachable!("test staging stub should not build an admission request")
        }

        fn media_admission_target_matches(
            &self,
            _target: &transcript_media_admission::TranscriptMediaAdmissionTarget,
        ) -> bool {
            false
        }

        fn note_media_admission_summary(
            &mut self,
            summary: transcript_media_admission::TranscriptMediaAdmissionSummary,
        ) -> transcript_media_admission::TranscriptMediaAdmissionSummary {
            summary
        }

        fn prepublication_preparation_request(
            &self,
            _layout: transcript_prepublication_preparation::TranscriptPrepublicationPreparationLayout,
        ) -> transcript_prepublication_preparation::TranscriptPrepublicationPreparationRequest
        {
            unreachable!("test staging stub should not build a preparation request")
        }

        fn prepublication_preparation_target_matches(
            &self,
            _target: &transcript_media_admission::TranscriptMediaAdmissionTarget,
        ) -> bool {
            false
        }

        fn note_prepublication_preparation_summary(
            &mut self,
            summary: transcript_prepublication_preparation::TranscriptPrepublicationPreparationSummary,
        ) -> transcript_prepublication_preparation::TranscriptPrepublicationPreparationSummary
        {
            summary
        }
    }

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
            self.presentation.replace_turn(index, turn).row_index();
            Some(index)
        }

        pub(super) fn append_live_user_fragment(&mut self, index: usize, text: &str) {
            self.details
                .append_user_input_fragment(index, execution_detail::UserInputFragment::text(text))
                .expect("live turn should exist");
            let turn = self.details.turns()[index].clone();
            self.presentation.replace_turn(index, turn).row_index();
        }

        pub(super) fn release_range_with_heights(
            &mut self,
            range: Range<usize>,
            _heights: &[Pixels],
        ) -> usize {
            let replacements = self.details.release_history_range(range);
            let count = replacements.len();
            for replacement in replacements {
                self.presentation
                    .replace_turn(replacement.index, replacement.turn);
            }
            count
        }

        pub(super) fn release_turns_by_id(&mut self, turn_ids: &[&str]) -> usize {
            let replacements = self
                .details
                .release_history_turns_by_id(turn_ids.iter().copied());
            let count = replacements.len();
            for replacement in replacements {
                self.presentation
                    .replace_turn(replacement.index, replacement.turn);
            }
            count
        }

        pub(super) fn restore_history_page(
            &mut self,
            thread_id: &str,
            row_start: usize,
            expected_turn_ids: &[String],
            turns: Vec<TurnInfo>,
        ) -> Vec<TranscriptPresentationMutation> {
            self.details
                .restore_history_page(thread_id, row_start, expected_turn_ids, turns)
                .into_iter()
                .map(|replacement| {
                    self.presentation
                        .replace_turn(replacement.index, replacement.turn)
                })
                .collect()
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

        pub(super) fn row_model_source_turn_index_at(&self, index: usize) -> Option<usize> {
            self.presentation
                .turn_at(index)
                .map(|row| row.model.source_turn_identity().source_turn_index)
        }

        pub(super) fn row_model_revision_at(&self, index: usize) -> Option<String> {
            self.presentation
                .turn_at(index)
                .map(|row| format!("{:?}", row.model.revision()))
        }

        pub(super) fn row_model_units_at(&self, index: usize) -> Vec<String> {
            self.presentation
                .turn_at(index)
                .map(|row| {
                    row.model
                        .narrative_units()
                        .iter()
                        .map(|unit| match unit {
                            transcript_presentation::TranscriptRowNarrativeUnit::UserInput {
                                fragment_index,
                                ..
                            } => format!("user:{fragment_index}"),
                            transcript_presentation::TranscriptRowNarrativeUnit::Item {
                                item_id,
                                item_index,
                            } => format!("item:{item_index}:{item_id}"),
                            transcript_presentation::TranscriptRowNarrativeUnit::TerminalFallback => {
                                "fallback".to_string()
                            }
                        })
                        .collect()
                })
                .unwrap_or_default()
        }

        pub(super) fn row_model_chunk_summary_at(&self, index: usize) -> Option<(usize, bool)> {
            self.presentation.turn_at(index).map(|row| {
                let chunks = row.model.chunk_presentation();
                (chunks.estimated_render_blocks(), chunks.requires_chunking())
            })
        }

        pub(super) fn row_model_chunk_kinds_at(&self, index: usize) -> Vec<String> {
            self.presentation
                .turn_at(index)
                .map(|row| {
                    row.model
                        .chunk_presentation()
                        .chunks()
                        .iter()
                        .map(|chunk| match &chunk.owner {
                            TranscriptRowChunkOwner::NarrativeUnit { unit_index } => {
                                format!("narrative:{unit_index}")
                            }
                            TranscriptRowChunkOwner::MarkdownSource {
                                key,
                                first_unit_index,
                                unit_count,
                                ..
                            } => {
                                format!(
                                    "markdown:{key}:{first_unit_index}:{}",
                                    first_unit_index.saturating_add(*unit_count)
                                )
                            }
                            TranscriptRowChunkOwner::MediaDescriptor { key } => {
                                format!("media:{key}")
                            }
                        })
                        .collect()
                })
                .unwrap_or_default()
        }

        pub(super) fn row_model_chunk_costs_at(&self, index: usize) -> Vec<usize> {
            self.presentation
                .turn_at(index)
                .map(|row| {
                    row.model
                        .chunk_presentation()
                        .chunks()
                        .iter()
                        .map(|chunk| chunk.estimated_render_blocks)
                        .collect()
                })
                .unwrap_or_default()
        }

        pub(super) fn terminal_fallback_text_at(&self, index: usize) -> Option<&'static str> {
            self.presentation
                .turn_at(index)
                .and_then(|row| row.turn.terminal_fallback_text())
        }

        pub(super) fn row_has_resident_payload(&self, index: usize) -> bool {
            self.presentation
                .turn_at(index)
                .is_some_and(|row| row.turn.has_resident_payload())
        }

        pub(super) fn first_markdown_key_at(&self, index: usize) -> Option<String> {
            self.presentation.turn_at(index).and_then(|row| {
                row.model
                    .markdown_sources()
                    .first()
                    .map(|source| source.key.clone())
            })
        }

        pub(super) fn row_index_for_markdown_key(&self, key: &str) -> Option<usize> {
            self.presentation.row_index_for_markdown_key(key)
        }

        pub(super) fn measurement_key_at(
            &self,
            index: usize,
            width: f32,
            theme_revision: u64,
            display_state: TranscriptRowMeasurementDisplayState,
        ) -> Option<String> {
            self.presentation
                .measurement_key_for_row(index, px(width), theme_revision, display_state)
                .map(|key| format!("{key:?}"))
        }

        pub(super) fn first_chunk_measurement_key_at(
            &self,
            index: usize,
            width: f32,
            theme_revision: u64,
            display_state: TranscriptRowMeasurementDisplayState,
        ) -> Option<String> {
            let row = self.presentation.turn_at(index)?;
            let row_key = self.presentation.measurement_key_for_row(
                index,
                px(width),
                theme_revision,
                display_state,
            )?;
            let chunk = row.model.chunk_presentation().chunks().first()?;
            Some(format!(
                "{:?}",
                TranscriptRowChunkMeasurementKey::new(row_key, chunk)
            ))
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

        pub(super) fn derived_retained_counts(&self) -> (usize, usize, usize) {
            let counts = self.presentation.retained_counts();
            (
                counts.derived_bytes,
                counts.markdown_source_bytes,
                counts.media_descriptors,
            )
        }

        pub(super) fn presentability_summary(
            &self,
            context: transcript_presentability::TranscriptRowPresentabilityContext,
        ) -> transcript_presentability::TranscriptPresentabilitySummary {
            transcript_presentability::TranscriptPresentabilityWindow::from_turn_records(
                self.details.turns(),
                0,
                context,
            )
            .summary()
        }

        pub(super) fn media_admission_summary(
            &self,
        ) -> transcript_media_admission::TranscriptMediaAdmissionSummary {
            transcript_media_admission::TranscriptMediaAdmissionWindow::from_turn_records(
                self.details.turns(),
                0,
            )
            .last_summary()
        }

        pub(super) fn media_admission_window(
            &self,
        ) -> transcript_media_admission::TranscriptMediaAdmissionWindow {
            transcript_media_admission::TranscriptMediaAdmissionWindow::from_turn_records(
                self.details.turns(),
                0,
            )
        }

        pub(super) fn prepublication_preparation_window(
            &self,
        ) -> transcript_prepublication_preparation::TranscriptPrepublicationPreparationWindow
        {
            transcript_prepublication_preparation::TranscriptPrepublicationPreparationWindow::from_turn_records(
                self.details.turns(),
                0,
            )
        }

        pub(super) fn requires_completed_media_admission(&self) -> bool {
            transcript_media_admission::TranscriptMediaAdmissionWindow::from_turn_records(
                self.details.turns(),
                0,
            )
            .requires_completed_media_admission()
        }

        pub(super) fn first_row_completed_media(
            &self,
            context: transcript_presentability::TranscriptRowPresentabilityContext,
        ) -> transcript_presentability::TranscriptCompletedMediaReadiness {
            transcript_presentability::TranscriptPresentabilityWindow::from_turn_records(
                self.details.turns(),
                0,
                context,
            )
            .rows()
            .first()
            .map(|row| row.completed_media().clone())
            .unwrap_or(transcript_presentability::TranscriptCompletedMediaReadiness::NotRequired)
        }
    }
}

use shell::{
    PresentationHarness, SourceBackedUploadAdmissionDecision, TranscriptCompletedMediaReadiness,
    TranscriptMediaAdmissionSummary, TranscriptMediaAdmissionTarget, TranscriptMediaLoadOutcome,
    TranscriptMediaPresentability, TranscriptMediaReadinessKey, TranscriptMediaRequestedRenderSize,
    TranscriptMediaTerminalFallback, TranscriptPrepublicationPreparationBudget,
    TranscriptPrepublicationPreparationDriver, TranscriptPrepublicationPreparationLayout,
    TranscriptPresentabilitySummary, TranscriptPresentationMutation, TranscriptRowIdentity,
    TranscriptRowMeasurementDisplayState, TranscriptRowPresentabilityContext,
    TranscriptRowPresentationRevision, note_source_backed_upload_admission,
    transcript_row_chunk_render_window,
};

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
fn oversized_fallback_turn_projects_terminal_row_without_resident_payload_sources() {
    let mut harness = PresentationHarness::new();
    harness.replace_history("thread_a", vec![oversized_fallback_turn("turn_big")]);

    assert_eq!(harness.presentation_len(), 1);
    assert_eq!(harness.turn_id_at(0), Some("turn_big".to_string()));
    assert_eq!(harness.row_model_units_at(0), vec!["fallback".to_string()]);
    assert_eq!(
        harness.terminal_fallback_text_at(0),
        Some(
            "This turn is too large to fit in Beryl's transcript memory budget. Its contents are omitted."
        )
    );
    assert!(!harness.row_has_resident_payload(0));
    assert!(harness.visible_narrative_texts_at(0).is_empty());
    assert!(harness.visible_item_kinds_at(0).is_empty());
    assert_eq!(harness.first_markdown_key_at(0), None);
    assert_eq!(harness.render_metrics(), (1, 0, 0));
    assert_eq!(harness.retained_counts(), (1, 0, 0));

    let presentability =
        harness.presentability_summary(TranscriptRowPresentabilityContext::HistoricalOrCompleted);
    assert_eq!(
        presentability,
        TranscriptPresentabilitySummary {
            row_count: 1,
            presentable_rows: 1,
            ..TranscriptPresentabilitySummary::default()
        }
    );
    assert!(!harness.requires_completed_media_admission());
}

#[test]
fn row_model_source_identity_updates_on_prepend_without_changing_stable_backend_revision() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![
            prompt_turn("turn_3", "Prompt 3"),
            prompt_turn("turn_4", "Prompt 4"),
        ],
    );
    let turn_3_identity = harness.row_identity(0);
    let turn_3_revision = harness.row_model_revision_at(0).unwrap();

    harness.prepend_history(
        "thread_a",
        vec![
            prompt_turn("turn_1", "Prompt 1"),
            prompt_turn("turn_2", "Prompt 2"),
        ],
    );

    let turn_3_index = harness.row_index_for_identity(&turn_3_identity).unwrap();
    assert_eq!(turn_3_index, 2);
    assert_eq!(harness.source_turn_index_at(turn_3_index), Some(2));
    assert_eq!(
        harness.row_model_source_turn_index_at(turn_3_index),
        Some(2)
    );
    assert_eq!(
        harness.row_model_revision_at(turn_3_index).unwrap(),
        turn_3_revision
    );
}

#[test]
fn row_presentability_marks_rows_without_media_presentable() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![agent_markdown_turn("turn_1", "Plain assistant response.")],
    );

    assert_eq!(
        harness.presentability_summary(TranscriptRowPresentabilityContext::HistoricalOrCompleted),
        TranscriptPresentabilitySummary {
            row_count: 1,
            presentable_rows: 1,
            ..TranscriptPresentabilitySummary::default()
        }
    );
}

#[test]
fn media_admission_marks_rows_without_completed_media_settled() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![agent_markdown_turn("turn_1", "Plain assistant response.")],
    );

    let summary = harness.media_admission_summary();

    assert_eq!(
        summary,
        TranscriptMediaAdmissionSummary {
            row_count: 1,
            ..TranscriptMediaAdmissionSummary::default()
        }
    );
    assert!(summary.is_completed_media_settled());
    assert!(!harness.requires_completed_media_admission());
}

#[test]
fn media_admission_budget_exhaustion_requests_retry() {
    let exhausted = TranscriptMediaAdmissionSummary {
        row_count: 36,
        completed_media_items: 36,
        pending_completed_media_items: 12,
        rows_budget_exhausted: true,
        ..TranscriptMediaAdmissionSummary::default()
    };
    let pending_io = TranscriptMediaAdmissionSummary {
        row_count: 1,
        completed_media_items: 1,
        pending_completed_media_items: 1,
        ..TranscriptMediaAdmissionSummary::default()
    };
    let exhausted_without_media = TranscriptMediaAdmissionSummary {
        row_count: 36,
        rows_budget_exhausted: true,
        ..TranscriptMediaAdmissionSummary::default()
    };
    let settled = TranscriptMediaAdmissionSummary {
        row_count: 1,
        completed_media_items: 1,
        ready_completed_media_items: 1,
        ..TranscriptMediaAdmissionSummary::default()
    };

    assert!(exhausted.requires_retry());
    assert!(!exhausted.is_completed_media_settled());
    assert!(!pending_io.requires_retry());
    assert!(!pending_io.is_completed_media_settled());
    assert!(!exhausted_without_media.requires_retry());
    assert!(exhausted_without_media.is_completed_media_settled());
    assert!(!settled.requires_retry());
    assert!(settled.is_completed_media_settled());
}

#[test]
fn media_admission_retry_request_advances_after_row_budget_exhaustion() {
    let mut harness = PresentationHarness::new();
    let mut turns = (0..24)
        .map(|index| prompt_turn(&format!("turn_{index}"), "Prompt without media."))
        .collect::<Vec<_>>();
    turns.push(generated_images_turn("turn_24", 1));
    harness.replace_history("thread_a", turns);
    let mut window = harness.media_admission_window();

    assert_eq!(window.rows().len(), 25);
    window.note_summary(TranscriptMediaAdmissionSummary {
        row_count: 25,
        completed_media_items: 1,
        pending_completed_media_items: 1,
        scan_start_row_index: 0,
        scanned_rows: 24,
        deferred_completed_media_items: 1,
        rows_budget_exhausted: true,
        ..TranscriptMediaAdmissionSummary::default()
    });

    let retry = window.admission_request(prepublication_target());
    assert_eq!(retry.scan_start_row_index(), 24);
    assert_eq!(retry.scan_start_item_index(), 0);
    assert!(!retry.prefix_recheck_required());
    assert_eq!(retry.rows().len(), 1);

    window.note_summary(TranscriptMediaAdmissionSummary {
        row_count: 25,
        completed_media_items: 1,
        ready_completed_media_items: 1,
        scan_start_row_index: 24,
        scan_start_item_index: 0,
        scanned_rows: 1,
        scanned_media_items: 1,
        ..TranscriptMediaAdmissionSummary::default()
    });

    assert!(window.is_settled_for_publication());
}

#[test]
fn media_admission_retry_rechecks_pending_prefix_after_suffix_scan() {
    let mut harness = PresentationHarness::new();
    let mut turns = vec![generated_images_turn("turn_0", 1)];
    turns.extend(
        (1..24).map(|index| prompt_turn(&format!("turn_{index}"), "Prompt without media.")),
    );
    turns.push(generated_images_turn("turn_24", 1));
    harness.replace_history("thread_a", turns);
    let mut window = harness.media_admission_window();

    window.note_summary(TranscriptMediaAdmissionSummary {
        row_count: 25,
        completed_media_items: 2,
        pending_completed_media_items: 2,
        scan_start_row_index: 0,
        scan_start_item_index: 0,
        scanned_rows: 24,
        scanned_media_items: 1,
        deferred_completed_media_items: 1,
        rows_budget_exhausted: true,
        ..TranscriptMediaAdmissionSummary::default()
    });

    let retry = window.admission_request(prepublication_target());
    assert_eq!(retry.scan_start_row_index(), 24);
    assert_eq!(retry.scan_start_item_index(), 0);
    assert!(retry.prefix_recheck_required());
    assert_eq!(retry.rows().len(), 1);

    window.note_summary(TranscriptMediaAdmissionSummary {
        row_count: 25,
        completed_media_items: 1,
        ready_completed_media_items: 1,
        scan_start_row_index: 24,
        scan_start_item_index: 0,
        scanned_rows: 1,
        scanned_media_items: 1,
        prefix_recheck_required: true,
        ..TranscriptMediaAdmissionSummary::default()
    });

    assert!(!window.is_settled_for_publication());
    let full_recheck = window.admission_request(prepublication_target());
    assert_eq!(full_recheck.scan_start_row_index(), 0);
    assert_eq!(full_recheck.scan_start_item_index(), 0);
    assert!(!full_recheck.prefix_recheck_required());
    assert_eq!(full_recheck.rows().len(), 25);

    window.note_summary(TranscriptMediaAdmissionSummary {
        row_count: 25,
        completed_media_items: 2,
        pending_completed_media_items: 2,
        scan_start_row_index: 0,
        scan_start_item_index: 0,
        scanned_rows: 24,
        scanned_media_items: 1,
        deferred_completed_media_items: 1,
        rows_budget_exhausted: true,
        ..TranscriptMediaAdmissionSummary::default()
    });

    let waiting_summary = window.last_summary();
    assert!(waiting_summary.waiting_on_prefix_media);
    assert!(waiting_summary.requires_retry());
    let waiting_recheck = window.admission_request(prepublication_target());
    assert_eq!(waiting_recheck.scan_start_row_index(), 0);
    assert_eq!(waiting_recheck.scan_start_item_index(), 0);
    assert!(!waiting_recheck.prefix_recheck_required());
}

#[test]
fn media_admission_retry_resumes_inside_row_after_media_budget_exhaustion() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![
            prompt_turn("turn_0", "Prompt without media."),
            prompt_turn("turn_1", "Prompt without media."),
            generated_images_turn("turn_2", 40),
        ],
    );
    let mut window = harness.media_admission_window();

    window.note_summary(TranscriptMediaAdmissionSummary {
        row_count: 3,
        completed_media_items: 40,
        pending_completed_media_items: 8,
        scan_start_row_index: 0,
        scan_start_item_index: 0,
        scanned_rows: 2,
        scanned_media_items: 32,
        deferred_completed_media_items: 8,
        media_budget_exhausted: true,
        ..TranscriptMediaAdmissionSummary::default()
    });

    let retry = window.admission_request(prepublication_target());
    assert_eq!(retry.scan_start_row_index(), 2);
    assert_eq!(retry.scan_start_item_index(), 32);
    assert!(!retry.prefix_recheck_required());
    assert_eq!(retry.rows().len(), 1);

    window.note_summary(TranscriptMediaAdmissionSummary {
        row_count: 3,
        completed_media_items: 8,
        ready_completed_media_items: 8,
        scan_start_row_index: 2,
        scan_start_item_index: 32,
        scanned_rows: 1,
        scanned_media_items: 8,
        ..TranscriptMediaAdmissionSummary::default()
    });

    assert!(window.is_settled_for_publication());
}

#[test]
fn media_admission_retry_current_item_after_media_budget_exhaustion_in_later_row() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![
            prompt_turn("turn_0", "Prompt without media."),
            prompt_turn("turn_1", "Prompt without media."),
            generated_images_turn("turn_2", 1),
        ],
    );
    let mut window = harness.media_admission_window();

    window.note_summary(TranscriptMediaAdmissionSummary {
        row_count: 3,
        completed_media_items: 1,
        pending_completed_media_items: 1,
        scan_start_row_index: 0,
        scan_start_item_index: 0,
        scanned_rows: 2,
        scanned_media_items: 0,
        deferred_completed_media_items: 1,
        media_budget_exhausted: true,
        ..TranscriptMediaAdmissionSummary::default()
    });

    let retry = window.admission_request(prepublication_target());
    assert_eq!(retry.scan_start_row_index(), 2);
    assert_eq!(retry.scan_start_item_index(), 0);
    assert_eq!(retry.rows().len(), 1);
    assert!(window.last_summary().requires_retry());
}

#[test]
fn media_admission_marks_markdown_image_candidates_pending() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![agent_markdown_turn(
            "turn_1",
            "Look at ![cat](images/cat.png) before continuing.",
        )],
    );

    let summary = harness.media_admission_summary();

    assert_eq!(summary.row_count, 1);
    assert_eq!(summary.completed_media_items, 1);
    assert_eq!(summary.pending_completed_media_items, 1);
    assert!(!summary.is_completed_media_settled());
    assert!(harness.requires_completed_media_admission());
}

#[test]
fn media_admission_marks_user_markdown_image_candidates_pending() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![prompt_turn(
            "turn_1",
            "Please inspect ![diagram](images/diagram.png) before answering.",
        )],
    );

    let summary = harness.media_admission_summary();

    assert_eq!(summary.row_count, 1);
    assert_eq!(summary.completed_media_items, 1);
    assert_eq!(summary.pending_completed_media_items, 1);
    assert!(!summary.is_completed_media_settled());
    assert!(harness.requires_completed_media_admission());
}

#[test]
fn media_admission_tracks_each_completed_generated_image() {
    let mut harness = PresentationHarness::new();
    harness.replace_history("thread_a", vec![generated_images_turn("turn_1", 3)]);

    let summary = harness.media_admission_summary();

    assert_eq!(summary.row_count, 1);
    assert_eq!(summary.completed_media_items, 3);
    assert_eq!(summary.pending_completed_media_items, 3);
    assert!(!summary.is_completed_media_settled());
    assert!(harness.requires_completed_media_admission());
}

#[test]
fn media_admission_requires_window_pass_for_source_backed_generated_image() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![generated_image_turn(
            "turn_1",
            "image_generation_source_backed",
            None,
            Some("images/generated.png".to_string()),
        )],
    );

    let summary = harness.media_admission_summary();

    assert_eq!(summary.completed_media_items, 1);
    assert_eq!(summary.pending_completed_media_items, 1);
    assert!(harness.requires_completed_media_admission());
}

#[test]
fn media_admission_treats_over_full_budget_source_backed_upload_as_terminal() {
    let mut summary = TranscriptMediaAdmissionSummary {
        row_count: 1,
        completed_media_items: 1,
        ..TranscriptMediaAdmissionSummary::default()
    };

    let decision = note_source_backed_upload_admission(&mut summary, 101, 100, 100);

    assert_eq!(
        decision,
        SourceBackedUploadAdmissionDecision::TerminalFallback
    );
    assert_eq!(summary.terminal_fallback_completed_media_items, 1);
    assert_eq!(summary.pending_completed_media_items, 0);
    assert!(!summary.media_budget_exhausted);
    assert!(summary.is_completed_media_settled());
    assert!(!summary.requires_retry());
}

#[test]
fn media_admission_retries_source_backed_upload_over_remaining_budget() {
    let mut summary = TranscriptMediaAdmissionSummary {
        row_count: 1,
        completed_media_items: 1,
        ..TranscriptMediaAdmissionSummary::default()
    };

    let decision = note_source_backed_upload_admission(&mut summary, 75, 100, 50);

    assert_eq!(decision, SourceBackedUploadAdmissionDecision::RetryCurrent);
    assert_eq!(summary.terminal_fallback_completed_media_items, 0);
    assert!(summary.media_budget_exhausted);

    summary.note_deferred_items(1);
    assert!(summary.requires_retry());
}

#[test]
fn row_presentability_treats_markdown_image_plan_as_pending() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![agent_markdown_turn(
            "turn_1",
            "Look at ![cat](images/cat.png) before continuing.",
        )],
    );

    let summary =
        harness.presentability_summary(TranscriptRowPresentabilityContext::HistoricalOrCompleted);

    assert_eq!(summary.row_count, 1);
    assert_eq!(summary.presentable_rows, 0);
    assert_eq!(summary.markdown_plan_pending_rows, 1);
    assert_eq!(summary.completed_media_pending_rows, 0);
}

#[test]
fn live_pending_generated_image_is_the_only_pending_placeholder_exception() {
    let mut harness = PresentationHarness::new();
    let live_index = harness.begin_live_turn("Draw a city skyline");
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
            item: ThreadItem::ImageGeneration(ImageGenerationItem {
                id: "image_generation_live".to_string(),
                status: Some("generating".to_string()),
                revised_prompt: Some("A city skyline".to_string()),
                result: None,
                saved_path: None,
            }),
        })
        .unwrap();

    let historical =
        harness.presentability_summary(TranscriptRowPresentabilityContext::HistoricalOrCompleted);
    let live = harness.presentability_summary(TranscriptRowPresentabilityContext::Live);

    assert_eq!(live_index, 0);
    assert_eq!(historical.presentable_rows, 0);
    assert_eq!(historical.completed_media_pending_rows, 1);
    assert_eq!(historical.live_pending_placeholder_items, 0);
    assert_eq!(live.presentable_rows, 1);
    assert_eq!(live.completed_media_pending_rows, 0);
    assert_eq!(live.live_pending_placeholder_items, 1);
}

#[test]
fn completed_generated_image_without_result_reaches_terminal_fallback() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![generated_image_turn(
            "turn_1",
            "image_generation_missing",
            None,
            None,
        )],
    );

    let summary =
        harness.presentability_summary(TranscriptRowPresentabilityContext::HistoricalOrCompleted);
    let completed_media = harness
        .first_row_completed_media(TranscriptRowPresentabilityContext::HistoricalOrCompleted);

    assert_eq!(summary.presentable_rows, 1);
    assert_eq!(summary.terminal_fallback_media_items, 1);
    assert!(matches!(
        completed_media,
        TranscriptCompletedMediaReadiness::Settled { .. }
    ));
}

#[test]
fn media_presentability_terminal_fallbacks_are_presentable() {
    let terminal_items = [
        TranscriptMediaPresentability::from_load_outcome(
            presentability_key("unsupported", 1),
            &TranscriptMediaLoadOutcome::RenderNotSupported {
                alt: "unsupported".to_string(),
            },
        ),
        TranscriptMediaPresentability::from_load_outcome(
            presentability_key("too-large", 2),
            &TranscriptMediaLoadOutcome::TooLarge {
                alt: "too large".to_string(),
            },
        ),
        TranscriptMediaPresentability::from_load_outcome(
            presentability_key("unavailable", 3),
            &TranscriptMediaLoadOutcome::FileUnavailable {
                alt: "unavailable".to_string(),
            },
        ),
        TranscriptMediaPresentability::from_load_outcome(
            presentability_key("disallowed", 4),
            &TranscriptMediaLoadOutcome::PathNotAllowed {
                alt: "disallowed".to_string(),
            },
        ),
        TranscriptMediaPresentability::TerminalFallback {
            key: presentability_key("decode-failed", 5),
            reason: TranscriptMediaTerminalFallback::DecodeFailed,
        },
        TranscriptMediaPresentability::TerminalFallback {
            key: presentability_key("admission-failed", 6),
            reason: TranscriptMediaTerminalFallback::AdmissionFailed,
        },
    ];

    assert!(terminal_items.iter().all(|item| item.is_presentable()));
}

#[test]
fn media_presentability_key_changes_with_source_layout_and_row_revision() {
    let base = TranscriptMediaReadinessKey::new(
        TranscriptRowIdentity::new("row-a"),
        "media-a",
        1,
        None,
        TranscriptMediaRequestedRenderSize::new(100, 50),
        1.0,
        TranscriptRowPresentationRevision::default(),
    );
    let changed_source = TranscriptMediaReadinessKey::new(
        TranscriptRowIdentity::new("row-a"),
        "media-a",
        2,
        None,
        TranscriptMediaRequestedRenderSize::new(100, 50),
        1.0,
        TranscriptRowPresentationRevision::default(),
    );
    let changed_size = TranscriptMediaReadinessKey::new(
        TranscriptRowIdentity::new("row-a"),
        "media-a",
        1,
        None,
        TranscriptMediaRequestedRenderSize::new(120, 50),
        1.0,
        TranscriptRowPresentationRevision::default(),
    );
    let changed_row = TranscriptMediaReadinessKey::new(
        TranscriptRowIdentity::new("row-b"),
        "media-a",
        1,
        None,
        TranscriptMediaRequestedRenderSize::new(100, 50),
        1.0,
        TranscriptRowPresentationRevision::default(),
    );

    assert_ne!(base, changed_source);
    assert_ne!(base, changed_size);
    assert_ne!(base, changed_row);
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
fn row_model_preserves_steering_fragment_narrative_order() {
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
        harness.row_model_units_at(live_index),
        vec![
            "user:0",
            "item:0:assistant_before",
            "user:1",
            "item:1:assistant_after",
        ]
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
fn transcript_presentation_retained_counts_include_derived_row_state() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![agent_markdown_turn(
            "turn_1",
            "Assistant **markdown** with ![alt](images/cat.png)",
        )],
    );

    let (derived_bytes, markdown_source_bytes, media_descriptors) =
        harness.derived_retained_counts();
    assert!(derived_bytes > markdown_source_bytes);
    assert!(markdown_source_bytes >= "Assistant **markdown**".len());
    assert_eq!(media_descriptors, 1);
}

#[test]
fn row_model_keeps_small_turns_on_row_level_rendering() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![agent_markdown_turn("turn_1", "Small assistant response.")],
    );

    assert_eq!(harness.row_model_chunk_summary_at(0), Some((1, false)));
    assert!(
        harness
            .row_model_chunk_kinds_at(0)
            .iter()
            .any(|kind| kind.starts_with("markdown:"))
    );
}

#[test]
fn row_model_marks_large_markdown_turns_for_chunked_rendering() {
    let mut harness = PresentationHarness::new();
    let markdown = (0..80)
        .map(|index| format!("Paragraph {index} with enough text to own a block."))
        .collect::<Vec<_>>()
        .join("\n\n");
    harness.replace_history("thread_a", vec![agent_markdown_turn("turn_1", &markdown)]);

    let (estimated_blocks, requires_split) = harness
        .row_model_chunk_summary_at(0)
        .expect("large markdown row should project");
    assert!(requires_split);
    assert!(estimated_blocks >= 32);
    let chunks = harness.row_model_chunk_kinds_at(0);
    assert!(chunks.iter().any(|kind| kind.contains(":0:")));
    assert!(
        chunks.len() > 1,
        "large markdown should split into stable render chunks"
    );
    assert!(
        harness
            .row_model_chunk_costs_at(0)
            .into_iter()
            .all(|cost| cost <= 8)
    );
}

#[test]
fn row_model_has_no_unmeasured_chunk_render_window() {
    let mut harness = PresentationHarness::new();
    let markdown = (0..80)
        .map(|index| format!("Paragraph {index} with enough text to own a block."))
        .collect::<Vec<_>>()
        .join("\n\n");
    harness.replace_history("thread_a", vec![agent_markdown_turn("turn_1", &markdown)]);

    let (_, requires_chunking) = harness
        .row_model_chunk_summary_at(0)
        .expect("large row should project");
    assert!(requires_chunking);
    assert!(
        harness.row_model_chunk_kinds_at(0).len() > 1,
        "Phase 1 records chunks but does not expose guessed visible windows"
    );
}

#[test]
fn row_model_marks_generated_image_heavy_turns_for_chunked_rendering() {
    let mut harness = PresentationHarness::new();
    harness.replace_history("thread_a", vec![generated_images_turn("turn_1", 16)]);

    let (estimated_blocks, requires_split) = harness
        .row_model_chunk_summary_at(0)
        .expect("image row should project");
    assert!(requires_split);
    assert!(estimated_blocks >= 16);
    assert!(
        harness
            .row_model_chunk_kinds_at(0)
            .iter()
            .any(|kind| kind.starts_with("media:"))
    );
}

#[test]
fn row_model_marks_markdown_image_embed_heavy_turns_for_chunked_rendering() {
    let mut harness = PresentationHarness::new();
    let markdown = (0..16)
        .map(|index| format!("![generated {index}](images/generated-{index}.png)"))
        .collect::<Vec<_>>()
        .join("\n");
    harness.replace_history("thread_a", vec![agent_markdown_turn("turn_1", &markdown)]);

    let (estimated_blocks, requires_split) = harness
        .row_model_chunk_summary_at(0)
        .expect("markdown image row should project");
    assert!(requires_split);
    assert!(estimated_blocks >= 16);
    assert_eq!(harness.media_admission_summary().completed_media_items, 16);
}

#[test]
fn row_model_keeps_large_reasoning_sources_addressable_by_chunks() {
    let mut harness = PresentationHarness::new();
    let summary = (0..36)
        .map(|index| format!("Reasoning summary paragraph {index}."))
        .collect::<Vec<_>>();
    let content = (0..8)
        .map(|index| format!("Reasoning content paragraph {index}."))
        .collect::<Vec<_>>();
    harness.replace_history(
        "thread_a",
        vec![reasoning_turn("turn_reasoning", summary, content)],
    );

    let (estimated_blocks, requires_split) = harness
        .row_model_chunk_summary_at(0)
        .expect("reasoning row should project");
    assert!(requires_split);
    assert!(estimated_blocks >= 36);

    let chunks = harness.row_model_chunk_kinds_at(0);
    assert!(chunks.iter().any(|unit| unit.contains("reasoning-summary")));
    assert!(chunks.len() > 1);
}

#[test]
fn row_model_keeps_user_prompt_image_markers_addressable_by_chunks() {
    let mut harness = PresentationHarness::new();
    let before_image = (0..40)
        .map(|index| format!("Prompt paragraph before image {index}."))
        .collect::<Vec<_>>()
        .join("\n\n");
    let after_image = (0..40)
        .map(|index| format!("Prompt paragraph after image {index}."))
        .collect::<Vec<_>>()
        .join("\n\n");
    harness.replace_history(
        "thread_a",
        vec![prompt_turn_with_local_image(
            "turn_prompt_image",
            before_image,
            "C:\\images\\operator.png",
            after_image,
        )],
    );

    let (estimated_blocks, requires_split) = harness
        .row_model_chunk_summary_at(0)
        .expect("image-marker prompt row should project");
    assert!(requires_split);
    assert!(estimated_blocks >= 32);

    let chunks = harness.row_model_chunk_kinds_at(0);
    assert!(chunks.len() > 1);
    assert!(chunks.iter().any(|unit| unit.contains("user-prompt:0")));
}

#[test]
fn row_model_marks_huge_fenced_code_block_as_single_safe_chunk() {
    let mut harness = PresentationHarness::new();
    let code = "let value = 42;\n".repeat(1_200);
    let markdown = format!("```rust\n{code}```");
    harness.replace_history(
        "thread_a",
        vec![agent_markdown_turn("turn_code", &markdown)],
    );

    let (estimated_blocks, requires_split) = harness
        .row_model_chunk_summary_at(0)
        .expect("huge fenced code row should project");
    assert!(requires_split);
    assert!(estimated_blocks >= 1);
    assert_eq!(
        harness.row_model_chunk_kinds_at(0).len(),
        1,
        "an indivisible fenced code block must not be split on guessed geometry"
    );
}

#[test]
fn row_model_marks_huge_single_line_as_single_safe_chunk() {
    let mut harness = PresentationHarness::new();
    let markdown = "M".repeat(20 * 1024);
    harness.replace_history(
        "thread_a",
        vec![agent_markdown_turn("turn_single_line", &markdown)],
    );

    let (_, requires_split) = harness
        .row_model_chunk_summary_at(0)
        .expect("huge single-line row should project");
    assert!(requires_split);
    assert_eq!(
        harness.row_model_chunk_kinds_at(0).len(),
        1,
        "a single Markdown block remains one chunk until measured geometry exists"
    );
}

#[test]
fn prepublication_preparation_accumulates_bounded_rows_for_layout() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![
            agent_markdown_turn("turn_1", "Assistant 1"),
            agent_markdown_turn("turn_2", "Assistant 2"),
            agent_markdown_turn("turn_3", "Assistant 3"),
        ],
    );
    let layout = prepublication_layout(1.0, 1);
    let mut window = harness.prepublication_preparation_window();
    let mut driver = TranscriptPrepublicationPreparationDriver::with_budget_for_test(
        TranscriptPrepublicationPreparationBudget::with_test_limits(
            1,
            usize::MAX,
            usize::MAX,
            usize::MAX,
        ),
    );

    let first = driver.drain_pending(window.preparation_request(prepublication_target(), layout));
    assert_eq!(first.summary.row_count, 3);
    assert_eq!(first.summary.prepared_rows, 1);
    assert_eq!(first.summary.pending_rows, 2);
    assert!(first.summary.rows_budget_exhausted);
    assert!(first.summary.requires_retry());
    window.note_summary(first.summary);
    assert!(!window.is_settled_for_publication());

    let second = driver.drain_pending(window.preparation_request(prepublication_target(), layout));
    assert_eq!(second.summary.prepared_rows, 2);
    assert_eq!(second.summary.pending_rows, 1);
    assert!(second.summary.requires_retry());
    window.note_summary(second.summary);
    assert!(!window.is_settled_for_publication());

    let third = driver.drain_pending(window.preparation_request(prepublication_target(), layout));
    assert_eq!(third.summary.prepared_rows, 3);
    assert_eq!(third.summary.pending_rows, 0);
    assert!(!third.summary.rows_budget_exhausted);
    assert!(!third.summary.requires_retry());
    window.note_summary(third.summary);
    assert!(window.is_settled_for_publication());
}

#[test]
fn prepublication_preparation_layout_change_requeues_staged_rows() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![
            agent_markdown_turn("turn_1", "Assistant 1"),
            agent_markdown_turn("turn_2", "Assistant 2"),
        ],
    );
    let layout_a = prepublication_layout(1.0, 1);
    let layout_b = prepublication_layout(1.25, 2);
    let mut window = harness.prepublication_preparation_window();
    let mut driver = TranscriptPrepublicationPreparationDriver::default();

    let first = driver.drain_pending(window.preparation_request(prepublication_target(), layout_a));
    assert_eq!(first.summary.pending_rows, 0);
    window.note_summary(first.summary);
    assert!(window.is_settled_for_publication());

    let request_after_layout_change = window.preparation_request(prepublication_target(), layout_b);
    assert_eq!(request_after_layout_change.pending_row_count(), 2);
    let second = driver.drain_pending(request_after_layout_change);
    assert_eq!(second.summary.pending_rows, 0);
    window.note_summary(second.summary);
    assert!(window.is_settled_for_publication());
}

#[test]
fn prepublication_preparation_processes_one_oversized_row_per_pass() {
    let mut harness = PresentationHarness::new();
    harness.replace_history("thread_a", vec![generated_images_turn("turn_1", 16)]);
    let layout = prepublication_layout(1.0, 1);
    let mut window = harness.prepublication_preparation_window();
    let mut driver = TranscriptPrepublicationPreparationDriver::with_budget_for_test(
        TranscriptPrepublicationPreparationBudget::with_test_limits(0, 1, 1, 1),
    );

    let drain = driver.drain_pending(window.preparation_request(prepublication_target(), layout));
    assert_eq!(drain.summary.prepared_rows, 1);
    assert_eq!(drain.summary.pending_rows, 0);
    assert!(!drain.summary.rows_budget_exhausted);
    assert!(!drain.summary.block_budget_exhausted);
    assert!(!drain.summary.media_budget_exhausted);
    assert!(!drain.summary.byte_budget_exhausted);
    window.note_summary(drain.summary);
    assert!(window.is_settled_for_publication());
}

#[test]
fn row_model_indexes_markdown_keys_to_the_owning_row() {
    let mut harness = PresentationHarness::new();
    harness.replace_history(
        "thread_a",
        vec![
            prompt_turn("turn_1", "Prompt 1"),
            agent_markdown_turn("turn_2", "Assistant **markdown**"),
        ],
    );

    let prompt_key = harness.first_markdown_key_at(0).unwrap();
    let assistant_key = harness.first_markdown_key_at(1).unwrap();

    assert_eq!(harness.row_index_for_markdown_key(&prompt_key), Some(0));
    assert_eq!(harness.row_index_for_markdown_key(&assistant_key), Some(1));
    assert_eq!(harness.row_index_for_markdown_key("missing:key"), None);
}

#[test]
fn row_measurement_key_tracks_revision_width_theme_and_display_state() {
    let mut harness = PresentationHarness::new();
    harness.replace_history("thread_a", vec![prompt_turn("turn_1", "Prompt 1")]);
    let base_display = TranscriptRowMeasurementDisplayState {
        is_first_row: true,
        show_activity_caret: false,
        promoted_media_key: None,
        code_panel_state_digest: 0,
    };
    let base = harness
        .measurement_key_at(0, 640.0, 1, base_display.clone())
        .unwrap();

    assert_ne!(
        harness
            .measurement_key_at(0, 720.0, 1, base_display.clone())
            .unwrap(),
        base
    );
    assert_ne!(
        harness
            .measurement_key_at(0, 640.0, 2, base_display.clone())
            .unwrap(),
        base
    );
    assert_ne!(
        harness
            .measurement_key_at(
                0,
                640.0,
                1,
                TranscriptRowMeasurementDisplayState {
                    show_activity_caret: true,
                    ..base_display
                },
            )
            .unwrap(),
        base
    );
}

#[test]
fn chunk_geometry_uses_measured_prefix_and_suffix_spacers() {
    let measured = vec![Some(px(100.0)); 8];
    let window = transcript_row_chunk_render_window(8, measured.as_slice(), px(250.0), px(200.0));

    assert_eq!(window.range, 1..6);
    assert_eq!(window.top_spacer_height, px(100.0));
    assert_eq!(window.bottom_spacer_height, px(200.0));
    assert_eq!(window.rendered_unknown_chunks, 0);
    assert_eq!(window.skipped_unknown_chunks, 0);
}

#[test]
fn chunk_geometry_covers_short_chunks_past_the_viewport() {
    let measured = vec![Some(px(20.0)); 40];
    let window = transcript_row_chunk_render_window(40, measured.as_slice(), px(200.0), px(200.0));

    assert_eq!(window.range, 5..25);
    assert_eq!(window.top_spacer_height, px(100.0));
    assert_eq!(window.bottom_spacer_height, px(300.0));
    assert_eq!(window.rendered_unknown_chunks, 0);
}

#[test]
fn chunk_geometry_does_not_create_spacers_for_unknown_suffix() {
    let mut measured = vec![Some(px(20.0)); 5];
    measured.extend(std::iter::repeat(None).take(40));
    let window =
        transcript_row_chunk_render_window(measured.len(), measured.as_slice(), px(0.0), px(60.0));

    assert_eq!(window.range, 0..29);
    assert_eq!(window.top_spacer_height, px(0.0));
    assert_eq!(window.bottom_spacer_height, px(0.0));
    assert_eq!(window.rendered_unknown_chunks, 24);
    assert_eq!(window.skipped_unknown_chunks, 16);
}

#[test]
fn chunk_measurement_key_tracks_row_layout_inputs() {
    let mut harness = PresentationHarness::new();
    let markdown = (0..40)
        .map(|index| format!("Paragraph {index} with enough text to own a block."))
        .collect::<Vec<_>>()
        .join("\n\n");
    harness.replace_history("thread_a", vec![agent_markdown_turn("turn_1", &markdown)]);
    let base_display = TranscriptRowMeasurementDisplayState {
        is_first_row: true,
        show_activity_caret: false,
        promoted_media_key: None,
        code_panel_state_digest: 0,
    };
    let base = harness
        .first_chunk_measurement_key_at(0, 640.0, 1, base_display.clone())
        .unwrap();

    assert_ne!(
        harness
            .first_chunk_measurement_key_at(0, 720.0, 1, base_display.clone())
            .unwrap(),
        base
    );
    assert_ne!(
        harness
            .first_chunk_measurement_key_at(0, 640.0, 2, base_display.clone())
            .unwrap(),
        base
    );
    assert_ne!(
        harness
            .first_chunk_measurement_key_at(
                0,
                640.0,
                1,
                TranscriptRowMeasurementDisplayState {
                    promoted_media_key: Some("media-a".to_string()),
                    ..base_display
                },
            )
            .unwrap(),
        base
    );
}

#[test]
fn chunk_geometry_reconciles_anchor_offset_when_measurements_change() {
    let before = vec![Some(px(100.0)); 8];
    let after = vec![
        Some(px(100.0)),
        Some(px(160.0)),
        Some(px(100.0)),
        Some(px(100.0)),
        Some(px(100.0)),
        Some(px(100.0)),
        Some(px(100.0)),
        Some(px(100.0)),
    ];

    let before_window =
        transcript_row_chunk_render_window(8, before.as_slice(), px(250.0), px(200.0));
    let after_window =
        transcript_row_chunk_render_window(8, after.as_slice(), px(310.0), px(200.0));

    assert_eq!(before_window.range.start, 1);
    assert_eq!(after_window.range.start, 1);
    assert_eq!(before_window.top_spacer_height, px(100.0));
    assert_eq!(after_window.top_spacer_height, px(100.0));
}

#[test]
fn row_model_revision_changes_when_live_text_changes() {
    let mut harness = PresentationHarness::new();
    let live_index = harness.begin_live_turn("Initial prompt");
    let prompt_revision = harness.row_model_revision_at(live_index).unwrap();

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
                id: "answer_live".to_string(),
                phase: Some(ProtocolPhase::FinalAnswer),
                text: "Assistant response.".to_string(),
            }),
        })
        .unwrap();

    assert_ne!(
        harness.row_model_revision_at(live_index).unwrap(),
        prompt_revision
    );
}

#[test]
fn transcript_render_metrics_remain_correct_after_cached_mutations() {
    let mut harness = PresentationHarness::new();
    let mixed_text = "Explain the workspace".len()
        + "I will inspect the package layout.".len()
        + "I inspected the package layout.".len()
        + "The workspace has a root Cargo package.".len();
    harness.replace_history("thread_a", vec![mixed_operational_turn("turn_2")]);
    assert_eq!(harness.render_metrics(), (1, 3, mixed_text));

    let earlier_prompt = "Earlier prompt";
    assert_eq!(
        harness.prepend_history("thread_a", vec![prompt_turn("turn_1", earlier_prompt)]),
        1
    );
    let mut expected_text = mixed_text + earlier_prompt.len();
    assert_eq!(harness.render_metrics(), (2, 3, expected_text));

    let live_prompt = "Live prompt";
    let live_index = harness.begin_live_turn(live_prompt);
    expected_text += live_prompt.len();
    assert_eq!(harness.render_metrics(), (3, 3, expected_text));

    let steering_prompt = "Steered follow-up";
    harness.append_live_user_fragment(live_index, steering_prompt);
    expected_text += steering_prompt.len();
    assert_eq!(harness.render_metrics(), (3, 3, expected_text));

    harness
        .apply_stream_event(TurnStreamEvent::TurnStarted {
            thread_id: "thread_a".to_string(),
            turn: empty_turn("turn_live", TurnStatus::InProgress),
        })
        .unwrap();
    let assistant_text = "Assistant after steering.";
    harness
        .apply_stream_event(TurnStreamEvent::ItemCompleted {
            thread_id: "thread_a".to_string(),
            turn_id: "turn_live".to_string(),
            item: ThreadItem::AgentMessage(AgentMessageItem {
                id: "assistant_after".to_string(),
                phase: Some(ProtocolPhase::FinalAnswer),
                text: assistant_text.to_string(),
            }),
        })
        .unwrap();
    expected_text += assistant_text.len();
    assert_eq!(harness.render_metrics(), (3, 4, expected_text));

    assert_eq!(harness.release_range_with_heights(0..1, &[px(40.0)]), 1);
    expected_text -= earlier_prompt.len();
    assert_eq!(harness.render_metrics(), (2, 4, expected_text));
    assert_eq!(harness.retained_counts(), (2, 4, expected_text));
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
fn operational_detail_release_does_not_create_transcript_rows() {
    let mut harness = PresentationHarness::new();
    let operational = command_turn("turn_cmd", "command_a", "cargo nextest", "ok");
    harness.replace_history("thread_a", vec![operational.clone()]);

    assert_eq!(harness.presentation_len(), 0);

    assert_eq!(harness.release_range_with_heights(0..1, &[px(120.0)]), 1);
    assert_eq!(harness.presentation_len(), 0);

    assert_eq!(
        harness.restore_history_page("thread_a", 0, &["turn_cmd".to_string()], vec![operational],),
        vec![TranscriptPresentationMutation::Unchanged]
    );
    assert_eq!(harness.presentation_len(), 0);
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
fn released_history_rows_are_removed_and_restored_by_full_pages() {
    let mut harness = PresentationHarness::new();
    let turn_1 = prompt_turn("turn_1", "Prompt 1");
    let turn_2 = prompt_turn("turn_2", "Prompt 2");
    let turn_3 = prompt_turn("turn_3", "Prompt 3");
    harness.replace_history("thread_a", vec![turn_1.clone(), turn_2.clone(), turn_3]);
    let turn_1_identity = harness.row_identity(0);
    let turn_2_identity = harness.row_identity(1);
    let turn_3_identity = harness.row_identity(2);

    assert_eq!(
        harness.release_range_with_heights(0..2, &[px(120.0), px(160.0)]),
        2
    );

    assert_eq!(harness.presentation_len(), 1);
    assert_eq!(harness.row_identity(0), turn_3_identity);
    assert_eq!(harness.turn_id_at(0).as_deref(), Some("turn_3"));
    assert_eq!(
        harness.latest_user_prompt_anchor(),
        Some((0, 0, "Prompt 3".to_string()))
    );

    assert_eq!(
        harness.restore_history_page(
            "thread_a",
            0,
            &["turn_1".to_string(), "turn_2".to_string()],
            vec![turn_1, turn_2],
        ),
        vec![
            TranscriptPresentationMutation::Inserted { index: 0, count: 1 },
            TranscriptPresentationMutation::Inserted { index: 1, count: 1 },
        ]
    );
    assert_eq!(harness.presentation_len(), 3);
    assert_eq!(harness.row_identity(0), turn_1_identity);
    assert_eq!(harness.row_identity(1), turn_2_identity);
    assert_eq!(harness.row_identity(2), turn_3_identity);
    assert_eq!(
        harness.latest_user_prompt_anchor(),
        Some((2, 0, "Prompt 3".to_string()))
    );
}

#[test]
fn turn_id_detail_release_removes_only_targeted_presentation_rows() {
    let mut harness = PresentationHarness::new();
    let turn_1 = prompt_turn("turn_1", "Prompt 1");
    let turn_2 = prompt_turn("turn_2", "Prompt 2");
    let turn_3 = prompt_turn("turn_3", "Prompt 3");
    harness.replace_history("thread_a", vec![turn_1, turn_2, turn_3]);
    let turn_2_identity = harness.row_identity(1);
    let turn_3_identity = harness.row_identity(2);

    assert_eq!(harness.release_turns_by_id(&["turn_1"]), 1);

    assert_eq!(harness.presentation_len(), 2);
    assert_eq!(harness.row_identity(0), turn_2_identity);
    assert_eq!(harness.row_identity(1), turn_3_identity);
    assert_eq!(harness.window_turn_ids(0..2), vec!["turn_2", "turn_3"]);
    assert_eq!(
        harness.latest_user_prompt_anchor(),
        Some((1, 0, "Prompt 3".to_string()))
    );
}

#[test]
fn narrative_detail_release_removes_row_and_full_page_restore_reinserts() {
    let mut harness = PresentationHarness::new();
    let prompt = prompt_turn("turn_1", "Prompt 1");
    harness.replace_history("thread_a", vec![prompt.clone()]);
    let identity = harness.row_identity(0);

    assert_eq!(harness.release_range_with_heights(0..1, &[px(120.0)]), 1);
    assert_eq!(harness.presentation_len(), 0);

    assert_eq!(
        harness.restore_history_page("thread_a", 0, &["turn_1".to_string()], vec![prompt],),
        vec![TranscriptPresentationMutation::Inserted { index: 0, count: 1 }]
    );
    assert_eq!(harness.presentation_len(), 1);
    assert_eq!(harness.row_identity(0), identity);
    assert_eq!(
        harness.visible_narrative_texts_at(0),
        vec!["user: Prompt 1"]
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

fn prepublication_layout(
    window_scale: f32,
    theme_revision: u64,
) -> TranscriptPrepublicationPreparationLayout {
    TranscriptPrepublicationPreparationLayout::new(
        px(720.0),
        px(480.0),
        px(672.0),
        px(12.0),
        window_scale,
        theme_revision,
    )
}

fn prepublication_target() -> TranscriptMediaAdmissionTarget {
    TranscriptMediaAdmissionTarget::SelectedThread {
        thread_id: "thread_a".to_string(),
    }
}

fn prompt_turn(id: &str, prompt: &str) -> TurnInfo {
    prompt_turn_with_fragments(id, &[prompt])
}

fn agent_markdown_turn(id: &str, text: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items_view: beryl_backend::TurnItemsView::Full,
        items: vec![ThreadItem::AgentMessage(AgentMessageItem {
            id: format!("{id}_agent"),
            phase: Some(ProtocolPhase::FinalAnswer),
            text: text.to_string(),
        })],
        error: None,
    }
}

fn generated_image_turn(
    turn_id: &str,
    image_id: &str,
    result: Option<String>,
    saved_path: Option<String>,
) -> TurnInfo {
    TurnInfo {
        id: turn_id.to_string(),
        status: TurnStatus::Completed,
        items_view: beryl_backend::TurnItemsView::Full,
        items: vec![ThreadItem::ImageGeneration(ImageGenerationItem {
            id: image_id.to_string(),
            status: Some("completed".to_string()),
            revised_prompt: Some("Generated image".to_string()),
            result,
            saved_path,
        })],
        error: None,
    }
}

fn generated_images_turn(turn_id: &str, count: usize) -> TurnInfo {
    TurnInfo {
        id: turn_id.to_string(),
        status: TurnStatus::Completed,
        items_view: beryl_backend::TurnItemsView::Full,
        items: (0..count)
            .map(|index| {
                ThreadItem::ImageGeneration(ImageGenerationItem {
                    id: format!("image_generation_{index}"),
                    status: Some("completed".to_string()),
                    revised_prompt: Some(format!("Generated image {index}")),
                    result: Some(format!("result-{index}")),
                    saved_path: None,
                })
            })
            .collect(),
        error: None,
    }
}

fn reasoning_turn(id: &str, summary: Vec<String>, content: Vec<String>) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items_view: beryl_backend::TurnItemsView::Full,
        items: vec![ThreadItem::Reasoning(ReasoningItem {
            id: format!("{id}_reasoning"),
            summary,
            content,
        })],
        error: None,
    }
}

fn prompt_turn_with_local_image(
    id: &str,
    before_image: String,
    image_path: &str,
    after_image: String,
) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items_view: beryl_backend::TurnItemsView::Full,
        items: vec![ThreadItem::UserMessage(UserMessageItem {
            id: format!("{id}_user"),
            content: vec![
                UserInput::Text { text: before_image },
                UserInput::LocalImage {
                    path: image_path.to_string(),
                },
                UserInput::Text { text: after_image },
            ],
        })],
        error: None,
    }
}

fn presentability_key(media_key: &str, source_revision: u64) -> TranscriptMediaReadinessKey {
    TranscriptMediaReadinessKey::new(
        TranscriptRowIdentity::new("row-a"),
        media_key,
        source_revision,
        None,
        TranscriptMediaRequestedRenderSize::new(100, 50),
        1.0,
        TranscriptRowPresentationRevision::default(),
    )
}

fn prompt_turn_with_fragments(id: &str, prompts: &[&str]) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items_view: beryl_backend::TurnItemsView::Full,
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
        items_view: beryl_backend::TurnItemsView::Full,
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
        items_view: beryl_backend::TurnItemsView::Full,
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

fn oversized_fallback_turn(id: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items_view: beryl_backend::TurnItemsView::Summary,
        items: vec![ThreadItem::Generic(beryl_backend::GenericThreadItem {
            id: format!("beryl:oversized-turn-fallback:{id}"),
            item_type: "beryl.oversizedTurnFallback".to_string(),
            tool: None,
            server: None,
            namespace: None,
            mcp_app_resource_uri: None,
            status: None,
            model: None,
            reasoning_effort: None,
            receiver_thread_ids: Vec::new(),
            agents_states: BTreeMap::new(),
            agent_nickname: None,
        })],
        error: None,
    }
}

fn empty_turn(id: &str, status: TurnStatus) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status,
        items_view: beryl_backend::TurnItemsView::Full,
        items: Vec::new(),
        error: None,
    }
}
