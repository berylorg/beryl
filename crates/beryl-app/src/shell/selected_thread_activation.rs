use std::time::Instant;

use beryl_backend::{ThreadInfo, ThreadSessionMetadata, ThreadSummary};
use beryl_model::workspace::WorkspaceId;
use tracing::debug;

use crate::memory_diagnostics::{self, MemoryMilestone};

use super::execution_detail::{ExecutionItem, TranscriptImagePathResolver};
use super::thread_navigation::ThreadNavigationActivationSource;
use super::transcript_history::TranscriptHistoryWindow;
use super::transcript_media_admission::{
    TranscriptMediaAdmissionRequest, TranscriptMediaAdmissionSummary,
    TranscriptMediaAdmissionTarget, TranscriptMediaAdmissionWindow,
};
use super::transcript_presentability::TranscriptPresentabilityWindow;
use super::{ConversationSurfaceState, elapsed_ms, transcript_residency_logging};

#[derive(Clone)]
pub(super) struct PendingThreadActivation {
    pub(super) label: String,
    thread_id: String,
    execution_target: WorkspaceId,
    source: ThreadNavigationActivationSource,
}

impl PendingThreadActivation {
    fn matches(&self, thread_id: &str, execution_target: &WorkspaceId) -> bool {
        self.thread_id == thread_id && &self.execution_target == execution_target
    }
}

#[derive(Clone)]
pub(super) struct StagedSelectedThreadActivation {
    execution_target: WorkspaceId,
    thread: ThreadInfo,
    history_window: TranscriptHistoryWindow,
    image_resolver: TranscriptImagePathResolver,
    session_metadata: Option<ThreadSessionMetadata>,
    source: SelectedThreadActivationSource,
    initial_viewport_policy: SelectedThreadInitialViewportPolicy,
    presentability: TranscriptPresentabilityWindow,
    media_admission: TranscriptMediaAdmissionWindow,
}

pub(super) struct PublishedSelectedThreadActivation {
    pub(super) summary: ThreadSummary,
    pub(super) execution_target: WorkspaceId,
    pub(super) source: SelectedThreadActivationSource,
    pub(super) activated_idle: bool,
    pub(super) history_turn_count: usize,
    pub(super) history_item_count: usize,
    pub(super) history_generated_image_count: usize,
}

impl StagedSelectedThreadActivation {
    pub(super) fn new(
        execution_target: WorkspaceId,
        thread: ThreadInfo,
        history_window: TranscriptHistoryWindow,
        image_resolver: TranscriptImagePathResolver,
        session_metadata: Option<ThreadSessionMetadata>,
        source: SelectedThreadActivationSource,
        initial_viewport_policy: SelectedThreadInitialViewportPolicy,
    ) -> Self {
        let presentability = TranscriptPresentabilityWindow::from_selected_thread_activation(
            &thread,
            &image_resolver,
        );
        let media_admission = TranscriptMediaAdmissionWindow::from_selected_thread_activation(
            &thread,
            &image_resolver,
        );
        Self {
            execution_target,
            thread,
            history_window,
            image_resolver,
            session_metadata,
            source,
            initial_viewport_policy,
            presentability,
            media_admission,
        }
    }

    pub(super) fn media_admission_request(&self) -> TranscriptMediaAdmissionRequest {
        self.media_admission
            .admission_request(TranscriptMediaAdmissionTarget::SelectedThread {
                thread_id: self.thread.summary().id,
            })
    }

