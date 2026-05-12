use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use beryl_backend::{
    AgentMessageItem, CommandExecutionItem, CommandExecutionStatus, FileChangeItem,
    ImageGenerationItem, PatchApplyStatus, ProtocolPhase, ThreadInfo, ThreadItem, TurnError,
    TurnInfo, TurnStatus, TurnStreamEvent, UserInput, UserMessageItem,
};
use tracing::debug;

#[path = "execution_detail/transcript_images.rs"]
mod transcript_images;
#[allow(unused_imports)]
pub(crate) use transcript_images::{
    TranscriptImageInputSource, TranscriptImageLabelSource, TranscriptImageMarker,
    TranscriptImageMarkerSpec, TranscriptImagePathResolver, TranscriptImagePreviewState,
    TranscriptImageSource, TranscriptImageSourceResolution,
    transcript_image_source_from_local_image, transcript_image_source_from_local_image_with_format,
};
use transcript_images::{
    transcript_image_marker_specs_from_markers, transcript_image_markers_from_specs,
    transcript_image_parts_for_backend_records,
};

static NEXT_USER_INPUT_FRAGMENT_ID: AtomicU64 = AtomicU64::new(1);
const MAX_HISTORY_INLINE_GENERATED_IMAGE_RESULT_BYTES: usize = 256 * 1024;
pub(crate) const MAX_INLINE_GENERATED_IMAGE_RESULT_BYTES: usize = 256 * 1024;
pub(crate) const MAX_REASONING_CONTENT_BYTES: usize = 256 * 1024;
pub(crate) const MAX_REASONING_SUMMARY_BYTES: usize = 512 * 1024;
pub(crate) const MAX_COMMAND_OUTPUT_BYTES: usize = 256 * 1024;
pub(crate) const MAX_FILE_CHANGE_OUTPUT_BYTES: usize = 256 * 1024;
pub(crate) const MAX_ERROR_MESSAGE_BYTES: usize = 128 * 1024;

#[derive(Clone, Default)]
pub(super) struct ExecutionDetailState {
    turns: Vec<Arc<TurnExecutionRecord>>,
    active_turn_index: Option<usize>,
}

