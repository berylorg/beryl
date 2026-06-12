use super::transcript_media_admission::{
    TranscriptMediaAdmissionRequest, TranscriptMediaAdmissionSummary,
    TranscriptMediaAdmissionTarget, TranscriptMediaAdmissionWindow,
};
use super::transcript_presentability::TranscriptPresentabilityWindow;
use super::transcript_residency_logging::{
    log_transcript_turns_loaded, log_transcript_turns_unloaded,
};
use super::*;
use tracing::debug;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingTranscriptResidencyPageRequest {
    thread_id: String,
    request: TranscriptHistoryPageRequest,
    presentation_visible_range: std::ops::Range<usize>,
    source_visible_range: std::ops::Range<usize>,
    cancellation_generation: u64,
}

impl PendingTranscriptResidencyPageRequest {
    fn new(
        thread_id: String,
        request: TranscriptHistoryPageRequest,
        presentation_visible_range: std::ops::Range<usize>,
        source_visible_range: std::ops::Range<usize>,
        cancellation_generation: u64,
    ) -> Self {
        Self {
            thread_id,
            request,
            presentation_visible_range,
            source_visible_range,
            cancellation_generation,
        }
    }

    pub(super) fn thread_id(&self) -> &str {
        &self.thread_id
    }

    pub(super) fn request(&self) -> &TranscriptHistoryPageRequest {
        &self.request
    }
}

#[derive(Clone)]
pub(super) struct StagedTranscriptResidencyPageAdmission {
    request: PendingTranscriptResidencyPageRequest,
    page: LoadedTranscriptHistoryPage,
    image_resolver: TranscriptImagePathResolver,
    presentability: TranscriptPresentabilityWindow,
    media_admission: TranscriptMediaAdmissionWindow,
}

impl StagedTranscriptResidencyPageAdmission {
    fn new(
        request: PendingTranscriptResidencyPageRequest,
        page: LoadedTranscriptHistoryPage,
        image_resolver: TranscriptImagePathResolver,
        presentability: TranscriptPresentabilityWindow,
        media_admission: TranscriptMediaAdmissionWindow,
    ) -> Self {
        Self {
            request,
            page,
            image_resolver,
            presentability,
            media_admission,
        }
    }

    pub(super) fn media_admission_request(&self) -> TranscriptMediaAdmissionRequest {
        self.media_admission
            .admission_request(TranscriptMediaAdmissionTarget::ResidencyPage {
                thread_id: self.request.thread_id.clone(),
                request: self.request.request.clone(),
                cancellation_generation: self.request.cancellation_generation,
            })
    }

    pub(super) fn media_admission_target_matches(
        &self,
        target: &TranscriptMediaAdmissionTarget,
    ) -> bool {
        matches!(
            target,
            TranscriptMediaAdmissionTarget::ResidencyPage {
                thread_id,
                request,
                cancellation_generation,
            } if thread_id == &self.request.thread_id
                && request == &self.request.request
                && *cancellation_generation == self.request.cancellation_generation
        )
    }

    pub(super) fn note_media_admission_summary(
        &mut self,
        summary: TranscriptMediaAdmissionSummary,
    ) {
        self.media_admission.note_summary(summary);
    }

    fn is_ready_for_publication(&self) -> bool {
        self.presentability.structural_readiness_settled()
            && self.media_admission.is_settled_for_publication()
    }
}

impl ConversationSurfaceState {
    pub(super) fn begin_loading_thread_history_page(
        &mut self,
        visible_range: &std::ops::Range<usize>,
    ) -> Option<PendingTranscriptResidencyPageRequest> {
        if self.staged_transcript_residency_page.is_some() {
            return None;
        }
        let thread_id = self.selected_thread_id()?.to_string();
        let source_visible_range = self
            .transcript_presentation
            .source_range_for_presentation_range(visible_range);
        let request = self
            .transcript_history_window
            .begin_loading_page_for_visible_range(&source_visible_range)?;
        self.transcript_residency_page_cancellation_generation = self
            .transcript_residency_page_cancellation_generation
            .saturating_add(1);
        self.note_transcript_residency_request_started();
        Some(PendingTranscriptResidencyPageRequest::new(
            thread_id,
            request,
            visible_range.clone(),
            source_visible_range,
            self.transcript_residency_page_cancellation_generation,
        ))
    }

