#![allow(dead_code)]

use beryl_backend::{ThreadInfo, TurnInfo};

use super::{
    ConversationSurfaceState,
    execution_detail::{ExecutionDetailState, TranscriptImagePathResolver},
    transcript_history::TranscriptHistoryPageRequest,
    transcript_presentation::{
        TranscriptPresentationWindow, TranscriptPresentedRow, TranscriptRowMediaDescriptorKind,
    },
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TranscriptMediaAdmissionTarget {
    SelectedThread {
        thread_id: String,
    },
    ResidencyPage {
        thread_id: String,
        request: TranscriptHistoryPageRequest,
        cancellation_generation: u64,
    },
}

#[derive(Clone, Default)]
pub(crate) struct TranscriptMediaAdmissionWindow {
    rows: Vec<TranscriptPresentedRow>,
    next_scan_row_index: usize,
    next_scan_item_index: usize,
    prefix_recheck_required: bool,
    full_prefix_recheck_in_progress: bool,
    last_summary: TranscriptMediaAdmissionSummary,
}

#[derive(Clone)]
pub(crate) struct TranscriptMediaAdmissionRequest {
    target: TranscriptMediaAdmissionTarget,
    rows: Vec<TranscriptPresentedRow>,
    total_rows: usize,
    scan_start_row_index: usize,
    scan_start_item_index: usize,
    prefix_recheck_required: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptMediaAdmissionSummary {
    pub(crate) row_count: usize,
    pub(crate) completed_media_items: usize,
    pub(crate) ready_completed_media_items: usize,
    pub(crate) pending_completed_media_items: usize,
    pub(crate) terminal_fallback_completed_media_items: usize,
    pub(crate) scheduled_loads: usize,
    pub(crate) source_backed_preloads: usize,
    pub(crate) requested_upload_bytes: usize,
    pub(crate) scan_start_row_index: usize,
    pub(crate) scan_start_item_index: usize,
    pub(crate) scanned_rows: usize,
    pub(crate) scanned_media_items: usize,
    pub(crate) deferred_completed_media_items: usize,
    pub(crate) prefix_recheck_required: bool,
    pub(crate) waiting_on_prefix_media: bool,
    pub(crate) rows_budget_exhausted: bool,
    pub(crate) media_budget_exhausted: bool,
    pub(crate) time_budget_exhausted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceBackedUploadAdmissionDecision {
    ReadyToRequest,
    RetryCurrent,
    TerminalFallback,
}

impl TranscriptMediaAdmissionWindow {
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

    #[allow(private_interfaces)]
    pub(crate) fn from_turn_records(
        turns: &[std::sync::Arc<super::execution_detail::TurnExecutionRecord>],
        source_start: usize,
    ) -> Self {
        let rows = TranscriptPresentationWindow::from_turn_records(turns, source_start)
            .rows()
            .to_vec();
        let last_summary = base_summary_for_rows(rows.as_slice());
        Self {
            rows,
            next_scan_row_index: 0,
            next_scan_item_index: 0,
            prefix_recheck_required: false,
            full_prefix_recheck_in_progress: false,
            last_summary,
        }
    }

    pub(crate) fn admission_request(
        &self,
        target: TranscriptMediaAdmissionTarget,
    ) -> TranscriptMediaAdmissionRequest {
        let scan_start_row_index = self.next_scan_row_index.min(self.rows.len());
        let scan_start_item_index = if scan_start_row_index == self.next_scan_row_index {
            self.next_scan_item_index
        } else {
            0
        };
        TranscriptMediaAdmissionRequest {
            target,
            rows: self.rows[scan_start_row_index..].to_vec(),
            total_rows: self.rows.len(),
            scan_start_row_index,
            scan_start_item_index,
            prefix_recheck_required: self.prefix_recheck_required,
        }
    }

    pub(crate) fn rows(&self) -> &[TranscriptPresentedRow] {
        &self.rows
    }

    pub(crate) fn last_summary(&self) -> TranscriptMediaAdmissionSummary {
        self.last_summary
    }

    pub(crate) fn note_summary(&mut self, mut summary: TranscriptMediaAdmissionSummary) {
        let scan_end = summary
            .scan_start_row_index
            .saturating_add(summary.scanned_rows)
            .min(self.rows.len());
        let scanned_pending_completed_media_items = summary
            .pending_completed_media_items
            .saturating_sub(summary.deferred_completed_media_items);
        let prefix_recheck_required =
            summary.prefix_recheck_required || scanned_pending_completed_media_items > 0;
        if self.full_prefix_recheck_in_progress && scanned_pending_completed_media_items > 0 {
            summary.waiting_on_prefix_media = true;
            self.next_scan_row_index = 0;
            self.next_scan_item_index = 0;
            self.prefix_recheck_required = false;
            self.full_prefix_recheck_in_progress = true;
        } else if summary.media_budget_exhausted
            && summary.deferred_completed_media_items > 0
            && summary.scan_start_row_index < self.rows.len()
        {
            self.next_scan_row_index = summary
                .scan_start_row_index
                .saturating_add(summary.scanned_rows)
                .min(self.rows.len());
            let current_row_start_item_index = if summary.scanned_rows == 0 {
                summary.scan_start_item_index
            } else {
                0
            };
            self.next_scan_item_index =
                current_row_start_item_index.saturating_add(summary.scanned_media_items);
            self.prefix_recheck_required = prefix_recheck_required;
            self.full_prefix_recheck_in_progress = false;
        } else if (summary.rows_budget_exhausted || summary.time_budget_exhausted)
            && summary.scanned_rows > 0
            && scan_end < self.rows.len()
        {
            self.next_scan_row_index = scan_end;
            self.next_scan_item_index = 0;
            self.prefix_recheck_required = prefix_recheck_required;
            self.full_prefix_recheck_in_progress = false;
        } else if summary.prefix_recheck_required {
            self.next_scan_row_index = 0;
            self.next_scan_item_index = 0;
            self.prefix_recheck_required = false;
            self.full_prefix_recheck_in_progress = true;
        } else {
            self.next_scan_row_index = 0;
            self.next_scan_item_index = 0;
            self.prefix_recheck_required = false;
            self.full_prefix_recheck_in_progress = false;
        }
        self.last_summary = summary;
    }

    pub(crate) fn requires_completed_media_admission(&self) -> bool {
        self.last_summary.completed_media_items > 0
    }

    pub(crate) fn is_settled_for_publication(&self) -> bool {
        self.last_summary.is_completed_media_settled()
    }
}

impl TranscriptMediaAdmissionRequest {
    pub(crate) fn target(&self) -> &TranscriptMediaAdmissionTarget {
        &self.target
    }

    pub(crate) fn rows(&self) -> &[TranscriptPresentedRow] {
        &self.rows
    }

    pub(crate) fn total_rows(&self) -> usize {
        self.total_rows
    }

    pub(crate) fn scan_start_row_index(&self) -> usize {
        self.scan_start_row_index
    }

    pub(crate) fn scan_start_item_index(&self) -> usize {
        self.scan_start_item_index
    }

    pub(crate) fn prefix_recheck_required(&self) -> bool {
        self.prefix_recheck_required
    }

    pub(crate) fn into_rows(self) -> Vec<TranscriptPresentedRow> {
        self.rows
    }
}

impl TranscriptMediaAdmissionSummary {
    pub(crate) fn is_completed_media_settled(self) -> bool {
        self.pending_completed_media_items == 0
            && !self.prefix_recheck_required
            && !self.waiting_on_prefix_media
    }

    pub(crate) fn requires_retry(self) -> bool {
        let budget_exhausted =
            self.rows_budget_exhausted || self.media_budget_exhausted || self.time_budget_exhausted;
        if self.waiting_on_prefix_media {
            return self.pending_completed_media_items > 0 && budget_exhausted;
        }
        self.prefix_recheck_required || !self.is_completed_media_settled() && budget_exhausted
    }

    pub(crate) fn note_ready_item(&mut self) {
        self.ready_completed_media_items = self.ready_completed_media_items.saturating_add(1);
    }

    pub(crate) fn note_pending_item(&mut self) {
        self.pending_completed_media_items = self.pending_completed_media_items.saturating_add(1);
    }

    pub(crate) fn note_terminal_fallback_item(&mut self) {
        self.terminal_fallback_completed_media_items = self
            .terminal_fallback_completed_media_items
            .saturating_add(1);
    }

    pub(crate) fn note_deferred_items(&mut self, count: usize) {
        self.completed_media_items = self.completed_media_items.saturating_add(count);
        self.pending_completed_media_items =
            self.pending_completed_media_items.saturating_add(count);
        self.deferred_completed_media_items =
            self.deferred_completed_media_items.saturating_add(count);
    }
}

pub(crate) fn note_source_backed_upload_admission(
    summary: &mut TranscriptMediaAdmissionSummary,
    requested_upload_bytes: usize,
    max_upload_bytes: usize,
    remaining_upload_bytes: usize,
) -> SourceBackedUploadAdmissionDecision {
    if requested_upload_bytes > max_upload_bytes {
        summary.note_terminal_fallback_item();
        return SourceBackedUploadAdmissionDecision::TerminalFallback;
    }
    if requested_upload_bytes > remaining_upload_bytes {
        summary.media_budget_exhausted = true;
        return SourceBackedUploadAdmissionDecision::RetryCurrent;
    }
    SourceBackedUploadAdmissionDecision::ReadyToRequest
}

fn base_summary_for_rows(rows: &[TranscriptPresentedRow]) -> TranscriptMediaAdmissionSummary {
    let completed_media_items = rows.iter().map(estimated_required_media_item_count).sum();
    TranscriptMediaAdmissionSummary {
        row_count: rows.len(),
        completed_media_items,
        pending_completed_media_items: completed_media_items,
        ..TranscriptMediaAdmissionSummary::default()
    }
}

pub(crate) fn estimated_required_media_item_count(row: &TranscriptPresentedRow) -> usize {
    row.model
        .media_descriptors()
        .iter()
        .filter(|descriptor| {
            matches!(
                descriptor.source_kind,
                TranscriptRowMediaDescriptorKind::MarkdownImageCandidate
                    | TranscriptRowMediaDescriptorKind::NativeGeneratedImage
            )
        })
        .map(|descriptor| descriptor.estimated_items.max(1))
        .sum()
}

impl ConversationSurfaceState {
    pub(super) fn staged_transcript_media_admission_request(
        &self,
    ) -> Option<TranscriptMediaAdmissionRequest> {
        self.staged_transcript_residency_page
            .as_ref()
            .map(|staged| staged.media_admission_request())
    }

    pub(super) fn note_staged_transcript_media_admission_summary(
        &mut self,
        target: &TranscriptMediaAdmissionTarget,
        summary: TranscriptMediaAdmissionSummary,
    ) -> Option<TranscriptMediaAdmissionSummary> {
        if let Some(staged) = self.staged_transcript_residency_page.as_mut()
            && staged.media_admission_target_matches(target)
        {
            return Some(staged.note_media_admission_summary(summary));
        }

        None
    }
}
