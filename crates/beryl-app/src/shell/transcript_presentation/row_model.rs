use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use gpui::Pixels;

use super::super::execution_detail::{
    ExecutionItem, TurnExecutionRecord, TurnExecutionStatus, TurnNarrativeEntry,
};
use super::TranscriptRowIdentity;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptRowPresentationModel {
    source_turn_identity: TranscriptRowSourceIdentity,
    narrative_units: Vec<TranscriptRowNarrativeUnit>,
    markdown_sources: Vec<TranscriptRowMarkdownSource>,
    media_descriptors: Vec<TranscriptRowMediaDescriptor>,
    item_count: usize,
    text_chars: usize,
    revision: TranscriptRowPresentationRevision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptRowSourceIdentity {
    pub(crate) source_turn_index: usize,
    pub(crate) thread_id: Option<String>,
    pub(crate) turn_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TranscriptRowNarrativeUnit {
    UserInput {
        fragment_id: u64,
        fragment_index: usize,
    },
    Item {
        item_id: String,
        item_index: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptRowMarkdownSource {
    pub(crate) key: String,
    pub(crate) block_path: String,
    pub(crate) source_kind: TranscriptRowMarkdownSourceKind,
    pub(crate) source_bytes: usize,
    pub(crate) source_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TranscriptRowMarkdownSourceKind {
    UserInput,
    AgentMessage,
    ReasoningSummary,
    ReasoningContent,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptRowMediaDescriptor {
    pub(crate) key: String,
    pub(crate) source_kind: TranscriptRowMediaDescriptorKind,
    pub(crate) source_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TranscriptRowMediaDescriptorKind {
    MarkdownImageCandidate,
    NativeGeneratedImage,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptRowPresentationRevision(u64);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptRowMeasurementDisplayState {
    pub(crate) is_first_row: bool,
    pub(crate) show_activity_caret: bool,
    pub(crate) promoted_media_key: Option<String>,
    pub(crate) code_panel_state_digest: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptRowMeasurementKey {
    pub(crate) row_identity: TranscriptRowIdentity,
    pub(crate) row_revision: TranscriptRowPresentationRevision,
    pub(crate) transcript_width_bits: u32,
    pub(crate) theme_revision: u64,
    pub(crate) display_state: TranscriptRowMeasurementDisplayState,
}

impl TranscriptRowPresentationModel {
    pub(in crate::shell) fn derive(source_turn_index: usize, turn: &TurnExecutionRecord) -> Self {
        let source_turn_identity = TranscriptRowSourceIdentity {
            source_turn_index,
            thread_id: turn.thread_id.clone(),
            turn_id: turn.turn_id.clone(),
        };
        let mut narrative_units = Vec::with_capacity(turn.narrative_entries().len());
        let mut markdown_sources = Vec::new();
        let mut media_descriptors = Vec::new();

        for entry in turn.narrative_entries() {
            match entry {
                TurnNarrativeEntry::UserInput { fragment_id } => {
                    let Some((fragment_index, fragment)) =
                        turn.user_input_fragment_by_id(*fragment_id)
                    else {
                        continue;
                    };
                    narrative_units.push(TranscriptRowNarrativeUnit::UserInput {
                        fragment_id: *fragment_id,
                        fragment_index,
                    });
                    if !fragment.text.is_empty() {
                        let key = turn_markdown_key(
                            &source_turn_identity,
                            turn,
                            &user_prompt_block_path(fragment_index),
                        );
                        let block_path = user_prompt_block_path(fragment_index);
                        let source_revision = hash_fragment_source(fragment);
                        markdown_sources.push(TranscriptRowMarkdownSource {
                            key: key.clone(),
                            block_path,
                            source_kind: TranscriptRowMarkdownSourceKind::UserInput,
                            source_bytes: fragment.text.len(),
                            source_revision,
                        });
                        if fragment.image_markers().is_empty()
                            && contains_markdown_image_candidate(fragment.text.as_str())
                        {
                            media_descriptors.push(TranscriptRowMediaDescriptor {
                                key,
                                source_kind:
                                    TranscriptRowMediaDescriptorKind::MarkdownImageCandidate,
                                source_revision,
                            });
                        }
                    }
                }
                TurnNarrativeEntry::Item { item_id } => {
                    let Some((item_index, item)) = turn
                        .items
                        .iter()
                        .enumerate()
                        .find(|(_, item)| item.id() == item_id)
                    else {
                        continue;
                    };
                    narrative_units.push(TranscriptRowNarrativeUnit::Item {
                        item_id: item_id.clone(),
                        item_index,
                    });
                    append_item_facts(
                        &source_turn_identity,
                        turn,
                        item,
                        &mut markdown_sources,
                        &mut media_descriptors,
                    );
                }
            }
        }

        let item_count = turn.item_count();
        let text_chars = turn.text_char_count();
        let revision = presentation_revision(
            &source_turn_identity,
            turn,
            &narrative_units,
            &markdown_sources,
            &media_descriptors,
        );

        Self {
            source_turn_identity,
            narrative_units,
            markdown_sources,
            media_descriptors,
            item_count,
            text_chars,
            revision,
        }
    }

    pub(crate) fn source_turn_identity(&self) -> &TranscriptRowSourceIdentity {
        &self.source_turn_identity
    }

    pub(crate) fn narrative_units(&self) -> &[TranscriptRowNarrativeUnit] {
        &self.narrative_units
    }

    pub(crate) fn markdown_sources(&self) -> &[TranscriptRowMarkdownSource] {
        &self.markdown_sources
    }

    pub(crate) fn media_descriptors(&self) -> &[TranscriptRowMediaDescriptor] {
        &self.media_descriptors
    }

    pub(crate) fn item_count(&self) -> usize {
        self.item_count
    }

    pub(crate) fn text_chars(&self) -> usize {
        self.text_chars
    }

    pub(crate) fn revision(&self) -> TranscriptRowPresentationRevision {
        self.revision
    }
}

impl TranscriptRowMeasurementKey {
    pub(crate) fn new(
        row_identity: TranscriptRowIdentity,
        row_revision: TranscriptRowPresentationRevision,
        transcript_width: Pixels,
        theme_revision: u64,
        display_state: TranscriptRowMeasurementDisplayState,
    ) -> Self {
        Self {
            row_identity,
            row_revision,
            transcript_width_bits: f32::from(transcript_width).to_bits(),
            theme_revision,
            display_state,
        }
    }
}

fn append_item_facts(
    source_turn_identity: &TranscriptRowSourceIdentity,
    turn: &TurnExecutionRecord,
    item: &ExecutionItem,
    markdown_sources: &mut Vec<TranscriptRowMarkdownSource>,
    media_descriptors: &mut Vec<TranscriptRowMediaDescriptor>,
) {
    match item {
        ExecutionItem::AgentMessage(message) => {
            if message.text.is_empty() {
                return;
            }
            let key = item_markdown_key(
                source_turn_identity,
                turn,
                message.id.as_str(),
                "agent-message",
            );
            let source_revision = hash_agent_message_source(message);
            markdown_sources.push(TranscriptRowMarkdownSource {
                key: key.clone(),
                block_path: format!("item:{}:agent-message", message.id),
                source_kind: TranscriptRowMarkdownSourceKind::AgentMessage,
                source_bytes: message.text.len(),
                source_revision,
            });
            if contains_markdown_image_candidate(message.text.as_str()) {
                media_descriptors.push(TranscriptRowMediaDescriptor {
                    key,
                    source_kind: TranscriptRowMediaDescriptorKind::MarkdownImageCandidate,
                    source_revision,
                });
            }
        }
        ExecutionItem::Reasoning(reasoning) => {
            append_reasoning_markdown_sources(
                source_turn_identity,
                turn,
                reasoning.id.as_str(),
                "reasoning-summary",
                TranscriptRowMarkdownSourceKind::ReasoningSummary,
                &reasoning.summary,
                reasoning.complete,
                markdown_sources,
            );
            append_reasoning_markdown_sources(
                source_turn_identity,
                turn,
                reasoning.id.as_str(),
                "reasoning-content",
                TranscriptRowMarkdownSourceKind::ReasoningContent,
                &reasoning.content,
                reasoning.complete,
                markdown_sources,
            );
        }
        ExecutionItem::GeneratedImage(image) => {
            media_descriptors.push(TranscriptRowMediaDescriptor {
                key: format!(
                    "{}:generated-image",
                    item_markdown_key(
                        source_turn_identity,
                        turn,
                        image.id.as_str(),
                        "generated-image"
                    )
                ),
                source_kind: TranscriptRowMediaDescriptorKind::NativeGeneratedImage,
                source_revision: hash_generated_image_source(image),
            });
        }
        ExecutionItem::CommandExecution(_)
        | ExecutionItem::FileChange(_)
        | ExecutionItem::Generic(_) => {}
    }
}

fn append_reasoning_markdown_sources(
    source_turn_identity: &TranscriptRowSourceIdentity,
    turn: &TurnExecutionRecord,
    item_id: &str,
    slot: &str,
    source_kind: TranscriptRowMarkdownSourceKind,
    items: &[String],
    complete: bool,
    markdown_sources: &mut Vec<TranscriptRowMarkdownSource>,
) {
    for (index, source) in items.iter().enumerate() {
        if source.is_empty() {
            continue;
        }
        markdown_sources.push(TranscriptRowMarkdownSource {
            key: indexed_item_markdown_key(source_turn_identity, turn, item_id, slot, index),
            block_path: format!("item:{item_id}:{slot}:{index}"),
            source_kind,
            source_bytes: source.len(),
            source_revision: hash_reasoning_source(item_id, slot, index, source, complete),
        });
    }
}

fn presentation_revision(
    source_turn_identity: &TranscriptRowSourceIdentity,
    turn: &TurnExecutionRecord,
    narrative_units: &[TranscriptRowNarrativeUnit],
    markdown_sources: &[TranscriptRowMarkdownSource],
    media_descriptors: &[TranscriptRowMediaDescriptor],
) -> TranscriptRowPresentationRevision {
    let mut hasher = DefaultHasher::new();
    turn_identity(source_turn_identity, turn).hash(&mut hasher);
    turn_status_label(turn.status).hash(&mut hasher);
    turn.awaiting_user_input.hash(&mut hasher);
    turn.terminal_assistant_item_id.hash(&mut hasher);
    turn.error_message
        .as_ref()
        .map(String::len)
        .hash(&mut hasher);
    narrative_units.hash(&mut hasher);
    markdown_sources.hash(&mut hasher);
    media_descriptors.hash(&mut hasher);
    TranscriptRowPresentationRevision(hasher.finish())
}

fn hash_fragment_source(fragment: &super::super::execution_detail::UserInputFragment) -> u64 {
    let mut hasher = DefaultHasher::new();
    fragment.id.hash(&mut hasher);
    fragment.text.hash(&mut hasher);
    fragment.backend_input().len().hash(&mut hasher);
    let image_markers = fragment.image_marker_specs();
    image_markers.len().hash(&mut hasher);
    hasher.finish()
}

fn hash_agent_message_source(message: &super::super::execution_detail::AgentMessageDetail) -> u64 {
    let mut hasher = DefaultHasher::new();
    message.id.hash(&mut hasher);
    message.phase.map(protocol_phase_label).hash(&mut hasher);
    message.text.hash(&mut hasher);
    message.complete.hash(&mut hasher);
    hasher.finish()
}

fn hash_reasoning_source(
    item_id: &str,
    slot: &str,
    index: usize,
    source: &str,
    complete: bool,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    item_id.hash(&mut hasher);
    slot.hash(&mut hasher);
    index.hash(&mut hasher);
    source.hash(&mut hasher);
    complete.hash(&mut hasher);
    hasher.finish()
}

fn hash_generated_image_source(
    image: &super::super::execution_detail::GeneratedImageDetail,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    image.id.hash(&mut hasher);
    image.status.hash(&mut hasher);
    image.revised_prompt.hash(&mut hasher);
    image
        .result
        .as_ref()
        .map(|result| result.len())
        .hash(&mut hasher);
    image.saved_path.hash(&mut hasher);
    image.complete.hash(&mut hasher);
    hasher.finish()
}

fn contains_markdown_image_candidate(source: &str) -> bool {
    source.contains("](") || source.contains("![")
}

fn turn_status_label(status: TurnExecutionStatus) -> &'static str {
    match status {
        TurnExecutionStatus::Queued => "queued",
        TurnExecutionStatus::Starting => "starting",
        TurnExecutionStatus::Running => "running",
        TurnExecutionStatus::Completed => "completed",
        TurnExecutionStatus::Interrupted => "interrupted",
        TurnExecutionStatus::Failed => "failed",
    }
}

fn protocol_phase_label(phase: beryl_backend::ProtocolPhase) -> &'static str {
    match phase {
        beryl_backend::ProtocolPhase::Commentary => "commentary",
        beryl_backend::ProtocolPhase::FinalAnswer => "final-answer",
    }
}

fn user_prompt_block_path(fragment_index: usize) -> String {
    format!("user-prompt:{fragment_index}")
}

fn turn_markdown_key(
    identity: &TranscriptRowSourceIdentity,
    turn: &TurnExecutionRecord,
    slot: &str,
) -> String {
    format!("{}:{slot}", turn_identity(identity, turn))
}

fn item_markdown_key(
    identity: &TranscriptRowSourceIdentity,
    turn: &TurnExecutionRecord,
    item_id: &str,
    slot: &str,
) -> String {
    format!("{}:item:{item_id}:{slot}", turn_identity(identity, turn))
}

fn indexed_item_markdown_key(
    identity: &TranscriptRowSourceIdentity,
    turn: &TurnExecutionRecord,
    item_id: &str,
    slot: &str,
    item_index: usize,
) -> String {
    format!(
        "{}:item:{item_id}:{slot}:{item_index}",
        turn_identity(identity, turn)
    )
}

fn turn_identity(identity: &TranscriptRowSourceIdentity, turn: &TurnExecutionRecord) -> String {
    match (turn.thread_id.as_deref(), turn.turn_id.as_deref()) {
        (Some(thread_id), Some(turn_id)) => format!("thread:{thread_id}:turn:{turn_id}"),
        (Some(thread_id), None) => {
            format!(
                "thread:{thread_id}:turn-index:{}",
                identity.source_turn_index
            )
        }
        (None, Some(turn_id)) => format!("pending-thread:turn:{turn_id}"),
        (None, None) => format!("pending-thread:turn-index:{}", identity.source_turn_index),
    }
}

impl Hash for TranscriptRowSourceIdentity {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.source_turn_index.hash(state);
        self.thread_id.hash(state);
        self.turn_id.hash(state);
    }
}
