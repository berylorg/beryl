use std::{collections::HashSet, ops::Range, sync::Arc};

use gpui::Pixels;

use super::{
    execution_detail::{TranscriptRenderMetrics, TurnExecutionRecord},
    transcript_projection::project_parent_narrative_turn,
};

#[path = "transcript_presentation/identity.rs"]
mod identity;
#[path = "transcript_presentation/metrics.rs"]
mod metrics;
#[allow(dead_code)]
#[path = "transcript_presentation/range.rs"]
mod range;

use identity::{latest_user_prompt_anchor_in_rows, stable_row_identity, user_prompt_anchor_text};
use metrics::TranscriptPresentationRowMetrics;

#[allow(unused_imports)]
pub(crate) use range::{
    TRANSCRIPT_INITIAL_PRESENTATION_ROWS, TRANSCRIPT_MAX_PRESENTATION_ROWS,
    transcript_frame_preload_range, transcript_frame_presentation_range,
};

#[derive(Clone, Default)]
pub(crate) struct TranscriptPresentationState {
    rows: Vec<TranscriptPresentationRow>,
    render_metrics: TranscriptRenderMetrics,
    latest_user_prompt_anchor: Option<(usize, usize, String)>,
    next_ephemeral_row_id: u64,
}

#[derive(Clone)]
struct TranscriptPresentationRow {
    identity: TranscriptRowIdentity,
    source_turn_index: usize,
    turn: Arc<TurnExecutionRecord>,
    placeholder_height: Option<Pixels>,
    metrics: TranscriptPresentationRowMetrics,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptRowIdentity(String);

impl TranscriptRowIdentity {
    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone)]
pub(crate) struct TranscriptPresentedRow {
    pub(crate) index: usize,
    pub(crate) identity: TranscriptRowIdentity,
    pub(crate) source_turn_index: usize,
    pub(crate) turn: Arc<TurnExecutionRecord>,
    pub(crate) placeholder_height: Option<Pixels>,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptPresentationRetainedCounts {
    pub(crate) rows: usize,
    pub(crate) items: usize,
    pub(crate) text_bytes: usize,
    pub(crate) identity_bytes: usize,
    pub(crate) anchor_bytes: usize,
    pub(crate) placeholder_rows: usize,
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
        self.rebuild_latest_user_prompt_anchor();
    }

    pub(crate) fn prepend_from_turns(&mut self, turns: &[Arc<TurnExecutionRecord>]) -> usize {
        if turns.is_empty() {
            return 0;
        }

        for row in &mut self.rows {
            row.source_turn_index += turns.len();
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
        self.rows.push(row);
        self.update_latest_user_prompt_for_replaced_row(index);
        Some(index)
    }

    pub(crate) fn replace_turn(
        &mut self,
        source_turn_index: usize,
        turn: Arc<TurnExecutionRecord>,
    ) -> Option<usize> {
        let row_index = self.presentation_index_for_source_turn(source_turn_index);
        let projected = project_parent_narrative_turn(turn.as_ref()).map(Arc::new);

        match (row_index, projected) {
            (Some(index), Some(turn)) => {
                let old_metrics = self.rows[index].metrics;
                let new_metrics = TranscriptPresentationRowMetrics::from_turn(turn.as_ref());
                self.subtract_row_metrics(old_metrics);
                self.add_row_metrics(new_metrics);
                let row = &mut self.rows[index];
                row.turn = turn;
                row.placeholder_height = None;
                row.metrics = new_metrics;
                self.update_latest_user_prompt_for_replaced_row(index);
                Some(index)
            }
            (Some(index), None) => {
                let row = self.rows.remove(index);
                self.subtract_row_metrics(row.metrics);
                self.rebuild_latest_user_prompt_anchor();
                None
            }
            (None, Some(turn)) => {
                let index = self.insertion_index_for_source_turn(source_turn_index);
                let row = self.presentation_row_for_projected_turn(source_turn_index, turn);
                self.add_row_metrics(row.metrics);
                self.rows.insert(index, row);
                self.rebuild_latest_user_prompt_anchor();
                Some(index)
            }
            (None, None) => None,
        }
    }

    pub(crate) fn replace_turn_with_placeholder(
        &mut self,
        source_turn_index: usize,
        turn: Arc<TurnExecutionRecord>,
        placeholder_height: Option<Pixels>,
    ) -> Option<usize> {
        let index = self.presentation_index_for_source_turn(source_turn_index)?;
        let Some(projected) = project_parent_narrative_turn(turn.as_ref()) else {
            let row = self.rows.remove(index);
            self.subtract_row_metrics(row.metrics);
            self.rebuild_latest_user_prompt_anchor();
            return None;
        };

        let old_metrics = self.rows[index].metrics;
        let new_metrics = TranscriptPresentationRowMetrics::from_turn(&projected);
        self.subtract_row_metrics(old_metrics);
        self.add_row_metrics(new_metrics);
        let row = &mut self.rows[index];
        row.turn = Arc::new(projected);
        row.placeholder_height = placeholder_height;
        row.metrics = new_metrics;
        self.update_latest_user_prompt_for_replaced_row(index);
        Some(index)
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
                    counts.items = counts.items.saturating_add(row.metrics.item_count);
                    counts.text_bytes = counts.text_bytes.saturating_add(row.metrics.text_chars);
                    counts.identity_bytes = counts
                        .identity_bytes
                        .saturating_add(row.identity.as_str().len());
                    counts.placeholder_rows = counts
                        .placeholder_rows
                        .saturating_add(usize::from(row.placeholder_height.is_some()));
                    counts
                },
            )
            .with_anchor_bytes(
                self.latest_user_prompt_anchor
                    .as_ref()
                    .map_or(0, |(_, _, value)| value.len()),
            )
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

    pub(crate) fn turn_at(&self, index: usize) -> Option<TranscriptPresentedRow> {
        self.rows.get(index).map(|row| row.presented_row_at(index))
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
        TranscriptPresentationRow {
            identity,
            source_turn_index,
            metrics: TranscriptPresentationRowMetrics::from_turn(turn.as_ref()),
            turn,
            placeholder_height: None,
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
            source_turn_index: self.source_turn_index,
            turn: self.turn.clone(),
            placeholder_height: self.placeholder_height,
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
