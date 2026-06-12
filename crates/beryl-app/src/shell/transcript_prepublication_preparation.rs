#![allow(dead_code)]

use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use beryl_backend::{ThreadInfo, TurnInfo};
use gpui::Pixels;

use super::{
    ConversationSurfaceState,
    execution_detail::{ExecutionDetailState, TranscriptImagePathResolver},
    transcript_media_admission::TranscriptMediaAdmissionTarget,
    transcript_presentation::{
        TranscriptPresentationWindow, TranscriptPresentedRow, TranscriptRowIdentity,
        TranscriptRowPresentationRevision,
    },
};

const DEFAULT_MAX_ROWS_PER_DRAIN: usize = 12;
const DEFAULT_MAX_BLOCK_UNITS_PER_DRAIN: usize = 192;
const DEFAULT_MAX_MEDIA_ITEMS_PER_DRAIN: usize = 32;
const DEFAULT_MAX_PREPARATION_BYTES_PER_DRAIN: usize = 16 * 1024 * 1024;
const DEFAULT_MAX_IN_FLIGHT_PREPARATION_PASSES: usize = 1;
const DEFAULT_MAX_DRAIN_TIME: Duration = Duration::from_millis(2);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptPrepublicationPreparationLayout {
    transcript_width_bits: u32,
    viewport_height_bits: u32,
    media_padded_content_width_bits: u32,
    media_conversation_m_advance_bits: u32,
    window_scale_bits: u32,
    theme_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptPrepublicationPreparedRow {
    row_identity: TranscriptRowIdentity,
    row_revision: TranscriptRowPresentationRevision,
    layout: TranscriptPrepublicationPreparationLayout,
}

#[derive(Clone, Default)]
pub(crate) struct TranscriptPrepublicationPreparationWindow {
    rows: Vec<TranscriptPresentedRow>,
    prepared_rows: HashSet<TranscriptPrepublicationPreparedRow>,
    last_layout: Option<TranscriptPrepublicationPreparationLayout>,
    last_summary: TranscriptPrepublicationPreparationSummary,
}

#[derive(Clone)]
pub(crate) struct TranscriptPrepublicationPreparationRequest {
    target: TranscriptMediaAdmissionTarget,
    layout: TranscriptPrepublicationPreparationLayout,
    rows: Vec<TranscriptPresentedRow>,
    total_rows: usize,
    already_prepared_rows: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptPrepublicationPreparationSummary {
    pub(crate) layout: Option<TranscriptPrepublicationPreparationLayout>,
    pub(crate) row_count: usize,
    pub(crate) prepared_rows: usize,
    pub(crate) pending_rows: usize,
    pub(crate) prepared_block_units: usize,
    pub(crate) prepared_media_items: usize,
    pub(crate) preparation_bytes: usize,
    pub(crate) prepared_row_keys: Vec<TranscriptPrepublicationPreparedRow>,
    pub(crate) rows_budget_exhausted: bool,
    pub(crate) block_budget_exhausted: bool,
    pub(crate) media_budget_exhausted: bool,
    pub(crate) byte_budget_exhausted: bool,
    pub(crate) time_budget_exhausted: bool,
    pub(crate) in_flight_budget_exhausted: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct TranscriptPrepublicationPreparationBudget {
    max_rows_per_drain: usize,
    max_block_units_per_drain: usize,
    max_media_items_per_drain: usize,
    max_preparation_bytes_per_drain: usize,
    max_in_flight_preparation_passes: usize,
    max_drain_time: Duration,
}

#[derive(Default)]
pub(crate) struct TranscriptPrepublicationPreparationDriver {
    budget: TranscriptPrepublicationPreparationBudget,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TranscriptPrepublicationRowCost {
    block_units: usize,
    media_items: usize,
    bytes: usize,
}

struct TranscriptPrepublicationDrainBudget {
    started_at: Instant,
    max_rows: usize,
    max_block_units: usize,
    max_media_items: usize,
    max_bytes: usize,
    max_elapsed: Duration,
    processed_rows: usize,
    processed_block_units: usize,
    processed_media_items: usize,
    processed_bytes: usize,
}

impl TranscriptPrepublicationPreparationLayout {
    pub(crate) fn new(
        transcript_width: Pixels,
        viewport_height: Pixels,
        media_padded_content_width: Pixels,
        media_conversation_m_advance: Pixels,
        window_scale: f32,
        theme_revision: u64,
    ) -> Self {
        Self {
            transcript_width_bits: f32::from(transcript_width.max(gpui::px(0.0))).to_bits(),
            viewport_height_bits: f32::from(viewport_height.max(gpui::px(0.0))).to_bits(),
            media_padded_content_width_bits: f32::from(
                media_padded_content_width.max(gpui::px(0.0)),
            )
            .to_bits(),
            media_conversation_m_advance_bits: f32::from(
                media_conversation_m_advance.max(gpui::px(0.0)),
            )
            .to_bits(),
            window_scale_bits: window_scale.max(0.0).to_bits(),
            theme_revision,
        }
    }
}

impl TranscriptPrepublicationPreparedRow {
    fn new(
        row_identity: TranscriptRowIdentity,
        row_revision: TranscriptRowPresentationRevision,
        layout: TranscriptPrepublicationPreparationLayout,
    ) -> Self {
        Self {
            row_identity,
            row_revision,
            layout,
        }
    }
}

impl TranscriptPrepublicationPreparationWindow {
    pub(crate) fn from_selected_thread_activation(
        thread: &ThreadInfo,
        image_resolver: &TranscriptImagePathResolver,
    ) -> Self {
        let mut details = ExecutionDetailState::default();
        details.load_thread_history_with_image_resolver_and_partial_mode(
            thread,
            image_resolver,
            false,
        );
        Self::from_turn_records(details.turns(), 0)
    }

    pub(crate) fn from_history_page(
        thread_id: &str,
        turns: &[TurnInfo],
        image_resolver: &TranscriptImagePathResolver,
        source_start: usize,
    ) -> Self {
        let mut details = ExecutionDetailState::default();
        let _ = details.prepend_thread_history_page_with_image_resolver_and_partial_mode(
            thread_id,
            turns.to_vec(),
            image_resolver,
            false,
        );
        Self::from_turn_records(details.turns(), source_start)
    }

    pub(crate) fn from_turn_records(
        turns: &[std::sync::Arc<super::execution_detail::TurnExecutionRecord>],
        source_start: usize,
    ) -> Self {
        let rows = TranscriptPresentationWindow::from_turn_records(turns, source_start)
            .rows()
            .to_vec();
        let row_count = rows.len();
        Self {
            rows,
            prepared_rows: HashSet::new(),
            last_layout: None,
            last_summary: TranscriptPrepublicationPreparationSummary {
                row_count,
                pending_rows: row_count,
                ..TranscriptPrepublicationPreparationSummary::default()
            },
        }
    }

    pub(crate) fn preparation_request(
        &self,
        target: TranscriptMediaAdmissionTarget,
        layout: TranscriptPrepublicationPreparationLayout,
    ) -> TranscriptPrepublicationPreparationRequest {
        let rows = self
            .rows
            .iter()
            .filter(|row| !self.row_prepared_for_layout(row, layout))
            .cloned()
            .collect::<Vec<_>>();
        TranscriptPrepublicationPreparationRequest {
            target,
            layout,
            total_rows: self.rows.len(),
            already_prepared_rows: self.prepared_row_count_for_layout(layout),
            rows,
        }
    }

    pub(crate) fn note_summary(&mut self, summary: TranscriptPrepublicationPreparationSummary) {
        self.last_layout = summary.layout.or(self.last_layout);
        self.prepared_rows
            .extend(summary.prepared_row_keys.iter().cloned());
        self.last_summary = summary;
    }

    pub(crate) fn last_summary(&self) -> &TranscriptPrepublicationPreparationSummary {
        &self.last_summary
    }

    pub(crate) fn is_settled_for_publication(&self) -> bool {
        if self.rows.is_empty() {
            return true;
        }
        let Some(layout) = self.last_layout else {
            return false;
        };
        self.prepared_row_count_for_layout(layout) == self.rows.len()
            && self.last_summary.is_settled_for_publication()
    }

    fn row_prepared_for_layout(
        &self,
        row: &TranscriptPresentedRow,
        layout: TranscriptPrepublicationPreparationLayout,
    ) -> bool {
        self.prepared_rows
            .contains(&prepared_key_for_row(row, layout))
    }

    fn prepared_row_count_for_layout(
        &self,
        layout: TranscriptPrepublicationPreparationLayout,
    ) -> usize {
        self.rows
            .iter()
            .filter(|row| self.row_prepared_for_layout(row, layout))
            .count()
    }
}

impl TranscriptPrepublicationPreparationRequest {
    pub(crate) fn target(&self) -> &TranscriptMediaAdmissionTarget {
        &self.target
    }

    pub(crate) fn pending_row_count(&self) -> usize {
        self.rows.len()
    }
}

impl TranscriptPrepublicationPreparationSummary {
    pub(crate) fn is_settled_for_publication(&self) -> bool {
        self.pending_rows == 0
            && !self.rows_budget_exhausted
            && !self.block_budget_exhausted
            && !self.media_budget_exhausted
            && !self.byte_budget_exhausted
            && !self.time_budget_exhausted
            && !self.in_flight_budget_exhausted
    }

    pub(crate) fn requires_retry(&self) -> bool {
        self.pending_rows > 0
            && (self.rows_budget_exhausted
                || self.block_budget_exhausted
                || self.media_budget_exhausted
                || self.byte_budget_exhausted
                || self.time_budget_exhausted
                || self.in_flight_budget_exhausted)
    }
}

impl Default for TranscriptPrepublicationPreparationBudget {
    fn default() -> Self {
        Self {
            max_rows_per_drain: DEFAULT_MAX_ROWS_PER_DRAIN,
            max_block_units_per_drain: DEFAULT_MAX_BLOCK_UNITS_PER_DRAIN,
            max_media_items_per_drain: DEFAULT_MAX_MEDIA_ITEMS_PER_DRAIN,
            max_preparation_bytes_per_drain: DEFAULT_MAX_PREPARATION_BYTES_PER_DRAIN,
            max_in_flight_preparation_passes: DEFAULT_MAX_IN_FLIGHT_PREPARATION_PASSES,
            max_drain_time: DEFAULT_MAX_DRAIN_TIME,
        }
    }
}

impl TranscriptPrepublicationPreparationBudget {
    pub(crate) fn with_test_limits(
        max_rows_per_drain: usize,
        max_block_units_per_drain: usize,
        max_media_items_per_drain: usize,
        max_preparation_bytes_per_drain: usize,
    ) -> Self {
        Self {
            max_rows_per_drain,
            max_block_units_per_drain,
            max_media_items_per_drain,
            max_preparation_bytes_per_drain,
            max_in_flight_preparation_passes: DEFAULT_MAX_IN_FLIGHT_PREPARATION_PASSES,
            max_drain_time: DEFAULT_MAX_DRAIN_TIME,
        }
    }
}

impl TranscriptPrepublicationPreparationDriver {
    pub(crate) fn clear(&mut self) {}

    pub(crate) fn with_budget_for_test(budget: TranscriptPrepublicationPreparationBudget) -> Self {
        Self { budget }
    }

    pub(crate) fn drain_pending(
        &mut self,
        request: TranscriptPrepublicationPreparationRequest,
    ) -> TranscriptPrepublicationPreparationDrain {
        let mut summary = TranscriptPrepublicationPreparationSummary {
            layout: Some(request.layout),
            row_count: request.total_rows,
            prepared_rows: request.already_prepared_rows,
            pending_rows: request
                .total_rows
                .saturating_sub(request.already_prepared_rows),
            ..TranscriptPrepublicationPreparationSummary::default()
        };
        if request.rows.is_empty() {
            summary.pending_rows = 0;
            return TranscriptPrepublicationPreparationDrain {
                target: request.target,
                summary,
            };
        }
        if self.budget.max_in_flight_preparation_passes == 0 {
            summary.in_flight_budget_exhausted = true;
            return TranscriptPrepublicationPreparationDrain {
                target: request.target,
                summary,
            };
        }

        let mut budget = TranscriptPrepublicationDrainBudget::new(self.budget);
        for (row_index, row) in request.rows.iter().enumerate() {
            let cost = preparation_cost_for_row(row);
            if !budget.can_start_row(cost, &mut summary) {
                summary.pending_rows = request.rows.len().saturating_sub(row_index);
                break;
            }

            budget.note_row_processed(cost);
            summary.prepared_block_units = summary
                .prepared_block_units
                .saturating_add(cost.block_units);
            summary.prepared_media_items = summary
                .prepared_media_items
                .saturating_add(cost.media_items);
            summary.preparation_bytes = summary.preparation_bytes.saturating_add(cost.bytes);
            summary
                .prepared_row_keys
                .push(prepared_key_for_row(row, request.layout));
            summary.prepared_rows = summary.prepared_rows.saturating_add(1);
            summary.pending_rows = request.total_rows.saturating_sub(summary.prepared_rows);

            if row_index + 1 < request.rows.len()
                && budget.processed_rows > 0
                && budget.started_at.elapsed() >= budget.max_elapsed
            {
                summary.time_budget_exhausted = true;
                summary.pending_rows = request.rows.len().saturating_sub(row_index + 1);
                break;
            }
        }

        TranscriptPrepublicationPreparationDrain {
            target: request.target,
            summary,
        }
    }
}

pub(crate) struct TranscriptPrepublicationPreparationDrain {
    pub(crate) target: TranscriptMediaAdmissionTarget,
    pub(crate) summary: TranscriptPrepublicationPreparationSummary,
}

impl TranscriptPrepublicationDrainBudget {
    fn new(budget: TranscriptPrepublicationPreparationBudget) -> Self {
        Self {
            started_at: Instant::now(),
            max_rows: budget.max_rows_per_drain,
            max_block_units: budget.max_block_units_per_drain,
            max_media_items: budget.max_media_items_per_drain,
            max_bytes: budget.max_preparation_bytes_per_drain,
            max_elapsed: budget.max_drain_time,
            processed_rows: 0,
            processed_block_units: 0,
            processed_media_items: 0,
            processed_bytes: 0,
        }
    }

    fn can_start_row(
        &self,
        cost: TranscriptPrepublicationRowCost,
        summary: &mut TranscriptPrepublicationPreparationSummary,
    ) -> bool {
        if self.processed_rows == 0 {
            return true;
        }
        if self.processed_rows >= self.max_rows {
            summary.rows_budget_exhausted = true;
            return false;
        }
        if self.processed_block_units.saturating_add(cost.block_units) > self.max_block_units {
            summary.block_budget_exhausted = true;
            return false;
        }
        if self.processed_media_items.saturating_add(cost.media_items) > self.max_media_items {
            summary.media_budget_exhausted = true;
            return false;
        }
        if self.processed_bytes.saturating_add(cost.bytes) > self.max_bytes {
            summary.byte_budget_exhausted = true;
            return false;
        }
        if self.started_at.elapsed() >= self.max_elapsed {
            summary.time_budget_exhausted = true;
            return false;
        }
        true
    }

    fn note_row_processed(&mut self, cost: TranscriptPrepublicationRowCost) {
        self.processed_rows = self.processed_rows.saturating_add(1);
        self.processed_block_units = self.processed_block_units.saturating_add(cost.block_units);
        self.processed_media_items = self.processed_media_items.saturating_add(cost.media_items);
        self.processed_bytes = self.processed_bytes.saturating_add(cost.bytes);
    }
}

fn prepared_key_for_row(
    row: &TranscriptPresentedRow,
    layout: TranscriptPrepublicationPreparationLayout,
) -> TranscriptPrepublicationPreparedRow {
    TranscriptPrepublicationPreparedRow::new(row.identity.clone(), row.model.revision(), layout)
}

fn preparation_cost_for_row(row: &TranscriptPresentedRow) -> TranscriptPrepublicationRowCost {
    let block_units = row
        .model
        .block_presentation()
        .units()
        .iter()
        .map(|unit| unit.estimated_render_blocks.max(1))
        .sum::<usize>()
        .max(1);
    let media_items = row
        .model
        .media_descriptors()
        .iter()
        .map(|descriptor| descriptor.estimated_items.max(1))
        .sum::<usize>();
    let bytes = row.model.estimated_derived_bytes().total();
    TranscriptPrepublicationRowCost {
        block_units,
        media_items,
        bytes,
    }
}

impl ConversationSurfaceState {
    pub(super) fn staged_transcript_prepublication_preparation_request(
        &self,
        layout: TranscriptPrepublicationPreparationLayout,
    ) -> Option<TranscriptPrepublicationPreparationRequest> {
        self.staged_transcript_residency_page
            .as_ref()
            .map(|staged| staged.prepublication_preparation_request(layout))
    }

    pub(super) fn note_staged_transcript_prepublication_preparation_summary(
        &mut self,
        target: &TranscriptMediaAdmissionTarget,
        summary: TranscriptPrepublicationPreparationSummary,
    ) -> Option<TranscriptPrepublicationPreparationSummary> {
        if let Some(staged) = self.staged_transcript_residency_page.as_mut()
            && staged.prepublication_preparation_target_matches(target)
        {
            return Some(staged.note_prepublication_preparation_summary(summary));
        }

        None
    }
}
