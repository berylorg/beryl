use std::{
    collections::BTreeMap,
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

use gpui::Pixels;

use super::{
    execution_detail::{TranscriptRenderMetrics, TurnExecutionRecord},
    transcript_projection::project_parent_narrative_turn,
};

#[path = "transcript_presentation/chunk_geometry.rs"]
mod chunk_geometry;
#[path = "transcript_presentation/identity.rs"]
mod identity;
#[path = "transcript_presentation/metrics.rs"]
mod metrics;
#[allow(dead_code)]
#[path = "transcript_presentation/range.rs"]
mod range;
#[path = "transcript_presentation/render_budget.rs"]
mod render_budget;
#[path = "transcript_presentation/row_model.rs"]
mod row_model;

#[allow(unused_imports)]
pub(crate) use chunk_geometry::{
    TranscriptRowChunkMeasurementKey, TranscriptRowChunkRenderWindow,
    TranscriptRowStreamedAnchorPlacement, TranscriptRowStreamedRenderAnchor,
    measured_chunk_heights_for, transcript_row_chunk_render_window,
};
use identity::{latest_user_prompt_anchor_in_rows, stable_row_identity, user_prompt_anchor_text};
use metrics::TranscriptPresentationRowMetrics;
#[allow(unused_imports)]
pub(crate) use row_model::{
    TranscriptRowChunkOwner, TranscriptRowChunkPresentation, TranscriptRowDerivedByteEstimate,
    TranscriptRowMarkdownSource, TranscriptRowMarkdownSourceKind,
    TranscriptRowMeasurementDisplayState, TranscriptRowMeasurementKey,
    TranscriptRowMediaDescriptor, TranscriptRowMediaDescriptorKind, TranscriptRowNarrativeUnit,
    TranscriptRowPresentationModel, TranscriptRowPresentationRevision, TranscriptRowRenderChunk,
};

#[allow(unused_imports)]
pub(crate) use range::{
    TRANSCRIPT_INITIAL_PRESENTATION_ROWS, TRANSCRIPT_MAX_PRESENTATION_ROWS,
    transcript_frame_preload_range, transcript_frame_presentation_range,
};
#[allow(unused_imports)]
pub(crate) use render_budget::{
    TRANSCRIPT_RENDER_BUDGET_CHUNK_FALLBACK_MESSAGE,
    TRANSCRIPT_RENDER_BUDGET_FRAME_FALLBACK_MESSAGE, TranscriptRenderBudgetAdmission,
    TranscriptRenderBudgetFallbackReason, TranscriptRenderBudgetPolicy,
    TranscriptRenderChunkAdmission, TranscriptRenderChunkAdmissionDecision,
    transcript_render_chunk_cost_units, transcript_render_window_admission,
};

#[derive(Clone, Default)]
pub(crate) struct TranscriptPresentationState {
    rows: Vec<TranscriptPresentationRow>,
    markdown_key_row_identities: HashMap<String, TranscriptRowIdentity>,
    render_metrics: TranscriptRenderMetrics,
    latest_user_prompt_anchor: Option<(usize, usize, String)>,
    next_ephemeral_row_id: u64,
}

#[derive(Clone)]
struct TranscriptPresentationRow {
    identity: TranscriptRowIdentity,
    source_turn_index: usize,
    turn: Arc<TurnExecutionRecord>,
    metrics: TranscriptPresentationRowMetrics,
    model: Arc<TranscriptRowPresentationModel>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptRowIdentity(String);

impl TranscriptRowIdentity {
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone)]
pub(crate) struct TranscriptPresentedRow {
    pub(crate) index: usize,
    pub(crate) identity: TranscriptRowIdentity,
    pub(crate) source_turn_index: usize,
    #[allow(private_interfaces)]
    pub(crate) turn: Arc<TurnExecutionRecord>,
    pub(crate) model: Arc<TranscriptRowPresentationModel>,
}

#[allow(dead_code)]
#[derive(Clone, Default)]
pub(crate) struct TranscriptPresentationWindow {
    rows: Vec<TranscriptPresentedRow>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptPresentationPanelState {
    pub(crate) inspected_row_count: usize,
    pub(crate) active_nested_code_panel_ids: HashSet<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptPresentationMutation {
    Unchanged,
    Replaced { index: usize },
    Inserted { index: usize, count: usize },
    Removed { index: usize, count: usize },
}

impl TranscriptPresentationMutation {
    pub(crate) fn row_index(self) -> Option<usize> {
        match self {
            Self::Replaced { index } | Self::Inserted { index, count: 1 } => Some(index),
            Self::Unchanged | Self::Inserted { .. } | Self::Removed { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptPresentationRetainedCounts {
    pub(crate) rows: usize,
    pub(crate) items: usize,
    pub(crate) text_bytes: usize,
    pub(crate) identity_bytes: usize,
    pub(crate) anchor_bytes: usize,
    pub(crate) derived_bytes: usize,
    pub(crate) markdown_source_bytes: usize,
    pub(crate) media_descriptors: usize,
}

impl TranscriptPresentationRetainedCounts {
    fn with_anchor_bytes(mut self, anchor_bytes: usize) -> Self {
        self.anchor_bytes = anchor_bytes;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptActivityCaret {
    pub(crate) row_index: usize,
    pub(crate) row_identity: TranscriptRowIdentity,
}

impl TranscriptPresentationState {
    pub(crate) fn clear(&mut self) {
        self.rows.clear();
        self.markdown_key_row_identities.clear();
        self.render_metrics = TranscriptRenderMetrics::default();
        self.latest_user_prompt_anchor = None;
        self.next_ephemeral_row_id = 0;
    }

    pub(crate) fn replace_from_turns(&mut self, turns: &[Arc<TurnExecutionRecord>]) {
        self.clear();
        let rows = turns
            .iter()
            .cloned()
            .enumerate()
            .filter_map(|(source_turn_index, turn)| self.row_for_turn(source_turn_index, turn))
            .collect::<Vec<_>>();
        self.render_metrics = render_metrics_for_rows(&rows);
        self.rows = rows;
        self.rebuild_markdown_key_ownership();
        self.rebuild_latest_user_prompt_anchor();
    }

    pub(crate) fn prepend_from_turns(&mut self, turns: &[Arc<TurnExecutionRecord>]) -> usize {
        if turns.is_empty() {
            return 0;
        }

        for row in &mut self.rows {
            row.source_turn_index += turns.len();
            row.model = Arc::new(TranscriptRowPresentationModel::derive(
                row.source_turn_index,
                row.turn.as_ref(),
            ));
            row.metrics = TranscriptPresentationRowMetrics::from_model(row.model.as_ref());
        }

        let mut rows = turns
            .iter()
            .cloned()
            .enumerate()
            .filter_map(|(source_turn_index, turn)| self.row_for_turn(source_turn_index, turn))
            .collect::<Vec<_>>();
        let added = rows.len();
        self.add_render_metrics(render_metrics_for_rows(&rows));
        rows.append(&mut self.rows);
        self.rows = rows;
        self.rebuild_markdown_key_ownership();
        self.rebuild_latest_user_prompt_anchor();
        added
    }

    pub(crate) fn append_turn(
        &mut self,
        source_turn_index: usize,
        turn: Arc<TurnExecutionRecord>,
    ) -> Option<usize> {
        let index = self.rows.len();
        let row = self.row_for_turn(source_turn_index, turn)?;
        self.add_row_metrics(row.metrics);
        self.insert_markdown_key_ownership(&row);
        self.rows.push(row);
        self.update_latest_user_prompt_for_replaced_row(index);
        Some(index)
    }

    pub(crate) fn replace_turn(
        &mut self,
        source_turn_index: usize,
        turn: Arc<TurnExecutionRecord>,
    ) -> TranscriptPresentationMutation {
        let row_index = self.presentation_index_for_source_turn(source_turn_index);
        let projected = project_parent_narrative_turn(turn.as_ref()).map(Arc::new);

        match (row_index, projected) {
            (Some(index), Some(turn)) => {
                let old_metrics = self.rows[index].metrics;
                let model = Arc::new(TranscriptRowPresentationModel::derive(
                    source_turn_index,
                    turn.as_ref(),
                ));
                let new_metrics = TranscriptPresentationRowMetrics::from_model(model.as_ref());
                self.subtract_row_metrics(old_metrics);
                self.add_row_metrics(new_metrics);
                let identity = self.rows[index].identity.clone();
                self.markdown_key_row_identities
                    .retain(|_, row_identity| row_identity != &identity);
                let row = &mut self.rows[index];
                row.turn = turn;
                row.metrics = new_metrics;
                row.model = model;
                let row = self.rows[index].clone();
                self.insert_markdown_key_ownership(&row);
                self.update_latest_user_prompt_for_replaced_row(index);
                TranscriptPresentationMutation::Replaced { index }
            }
            (Some(index), None) => {
                let row = self.rows.remove(index);
                self.subtract_row_metrics(row.metrics);
                self.remove_markdown_key_ownership_for_identity(&row.identity);
                self.rebuild_latest_user_prompt_anchor();
                TranscriptPresentationMutation::Removed { index, count: 1 }
            }
            (None, Some(turn)) => {
                let index = self.insertion_index_for_source_turn(source_turn_index);
                let row = self.presentation_row_for_projected_turn(source_turn_index, turn);
                self.add_row_metrics(row.metrics);
                self.insert_markdown_key_ownership(&row);
                self.rows.insert(index, row);
                self.rebuild_latest_user_prompt_anchor();
                TranscriptPresentationMutation::Inserted { index, count: 1 }
            }
            (None, None) => TranscriptPresentationMutation::Unchanged,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub(crate) fn retained_counts(&self) -> TranscriptPresentationRetainedCounts {
        self.rows
            .iter()
            .fold(
                TranscriptPresentationRetainedCounts {
                    rows: self.rows.len(),
                    ..TranscriptPresentationRetainedCounts::default()
                },
                |mut counts, row| {
                    let derived = row.model.estimated_derived_bytes();
                    counts.items = counts.items.saturating_add(row.metrics.item_count);
                    counts.text_bytes = counts.text_bytes.saturating_add(row.metrics.text_chars);
                    counts.identity_bytes = counts
                        .identity_bytes
                        .saturating_add(row.identity.as_str().len());
                    counts.derived_bytes = counts.derived_bytes.saturating_add(derived.total());
                    counts.markdown_source_bytes = counts.markdown_source_bytes.saturating_add(
                        row.model
                            .markdown_sources()
                            .iter()
                            .map(|source| source.source_bytes)
                            .sum::<usize>(),
                    );
                    counts.media_descriptors = counts
                        .media_descriptors
                        .saturating_add(row.model.media_descriptors().len());
                    counts
                },
            )
            .with_anchor_bytes(
                self.latest_user_prompt_anchor
                    .as_ref()
                    .map_or(0, |(_, _, value)| value.len()),
            )
    }

    pub(crate) fn derived_byte_estimates_by_turn_id_for_range(
        &self,
        range: &Range<usize>,
    ) -> Vec<(String, usize)> {
        let start = range.start.min(self.rows.len());
        let end = range.end.min(self.rows.len()).max(start);
        let mut estimates = BTreeMap::new();
        for row in &self.rows[start..end] {
            let Some(turn_id) = row.model.source_turn_identity().turn_id.as_ref() else {
                continue;
            };
            let bytes = row.model.estimated_derived_bytes().total();
            estimates
                .entry(turn_id.clone())
                .and_modify(|current: &mut usize| {
                    *current = current.saturating_add(bytes);
                })
                .or_insert(bytes);
        }
        estimates.into_iter().collect()
    }

    #[allow(dead_code)]
    pub(crate) fn row_identity(&self, index: usize) -> Option<&TranscriptRowIdentity> {
        self.rows.get(index).map(|row| &row.identity)
    }

    pub(crate) fn row_index_for_identity(&self, identity: &str) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row.identity.as_str() == identity)
    }

    pub(crate) fn row_index_for_markdown_key(&self, markdown_key: &str) -> Option<usize> {
        let identity = self.markdown_key_row_identities.get(markdown_key)?;
        self.row_index_for_identity(identity.as_str())
    }

    pub(crate) fn turn_at(&self, index: usize) -> Option<TranscriptPresentedRow> {
        self.rows.get(index).map(|row| row.presented_row_at(index))
    }

    pub(crate) fn measurement_key_for_row(
        &self,
        index: usize,
        transcript_width: Pixels,
        theme_revision: u64,
        display_state: TranscriptRowMeasurementDisplayState,
    ) -> Option<TranscriptRowMeasurementKey> {
        let row = self.rows.get(index)?;
        Some(TranscriptRowMeasurementKey::new(
            row.identity.clone(),
            row.model.revision(),
            transcript_width,
            theme_revision,
            display_state,
        ))
    }

    #[allow(dead_code)]
    pub(crate) fn source_turn_index_at(&self, index: usize) -> Option<usize> {
        self.rows.get(index).map(|row| row.source_turn_index)
    }

    pub(crate) fn source_range_for_presentation_range(&self, range: &Range<usize>) -> Range<usize> {
        if self.rows.is_empty() {
            return 0..0;
        }

        let start = range.start.min(self.rows.len());
        let end = range.end.min(self.rows.len()).max(start);
        if start == end {
            let source = if start >= self.rows.len() {
                self.rows
                    .last()
                    .map(|row| row.source_turn_index.saturating_add(1))
                    .unwrap_or_default()
            } else {
                self.rows[start].source_turn_index
            };
            return source..source;
        }

        let source_start = self.rows[start].source_turn_index;
        let source_end = self.rows[end - 1].source_turn_index.saturating_add(1);
        source_start..source_end
    }

    pub(crate) fn presentation_range_for_source_range(&self, range: &Range<usize>) -> Range<usize> {
        let start = self
            .rows
            .partition_point(|row| row.source_turn_index < range.start);
        let end = self
            .rows
            .partition_point(|row| row.source_turn_index < range.end);
        start..end.max(start)
    }

    pub(crate) fn presentation_index_for_source_turn(
        &self,
        source_turn_index: usize,
    ) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row.source_turn_index == source_turn_index)
    }

    #[allow(dead_code)]
    pub(crate) fn window_for_range(&self, range: Range<usize>) -> TranscriptPresentationWindow {
        let start = range.start.min(self.rows.len());
        let end = range.end.min(self.rows.len()).max(start);
        TranscriptPresentationWindow {
            rows: self.rows[start..end]
                .iter()
                .enumerate()
                .map(|(offset, row)| row.presented_row_at(start + offset))
                .collect(),
        }
    }

    #[cfg(test)]
    pub(crate) fn latest_user_prompt_anchor(&self) -> Option<(usize, usize, String)> {
        self.latest_user_prompt_anchor.clone()
    }

    pub(crate) fn panel_state_for_range(
        &self,
        range: Range<usize>,
    ) -> TranscriptPresentationPanelState {
        let start = range.start.min(self.rows.len());
        let end = range.end.min(self.rows.len()).max(start);
        TranscriptPresentationPanelState {
            inspected_row_count: end.saturating_sub(start),
            ..TranscriptPresentationPanelState::default()
        }
    }

    pub(crate) fn render_metrics(&self) -> TranscriptRenderMetrics {
        let mut metrics = self.render_metrics;
        metrics.total_turns = self.rows.len();
        metrics
    }

    pub(crate) fn activity_caret_for_source_turn(
        &self,
        source_turn_index: Option<usize>,
    ) -> Option<TranscriptActivityCaret> {
        let source_turn_index = source_turn_index?;
        let row_index = self.presentation_index_for_source_turn(source_turn_index)?;
        let row = self.rows.get(row_index)?;
        Some(TranscriptActivityCaret {
            row_index,
            row_identity: row.identity.clone(),
        })
    }

    fn insertion_index_for_source_turn(&self, source_turn_index: usize) -> usize {
        self.rows
            .partition_point(|row| row.source_turn_index < source_turn_index)
    }

    fn row_for_turn(
        &mut self,
        source_turn_index: usize,
        turn: Arc<TurnExecutionRecord>,
    ) -> Option<TranscriptPresentationRow> {
        let projected = project_parent_narrative_turn(turn.as_ref())?;
        Some(self.presentation_row_for_projected_turn(source_turn_index, Arc::new(projected)))
    }

    fn presentation_row_for_projected_turn(
        &mut self,
        source_turn_index: usize,
        turn: Arc<TurnExecutionRecord>,
    ) -> TranscriptPresentationRow {
        let identity = self.identity_for_turn(turn.as_ref());
        let model = Arc::new(TranscriptRowPresentationModel::derive(
            source_turn_index,
            turn.as_ref(),
        ));
        TranscriptPresentationRow {
            identity,
            source_turn_index,
            metrics: TranscriptPresentationRowMetrics::from_model(model.as_ref()),
            model,
            turn,
        }
    }

    fn add_row_metrics(&mut self, metrics: TranscriptPresentationRowMetrics) {
        self.render_metrics.total_item_count = self
            .render_metrics
            .total_item_count
            .saturating_add(metrics.item_count);
        self.render_metrics.total_text_chars = self
            .render_metrics
            .total_text_chars
            .saturating_add(metrics.text_chars);
    }

    fn subtract_row_metrics(&mut self, metrics: TranscriptPresentationRowMetrics) {
        self.render_metrics.total_item_count = self
            .render_metrics
            .total_item_count
            .saturating_sub(metrics.item_count);
        self.render_metrics.total_text_chars = self
            .render_metrics
            .total_text_chars
            .saturating_sub(metrics.text_chars);
    }

    fn add_render_metrics(&mut self, metrics: TranscriptRenderMetrics) {
        self.render_metrics.total_item_count = self
            .render_metrics
            .total_item_count
            .saturating_add(metrics.total_item_count);
        self.render_metrics.total_text_chars = self
            .render_metrics
            .total_text_chars
            .saturating_add(metrics.total_text_chars);
    }

    fn insert_markdown_key_ownership(&mut self, row: &TranscriptPresentationRow) {
        for source in row.model.markdown_sources() {
            self.markdown_key_row_identities
                .insert(source.key.clone(), row.identity.clone());
        }
    }

    fn remove_markdown_key_ownership_for_identity(&mut self, identity: &TranscriptRowIdentity) {
        self.markdown_key_row_identities
            .retain(|_, row_identity| row_identity != identity);
    }

    fn rebuild_markdown_key_ownership(&mut self) {
        self.markdown_key_row_identities.clear();
        let ownership = self
            .rows
            .iter()
            .flat_map(|row| {
                row.model
                    .markdown_sources()
                    .iter()
                    .map(|source| (source.key.clone(), row.identity.clone()))
                    .collect::<Vec<_>>()
            })
            .collect::<HashMap<_, _>>();
        self.markdown_key_row_identities = ownership;
    }

    fn identity_for_turn(&mut self, turn: &TurnExecutionRecord) -> TranscriptRowIdentity {
        stable_row_identity(turn).unwrap_or_else(|| {
            let id = self.next_ephemeral_row_id;
            self.next_ephemeral_row_id += 1;
            TranscriptRowIdentity(format!("ephemeral-turn:{id}"))
        })
    }

    fn update_latest_user_prompt_for_replaced_row(&mut self, index: usize) {
        let Some(row) = self.rows.get(index) else {
            return;
        };
        let prompt = user_prompt_anchor_text(row.turn.as_ref());
        match (&mut self.latest_user_prompt_anchor, prompt) {
            (
                Some((latest_index, latest_fragment_index, latest_prompt)),
                Some((fragment_index, prompt)),
            ) if index >= *latest_index => {
                *latest_index = index;
                *latest_fragment_index = fragment_index;
                *latest_prompt = prompt;
            }
            (None, Some((fragment_index, prompt))) => {
                self.latest_user_prompt_anchor = Some((index, fragment_index, prompt));
            }
            (Some((latest_index, _, _)), None) if index == *latest_index => {
                self.rebuild_latest_user_prompt_anchor();
            }
            _ => {}
        }
    }

    fn rebuild_latest_user_prompt_anchor(&mut self) {
        self.latest_user_prompt_anchor = latest_user_prompt_anchor_in_rows(&self.rows);
    }
}

impl TranscriptPresentationWindow {
    pub(crate) fn from_turn_records(
        turns: &[Arc<TurnExecutionRecord>],
        source_start: usize,
    ) -> Self {
        let mut state = TranscriptPresentationState::default();
        let rows = turns
            .iter()
            .cloned()
            .enumerate()
            .filter_map(|(offset, turn)| {
                let source_turn_index = source_start.saturating_add(offset);
                state
                    .row_for_turn(source_turn_index, turn)
                    .map(|row| row.presented_row_at(offset))
            })
            .collect();
        Self { rows }
    }

    #[allow(dead_code)]
    pub(crate) fn rows(&self) -> &[TranscriptPresentedRow] {
        &self.rows
    }

    #[allow(dead_code)]
    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }

    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

impl TranscriptPresentationRow {
    fn presented_row_at(&self, index: usize) -> TranscriptPresentedRow {
        TranscriptPresentedRow {
            index,
            identity: self.identity.clone(),
            source_turn_index: self.model.source_turn_identity().source_turn_index,
            turn: self.turn.clone(),
            model: self.model.clone(),
        }
    }
}

fn render_metrics_for_rows(rows: &[TranscriptPresentationRow]) -> TranscriptRenderMetrics {
    rows.iter().fold(
        TranscriptRenderMetrics {
            total_turns: rows.len(),
            ..TranscriptRenderMetrics::default()
        },
        |mut metrics, row| {
            metrics.total_item_count = metrics
                .total_item_count
                .saturating_add(row.metrics.item_count);
            metrics.total_text_chars = metrics
                .total_text_chars
                .saturating_add(row.metrics.text_chars);
            metrics
        },
    )
}