#[derive(Clone)]
pub(super) struct TurnExecutionRecord {
    pub user_input_fragments: Vec<UserInputFragment>,
    pub narrative_entries: Vec<TurnNarrativeEntry>,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub status: TurnExecutionStatus,
    pub released_history_placeholder: bool,
    suppress_user_input_echoes: bool,
    pub awaiting_user_input: bool,
    pub terminal_assistant_item_id: Option<String>,
    pub error_message: Option<String>,
    pub items: Vec<ExecutionItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UserInputFragment {
    pub id: u64,
    pub text: String,
    backend_input: Vec<UserInput>,
    image_markers: Vec<TranscriptImageMarker>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TurnNarrativeEntry {
    UserInput { fragment_id: u64 },
    Item { item_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ActiveTurnIdentity {
    pub turn_index: usize,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TurnExecutionStatus {
    Queued,
    Starting,
    Running,
    Completed,
    Interrupted,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LastTurnState {
    Unknown,
    Working,
    Ok,
    Error,
}

#[derive(Clone)]
pub(super) enum ExecutionItem {
    AgentMessage(AgentMessageDetail),
    Reasoning(ReasoningDetail),
    CommandExecution(CommandExecutionDetail),
    FileChange(FileChangeDetail),
    GeneratedImage(GeneratedImageDetail),
    Generic(GenericDetail),
}

#[derive(Clone)]
pub(super) struct AgentMessageDetail {
    pub id: String,
    pub phase: Option<ProtocolPhase>,
    pub text: String,
    pub complete: bool,
}

#[derive(Clone)]
pub(super) struct ReasoningDetail {
    pub id: String,
    pub summary: Vec<String>,
    pub content: Vec<String>,
    pub complete: bool,
}

#[derive(Clone)]
pub(super) struct CommandExecutionDetail {
    pub id: String,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub status: CommandExecutionStatus,
    pub output: String,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<i64>,
}

#[derive(Clone)]
pub(super) struct FileChangeDetail {
    pub id: String,
    pub status: PatchApplyStatus,
    pub changes: Vec<FileChangeEntry>,
    pub output: String,
}

#[derive(Clone)]
pub(super) struct FileChangeEntry {
    pub path: String,
}

#[derive(Clone)]
pub(super) struct GeneratedImageDetail {
    pub id: String,
    pub status: Option<String>,
    pub revised_prompt: Option<String>,
    pub result: Option<Arc<String>>,
    pub saved_path: Option<String>,
    pub complete: bool,
}

#[derive(Clone)]
pub(super) struct GenericDetail {
    pub id: String,
    pub item_type: String,
    pub complete: bool,
}

pub(super) struct PrependedHistoryPage {
    pub added_count: usize,
    pub turn_ids: Vec<String>,
}

pub(super) struct HistoryTurnReplacement {
    pub index: usize,
    pub turn: Arc<TurnExecutionRecord>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct TranscriptRenderMetrics {
    pub total_turns: usize,
    pub total_item_count: usize,
    pub total_text_chars: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TurnRenderMetrics {
    item_count: usize,
    text_chars: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ExecutionDetailRetainedCounts {
    pub(super) turns: usize,
    pub(super) items: usize,
    pub(super) text_bytes: usize,
    pub(super) user_fragments: usize,
    pub(super) user_fragment_text_bytes: usize,
    pub(super) backend_input_records: usize,
    pub(super) backend_input_bytes: usize,
    pub(super) image_marker_bytes: usize,
    pub(super) narrative_entries: usize,
    pub(super) released_placeholders: usize,
    pub(super) generated_image_items: usize,
    pub(super) active_turn_payload_bytes: usize,
    pub(super) agent_text_bytes: usize,
    pub(super) reasoning_summary_bytes: usize,
    pub(super) reasoning_content_bytes: usize,
    pub(super) command_text_bytes: usize,
    pub(super) command_output_bytes: usize,
    pub(super) file_change_path_bytes: usize,
    pub(super) file_change_output_bytes: usize,
    pub(super) generated_image_inline_bytes: usize,
    pub(super) generated_image_metadata_bytes: usize,
    pub(super) error_bytes: usize,
    pub(super) identity_bytes: usize,
    pub(super) payload_bytes: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TurnPayloadRetainedCounts {
    user_fragment_text_bytes: usize,
    backend_input_bytes: usize,
    image_marker_bytes: usize,
    agent_text_bytes: usize,
    reasoning_summary_bytes: usize,
    reasoning_content_bytes: usize,
    command_text_bytes: usize,
    command_output_bytes: usize,
    file_change_path_bytes: usize,
    file_change_output_bytes: usize,
    generated_image_inline_bytes: usize,
    generated_image_metadata_bytes: usize,
    error_bytes: usize,
    identity_bytes: usize,
}

impl ExecutionDetailState {
    pub fn reset(&mut self) {
        self.turns.clear();
        self.active_turn_index = None;
    }

    #[allow(dead_code)]
    pub fn begin_turn(&mut self, user_input: String) -> usize {
        self.begin_turn_with_fragments(vec![UserInputFragment::text(user_input)])
    }

    pub fn begin_turn_with_fragments(
        &mut self,
        user_input_fragments: Vec<UserInputFragment>,
    ) -> usize {
        self.push_turn_with_fragments(
            None,
            user_input_fragments,
            TurnExecutionStatus::Starting,
            true,
        )
    }

    pub fn begin_turn_with_thread_fragments(
        &mut self,
        thread_id: Option<String>,
        user_input_fragments: Vec<UserInputFragment>,
    ) -> usize {
        self.push_turn_with_fragments(
            thread_id,
            user_input_fragments,
            TurnExecutionStatus::Starting,
            true,
        )
    }

    pub fn begin_pending_turn_with_fragments(
        &mut self,
        user_input_fragments: Vec<UserInputFragment>,
    ) -> usize {
        self.push_turn_with_fragments(
            None,
            user_input_fragments,
            TurnExecutionStatus::Queued,
            false,
        )
    }

    fn push_turn_with_fragments(
        &mut self,
        thread_id: Option<String>,
        user_input_fragments: Vec<UserInputFragment>,
        status: TurnExecutionStatus,
        activate: bool,
    ) -> usize {
        let narrative_entries = user_input_fragments
            .iter()
            .map(|fragment| TurnNarrativeEntry::UserInput {
                fragment_id: fragment.id,
            })
            .collect();
        let turn = TurnExecutionRecord {
            user_input_fragments,
            narrative_entries,
            thread_id,
            turn_id: None,
            status,
            released_history_placeholder: false,
            suppress_user_input_echoes: true,
            awaiting_user_input: false,
            terminal_assistant_item_id: None,
            error_message: None,
            items: Vec::new(),
        };
        self.turns.push(Arc::new(turn));
        let turn_index = self.turns.len() - 1;
        if activate {
            self.active_turn_index = Some(turn_index);
        }
        turn_index
    }

    pub fn append_user_input_fragment(
        &mut self,
        turn_index: usize,
        fragment: UserInputFragment,
    ) -> Option<usize> {
        let turn = self.turns.get_mut(turn_index)?;
        Arc::make_mut(turn).append_user_input_fragment_to_narrative(fragment);
        Some(turn_index)
    }

    pub fn remove_user_input_fragments(&mut self, removals: &[(usize, u64, &str)]) -> Vec<usize> {
        let mut affected_turns = Vec::new();
        for (turn_index, fragment_id, expected_text) in removals.iter().copied() {
            let Some(turn) = self.turns.get_mut(turn_index) else {
                continue;
            };
            let Some(fragment_index) = turn
                .user_input_fragments
                .iter()
                .position(|fragment| fragment.id == fragment_id && fragment.text == expected_text)
            else {
                continue;
            };

            let turn = Arc::make_mut(turn);
            let removed = turn.user_input_fragments.remove(fragment_index);
            turn.narrative_entries.retain(|entry| match entry {
                TurnNarrativeEntry::UserInput { fragment_id } => *fragment_id != removed.id,
                TurnNarrativeEntry::Item { .. } => true,
            });
            if !affected_turns.contains(&turn_index) {
                affected_turns.push(turn_index);
            }
        }
        affected_turns.reverse();
        affected_turns
    }

    pub fn activate_pending_turn(&mut self, turn_index: usize) -> bool {
        if turn_index >= self.turns.len() {
            return false;
        }
        let turn = Arc::make_mut(&mut self.turns[turn_index]);
        turn.status = TurnExecutionStatus::Starting;
        turn.error_message = None;
        self.active_turn_index = Some(turn_index);
        true
    }

    pub fn turns(&self) -> &[Arc<TurnExecutionRecord>] {
        &self.turns
    }

    pub fn retained_counts(&self) -> ExecutionDetailRetainedCounts {
        self.turns.iter().fold(
            ExecutionDetailRetainedCounts {
                turns: self.turns.len(),
                ..ExecutionDetailRetainedCounts::default()
            },
            |mut counts, turn| {
                counts.items = counts.items.saturating_add(turn.item_count());
                counts.text_bytes = counts.text_bytes.saturating_add(turn.text_char_count());
                counts.user_fragments = counts
                    .user_fragments
                    .saturating_add(turn.user_input_fragments.len());
                counts.backend_input_records = counts.backend_input_records.saturating_add(
                    turn.user_input_fragments
                        .iter()
                        .map(|fragment| fragment.backend_input.len())
                        .sum::<usize>(),
                );
                counts.narrative_entries = counts
                    .narrative_entries
                    .saturating_add(turn.narrative_entries.len());
                counts.released_placeholders = counts
                    .released_placeholders
                    .saturating_add(usize::from(turn.released_history_placeholder));
                counts.generated_image_items = counts.generated_image_items.saturating_add(
                    turn.items
                        .iter()
                        .filter(|item| matches!(item, ExecutionItem::GeneratedImage(_)))
                        .count(),
                );
                let turn_payload = turn.retained_payload_counts();
                counts.user_fragment_text_bytes = counts
                    .user_fragment_text_bytes
                    .saturating_add(turn_payload.user_fragment_text_bytes);
                counts.backend_input_bytes = counts
                    .backend_input_bytes
                    .saturating_add(turn_payload.backend_input_bytes);
                counts.image_marker_bytes = counts
                    .image_marker_bytes
                    .saturating_add(turn_payload.image_marker_bytes);
                counts.agent_text_bytes = counts
                    .agent_text_bytes
                    .saturating_add(turn_payload.agent_text_bytes);
                counts.reasoning_summary_bytes = counts
                    .reasoning_summary_bytes
                    .saturating_add(turn_payload.reasoning_summary_bytes);
                counts.reasoning_content_bytes = counts
                    .reasoning_content_bytes
                    .saturating_add(turn_payload.reasoning_content_bytes);
                counts.command_text_bytes = counts
                    .command_text_bytes
                    .saturating_add(turn_payload.command_text_bytes);
                counts.command_output_bytes = counts
                    .command_output_bytes
                    .saturating_add(turn_payload.command_output_bytes);
                counts.file_change_path_bytes = counts
                    .file_change_path_bytes
                    .saturating_add(turn_payload.file_change_path_bytes);
                counts.file_change_output_bytes = counts
                    .file_change_output_bytes
                    .saturating_add(turn_payload.file_change_output_bytes);
                counts.generated_image_inline_bytes = counts
                    .generated_image_inline_bytes
                    .saturating_add(turn_payload.generated_image_inline_bytes);
                counts.generated_image_metadata_bytes = counts
                    .generated_image_metadata_bytes
                    .saturating_add(turn_payload.generated_image_metadata_bytes);
                counts.error_bytes = counts.error_bytes.saturating_add(turn_payload.error_bytes);
                counts.identity_bytes = counts
                    .identity_bytes
                    .saturating_add(turn_payload.identity_bytes);
                if self.active_turn_index.is_some_and(|index| {
                    self.turns
                        .get(index)
                        .is_some_and(|active_turn| Arc::ptr_eq(active_turn, turn))
                }) {
                    counts.active_turn_payload_bytes = counts
                        .active_turn_payload_bytes
                        .saturating_add(turn_payload.total_bytes());
                }
                counts.payload_bytes = counts
                    .payload_bytes
                    .saturating_add(turn_payload.total_bytes());
                counts
            },
        )
    }

    pub fn working_turn_index(&self) -> Option<usize> {
        if let Some(index) = self.active_turn_index {
            return Some(index);
        }

        self.turns
            .last()
            .is_some_and(|turn| {
                matches!(
                    turn.status,
                    TurnExecutionStatus::Starting | TurnExecutionStatus::Running
                )
            })
            .then(|| self.turns.len().saturating_sub(1))
    }

    pub fn active_turn_identity(&self) -> Option<ActiveTurnIdentity> {
        let turn_index = self.active_turn_index?;
        let turn = self.turns.get(turn_index)?;
        matches!(
            turn.status,
            TurnExecutionStatus::Starting | TurnExecutionStatus::Running
        )
        .then(|| ActiveTurnIdentity {
            turn_index,
            thread_id: turn.thread_id.clone(),
            turn_id: turn.turn_id.clone(),
        })
    }

    pub fn last_turn_state(&self) -> LastTurnState {
        if self.active_turn_index.is_some() {
            return LastTurnState::Working;
        }

        match self.turns.last().map(|turn| turn.status) {
            None | Some(TurnExecutionStatus::Queued) => LastTurnState::Unknown,
            Some(TurnExecutionStatus::Starting | TurnExecutionStatus::Running) => {
                LastTurnState::Working
            }
            Some(TurnExecutionStatus::Completed) => LastTurnState::Ok,
            Some(TurnExecutionStatus::Interrupted | TurnExecutionStatus::Failed) => {
                LastTurnState::Error
            }
        }
    }

    #[allow(dead_code)]
    pub fn load_thread_history(&mut self, thread: &ThreadInfo) {
        self.load_thread_history_with_image_resolver(
            thread,
            &TranscriptImagePathResolver::default(),
        );
    }

    pub fn load_thread_history_with_image_resolver(
        &mut self,
        thread: &ThreadInfo,
        image_resolver: &TranscriptImagePathResolver,
    ) {
        let load_started = Instant::now();
        let history_stats = history_generated_image_projection_stats(&thread.turns);
        self.reset();

        let thread_id = thread.summary().id;
        self.turns = thread
            .turns
            .iter()
            .map(|turn| {
                Arc::new(TurnExecutionRecord::from_history_turn(
                    &thread_id,
                    turn,
                    image_resolver,
                ))
            })
            .collect();
        if thread.status.waiting_on_user_input()
            && let Some(turn) = self.turns.last_mut()
        {
            Arc::make_mut(turn).awaiting_user_input = true;
        }
        debug!(
            thread_id = thread_id.as_str(),
            history_turn_count = thread.turns.len(),
            history_item_count = history_stats.item_count,
            history_generated_image_saved_path_count = history_stats.saved_path_count,
            history_generated_image_inline_retained_count = history_stats.inline_retained_count,
            history_generated_image_inline_dropped_count = history_stats.inline_dropped_count,
            history_inline_result_bytes_retained = history_stats.inline_bytes_retained,
            history_inline_result_bytes_dropped = history_stats.inline_bytes_dropped,
            load_thread_history_ms = elapsed_ms(load_started.elapsed()),
            "loaded thread history into execution detail state"
        );
    }

    #[allow(dead_code)]
    pub fn prepend_thread_history_page(&mut self, thread_id: &str, turns: Vec<TurnInfo>) -> usize {
        self.prepend_thread_history_page_with_ids(thread_id, turns)
            .added_count
    }

    pub fn prepend_thread_history_page_with_ids(
        &mut self,
        thread_id: &str,
        turns: Vec<TurnInfo>,
    ) -> PrependedHistoryPage {
        self.prepend_thread_history_page_with_image_resolver(
            thread_id,
            turns,
            &TranscriptImagePathResolver::default(),
        )
    }

    pub fn prepend_thread_history_page_with_image_resolver(
        &mut self,
        thread_id: &str,
        turns: Vec<TurnInfo>,
        image_resolver: &TranscriptImagePathResolver,
    ) -> PrependedHistoryPage {
        if turns.is_empty() {
            return PrependedHistoryPage {
                added_count: 0,
                turn_ids: Vec::new(),
            };
        }

        let prepend_started = Instant::now();
        let history_stats = history_generated_image_projection_stats(&turns);
        let existing_turn_ids = self
            .turns
            .iter()
            .filter_map(|turn| turn.turn_id.as_deref())
            .map(str::to_string)
            .collect::<HashSet<_>>();
        let mut records = turns
            .iter()
            .filter(|turn| !existing_turn_ids.contains(turn.id.as_str()))
            .map(|turn| {
                Arc::new(TurnExecutionRecord::from_history_turn(
                    thread_id,
                    turn,
                    image_resolver,
                ))
            })
            .collect::<Vec<_>>();
        let added = records.len();
        if added == 0 {
            return PrependedHistoryPage {
                added_count: 0,
                turn_ids: Vec::new(),
            };
        }
        let turn_ids = records
            .iter()
            .filter_map(|turn| turn.turn_id.clone())
            .collect::<Vec<_>>();
        records.append(&mut self.turns);
        self.turns = records;
        if let Some(index) = self.active_turn_index.as_mut() {
            *index += added;
        }
        debug!(
            thread_id,
            added_turn_count = added,
            history_item_count = history_stats.item_count,
            history_generated_image_saved_path_count = history_stats.saved_path_count,
            history_generated_image_inline_retained_count = history_stats.inline_retained_count,
            history_generated_image_inline_dropped_count = history_stats.inline_dropped_count,
            history_inline_result_bytes_retained = history_stats.inline_bytes_retained,
            history_inline_result_bytes_dropped = history_stats.inline_bytes_dropped,
            prepend_thread_history_ms = elapsed_ms(prepend_started.elapsed()),
            "prepended thread history page into execution detail state"
        );
        PrependedHistoryPage {
            added_count: added,
            turn_ids,
        }
    }

    pub fn release_history_range(&mut self, range: Range<usize>) -> Vec<HistoryTurnReplacement> {
        let end = range.end.min(self.turns.len());
        let start = range.start.min(end);
        let mut replacements = Vec::new();

        for index in start..end {
            if self.active_turn_index == Some(index) {
                continue;
            }

            let Some(placeholder) =
                TurnExecutionRecord::released_history_placeholder_from(self.turns[index].as_ref())
            else {
                continue;
            };
            let placeholder = Arc::new(placeholder);
            self.turns[index] = placeholder.clone();
            replacements.push(HistoryTurnReplacement {
                index,
                turn: placeholder,
            });
        }

        replacements
    }

    #[allow(dead_code)]
    pub fn restore_history_page(
        &mut self,
        thread_id: &str,
        row_start: usize,
        expected_turn_ids: &[String],
        turns: Vec<TurnInfo>,
    ) -> Vec<HistoryTurnReplacement> {
        self.restore_history_page_with_image_resolver(
            thread_id,
            row_start,
            expected_turn_ids,
            turns,
            &TranscriptImagePathResolver::default(),
        )
    }

    pub fn restore_history_page_with_image_resolver(
        &mut self,
        thread_id: &str,
        row_start: usize,
        expected_turn_ids: &[String],
        turns: Vec<TurnInfo>,
        image_resolver: &TranscriptImagePathResolver,
    ) -> Vec<HistoryTurnReplacement> {
        let mut turns_by_id = turns
            .into_iter()
            .map(|turn| (turn.id.clone(), turn))
            .collect::<HashMap<_, _>>();
        let mut replacements = Vec::new();

        for (offset, turn_id) in expected_turn_ids.iter().enumerate() {
            let index = row_start + offset;
            if index >= self.turns.len() {
                continue;
            }
            if self.turns[index].turn_id.as_deref() != Some(turn_id.as_str()) {
                continue;
            }
            let Some(turn) = turns_by_id.remove(turn_id.as_str()) else {
                continue;
            };

            let restored = Arc::new(TurnExecutionRecord::from_history_turn(
                thread_id,
                &turn,
                image_resolver,
            ));
            self.turns[index] = restored.clone();
            replacements.push(HistoryTurnReplacement {
                index,
                turn: restored,
            });
        }

        replacements
    }

    pub fn finish_turn_failure(&mut self, message: impl Into<String>) -> Option<usize> {
        let Some(index) = self.active_turn_index else {
            return None;
        };

        let turn = Arc::make_mut(&mut self.turns[index]);
        turn.status = TurnExecutionStatus::Failed;
        turn.error_message = Some(bounded_text(
            message.into(),
            MAX_ERROR_MESSAGE_BYTES,
            "turn error detail",
        ));
        turn.compact_terminal_operational_detail();
        self.active_turn_index = None;
        Some(index)
    }

    pub fn apply_stream_event(&mut self, event: TurnStreamEvent) -> Option<usize> {
        let Some(index) = self.active_turn_index else {
            return None;
        };

        if !stream_event_matches_active_turn(&self.turns[index], &event) {
            return None;
        }

        let mut finished_turn = false;
        {
            let turn = Arc::make_mut(&mut self.turns[index]);
            match event {
                TurnStreamEvent::ThreadStatusChanged { thread_id, status } => {
                    if turn.thread_id.as_deref() == Some(thread_id.as_str()) {
                        turn.awaiting_user_input = status.waiting_on_user_input();
                    }
                }
                TurnStreamEvent::TurnStarted {
                    thread_id,
                    turn: info,
                } => {
                    turn.thread_id = Some(thread_id);
                    turn.turn_id = Some(info.id.clone());
                    turn.status = TurnExecutionStatus::Running;
                    turn.error_message = None;
                }
                TurnStreamEvent::TurnCompleted {
                    thread_id,
                    turn: info,
                } => {
                    turn.thread_id = Some(thread_id);
                    turn.turn_id = Some(info.id.clone());
                    turn.status = execution_status_from_turn(&info);
                    turn.error_message = info.error.as_ref().map(|error| {
                        bounded_text(
                            backend_turn_error_detail(Some(error)),
                            MAX_ERROR_MESSAGE_BYTES,
                            "turn error detail",
                        )
                    });
                    turn.terminal_assistant_item_id = resolve_terminal_assistant_item(turn);
                    turn.compact_terminal_operational_detail();
                    finished_turn = true;
                }
                TurnStreamEvent::ItemStarted { item, .. } => {
                    turn.upsert_item(item, false, &TranscriptImagePathResolver::default());
                }
                TurnStreamEvent::ItemCompleted { item, .. } => {
                    turn.upsert_item(item, true, &TranscriptImagePathResolver::default());
                    turn.terminal_assistant_item_id = resolve_terminal_assistant_item(turn);
                }
                TurnStreamEvent::AgentMessageDelta { item_id, delta, .. } => {
                    turn.ensure_agent_message(item_id).text.push_str(&delta);
                }
                TurnStreamEvent::ReasoningSummaryPartAdded {
                    item_id,
                    summary_index,
                    ..
                } => {
                    ensure_text_slot(&mut turn.ensure_reasoning(item_id).summary, summary_index);
                }
                TurnStreamEvent::ReasoningSummaryTextDelta {
                    item_id,
                    summary_index,
                    delta,
                    ..
                } => {
                    let item = turn.ensure_reasoning(item_id);
                    ensure_text_slot(&mut item.summary, summary_index);
                    push_bounded_text(
                        &mut item.summary[summary_index],
                        &delta,
                        MAX_REASONING_SUMMARY_BYTES,
                        "reasoning summary",
                    );
                }
                TurnStreamEvent::ReasoningTextDelta {
                    item_id,
                    content_index,
                    delta,
                    ..
                } => {
                    let item = turn.ensure_reasoning(item_id);
                    ensure_text_slot(&mut item.content, content_index);
                    push_bounded_text(
                        &mut item.content[content_index],
                        &delta,
                        MAX_REASONING_CONTENT_BYTES,
                        "reasoning detail",
                    );
                }
                TurnStreamEvent::CommandExecutionOutputDelta { item_id, delta, .. } => {
                    push_bounded_text(
                        &mut turn.ensure_command_execution(item_id).output,
                        &delta,
                        MAX_COMMAND_OUTPUT_BYTES,
                        "command output",
                    );
                }
                TurnStreamEvent::FileChangeOutputDelta { item_id, delta, .. } => {
                    push_bounded_text(
                        &mut turn.ensure_file_change(item_id).output,
                        &delta,
                        MAX_FILE_CHANGE_OUTPUT_BYTES,
                        "file-change output",
                    );
                }
                TurnStreamEvent::ThreadStarted { .. } => {}
                TurnStreamEvent::AgentLabelUpdated { .. } => {}
                TurnStreamEvent::TokenUsageUpdated { .. } => {}
                TurnStreamEvent::AccountRateLimitsUpdated { .. } => {}
                TurnStreamEvent::ThreadNameUpdated { .. } => {}
                TurnStreamEvent::ThreadClosed { .. } => {}
                TurnStreamEvent::ApprovalRequested(_) => {}
                TurnStreamEvent::DynamicToolCallRequested(_) => {}
                TurnStreamEvent::ProtocolError { .. } => {}
            }
        }
        if finished_turn {
            self.active_turn_index = None;
        }
        Some(index)
    }
}

fn stream_event_matches_active_turn(turn: &TurnExecutionRecord, event: &TurnStreamEvent) -> bool {
    match event {
        TurnStreamEvent::ThreadStarted { thread } => {
            turn.thread_id.as_deref() == Some(thread.id.as_str())
        }
        TurnStreamEvent::AgentLabelUpdated { thread_id, .. }
        | TurnStreamEvent::ThreadStatusChanged { thread_id, .. }
        | TurnStreamEvent::ThreadClosed { thread_id }
        | TurnStreamEvent::ThreadNameUpdated { thread_id, .. } => {
            turn.thread_id.as_deref() == Some(thread_id.as_str())
        }
        TurnStreamEvent::ApprovalRequested(request) => {
            request.thread_id() == turn.thread_id.as_deref()
                && match request.turn_id() {
                    Some(turn_id) => turn.turn_id.as_deref() == Some(turn_id),
                    None => true,
                }
        }
        TurnStreamEvent::DynamicToolCallRequested(request) => {
            Some(request.thread_id()) == turn.thread_id.as_deref()
                && turn.turn_id.as_deref() == Some(request.turn_id())
        }
        TurnStreamEvent::TurnStarted {
            thread_id,
            turn: info,
        } => stream_turn_start_matches_active_turn(turn, thread_id, &info.id),
        TurnStreamEvent::TurnCompleted {
            thread_id,
            turn: info,
        } => stream_turn_identity_matches_active_turn(turn, thread_id, &info.id),
        TurnStreamEvent::ItemStarted {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::ItemCompleted {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::AgentMessageDelta {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::ReasoningSummaryPartAdded {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::ReasoningSummaryTextDelta {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::ReasoningTextDelta {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::CommandExecutionOutputDelta {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::FileChangeOutputDelta {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::TokenUsageUpdated {
            thread_id, turn_id, ..
        } => stream_turn_identity_matches_active_turn(turn, thread_id, turn_id),
        TurnStreamEvent::AccountRateLimitsUpdated { .. } => false,
        TurnStreamEvent::ProtocolError { .. } => true,
    }
}

fn stream_turn_start_matches_active_turn(
    turn: &TurnExecutionRecord,
    thread_id: &str,
    turn_id: &str,
) -> bool {
    optional_stream_identity_matches(turn.thread_id.as_deref(), thread_id)
        && optional_stream_identity_matches(turn.turn_id.as_deref(), turn_id)
}

fn stream_turn_identity_matches_active_turn(
    turn: &TurnExecutionRecord,
    thread_id: &str,
    turn_id: &str,
) -> bool {
    turn.thread_id.as_deref() == Some(thread_id) && turn.turn_id.as_deref() == Some(turn_id)
}

fn optional_stream_identity_matches(active: Option<&str>, event_value: &str) -> bool {
    match active {
        Some(active) => active == event_value,
        None => true,
    }
}

fn backend_turn_error_detail(error: Option<&TurnError>) -> String {
    let Some(error) = error else {
        return "The turn failed without an error payload from the backend.".to_string();
    };

    let primary = error.message.trim();
    let mut detail = if primary.is_empty() {
        "The turn failed without an error message.".to_string()
    } else {
        primary.to_string()
    };

    if let Some(additional) = error.additional_details.as_deref().map(str::trim)
        && !additional.is_empty()
    {
        detail.push_str("\n\n");
        detail.push_str(additional);
    }

    detail
}

impl LastTurnState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Working => "working",
            Self::Ok => "ok",
            Self::Error => "error",
        }
    }
}

impl UserInputFragment {
    pub fn text(text: impl Into<String>) -> Self {
        let text = text.into();
        Self::from_backend_input(text.clone(), vec![UserInput::text(text)])
    }

    pub fn from_backend_input(text: impl Into<String>, backend_input: Vec<UserInput>) -> Self {
        let text = text.into();
        let parts = transcript_image_parts_for_backend_records(
            &backend_input,
            &TranscriptImagePathResolver::default(),
        );
        let marker_specs = (parts.display_text() == text)
            .then(|| parts.into_image_markers())
            .unwrap_or_default();
        Self::from_backend_input_with_image_markers(text, backend_input, marker_specs)
    }

    pub fn from_backend_input_with_image_markers(
        text: impl Into<String>,
        backend_input: Vec<UserInput>,
        image_markers: Vec<TranscriptImageMarkerSpec>,
    ) -> Self {
        let id = NEXT_USER_INPUT_FRAGMENT_ID.fetch_add(1, Ordering::Relaxed);
        Self {
            id,
            text: text.into(),
            backend_input,
            image_markers: transcript_image_markers_from_specs(id, image_markers),
        }
    }

    pub fn backend_input(&self) -> &[UserInput] {
        &self.backend_input
    }

    #[allow(dead_code)]
    pub fn image_markers(&self) -> &[TranscriptImageMarker] {
        &self.image_markers
    }

    pub fn image_marker_specs(&self) -> Vec<TranscriptImageMarkerSpec> {
        transcript_image_marker_specs_from_markers(&self.image_markers)
    }

    pub fn retained_payload_bytes_lower_bound(&self) -> usize {
        self.text
            .len()
            .saturating_add(
                self.backend_input
                    .iter()
                    .map(user_input_payload_bytes)
                    .sum::<usize>(),
            )
            .saturating_add(self.image_markers.len().saturating_mul(32))
    }

    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty() && self.backend_input.is_empty()
    }
}

impl TurnExecutionRecord {
    pub fn is_released_history_placeholder(&self) -> bool {
        self.released_history_placeholder
    }

    pub fn user_input_fragments(&self) -> &[UserInputFragment] {
        &self.user_input_fragments
    }

    pub fn narrative_entries(&self) -> &[TurnNarrativeEntry] {
        &self.narrative_entries
    }

    pub fn user_input_fragment_by_id(
        &self,
        fragment_id: u64,
    ) -> Option<(usize, &UserInputFragment)> {
        self.user_input_fragments
            .iter()
            .enumerate()
            .find(|(_, fragment)| fragment.id == fragment_id)
    }

    pub fn item_by_id(&self, item_id: &str) -> Option<&ExecutionItem> {
        self.items.iter().find(|item| item.id() == item_id)
    }

    pub fn has_user_input_fragments(&self) -> bool {
        self.user_input_fragments
            .iter()
            .any(|fragment| !fragment.is_blank())
    }

    pub fn first_user_input_fragment_text(&self) -> Option<&str> {
        self.user_input_fragments
            .iter()
            .find(|fragment| !fragment.is_blank())
            .map(|fragment| fragment.text.as_str())
    }

    pub fn latest_user_input_fragment(&self) -> Option<(usize, &UserInputFragment)> {
        self.user_input_fragments
            .iter()
            .enumerate()
            .rev()
            .find(|(_, fragment)| !fragment.is_blank())
    }

    pub fn terminal_assistant_message(&self) -> Option<&AgentMessageDetail> {
        let terminal_id = self.terminal_assistant_item_id.as_deref()?;
        self.items.iter().find_map(|item| match item {
            ExecutionItem::AgentMessage(message) if message.id == terminal_id => Some(message),
            _ => None,
        })
    }

    pub fn text_char_count(&self) -> usize {
        self.render_metrics().text_chars
    }

    pub fn item_count(&self) -> usize {
        if self.released_history_placeholder {
            0
        } else {
            self.items.len()
        }
    }

    fn retained_payload_counts(&self) -> TurnPayloadRetainedCounts {
        if self.released_history_placeholder {
            return TurnPayloadRetainedCounts {
                identity_bytes: self
                    .thread_id
                    .as_ref()
                    .map_or(0, String::len)
                    .saturating_add(self.turn_id.as_ref().map_or(0, String::len)),
                ..TurnPayloadRetainedCounts::default()
            };
        }

        let mut counts = TurnPayloadRetainedCounts {
            identity_bytes: self
                .thread_id
                .as_ref()
                .map_or(0, String::len)
                .saturating_add(self.turn_id.as_ref().map_or(0, String::len))
                .saturating_add(
                    self.terminal_assistant_item_id
                        .as_ref()
                        .map_or(0, String::len),
                ),
            error_bytes: self.error_message.as_ref().map_or(0, String::len),
            ..TurnPayloadRetainedCounts::default()
        };

        for fragment in &self.user_input_fragments {
            counts.user_fragment_text_bytes = counts
                .user_fragment_text_bytes
                .saturating_add(fragment.text.len());
            counts.backend_input_bytes = counts.backend_input_bytes.saturating_add(
                fragment
                    .backend_input
                    .iter()
                    .map(user_input_payload_bytes)
                    .sum::<usize>(),
            );
            counts.image_marker_bytes = counts
                .image_marker_bytes
                .saturating_add(fragment.image_markers.len().saturating_mul(32));
        }

        for item in &self.items {
            counts.add_item(item);
        }

        counts
    }

    fn render_metrics(&self) -> TurnRenderMetrics {
        if self.released_history_placeholder {
            return TurnRenderMetrics::default();
        }

        let mut text_chars = self
            .user_input_fragments
            .iter()
            .map(|fragment| fragment.text.len())
            .sum::<usize>();
        if let Some(message) = self.error_message.as_ref() {
            text_chars += message.len();
        }

        text_chars += self
            .items
            .iter()
            .map(|item| match item {
                ExecutionItem::AgentMessage(item) => item.text.len(),
                ExecutionItem::Reasoning(item) => {
                    item.summary.iter().map(String::len).sum::<usize>()
                        + item.content.iter().map(String::len).sum::<usize>()
                }
                ExecutionItem::CommandExecution(item) => {
                    item.command.as_ref().map_or(0, String::len)
                        + item.cwd.as_ref().map_or(0, String::len)
                        + item.output.len()
                }
                ExecutionItem::FileChange(item) => {
                    item.output.len()
                        + item
                            .changes
                            .iter()
                            .map(|change| change.path.len())
                            .sum::<usize>()
                }
                ExecutionItem::GeneratedImage(item) => {
                    item.status.as_ref().map_or(0, String::len)
                        + item.revised_prompt.as_ref().map_or(0, String::len)
                        + item.saved_path.as_ref().map_or(0, String::len)
                }
                ExecutionItem::Generic(item) => item.item_type.len(),
            })
            .sum::<usize>();

        TurnRenderMetrics {
            item_count: self.items.len(),
            text_chars,
        }
    }

    fn from_history_turn(
        thread_id: &str,
        turn: &TurnInfo,
        image_resolver: &TranscriptImagePathResolver,
    ) -> Self {
        let mut record = Self {
            user_input_fragments: Vec::new(),
            narrative_entries: Vec::new(),
            thread_id: Some(thread_id.to_string()),
            turn_id: Some(turn.id.clone()),
            status: execution_status_from_turn(turn),
            released_history_placeholder: false,
            suppress_user_input_echoes: false,
            awaiting_user_input: false,
            terminal_assistant_item_id: None,
            error_message: turn.error.as_ref().map(|error| {
                bounded_text(
                    backend_turn_error_detail(Some(error)),
                    MAX_ERROR_MESSAGE_BYTES,
                    "turn error detail",
                )
            }),
            items: Vec::new(),
        };

        for item in turn.items.iter().cloned() {
            record.upsert_history_item(item, image_resolver);
        }

        record.terminal_assistant_item_id = resolve_terminal_assistant_item(&record);
        record
    }

    fn released_history_placeholder_from(turn: &TurnExecutionRecord) -> Option<Self> {
        if turn.released_history_placeholder {
            return None;
        }

        Some(Self {
            user_input_fragments: Vec::new(),
            narrative_entries: Vec::new(),
            thread_id: Some(turn.thread_id.clone()?),
            turn_id: Some(turn.turn_id.clone()?),
            status: turn.status,
            released_history_placeholder: true,
            suppress_user_input_echoes: false,
            awaiting_user_input: false,
            terminal_assistant_item_id: None,
            error_message: None,
            items: Vec::new(),
        })
    }

    fn ensure_agent_message(&mut self, item_id: String) -> &mut AgentMessageDetail {
        self.ensure_item(item_id.clone(), |id| {
            ExecutionItem::AgentMessage(AgentMessageDetail {
                id,
                phase: None,
                text: String::new(),
                complete: false,
            })
        });
        match self.find_item_mut(&item_id) {
            Some(ExecutionItem::AgentMessage(item)) => item,
            _ => unreachable!("agent message item must exist after insertion"),
        }
    }

    fn ensure_reasoning(&mut self, item_id: String) -> &mut ReasoningDetail {
        self.ensure_item(item_id.clone(), |id| {
            ExecutionItem::Reasoning(ReasoningDetail {
                id,
                summary: Vec::new(),
                content: Vec::new(),
                complete: false,
            })
        });
        match self.find_item_mut(&item_id) {
            Some(ExecutionItem::Reasoning(item)) => item,
            _ => unreachable!("reasoning item must exist after insertion"),
        }
    }

    fn ensure_command_execution(&mut self, item_id: String) -> &mut CommandExecutionDetail {
        self.ensure_item(item_id.clone(), |id| {
            ExecutionItem::CommandExecution(CommandExecutionDetail {
                id,
                command: None,
                cwd: None,
                status: CommandExecutionStatus::InProgress,
                output: String::new(),
                exit_code: None,
                duration_ms: None,
            })
        });
        match self.find_item_mut(&item_id) {
            Some(ExecutionItem::CommandExecution(item)) => item,
            _ => unreachable!("command execution item must exist after insertion"),
        }
    }

    fn ensure_file_change(&mut self, item_id: String) -> &mut FileChangeDetail {
        self.ensure_item(item_id.clone(), |id| {
            ExecutionItem::FileChange(FileChangeDetail {
                id,
                status: PatchApplyStatus::InProgress,
                changes: Vec::new(),
                output: String::new(),
            })
        });
        match self.find_item_mut(&item_id) {
            Some(ExecutionItem::FileChange(item)) => item,
            _ => unreachable!("file change item must exist after insertion"),
        }
    }

    fn upsert_item(
        &mut self,
        item: ThreadItem,
        complete: bool,
        image_resolver: &TranscriptImagePathResolver,
    ) {
        match item {
            ThreadItem::UserMessage(item) => self.merge_user_message(item, image_resolver),
            ThreadItem::AgentMessage(item) => self.upsert_agent_message(item, complete),
            ThreadItem::Reasoning(item) => self.upsert_reasoning(item, complete),
            ThreadItem::CommandExecution(item) => self.upsert_command_execution(item),
            ThreadItem::FileChange(item) => self.upsert_file_change(item),
            ThreadItem::ImageGeneration(item) => self.upsert_generated_image(item, complete, false),
            ThreadItem::Generic(item) => self.upsert_generic(item.id, item.item_type, complete),
        }
    }

    fn upsert_history_item(
        &mut self,
        item: ThreadItem,
        image_resolver: &TranscriptImagePathResolver,
    ) {
        match item {
            ThreadItem::ImageGeneration(item) => self.upsert_generated_image(item, true, true),
            item => self.upsert_item(item, true, image_resolver),
        }
    }

    fn merge_user_message(
        &mut self,
        item: UserMessageItem,
        image_resolver: &TranscriptImagePathResolver,
    ) {
        if self.suppress_user_input_echoes {
            if user_input_records_already_recorded(&self.user_input_fragments, &item.content) {
                return;
            }

            if let Some(remaining) =
                user_input_records_after_recorded_prefix(&self.user_input_fragments, &item.content)
            {
                if !remaining.is_empty() {
                    self.append_user_input_fragment_to_narrative(
                        user_input_fragment_from_backend_records(remaining, image_resolver),
                    );
                }
                return;
            }
        }

        let fragments = format_user_message_fragments(&item, image_resolver);
        if fragments.is_empty() {
            return;
        }

        self.append_user_input_fragments_to_narrative(fragments);
    }

    fn upsert_agent_message(&mut self, item: AgentMessageItem, complete: bool) {
        let target = self.ensure_agent_message(item.id);
        target.phase = item.phase;
        target.text = item.text;
        target.complete = complete;
    }

    fn upsert_reasoning(&mut self, item: beryl_backend::ReasoningItem, complete: bool) {
        let target = self.ensure_reasoning(item.id);
        target.summary = item
            .summary
            .into_iter()
            .map(|part| bounded_text(part, MAX_REASONING_SUMMARY_BYTES, "reasoning summary"))
            .collect();
        target.content = item
            .content
            .into_iter()
            .map(|part| bounded_text(part, MAX_REASONING_CONTENT_BYTES, "reasoning detail"))
            .collect();
        target.complete = complete;
    }

    fn upsert_command_execution(&mut self, item: CommandExecutionItem) {
        let target = self.ensure_command_execution(item.id);
        target.command = Some(item.command);
        target.cwd = Some(item.cwd);
        target.status = item.status;
        if let Some(output) = item.aggregated_output {
            target.output = bounded_text(output, MAX_COMMAND_OUTPUT_BYTES, "command output");
        }
        target.exit_code = item.exit_code;
        target.duration_ms = item.duration_ms;
    }

    fn upsert_file_change(&mut self, item: FileChangeItem) {
        let target = self.ensure_file_change(item.id);
        target.status = item.status;
        target.changes = item
            .changes
            .into_iter()
            .map(|change| FileChangeEntry {
                path: change.path.display().to_string(),
            })
            .collect();
    }

    fn upsert_generated_image(
        &mut self,
        item: ImageGenerationItem,
        complete: bool,
        history_item: bool,
    ) {
        let item_id = item.id.clone();
        self.ensure_item(item_id.clone(), |id| {
            ExecutionItem::GeneratedImage(GeneratedImageDetail {
                id,
                status: None,
                revised_prompt: None,
                result: None,
                saved_path: None,
                complete,
            })
        });
        if let Some(ExecutionItem::GeneratedImage(target)) = self.find_item_mut(&item_id) {
            let result = retain_generated_image_result(
                item.result,
                item.saved_path.as_deref(),
                history_item,
            );
            target.status = item.status;
            target.revised_prompt = item.revised_prompt;
            update_generated_image_result(&mut target.result, result);
            target.saved_path = item.saved_path;
            target.complete = complete;
        }
    }

    fn upsert_generic(&mut self, item_id: String, item_type: String, complete: bool) {
        self.ensure_item(item_id.clone(), |id| {
            ExecutionItem::Generic(GenericDetail {
                id,
                item_type: item_type.clone(),
                complete,
            })
        });
        if let Some(ExecutionItem::Generic(item)) = self.find_item_mut(&item_id) {
            item.item_type = item_type;
            item.complete = complete;
        }
    }

    fn ensure_item<F>(&mut self, item_id: String, create: F)
    where
        F: FnOnce(String) -> ExecutionItem,
    {
        if !self.items.iter().any(|item| item.id() == item_id) {
            self.narrative_entries.push(TurnNarrativeEntry::Item {
                item_id: item_id.clone(),
            });
            self.items.push(create(item_id));
        }
    }

    fn find_item_mut(&mut self, item_id: &str) -> Option<&mut ExecutionItem> {
        self.items.iter_mut().find(|item| item.id() == item_id)
    }

    fn append_user_input_fragment_to_narrative(&mut self, fragment: UserInputFragment) {
        self.narrative_entries.push(TurnNarrativeEntry::UserInput {
            fragment_id: fragment.id,
        });
        self.user_input_fragments.push(fragment);
    }

    fn append_user_input_fragments_to_narrative(
        &mut self,
        fragments: impl IntoIterator<Item = UserInputFragment>,
    ) {
        for fragment in fragments {
            self.append_user_input_fragment_to_narrative(fragment);
        }
    }

    fn compact_terminal_operational_detail(&mut self) {
        for item in &mut self.items {
            if let ExecutionItem::Reasoning(reasoning) = item {
                reasoning.content.clear();
            }
        }
    }
}

fn push_bounded_text(target: &mut String, delta: &str, limit: usize, label: &str) {
    if delta.is_empty() || target.contains("[Beryl omitted additional ") {
        return;
    }

    if target.len().saturating_add(delta.len()) <= limit {
        target.push_str(delta);
        return;
    }

    let marker = truncation_marker(label, limit);
    let retained_limit = limit.saturating_sub(marker.len());
    if target.len() > retained_limit {
        truncate_to_utf8_boundary(target, retained_limit);
    } else {
        let available = retained_limit.saturating_sub(target.len());
        target.push_str(prefix_at_utf8_boundary(delta, available));
    }
    target.push_str(marker.as_str());
}

fn bounded_text(mut text: String, limit: usize, label: &str) -> String {
    if text.len() <= limit {
        return text;
    }

    let marker = truncation_marker(label, limit);
    let retained_limit = limit.saturating_sub(marker.len());
    truncate_to_utf8_boundary(&mut text, retained_limit);
    text.push_str(marker.as_str());
    text
}

fn truncation_marker(label: &str, limit: usize) -> String {
    format!("\n[Beryl omitted additional {label} after {limit} retained bytes]")
}

fn truncate_to_utf8_boundary(text: &mut String, limit: usize) {
    let boundary = floor_char_boundary(text.as_str(), limit);
    text.truncate(boundary);
}

fn prefix_at_utf8_boundary(text: &str, limit: usize) -> &str {
    &text[..floor_char_boundary(text, limit)]
}

fn floor_char_boundary(text: &str, limit: usize) -> usize {
    let mut boundary = limit.min(text.len());
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    boundary
}

fn update_generated_image_result(current: &mut Option<Arc<String>>, next: Option<String>) {
    match next {
        Some(next) => {
            if current
                .as_ref()
                .is_some_and(|current| current.as_str() == next)
            {
                return;
            }
            *current = Some(Arc::new(next));
        }
        None => *current = None,
    }
}

fn retain_generated_image_result(
    result: Option<String>,
    saved_path: Option<&str>,
    _history_item: bool,
) -> Option<String> {
    if saved_path.is_some_and(|path| !path.trim().is_empty()) {
        return None;
    }

    if result
        .as_ref()
        .is_some_and(|result| result.len() > MAX_INLINE_GENERATED_IMAGE_RESULT_BYTES)
    {
        return None;
    }

    result
}

#[derive(Clone, Copy, Debug, Default)]
struct HistoryGeneratedImageProjectionStats {
    item_count: usize,
    saved_path_count: usize,
    inline_retained_count: usize,
    inline_dropped_count: usize,
    inline_bytes_retained: usize,
    inline_bytes_dropped: usize,
}

fn history_generated_image_projection_stats(
    turns: &[TurnInfo],
) -> HistoryGeneratedImageProjectionStats {
    let mut stats = HistoryGeneratedImageProjectionStats::default();
    for turn in turns {
        stats.item_count += turn.items.len();
        for item in &turn.items {
            let ThreadItem::ImageGeneration(item) = item else {
                continue;
            };

            let result_bytes = item.result.as_ref().map_or(0, String::len);
            let has_saved_path = item
                .saved_path
                .as_deref()
                .is_some_and(|path| !path.trim().is_empty());
            if has_saved_path {
                stats.saved_path_count += 1;
            }

            if result_bytes == 0 {
                continue;
            }

            if has_saved_path || result_bytes > MAX_HISTORY_INLINE_GENERATED_IMAGE_RESULT_BYTES {
                stats.inline_dropped_count += 1;
                stats.inline_bytes_dropped += result_bytes;
            } else {
                stats.inline_retained_count += 1;
                stats.inline_bytes_retained += result_bytes;
            }
        }
    }
    stats
}

fn elapsed_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

impl TurnPayloadRetainedCounts {
    fn add_item(&mut self, item: &ExecutionItem) {
        match item {
            ExecutionItem::AgentMessage(item) => {
                self.identity_bytes = self.identity_bytes.saturating_add(item.id.len());
                self.agent_text_bytes = self.agent_text_bytes.saturating_add(item.text.len());
            }
            ExecutionItem::Reasoning(item) => {
                self.identity_bytes = self.identity_bytes.saturating_add(item.id.len());
                self.reasoning_summary_bytes = self
                    .reasoning_summary_bytes
                    .saturating_add(item.summary.iter().map(String::len).sum::<usize>());
                self.reasoning_content_bytes = self
                    .reasoning_content_bytes
                    .saturating_add(item.content.iter().map(String::len).sum::<usize>());
            }
            ExecutionItem::CommandExecution(item) => {
                self.identity_bytes = self.identity_bytes.saturating_add(item.id.len());
                self.command_text_bytes = self
                    .command_text_bytes
                    .saturating_add(item.command.as_ref().map_or(0, String::len))
                    .saturating_add(item.cwd.as_ref().map_or(0, String::len));
                self.command_output_bytes =
                    self.command_output_bytes.saturating_add(item.output.len());
            }
            ExecutionItem::FileChange(item) => {
                self.identity_bytes = self.identity_bytes.saturating_add(item.id.len());
                self.file_change_path_bytes = self.file_change_path_bytes.saturating_add(
                    item.changes
                        .iter()
                        .map(|change| change.path.len())
                        .sum::<usize>(),
                );
                self.file_change_output_bytes = self
                    .file_change_output_bytes
                    .saturating_add(item.output.len());
            }
            ExecutionItem::GeneratedImage(item) => {
                self.identity_bytes = self.identity_bytes.saturating_add(item.id.len());
                self.generated_image_metadata_bytes = self
                    .generated_image_metadata_bytes
                    .saturating_add(item.status.as_ref().map_or(0, String::len))
                    .saturating_add(item.revised_prompt.as_ref().map_or(0, String::len))
                    .saturating_add(item.saved_path.as_ref().map_or(0, String::len));
                self.generated_image_inline_bytes = self
                    .generated_image_inline_bytes
                    .saturating_add(item.result.as_ref().map_or(0, |result| result.len()));
            }
            ExecutionItem::Generic(item) => {
                self.identity_bytes = self
                    .identity_bytes
                    .saturating_add(item.id.len())
                    .saturating_add(item.item_type.len());
            }
        }
    }

    fn total_bytes(self) -> usize {
        self.user_fragment_text_bytes
            .saturating_add(self.backend_input_bytes)
            .saturating_add(self.image_marker_bytes)
            .saturating_add(self.agent_text_bytes)
            .saturating_add(self.reasoning_summary_bytes)
            .saturating_add(self.reasoning_content_bytes)
            .saturating_add(self.command_text_bytes)
            .saturating_add(self.command_output_bytes)
            .saturating_add(self.file_change_path_bytes)
            .saturating_add(self.file_change_output_bytes)
            .saturating_add(self.generated_image_inline_bytes)
            .saturating_add(self.generated_image_metadata_bytes)
            .saturating_add(self.error_bytes)
            .saturating_add(self.identity_bytes)
    }
}

impl ExecutionItem {
    pub fn id(&self) -> &str {
        match self {
            Self::AgentMessage(item) => &item.id,
            Self::Reasoning(item) => &item.id,
            Self::CommandExecution(item) => &item.id,
            Self::FileChange(item) => &item.id,
            Self::GeneratedImage(item) => &item.id,
            Self::Generic(item) => &item.id,
        }
    }
}

fn user_input_payload_bytes(input: &UserInput) -> usize {
    match input {
        UserInput::Text { text } => text.len(),
        UserInput::Image { url } => url.len(),
        UserInput::LocalImage { path } => path.len(),
        UserInput::Skill { name, path } | UserInput::Mention { name, path } => {
            name.len().saturating_add(path.len())
        }
    }
}

fn execution_status_from_turn(turn: &TurnInfo) -> TurnExecutionStatus {
    match turn.status {
        TurnStatus::Completed => TurnExecutionStatus::Completed,
        TurnStatus::Interrupted => TurnExecutionStatus::Interrupted,
        TurnStatus::Failed => TurnExecutionStatus::Failed,
        TurnStatus::InProgress => TurnExecutionStatus::Running,
    }
}

fn resolve_terminal_assistant_item(turn: &TurnExecutionRecord) -> Option<String> {
    turn.items
        .iter()
        .rev()
        .find_map(|item| match item {
            ExecutionItem::AgentMessage(item) if item.phase == Some(ProtocolPhase::FinalAnswer) => {
                Some(item.id.clone())
            }
            _ => None,
        })
        .or_else(|| {
            turn.items.iter().rev().find_map(|item| match item {
                ExecutionItem::AgentMessage(item) => Some(item.id.clone()),
                _ => None,
            })
        })
}

fn ensure_text_slot(target: &mut Vec<String>, index: usize) {
    while target.len() <= index {
        target.push(String::new());
    }
}

fn format_user_message_fragments(
    item: &UserMessageItem,
    image_resolver: &TranscriptImagePathResolver,
) -> Vec<UserInputFragment> {
    if item.content.iter().any(is_image_user_input) {
        let fragment =
            user_input_fragment_from_backend_records(item.content.clone(), image_resolver);
        return (!fragment.text.is_empty())
            .then_some(fragment)
            .into_iter()
            .collect();
    }

    item.content
        .iter()
        .cloned()
        .filter_map(format_user_input_fragment)
        .filter(|fragment| !fragment.text.is_empty())
        .collect()
}

fn format_user_input_fragment(input: UserInput) -> Option<UserInputFragment> {
    let text = display_text_for_user_input(&input)?;
    Some(UserInputFragment::from_backend_input(text, vec![input]))
}

fn user_input_fragment_from_backend_records(
    records: Vec<UserInput>,
    image_resolver: &TranscriptImagePathResolver,
) -> UserInputFragment {
    let parts = transcript_image_parts_for_backend_records(&records, image_resolver);
    let text = parts.display_text().to_string();
    UserInputFragment::from_backend_input_with_image_markers(
        text,
        records,
        parts.into_image_markers(),
    )
}

fn display_text_for_user_input(input: &UserInput) -> Option<String> {
    match input {
        UserInput::Text { text } => Some(text.clone()),
        UserInput::Image { url } => Some(format!("Image: {url}")),
        UserInput::LocalImage { path } => Some(format!("Local image: {path}")),
        UserInput::Skill { name, path } => Some(format!("Skill: {name} ({path})")),
        UserInput::Mention { name, path } => Some(format!("Mention: {name} ({path})")),
    }
}

fn is_image_user_input(input: &UserInput) -> bool {
    matches!(
        input,
        UserInput::Image { .. } | UserInput::LocalImage { .. }
    )
}

fn user_input_records_already_recorded(
    recorded: &[UserInputFragment],
    incoming: &[UserInput],
) -> bool {
    !incoming.is_empty()
        && flattened_backend_input(recorded)
            .windows(incoming.len())
            .any(|window| window == incoming)
}

fn user_input_records_after_recorded_prefix(
    recorded: &[UserInputFragment],
    incoming: &[UserInput],
) -> Option<Vec<UserInput>> {
    let recorded = flattened_backend_input(recorded);
    (!recorded.is_empty() && recorded.len() < incoming.len() && incoming.starts_with(&recorded))
        .then(|| incoming[recorded.len()..].to_vec())
}

fn flattened_backend_input(fragments: &[UserInputFragment]) -> Vec<UserInput> {
    fragments
        .iter()
        .flat_map(|fragment| fragment.backend_input().iter().cloned())
        .collect()
}
