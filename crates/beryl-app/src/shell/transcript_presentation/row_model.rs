use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use gpui::{Pixels, px};

use super::super::execution_detail::{
    ExecutionItem, TurnExecutionRecord, TurnExecutionStatus, TurnNarrativeEntry,
};
use super::TranscriptRowIdentity;

const ESTIMATED_ROW_MODEL_BASE_BYTES: usize = 256;
const ESTIMATED_NARRATIVE_UNIT_BYTES: usize = 64;
const ESTIMATED_MARKDOWN_SOURCE_BYTES: usize = 160;
const ESTIMATED_MARKDOWN_STRUCTURE_BYTES_PER_SOURCE_BYTE: usize = 2;
const ESTIMATED_CODE_PANEL_PROJECTION_BYTES_PER_SOURCE_BYTE: usize = 1;
const ESTIMATED_MEDIA_DESCRIPTOR_BYTES: usize = 192;
const ESTIMATED_SOURCE_BACKED_MEDIA_LEASE_BYTES: usize = 1024 * 1024;
const ESTIMATED_RETAINED_MEDIA_LEASE_BYTES: usize = 256 * 1024;
const ESTIMATED_ROW_MEASUREMENT_BYTES: usize = 160;
const ESTIMATED_BLOCK_UNIT_BYTES: usize = 96;
const TRANSCRIPT_ROW_BLOCK_SPLIT_MIN_ESTIMATED_BLOCKS: usize = 32;
const TRANSCRIPT_ROW_BLOCK_SPLIT_MIN_TEXT_CHARS: usize = 16 * 1024;
const TRANSCRIPT_ROW_BLOCK_SPLIT_MIN_MEDIA_DESCRIPTORS: usize = 12;
const TRANSCRIPT_ROW_MARKDOWN_SOURCE_BYTES_PER_BLOCK: usize = 4 * 1024;
pub(crate) const TRANSCRIPT_ROW_BLOCK_ESTIMATED_HEIGHT_PX: f32 = 96.0;
pub(crate) const TRANSCRIPT_ROW_BLOCK_RENDER_OVERSCAN_BLOCKS: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptRowPresentationModel {
    source_turn_identity: TranscriptRowSourceIdentity,
    narrative_units: Vec<TranscriptRowNarrativeUnit>,
    markdown_sources: Vec<TranscriptRowMarkdownSource>,
    media_descriptors: Vec<TranscriptRowMediaDescriptor>,
    block_presentation: TranscriptRowBlockPresentation,
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
    TerminalFallback,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptRowMarkdownSource {
    pub(crate) key: String,
    pub(crate) block_path: String,
    pub(crate) source_kind: TranscriptRowMarkdownSourceKind,
    pub(crate) source_bytes: usize,
    pub(crate) estimated_render_blocks: usize,
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
    pub(crate) estimated_items: usize,
    pub(crate) source_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TranscriptRowMediaDescriptorKind {
    MarkdownImageCandidate,
    NativeGeneratedImage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptRowBlockPresentation {
    units: Vec<TranscriptRowBlockUnit>,
    estimated_render_blocks: usize,
    requires_block_split: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptRowBlockUnit {
    pub(crate) identity: String,
    pub(crate) owner: TranscriptRowBlockOwner,
    pub(crate) estimated_render_blocks: usize,
    pub(crate) source_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TranscriptRowBlockOwner {
    NarrativeUnit {
        unit_index: usize,
    },
    MarkdownSource {
        key: String,
        block_path: String,
        block_index: usize,
    },
    MediaDescriptor {
        key: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TranscriptRowBlockRenderWindow {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) total: usize,
    pub(crate) top_spacer_height: Pixels,
    pub(crate) bottom_spacer_height: Pixels,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptRowDerivedByteEstimate {
    pub(crate) presentation_model_bytes: usize,
    pub(crate) markdown_bytes: usize,
    pub(crate) code_panel_projection_bytes: usize,
    pub(crate) media_bytes: usize,
    pub(crate) row_measurement_bytes: usize,
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

        if turn.terminal_fallback().is_some() {
            narrative_units.push(TranscriptRowNarrativeUnit::TerminalFallback);
        }

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
                        let estimated_media_items =
                            estimate_markdown_media_candidate_count(fragment.text.as_str());
                        markdown_sources.push(TranscriptRowMarkdownSource {
                            key: key.clone(),
                            block_path,
                            source_kind: TranscriptRowMarkdownSourceKind::UserInput,
                            source_bytes: fragment.text.len(),
                            estimated_render_blocks:
                                estimate_markdown_source_render_blocks_from_text(
                                    fragment.text.as_str(),
                                )
                                .saturating_add(estimated_media_items),
                            source_revision,
                        });
                        if fragment.image_markers().is_empty() && estimated_media_items > 0 {
                            media_descriptors.push(TranscriptRowMediaDescriptor {
                                key,
                                source_kind:
                                    TranscriptRowMediaDescriptorKind::MarkdownImageCandidate,
                                estimated_items: estimated_media_items,
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
        let block_presentation = TranscriptRowBlockPresentation::derive(
            &source_turn_identity,
            turn,
            &narrative_units,
            &markdown_sources,
            &media_descriptors,
            text_chars,
        );
        let revision = presentation_revision(
            &source_turn_identity,
            turn,
            &narrative_units,
            &markdown_sources,
            &media_descriptors,
            &block_presentation,
        );

        Self {
            source_turn_identity,
            narrative_units,
            markdown_sources,
            media_descriptors,
            block_presentation,
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

    pub(crate) fn block_presentation(&self) -> &TranscriptRowBlockPresentation {
        &self.block_presentation
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

    pub(crate) fn estimated_derived_bytes(&self) -> TranscriptRowDerivedByteEstimate {
        let presentation_model_bytes = ESTIMATED_ROW_MODEL_BASE_BYTES
            .saturating_add(
                self.narrative_units
                    .iter()
                    .map(estimate_narrative_unit_bytes)
                    .sum::<usize>(),
            )
            .saturating_add(
                self.markdown_sources
                    .iter()
                    .map(|source| source.key.len().saturating_add(source.block_path.len()))
                    .sum::<usize>(),
            )
            .saturating_add(
                self.media_descriptors
                    .iter()
                    .map(|descriptor| descriptor.key.len())
                    .sum::<usize>(),
            )
            .saturating_add(
                self.block_presentation
                    .units()
                    .iter()
                    .map(estimate_block_unit_bytes)
                    .sum::<usize>(),
            );
        let markdown_bytes = self
            .markdown_sources
            .iter()
            .map(estimate_markdown_source_derived_bytes)
            .sum::<usize>();
        let code_panel_projection_bytes = self
            .markdown_sources
            .iter()
            .map(estimate_code_panel_projection_bytes)
            .sum::<usize>();
        let media_bytes = self
            .media_descriptors
            .iter()
            .map(estimate_media_descriptor_bytes)
            .sum::<usize>();
        TranscriptRowDerivedByteEstimate {
            presentation_model_bytes,
            markdown_bytes,
            code_panel_projection_bytes,
            media_bytes,
            row_measurement_bytes: ESTIMATED_ROW_MEASUREMENT_BYTES
                .saturating_add(
                    self.source_turn_identity
                        .thread_id
                        .as_ref()
                        .map_or(0, String::len),
                )
                .saturating_add(
                    self.source_turn_identity
                        .turn_id
                        .as_ref()
                        .map_or(0, String::len),
                ),
        }
    }
}

impl TranscriptRowDerivedByteEstimate {
    pub(crate) fn total(self) -> usize {
        self.presentation_model_bytes
            .saturating_add(self.markdown_bytes)
            .saturating_add(self.code_panel_projection_bytes)
            .saturating_add(self.media_bytes)
            .saturating_add(self.row_measurement_bytes)
    }
}

impl TranscriptRowBlockPresentation {
    fn derive(
        source_turn_identity: &TranscriptRowSourceIdentity,
        turn: &TurnExecutionRecord,
        narrative_units: &[TranscriptRowNarrativeUnit],
        markdown_sources: &[TranscriptRowMarkdownSource],
        media_descriptors: &[TranscriptRowMediaDescriptor],
        text_chars: usize,
    ) -> Self {
        let mut units = Vec::new();
        let turn_key = turn_identity(source_turn_identity, turn);

        for (unit_index, unit) in narrative_units.iter().enumerate() {
            match unit {
                TranscriptRowNarrativeUnit::UserInput { fragment_index, .. } => {
                    let block_path = user_prompt_block_path(*fragment_index);
                    if !push_markdown_block_units(
                        &turn_key,
                        markdown_sources,
                        block_path.as_str(),
                        &mut units,
                    ) {
                        push_narrative_block_unit(&turn_key, unit_index, unit, &mut units);
                    }
                }
                TranscriptRowNarrativeUnit::Item {
                    item_id,
                    item_index,
                } => {
                    let Some(item) = turn
                        .items
                        .get(*item_index)
                        .filter(|item| item.id() == item_id)
                    else {
                        push_narrative_block_unit(&turn_key, unit_index, unit, &mut units);
                        continue;
                    };
                    match item {
                        ExecutionItem::AgentMessage(message) => {
                            let block_path = format!("item:{}:agent-message", message.id);
                            if !push_markdown_block_units(
                                &turn_key,
                                markdown_sources,
                                block_path.as_str(),
                                &mut units,
                            ) {
                                push_narrative_block_unit(&turn_key, unit_index, unit, &mut units);
                            }
                        }
                        ExecutionItem::Reasoning(reasoning) => {
                            let mut pushed = false;
                            let prefix = format!("item:{}:", reasoning.id);
                            for source in markdown_sources
                                .iter()
                                .filter(|source| source.block_path.starts_with(&prefix))
                            {
                                push_markdown_source_units(&turn_key, source, &mut units);
                                pushed = true;
                            }
                            if !pushed {
                                push_narrative_block_unit(&turn_key, unit_index, unit, &mut units);
                            }
                        }
                        ExecutionItem::GeneratedImage(image) => {
                            let source_revision = hash_generated_image_source(image);
                            units.push(TranscriptRowBlockUnit {
                                identity: format!(
                                    "{turn_key}:generated-image:{}:rev:{source_revision}",
                                    image.id
                                ),
                                owner: TranscriptRowBlockOwner::MediaDescriptor {
                                    key: image.id.clone(),
                                },
                                estimated_render_blocks: 1,
                                source_revision,
                            });
                        }
                        ExecutionItem::CommandExecution(_)
                        | ExecutionItem::FileChange(_)
                        | ExecutionItem::Generic(_) => {
                            push_narrative_block_unit(&turn_key, unit_index, unit, &mut units);
                        }
                    }
                }
                TranscriptRowNarrativeUnit::TerminalFallback => {
                    push_narrative_block_unit(&turn_key, unit_index, unit, &mut units);
                }
            }
        }

        let estimated_render_blocks = units
            .iter()
            .map(|unit| unit.estimated_render_blocks.max(1))
            .sum::<usize>();
        let estimated_media_items = media_descriptors
            .iter()
            .map(|descriptor| descriptor.estimated_items.max(1))
            .sum::<usize>();
        let requires_block_split = estimated_render_blocks
            >= TRANSCRIPT_ROW_BLOCK_SPLIT_MIN_ESTIMATED_BLOCKS
            || text_chars >= TRANSCRIPT_ROW_BLOCK_SPLIT_MIN_TEXT_CHARS
            || estimated_media_items >= TRANSCRIPT_ROW_BLOCK_SPLIT_MIN_MEDIA_DESCRIPTORS;

        Self {
            units,
            estimated_render_blocks,
            requires_block_split,
        }
    }

    pub(crate) fn units(&self) -> &[TranscriptRowBlockUnit] {
        &self.units
    }

    #[cfg(test)]
    pub(crate) fn estimated_render_blocks(&self) -> usize {
        self.estimated_render_blocks
    }

    #[cfg(test)]
    pub(crate) fn requires_block_split(&self) -> bool {
        self.requires_block_split
    }

    pub(crate) fn render_window(
        &self,
        row_scroll_offset: Pixels,
        viewport_height: Pixels,
    ) -> Option<TranscriptRowBlockRenderWindow> {
        if !self.requires_block_split || self.units.is_empty() {
            return None;
        }

        let total = self.units.len();
        let block_height = px(TRANSCRIPT_ROW_BLOCK_ESTIMATED_HEIGHT_PX);
        let first_visible = ((f32::from(row_scroll_offset.max(px(0.0)))
            / TRANSCRIPT_ROW_BLOCK_ESTIMATED_HEIGHT_PX)
            .floor() as usize)
            .min(total);
        let visible_blocks = ((f32::from(viewport_height.max(block_height))
            / TRANSCRIPT_ROW_BLOCK_ESTIMATED_HEIGHT_PX)
            .ceil() as usize)
            .max(1);
        let start = first_visible.saturating_sub(TRANSCRIPT_ROW_BLOCK_RENDER_OVERSCAN_BLOCKS);
        let end = first_visible
            .saturating_add(visible_blocks)
            .saturating_add(TRANSCRIPT_ROW_BLOCK_RENDER_OVERSCAN_BLOCKS)
            .min(total)
            .max(start);
        Some(TranscriptRowBlockRenderWindow {
            start,
            end,
            total,
            top_spacer_height: block_height * start as f32,
            bottom_spacer_height: block_height * total.saturating_sub(end) as f32,
        })
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

fn estimate_narrative_unit_bytes(unit: &TranscriptRowNarrativeUnit) -> usize {
    ESTIMATED_NARRATIVE_UNIT_BYTES.saturating_add(match unit {
        TranscriptRowNarrativeUnit::UserInput { .. }
        | TranscriptRowNarrativeUnit::TerminalFallback => 0,
        TranscriptRowNarrativeUnit::Item { item_id, .. } => item_id.len(),
    })
}

fn estimate_markdown_source_derived_bytes(source: &TranscriptRowMarkdownSource) -> usize {
    ESTIMATED_MARKDOWN_SOURCE_BYTES
        .saturating_add(source.key.len())
        .saturating_add(source.block_path.len())
        .saturating_add(source.source_bytes)
        .saturating_add(
            source
                .source_bytes
                .saturating_mul(ESTIMATED_MARKDOWN_STRUCTURE_BYTES_PER_SOURCE_BYTE),
        )
}

fn estimate_code_panel_projection_bytes(source: &TranscriptRowMarkdownSource) -> usize {
    source
        .source_bytes
        .saturating_mul(ESTIMATED_CODE_PANEL_PROJECTION_BYTES_PER_SOURCE_BYTE)
}

fn estimate_media_descriptor_bytes(descriptor: &TranscriptRowMediaDescriptor) -> usize {
    ESTIMATED_MEDIA_DESCRIPTOR_BYTES
        .saturating_add(descriptor.key.len())
        .saturating_add(match descriptor.source_kind {
            TranscriptRowMediaDescriptorKind::MarkdownImageCandidate => {
                ESTIMATED_RETAINED_MEDIA_LEASE_BYTES
                    .saturating_mul(descriptor.estimated_items.max(1))
            }
            TranscriptRowMediaDescriptorKind::NativeGeneratedImage => {
                ESTIMATED_SOURCE_BACKED_MEDIA_LEASE_BYTES
            }
        })
}

fn push_markdown_block_units(
    turn_key: &str,
    markdown_sources: &[TranscriptRowMarkdownSource],
    block_path: &str,
    units: &mut Vec<TranscriptRowBlockUnit>,
) -> bool {
    let Some(source) = markdown_sources
        .iter()
        .find(|source| source.block_path == block_path)
    else {
        return false;
    };
    push_markdown_source_units(turn_key, source, units);
    true
}

fn push_markdown_source_units(
    _turn_key: &str,
    source: &TranscriptRowMarkdownSource,
    units: &mut Vec<TranscriptRowBlockUnit>,
) {
    let blocks = estimated_markdown_source_render_blocks(source);
    for block_index in 0..blocks {
        units.push(TranscriptRowBlockUnit {
            identity: format!(
                "{}:markdown-block:{block_index}:rev:{}",
                source.key, source.source_revision
            ),
            owner: TranscriptRowBlockOwner::MarkdownSource {
                key: source.key.clone(),
                block_path: source.block_path.clone(),
                block_index,
            },
            estimated_render_blocks: 1,
            source_revision: source.source_revision,
        });
    }
}

fn push_narrative_block_unit(
    turn_key: &str,
    unit_index: usize,
    unit: &TranscriptRowNarrativeUnit,
    units: &mut Vec<TranscriptRowBlockUnit>,
) {
    units.push(TranscriptRowBlockUnit {
        identity: format!("{turn_key}:narrative:{unit_index}"),
        owner: TranscriptRowBlockOwner::NarrativeUnit { unit_index },
        estimated_render_blocks: estimated_narrative_unit_render_blocks(unit),
        source_revision: 0,
    });
}

fn estimate_block_unit_bytes(unit: &TranscriptRowBlockUnit) -> usize {
    ESTIMATED_BLOCK_UNIT_BYTES
        .saturating_add(unit.identity.len())
        .saturating_add(match &unit.owner {
            TranscriptRowBlockOwner::NarrativeUnit { .. } => 0,
            TranscriptRowBlockOwner::MarkdownSource {
                key, block_path, ..
            } => key.len().saturating_add(block_path.len()),
            TranscriptRowBlockOwner::MediaDescriptor { key } => key.len(),
        })
}

fn estimated_narrative_unit_render_blocks(unit: &TranscriptRowNarrativeUnit) -> usize {
    match unit {
        TranscriptRowNarrativeUnit::UserInput { .. }
        | TranscriptRowNarrativeUnit::Item { .. }
        | TranscriptRowNarrativeUnit::TerminalFallback => 1,
    }
}

fn estimated_markdown_source_render_blocks(source: &TranscriptRowMarkdownSource) -> usize {
    source.estimated_render_blocks.max(
        source
            .source_bytes
            .max(1)
            .div_ceil(TRANSCRIPT_ROW_MARKDOWN_SOURCE_BYTES_PER_BLOCK),
    )
}

fn estimate_markdown_source_render_blocks_from_text(source: &str) -> usize {
    let source = source.replace("\r\n", "\n").replace('\r', "\n");
    let structural_blocks = source
        .split("\n\n")
        .filter(|block| !block.trim().is_empty())
        .map(|block| {
            let fenced_lines =
                if block.trim_start().starts_with("```") || block.trim_start().starts_with("~~~") {
                    block.lines().count().max(1)
                } else {
                    1
                };
            fenced_lines.max(1).max(
                block
                    .len()
                    .max(1)
                    .div_ceil(TRANSCRIPT_ROW_MARKDOWN_SOURCE_BYTES_PER_BLOCK),
            )
        })
        .sum::<usize>();
    structural_blocks.max(1)
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
            let estimated_media_items =
                estimate_markdown_media_candidate_count(message.text.as_str());
            markdown_sources.push(TranscriptRowMarkdownSource {
                key: key.clone(),
                block_path: format!("item:{}:agent-message", message.id),
                source_kind: TranscriptRowMarkdownSourceKind::AgentMessage,
                source_bytes: message.text.len(),
                estimated_render_blocks: estimate_markdown_source_render_blocks_from_text(
                    message.text.as_str(),
                )
                .saturating_add(estimated_media_items),
                source_revision,
            });
            if estimated_media_items > 0 {
                media_descriptors.push(TranscriptRowMediaDescriptor {
                    key,
                    source_kind: TranscriptRowMediaDescriptorKind::MarkdownImageCandidate,
                    estimated_items: estimated_media_items,
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
                estimated_items: 1,
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
            estimated_render_blocks: estimate_markdown_source_render_blocks_from_text(
                source.as_str(),
            ),
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
    block_presentation: &TranscriptRowBlockPresentation,
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
    block_presentation.units.hash(&mut hasher);
    block_presentation.estimated_render_blocks.hash(&mut hasher);
    block_presentation.requires_block_split.hash(&mut hasher);
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

fn estimate_markdown_media_candidate_count(source: &str) -> usize {
    let inline_image_candidates = source.match_indices("![").count();
    if inline_image_candidates > 0 {
        inline_image_candidates
    } else if source.contains("](") {
        1
    } else {
        0
    }
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
