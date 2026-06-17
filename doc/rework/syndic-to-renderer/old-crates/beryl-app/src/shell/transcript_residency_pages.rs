use super::transcript_history::{
    TRANSCRIPT_RESIDENCY_ESTIMATED_ROW_HEIGHT, TranscriptResidencyTargetPlan,
    sanitize_loaded_page_for_turn_admission_plan, turn_admission_plan_for_page_window,
};
use super::transcript_media_admission::{
    TranscriptMediaAdmissionRequest, TranscriptMediaAdmissionSummary,
    TranscriptMediaAdmissionTarget, TranscriptMediaAdmissionWindow,
};
use super::transcript_prepublication_preparation::{
    TranscriptPrepublicationPreparationLayout, TranscriptPrepublicationPreparationRequest,
    TranscriptPrepublicationPreparationSummary, TranscriptPrepublicationPreparationWindow,
};
use super::transcript_presentability::TranscriptPresentabilityWindow;
use super::transcript_residency_logging::{
    log_transcript_resident_turns_admitted, log_transcript_resident_turns_released,
    log_transcript_transport_page_received,
};
use super::transcript_residency_pins::TranscriptResidencyAdmissionSummary;
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
    pub(super) fn new(
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
    prepublication_preparation: TranscriptPrepublicationPreparationWindow,
    admission_summary: TranscriptResidencyAdmissionSummary,
}

