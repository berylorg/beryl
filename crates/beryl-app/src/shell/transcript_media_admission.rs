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
    last_summary: TranscriptMediaAdmissionSummary,
}

#[derive(Clone)]
pub(crate) struct TranscriptMediaAdmissionRequest {
    target: TranscriptMediaAdmissionTarget,
    rows: Vec<TranscriptPresentedRow>,
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
    pub(crate) rows_budget_exhausted: bool,
    pub(crate) media_budget_exhausted: bool,
    pub(crate) time_budget_exhausted: bool,
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

    pub(crate) fn from_turn_records(
        turns: &[std::sync::Arc<super::execution_detail::TurnExecutionRecord>],
        source_start: usize,
    ) -> Self {
        let rows = TranscriptPresentationWindow::from_turn_records(turns, source_start)
            .rows()
            .to_vec();
        let last_summary = base_summary_for_rows(rows.as_slice());
        Self { rows, last_summary }
    }

    pub(crate) fn admission_request(
        &self,
        target: TranscriptMediaAdmissionTarget,
    ) -> TranscriptMediaAdmissionRequest {
        TranscriptMediaAdmissionRequest {
            target,
            rows: self.rows.clone(),
        }
    }

    pub(crate) fn rows(&self) -> &[TranscriptPresentedRow] {
        &self.rows
    }

    pub(crate) fn last_summary(&self) -> TranscriptMediaAdmissionSummary {
        self.last_summary
    }

    pub(crate) fn note_summary(&mut self, summary: TranscriptMediaAdmissionSummary) {
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

    pub(crate) fn into_rows(self) -> Vec<TranscriptPresentedRow> {
        self.rows
    }
}

impl TranscriptMediaAdmissionSummary {
    pub(crate) fn is_completed_media_settled(self) -> bool {
        self.pending_completed_media_items == 0
            && !self.rows_budget_exhausted
            && !self.media_budget_exhausted
            && !self.time_budget_exhausted
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
        .count()
}

impl ConversationSurfaceState {
    pub(super) fn staged_transcript_media_admission_request(
        &self,
    ) -> Option<TranscriptMediaAdmissionRequest> {
        self.staged_selected_thread_activation
            .as_ref()
            .map(|staged| staged.media_admission_request())
            .or_else(|| {
                self.staged_transcript_residency_page
                    .as_ref()
                    .map(|staged| staged.media_admission_request())
            })
    }

    pub(super) fn note_staged_transcript_media_admission_summary(
        &mut self,
        target: &TranscriptMediaAdmissionTarget,
        summary: TranscriptMediaAdmissionSummary,
    ) -> bool {
        if let Some(staged) = self.staged_selected_thread_activation.as_mut()
            && staged.media_admission_target_matches(target)
        {
            staged.note_media_admission_summary(summary);
            return true;
        }

        if let Some(staged) = self.staged_transcript_residency_page.as_mut()
            && staged.media_admission_target_matches(target)
        {
            staged.note_media_admission_summary(summary);
            return true;
        }

        false
    }
}