    pub(super) fn stage_loading_thread_history_page(
        &mut self,
        request: PendingTranscriptResidencyPageRequest,
        page: LoadedTranscriptHistoryPage,
        image_resolver: TranscriptImagePathResolver,
    ) -> bool {
        if !self.transcript_residency_page_request_is_current(&request) {
            return false;
        }

        let source_start = self
            .transcript_history_window
            .source_start_for_loading_request(request.request())
            .unwrap_or(request.source_visible_range.start);
        let presentability = TranscriptPresentabilityWindow::from_history_page(
            request.thread_id(),
            &page.turns,
            &image_resolver,
            source_start,
        );
        let media_admission = TranscriptMediaAdmissionWindow::from_history_page(
            request.thread_id(),
            &page.turns,
            &image_resolver,
            source_start,
        );
        let presentability_summary = presentability.summary();
        debug!(
            thread_id = request.thread_id(),
            request = ?request.request(),
            presentation_visible_start = request.presentation_visible_range.start,
            presentation_visible_end = request.presentation_visible_range.end,
            source_visible_start = request.source_visible_range.start,
            source_visible_end = request.source_visible_range.end,
            presentability_rows = presentability_summary.row_count,
            presentable_rows = presentability_summary.presentable_rows,
            completed_media_pending_rows = presentability_summary.completed_media_pending_rows,
            "staged transcript residency page admission"
        );
        self.staged_transcript_residency_page = Some(StagedTranscriptResidencyPageAdmission::new(
            request,
            page,
            image_resolver,
            presentability,
            media_admission,
        ));
        true
    }

    pub(super) fn publish_staged_thread_history_page(&mut self) -> usize {
        let Some(staged) = self.staged_transcript_residency_page.as_ref() else {
            return 0;
        };
        if !self.transcript_residency_page_request_is_current(&staged.request) {
            self.staged_transcript_residency_page = None;
            return 0;
        }
        if !staged.is_ready_for_publication() {
            let presentability = staged.presentability.summary();
            let media_admission = staged.media_admission.last_summary();
            debug!(
                thread_id = staged.request.thread_id(),
                request = ?staged.request.request(),
                presentability_rows = presentability.row_count,
                presentable_rows = presentability.presentable_rows,
                completed_media_pending_rows = presentability.completed_media_pending_rows,
                media_admission_items = media_admission.completed_media_items,
                media_admission_pending_items = media_admission.pending_completed_media_items,
                media_admission_rows_budget_exhausted = media_admission.rows_budget_exhausted,
                media_admission_media_budget_exhausted = media_admission.media_budget_exhausted,
                media_admission_time_budget_exhausted = media_admission.time_budget_exhausted,
                "transcript residency page admission remains staged pending presentability"
            );
            return 0;
        }

        let Some(staged) = self.staged_transcript_residency_page.take() else {
            return 0;
        };

        let presentability = staged.presentability.summary();
        debug!(
            thread_id = staged.request.thread_id(),
            request = ?staged.request.request(),
            source_visible_start = staged.request.source_visible_range.start,
            source_visible_end = staged.request.source_visible_range.end,
            presentability_rows = presentability.row_count,
            presentable_rows = presentability.presentable_rows,
            completed_media_pending_rows = presentability.completed_media_pending_rows,
            "publishing staged transcript residency page admission"
        );
        self.publish_loaded_thread_history_page(staged)
    }

