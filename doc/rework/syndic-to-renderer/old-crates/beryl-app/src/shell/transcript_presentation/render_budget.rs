use std::ops::Range;

use super::row_model::{TranscriptRowChunkOwner, TranscriptRowRenderChunk};

pub(crate) const TRANSCRIPT_RENDER_BUDGET_CHUNK_FALLBACK_MESSAGE: &str =
    "This turn section is too expensive to render within Beryl's transcript frame render budget.";
pub(crate) const TRANSCRIPT_RENDER_BUDGET_FRAME_FALLBACK_MESSAGE: &str = "Additional turn sections were omitted from this frame because Beryl's transcript render budget was reached.";

const TRANSCRIPT_RENDER_COST_BASE_UNITS: usize = 12;
const TRANSCRIPT_RENDER_COST_BLOCK_UNITS: usize = 10;
const TRANSCRIPT_RENDER_COST_MARKDOWN_SOURCE_UNITS: usize = 18;
const TRANSCRIPT_RENDER_COST_MARKDOWN_UNIT_UNITS: usize = 4;
const TRANSCRIPT_RENDER_COST_MEDIA_UNITS: usize = 96;
const TRANSCRIPT_RENDER_COST_NARRATIVE_UNITS: usize = 20;
const TRANSCRIPT_RENDER_COST_DEFAULT_MAX_CHUNK_UNITS: usize = 2_048;
const TRANSCRIPT_RENDER_COST_DEFAULT_MAX_FRAME_UNITS: usize = 8_192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptRenderCostWeights {
    base_units: usize,
    block_units: usize,
    markdown_source_units: usize,
    markdown_unit_units: usize,
    media_units: usize,
    narrative_units: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptRenderBudgetPolicy {
    max_chunk_units: usize,
    max_frame_units: usize,
    weights: TranscriptRenderCostWeights,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptRenderBudgetFallbackReason {
    ChunkCostExceedsLimit,
    FrameCostExceedsLimit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptRenderChunkAdmission {
    pub(crate) chunk_index: usize,
    pub(crate) cost_units: usize,
    pub(crate) decision: TranscriptRenderChunkAdmissionDecision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptRenderChunkAdmissionDecision {
    Render,
    Fallback(TranscriptRenderBudgetFallbackReason),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptRenderBudgetAdmission {
    pub(crate) chunks: Vec<TranscriptRenderChunkAdmission>,
    pub(crate) rendered_chunks: usize,
    pub(crate) fallback_chunks: usize,
    pub(crate) rendered_cost_units: usize,
    pub(crate) fallback_cost_units: usize,
}

impl Default for TranscriptRenderCostWeights {
    fn default() -> Self {
        Self {
            base_units: TRANSCRIPT_RENDER_COST_BASE_UNITS,
            block_units: TRANSCRIPT_RENDER_COST_BLOCK_UNITS,
            markdown_source_units: TRANSCRIPT_RENDER_COST_MARKDOWN_SOURCE_UNITS,
            markdown_unit_units: TRANSCRIPT_RENDER_COST_MARKDOWN_UNIT_UNITS,
            media_units: TRANSCRIPT_RENDER_COST_MEDIA_UNITS,
            narrative_units: TRANSCRIPT_RENDER_COST_NARRATIVE_UNITS,
        }
    }
}

impl TranscriptRenderBudgetPolicy {
    pub(crate) fn default_frame() -> Self {
        Self::default()
    }

    pub(crate) fn max_chunk_units(self) -> usize {
        self.max_chunk_units
    }

    pub(crate) fn max_frame_units(self) -> usize {
        self.max_frame_units
    }

    #[cfg(test)]
    pub(crate) fn with_test_limits(max_chunk_units: usize, max_frame_units: usize) -> Self {
        Self {
            max_chunk_units,
            max_frame_units,
            weights: TranscriptRenderCostWeights::default(),
        }
    }
}

impl Default for TranscriptRenderBudgetPolicy {
    fn default() -> Self {
        Self {
            max_chunk_units: TRANSCRIPT_RENDER_COST_DEFAULT_MAX_CHUNK_UNITS,
            max_frame_units: TRANSCRIPT_RENDER_COST_DEFAULT_MAX_FRAME_UNITS,
            weights: TranscriptRenderCostWeights::default(),
        }
    }
}

impl TranscriptRenderBudgetFallbackReason {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::ChunkCostExceedsLimit => TRANSCRIPT_RENDER_BUDGET_CHUNK_FALLBACK_MESSAGE,
            Self::FrameCostExceedsLimit => TRANSCRIPT_RENDER_BUDGET_FRAME_FALLBACK_MESSAGE,
        }
    }

    pub(crate) fn diagnostic_label(self) -> &'static str {
        match self {
            Self::ChunkCostExceedsLimit => "chunk_cost_exceeds_limit",
            Self::FrameCostExceedsLimit => "frame_cost_exceeds_limit",
        }
    }
}

pub(crate) fn transcript_render_chunk_cost_units(
    chunk: &TranscriptRowRenderChunk,
    policy: TranscriptRenderBudgetPolicy,
) -> usize {
    let weights = policy.weights;
    let mut cost = weights.base_units.saturating_add(
        chunk
            .estimated_render_blocks
            .max(1)
            .saturating_mul(weights.block_units),
    );
    cost = cost.saturating_add(match &chunk.owner {
        TranscriptRowChunkOwner::NarrativeUnit { .. } => weights.narrative_units,
        TranscriptRowChunkOwner::MarkdownSource { unit_count, .. } => {
            weights.markdown_source_units.saturating_add(
                (*unit_count)
                    .max(1)
                    .saturating_mul(weights.markdown_unit_units),
            )
        }
        TranscriptRowChunkOwner::MediaDescriptor { .. } => weights.media_units,
    });
    cost.max(1)
}

pub(crate) fn transcript_render_window_admission(
    chunks: &[TranscriptRowRenderChunk],
    range: Range<usize>,
    policy: TranscriptRenderBudgetPolicy,
) -> TranscriptRenderBudgetAdmission {
    let start = range.start.min(chunks.len());
    let end = range.end.min(chunks.len()).max(start);
    let mut admission = TranscriptRenderBudgetAdmission::default();
    let mut rendered_cost_units = 0usize;
    let mut forced_fallback_reason = None;

    for chunk_index in start..end {
        let Some(chunk) = chunks.get(chunk_index) else {
            continue;
        };
        let cost_units = transcript_render_chunk_cost_units(chunk, policy);
        let reason = forced_fallback_reason.or_else(|| {
            if cost_units > policy.max_chunk_units {
                Some(TranscriptRenderBudgetFallbackReason::ChunkCostExceedsLimit)
            } else if rendered_cost_units.saturating_add(cost_units) > policy.max_frame_units {
                Some(TranscriptRenderBudgetFallbackReason::FrameCostExceedsLimit)
            } else {
                None
            }
        });

        match reason {
            Some(reason) => {
                forced_fallback_reason = Some(reason);
                admission.fallback_chunks = admission.fallback_chunks.saturating_add(1);
                admission.fallback_cost_units =
                    admission.fallback_cost_units.saturating_add(cost_units);
                admission.chunks.push(TranscriptRenderChunkAdmission {
                    chunk_index,
                    cost_units,
                    decision: TranscriptRenderChunkAdmissionDecision::Fallback(reason),
                });
            }
            None => {
                rendered_cost_units = rendered_cost_units.saturating_add(cost_units);
                admission.rendered_chunks = admission.rendered_chunks.saturating_add(1);
                admission.rendered_cost_units = rendered_cost_units;
                admission.chunks.push(TranscriptRenderChunkAdmission {
                    chunk_index,
                    cost_units,
                    decision: TranscriptRenderChunkAdmissionDecision::Render,
                });
            }
        }
    }

    admission
}