impl StagedTranscriptResidencyPageAdmission {
    fn new(
        request: PendingTranscriptResidencyPageRequest,
        page: LoadedTranscriptHistoryPage,
        image_resolver: TranscriptImagePathResolver,
        presentability: TranscriptPresentabilityWindow,
        media_admission: TranscriptMediaAdmissionWindow,
        prepublication_preparation: TranscriptPrepublicationPreparationWindow,
        admission_summary: TranscriptResidencyAdmissionSummary,
    ) -> Self {
        Self {
            request,
            page,
            image_resolver,
            presentability,
            media_admission,
            prepublication_preparation,
            admission_summary,
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

    pub(super) fn prepublication_preparation_request(
        &self,
        layout: TranscriptPrepublicationPreparationLayout,
    ) -> TranscriptPrepublicationPreparationRequest {
        self.prepublication_preparation.preparation_request(
            TranscriptMediaAdmissionTarget::ResidencyPage {
                thread_id: self.request.thread_id.clone(),
                request: self.request.request.clone(),
                cancellation_generation: self.request.cancellation_generation,
            },
            layout,
        )
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
    ) -> TranscriptMediaAdmissionSummary {
        self.media_admission.note_summary(summary);
        self.media_admission.last_summary()
    }

    pub(super) fn prepublication_preparation_target_matches(
        &self,
        target: &TranscriptMediaAdmissionTarget,
    ) -> bool {
        self.media_admission_target_matches(target)
    }

    pub(super) fn note_prepublication_preparation_summary(
        &mut self,
        summary: TranscriptPrepublicationPreparationSummary,
    ) -> TranscriptPrepublicationPreparationSummary {
        self.prepublication_preparation.note_summary(summary);
        self.prepublication_preparation.last_summary().clone()
    }

    fn is_ready_for_publication(&self) -> bool {
        self.presentability.structural_readiness_settled()
            && self.media_admission.is_settled_for_publication()
            && self.prepublication_preparation.is_settled_for_publication()
    }
}

impl ConversationSurfaceState {
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
        let controller_facts = self.latest_transcript_residency_controller_facts().cloned();
        let source_visible_range = controller_facts
            .as_ref()
            .map(|facts| facts.source_visible_range.clone())
            .unwrap_or_else(|| request.source_visible_range.clone());
        let local_visible_range =
            local_page_visible_range(&source_visible_range, source_start, page.turns.len());
        let viewport_height = controller_facts
            .as_ref()
            .map(|facts| facts.viewport_height)
            .unwrap_or_else(|| {
                local_visible_range
                    .len()
                    .max(1)
                    .saturating_mul(TRANSCRIPT_RESIDENCY_ESTIMATED_ROW_HEIGHT)
            });
        let admission_plan = turn_admission_plan_for_page_window(
            &page,
            local_visible_range,
            viewport_height,
            self.transcript_history_window.residency_target_policy(),
            self.transcript_history_window.pinned_turn_ids(),
        );
        let source_range = source_start..source_start.saturating_add(page.turns.len());
        let request_kind = request_kind_label(request.request());
        let transport_summary = TranscriptResidencyAdmissionSummary::from_transport_page(
            request_kind,
            source_range.clone(),
            &page.turns,
        );
        log_transcript_transport_page_received(request.thread_id(), &transport_summary);
        self.note_transcript_residency_transport_page(&transport_summary);
        let page = sanitize_loaded_page_for_turn_admission_plan(&page, &admission_plan);
        let staged_summary = TranscriptResidencyAdmissionSummary::from_admitted_turns(
            request_kind,
            source_range,
            &page.turns,
        )
        .with_transport_observation(&transport_summary);
        self.note_transcript_residency_staged_admission(&staged_summary);
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
        let prepublication_preparation =
            TranscriptPrepublicationPreparationWindow::from_history_page(
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
            prepublication_preparation,
            staged_summary,
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
            let preparation = staged.prepublication_preparation.last_summary();
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
                preparation_rows = preparation.row_count,
                preparation_pending_rows = preparation.pending_rows,
                preparation_rows_budget_exhausted = preparation.rows_budget_exhausted,
                preparation_block_budget_exhausted = preparation.block_budget_exhausted,
                preparation_media_budget_exhausted = preparation.media_budget_exhausted,
                preparation_byte_budget_exhausted = preparation.byte_budget_exhausted,
                preparation_time_budget_exhausted = preparation.time_budget_exhausted,
                "transcript residency page admission remains staged pending presentability"
            );
            return 0;
        }

        let Some(staged) = self.staged_transcript_residency_page.take() else {
            return 0;
        };

        let presentability = staged.presentability.summary();
        let preparation = staged.prepublication_preparation.last_summary().clone();
        debug!(
            thread_id = staged.request.thread_id(),
            request = ?staged.request.request(),
            source_visible_start = staged.request.source_visible_range.start,
            source_visible_end = staged.request.source_visible_range.end,
            presentability_rows = presentability.row_count,
            presentable_rows = presentability.presentable_rows,
            completed_media_pending_rows = presentability.completed_media_pending_rows,
            preparation_rows = preparation.row_count,
            preparation_pending_rows = preparation.pending_rows,
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
        let admission_summary = staged.admission_summary;
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
                    self.note_transcript_residency_admission(&admission_summary);
                    log_transcript_resident_turns_admitted(&thread_id, &admission_summary);
                    let prepended_turns =
                        self.execution_details.turns()[..prepended.added_count].to_vec();
                    self.prepend_transcript_presentation_rows(prepended_turns.as_slice());
                    self.invalidate_transcript_residency_controller();
                }
                prepended.added_count
            }
            TranscriptHistoryPageRequest::Indexed { page_id, .. }
            | TranscriptHistoryPageRequest::Released { page_id, .. } => {
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
                self.note_transcript_residency_admission(&admission_summary);
                log_transcript_resident_turns_admitted(&thread_id, &admission_summary);
                for replacement in replacements {
                    self.replace_transcript_presentation_turn(replacement.index, replacement.turn);
                }
                if restored_count > 0 {
                    self.invalidate_transcript_residency_controller();
                }
                restored_count
            }
        }
    }

    pub(super) fn clear_transcript_residency_page_admission(&mut self) {
        self.staged_transcript_residency_page = None;
        self.clear_transcript_residency_staged_admission();
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

    pub(super) fn release_resident_turn_payloads_for_plan(
        &mut self,
        plan: &TranscriptResidencyTargetPlan,
    ) -> bool {
        if plan.release_turn_ids.is_empty() {
            return false;
        }

        let released = self
            .transcript_history_window
            .release_resident_turns_by_id_with_oversized_fallbacks(
                plan.release_turn_ids.iter().map(String::as_str),
                plan.oversized_turn_fallback_ids.iter().map(String::as_str),
            );
        if released.released_turn_ids.is_empty() {
            return false;
        }

        self.note_transcript_residency_release(released.released_turn_ids.len());
        let thread_id = self.selected_thread_id().map(str::to_string);
        log_transcript_resident_turns_released(
            thread_id.as_deref(),
            released.released_turn_ids.len(),
            "controller_target",
        );

        let replacements = self
            .execution_details
            .release_history_turns_by_id_with_oversized_fallbacks(
                released.released_turn_ids.iter().map(String::as_str),
                plan.oversized_turn_fallback_ids.iter().map(String::as_str),
            );
        let mut released_row_identities = Vec::new();
        let mut released_markdown_keys = Vec::new();
        let mut released_media_keys = Vec::new();
        for replacement in replacements {
            let presentation_index = self
                .transcript_presentation
                .presentation_index_for_source_turn(replacement.index);
            if let Some(row) =
                presentation_index.and_then(|index| self.transcript_presentation.turn_at(index))
            {
                released_row_identities.push(row.identity.as_str().to_string());
                released_markdown_keys.extend(
                    row.model
                        .markdown_sources()
                        .iter()
                        .map(|source| source.key.clone()),
                );
                released_media_keys.extend(
                    row.model
                        .media_descriptors()
                        .iter()
                        .map(|descriptor| descriptor.key.clone()),
                );
            }
            self.replace_transcript_presentation_turn(replacement.index, replacement.turn);
        }
        if !released_row_identities.is_empty() {
            self.note_transcript_content_release(
                released_row_identities,
                released_markdown_keys,
                released_media_keys,
            );
        }
        true
    }

    fn note_transcript_content_release(
        &mut self,
        row_identities: Vec<String>,
        markdown_keys: Vec<String>,
        media_keys: Vec<String>,
    ) {
        self.transcript_content_release_generation =
            self.transcript_content_release_generation.saturating_add(1);
        self.transcript_content_release_row_identities = row_identities;
        self.transcript_content_release_markdown_keys = markdown_keys;
        self.transcript_content_release_media_keys = media_keys;
        self.transcript_residency_frame_facts = None;
        self.transcript_navigation_frame_snapshot = None;
        self.transcript_streamed_navigation_snapshot = None;
        self.transcript_event_time_scroll.clear();
        self.last_transcript_content_scroll_signature = None;
        self.reconcile_transcript_branch_menu_target();
        self.reconcile_transcript_edit_mode();
    }
}

fn local_page_visible_range(
    source_visible_range: &std::ops::Range<usize>,
    source_start: usize,
    page_len: usize,
) -> std::ops::Range<usize> {
    let page_end = source_start.saturating_add(page_len);
    let start = source_visible_range.start.max(source_start).min(page_end);
    let end = source_visible_range.end.max(start).min(page_end);
    start.saturating_sub(source_start)..end.saturating_sub(source_start)
}

fn request_kind_label(request: &TranscriptHistoryPageRequest) -> &'static str {
    match request {
        TranscriptHistoryPageRequest::Older { .. } => "older",
        TranscriptHistoryPageRequest::Indexed { .. } => "indexed",
        TranscriptHistoryPageRequest::Released { .. } => "released",
    }
}