    fn publish_loaded_thread_history_page(
        &mut self,
        staged: StagedTranscriptResidencyPageAdmission,
    ) -> usize {
        let thread_id = staged.request.thread_id;
        let request = staged.request.request;
        let page = staged.page;
        let image_resolver = staged.image_resolver;
        if self.selected_thread_id() != Some(thread_id.as_str()) {
            self.transcript_history_window.fail_loading_older();
            return 0;
        }

        match request {
            TranscriptHistoryPageRequest::Older { .. } => {
                self.composer_image_labels
                    .observe_thread_turns(&thread_id, &page.turns);
                let prepended = self
                    .execution_details
                    .prepend_thread_history_page_with_image_resolver_and_partial_mode(
                        &thread_id,
                        page.turns.clone(),
                        &image_resolver,
                        false,
                    );
                self.transcript_history_window
                    .finish_loading_older_with_turn_ids(&page, prepended.turn_ids);
                if prepended.added_count > 0 {
                    log_transcript_turns_loaded(
                        &thread_id,
                        prepended.added_count,
                        "older",
                        0..prepended.added_count,
                    );
                    let prepended_turns =
                        self.execution_details.turns()[..prepended.added_count].to_vec();
                    self.prepend_transcript_presentation_rows(prepended_turns.as_slice());
                    self.release_cold_history_pages_around_current_view();
                }
                prepended.added_count
            }
            TranscriptHistoryPageRequest::Released { page_id, .. } => {
                self.composer_image_labels
                    .observe_thread_turns(&thread_id, &page.turns);
                let Some(restored) = self
                    .transcript_history_window
                    .finish_loading_released_page(page_id, &page)
                else {
                    return 0;
                };
                let replacements = self
                    .execution_details
                    .restore_history_page_with_image_resolver_and_partial_mode(
                        &thread_id,
                        restored.range.start,
                        &restored.turn_ids,
                        page.turns,
                        &image_resolver,
                        false,
                    );
                let restored_count = replacements.len();
                log_transcript_turns_loaded(
                    &thread_id,
                    restored_count,
                    "released",
                    restored.range.clone(),
                );
                for replacement in replacements {
                    self.replace_transcript_presentation_turn(replacement.index, replacement.turn);
                }
                self.release_cold_history_pages_around_current_view();
                restored_count
            }
        }
    }

    pub(super) fn clear_transcript_residency_page_admission(&mut self) {
        self.staged_transcript_residency_page = None;
        self.transcript_residency_page_cancellation_generation = self
            .transcript_residency_page_cancellation_generation
            .saturating_add(1);
    }

    fn transcript_residency_page_request_is_current(
        &self,
        request: &PendingTranscriptResidencyPageRequest,
    ) -> bool {
        self.selected_thread_id() == Some(request.thread_id.as_str())
            && self.transcript_residency_page_cancellation_generation
                == request.cancellation_generation
            && self
                .transcript_history_window
                .loading_page_matches_request(&request.request)
    }

    pub(super) fn finish_loading_older_history_failure(&mut self) {
        self.clear_transcript_residency_page_admission();
        self.transcript_history_window.fail_loading_older();
    }

    pub(super) fn release_cold_history_pages(
        &mut self,
        visible_range: &std::ops::Range<usize>,
    ) -> bool {
        let source_visible_range = self
            .transcript_presentation
            .source_range_for_presentation_range(visible_range);
        let releases = self
            .transcript_history_window
            .release_cold_pages(&source_visible_range);
        if releases.is_empty() {
            return false;
        }

        for release in releases {
            log_transcript_turns_unloaded(release.page_id, release.range.clone());
            self.note_transcript_residency_release(release.range.len());
            let replacements = self
                .execution_details
                .release_history_range(release.range.clone());
            let mut released_row_identities = Vec::new();
            for replacement in replacements {
                let presentation_index = self
                    .transcript_presentation
                    .presentation_index_for_source_turn(replacement.index);
                if let Some(row_identity) = presentation_index
                    .and_then(|presentation_index| {
                        self.transcript_presentation
                            .row_identity(presentation_index)
                    })
                    .map(|identity| identity.as_str().to_string())
                {
                    released_row_identities.push(row_identity);
                }
                self.replace_transcript_presentation_turn(replacement.index, replacement.turn);
            }
            if !released_row_identities.is_empty() {
                self.note_transcript_content_release(released_row_identities);
            }
        }
        true
    }

    fn note_transcript_content_release(&mut self, row_identities: Vec<String>) {
        self.transcript_content_release_generation =
            self.transcript_content_release_generation.saturating_add(1);
        self.transcript_content_release_row_identities = row_identities;
        self.last_transcript_content_scroll_signature = None;
        self.reconcile_transcript_branch_menu_target();
        self.reconcile_transcript_edit_mode();
    }

    fn release_cold_history_pages_around_current_view(&mut self) -> bool {
        let visible_range = self.transcript_list_state.visible_range();
        self.release_cold_history_pages(&visible_range)
    }
}