    pub(super) fn media_admission_target_matches(
        &self,
        target: &TranscriptMediaAdmissionTarget,
    ) -> bool {
        matches!(
            target,
            TranscriptMediaAdmissionTarget::SelectedThread { thread_id }
                if thread_id == &self.thread.summary().id
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectedThreadActivationSource {
    StartupRestore,
    BackendReopenRefresh,
    Explicit(ThreadNavigationActivationSource),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectedThreadInitialViewportPolicy {
    Tail,
}

impl ConversationSurfaceState {
    pub(super) fn begin_thread_activation(
        &mut self,
        thread_id: impl Into<String>,
        execution_target: WorkspaceId,
        source: ThreadNavigationActivationSource,
        label: impl Into<String>,
    ) {
        self.pending_thread_activation = Some(PendingThreadActivation {
            label: label.into(),
            thread_id: thread_id.into(),
            execution_target,
            source,
        });
        self.staged_selected_thread_activation = None;
        self.clear_transcript_residency_page_admission();
        self.notices.clear_all();
        self.close_transcript_branch_menu();
        self.cancel_transcript_edit_mode();
    }

    pub(super) fn clear_pending_thread_activation(&mut self) {
        self.pending_thread_activation = None;
        self.staged_selected_thread_activation = None;
    }

    pub(super) fn pending_selected_thread_activation_source(
        &self,
        thread_id: &str,
        execution_target: &WorkspaceId,
    ) -> Option<SelectedThreadActivationSource> {
        let pending = self.pending_thread_activation.as_ref()?;
        pending
            .matches(thread_id, execution_target)
            .then_some(SelectedThreadActivationSource::Explicit(pending.source))
    }

    pub(super) fn stage_selected_thread_activation(
        &mut self,
        activation: StagedSelectedThreadActivation,
    ) {
        self.staged_selected_thread_activation = Some(activation);
    }

    pub(super) fn publish_staged_selected_thread_activation(
        &mut self,
    ) -> Option<PublishedSelectedThreadActivation> {
        let staged = self.staged_selected_thread_activation.as_ref()?;
        if !staged.is_ready_for_publication() {
            let presentability = staged.presentability.summary();
            let media_admission = staged.media_admission.last_summary();
            debug!(
                thread_id = staged.thread.summary().id.as_str(),
                presentability_rows = presentability.row_count,
                presentable_rows = presentability.presentable_rows,
                completed_media_pending_rows = presentability.completed_media_pending_rows,
                media_admission_items = media_admission.completed_media_items,
                media_admission_pending_items = media_admission.pending_completed_media_items,
                media_admission_rows_budget_exhausted = media_admission.rows_budget_exhausted,
                media_admission_media_budget_exhausted = media_admission.media_budget_exhausted,
                media_admission_time_budget_exhausted = media_admission.time_budget_exhausted,
                "selected-thread activation remains staged pending presentability"
            );
            return None;
        }

        let staged = self.staged_selected_thread_activation.take()?;
        let summary = staged.thread.summary();
        let source = staged.source;
        let execution_target = staged.execution_target.clone();
        let activated_idle = matches!(staged.thread.status, beryl_backend::ThreadStatus::Idle);
        let history_turn_count = staged.thread.turns.len();
        let history_item_count = staged
            .thread
            .turns
            .iter()
            .map(|turn| turn.items.len())
            .sum::<usize>();
        let history_generated_image_count = staged
            .thread
            .turns
            .iter()
            .flat_map(|turn| turn.items.iter())
            .filter(|item| matches!(item, beryl_backend::ThreadItem::ImageGeneration(_)))
            .count();
        let presentability = staged.presentability.summary();
        self.publish_selected_thread_history_window(
            &staged.thread,
            staged.history_window,
            &staged.image_resolver,
            staged.initial_viewport_policy,
        );
        if let Some(metadata) = staged.session_metadata {
            self.set_thread_session_metadata(metadata);
        }
        debug!(
            thread_id = summary.id.as_str(),
            runtime = execution_target.runtime_mode().display_name(),
            activation_source = ?source,
            presentability_rows = presentability.row_count,
            presentable_rows = presentability.presentable_rows,
            completed_media_pending_rows = presentability.completed_media_pending_rows,
            "published staged selected-thread activation"
        );
        Some(PublishedSelectedThreadActivation {
            summary,
            execution_target,
            source,
            activated_idle,
            history_turn_count,
            history_item_count,
            history_generated_image_count,
        })
    }

    pub(super) fn load_thread_history(&mut self, thread: &ThreadInfo) {
        self.load_thread_history_window(
            thread,
            TranscriptHistoryWindow::default(),
            &TranscriptImagePathResolver::default(),
        );
    }

    pub(super) fn load_thread_history_window(
        &mut self,
        thread: &ThreadInfo,
        history_window: TranscriptHistoryWindow,
        image_resolver: &TranscriptImagePathResolver,
    ) {
        self.publish_selected_thread_history_window(
            thread,
            history_window,
            image_resolver,
            SelectedThreadInitialViewportPolicy::Tail,
        );
    }

    pub(super) fn publish_selected_thread_history_window(
        &mut self,
        thread: &ThreadInfo,
        mut history_window: TranscriptHistoryWindow,
        image_resolver: &TranscriptImagePathResolver,
        initial_viewport_policy: SelectedThreadInitialViewportPolicy,
    ) {
        let load_started = Instant::now();
        let thread_id = thread.summary().id;
        match initial_viewport_policy {
            SelectedThreadInitialViewportPolicy::Tail => {}
        }
        if history_window.is_empty() {
            history_window = TranscriptHistoryWindow::from_turns(&thread.turns);
        }
        self.upsert_selected_thread(thread.summary());
        self.selected_thread_status = Some(thread.status.clone());
        self.sync_thread_selector_active_thread();
        self.composer_image_labels.observe_thread_history(thread);
        self.composer_image_labels.prepare_thread_history_scan(
            &thread.summary().id,
            history_window.has_older_pages()
                || thread
                    .turns
                    .iter()
                    .any(|turn| turn.items_view != beryl_backend::TurnItemsView::Full),
        );
        let execution_detail_started = Instant::now();
        self.execution_details
            .load_thread_history_with_image_resolver_and_partial_mode(
                thread,
                image_resolver,
                false,
            );
        let execution_detail_elapsed = execution_detail_started.elapsed();
        self.hard_stop_targets.clear_all();
        let presentation_started = Instant::now();
        self.transcript_presentation
            .replace_from_turns(self.execution_details.turns());
        let presentation_elapsed = presentation_started.elapsed();
        if memory_diagnostics::enabled() {
            let turn_count = self.execution_details.turns().len();
            let item_count = self
                .execution_details
                .turns()
                .iter()
                .map(|turn| turn.items.len())
                .sum::<usize>();
            let generated_image_count = self
                .execution_details
                .turns()
                .iter()
                .flat_map(|turn| turn.items.iter())
                .filter(|item| matches!(item, ExecutionItem::GeneratedImage(_)))
                .count();
            MemoryMilestone::new("transcript_projection_update")
                .thread_id(thread_id.as_str())
                .history_counts(turn_count, item_count, generated_image_count)
                .retained_state_if_enabled(|| self.retained_state_snapshot())
                .log();
        }
        self.status_line.clear_session_metadata();
        self.reset_loaded_history_live_scroll();
        self.transcript_user_scrolled = false;
        self.transcript_history_window = history_window;
        self.transcript_history_window
            .bind_residency_to_thread(thread_id.as_str());
        self.transcript_residency_diagnostics = Default::default();
        self.transcript_reset_generation = self.transcript_reset_generation.saturating_add(1);
        self.transcript_content_release_generation = 0;
        self.transcript_content_release_row_identities.clear();
        self.invalidated_stream_turns.clear();
        self.pending_thread_activation = None;
        self.staged_selected_thread_activation = None;
        self.clear_transcript_residency_page_admission();
        self.context_compaction_thread_id = None;
        self.close_transcript_branch_menu();
        self.cancel_transcript_edit_mode();
        self.pending_turn_input_queue = None;
        self.pending_active_turn_steering_queue = None;
        self.notices.clear_all();
        self.transcript_list_state
            .reset(self.transcript_list_item_count());
        let loaded_turns = self.execution_details.turns().len();
        transcript_residency_logging::log_transcript_turns_loaded(
            thread_id.as_str(),
            loaded_turns,
            "initial",
            0..loaded_turns,
        );
        debug!(
            thread_id = thread_id.as_str(),
            execution_detail_load_history_ms = elapsed_ms(execution_detail_elapsed),
            presentation_replace_from_turns_ms = elapsed_ms(presentation_elapsed),
            surface_load_thread_history_window_ms = elapsed_ms(load_started.elapsed()),
            "loaded thread history window into conversation surface"
        );
    }
}
