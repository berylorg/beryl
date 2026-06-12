use std::time::{Duration, Instant};

use beryl_backend::{ThreadInfo, ThreadSummary};
use beryl_model::workspace::WorkspaceId;

use crate::gui_control_dynamic_tools::{
    PendingActivationPresentabilityUiState, PendingActivationUiState,
};

mod preparation;
mod publisher;

pub(super) use preparation::{ActivationPreparer, StagedSelectedThreadActivation};

use publisher::SelectedThreadPublisher;

use super::execution_detail::TranscriptImagePathResolver;
use super::thread_navigation::ThreadNavigationActivationSource;
use super::transcript_history::TranscriptHistoryWindow;
use super::{ConversationSurfaceState, ShellView};

const PENDING_THREAD_ACTIVATION_INITIAL_PROGRESS: f32 = 0.06;
const PENDING_THREAD_ACTIVATION_WORKER_PROGRESS_CAP: f32 = 0.45;
const PENDING_THREAD_ACTIVATION_STAGED_PROGRESS_BASE: f32 = 0.60;
const PENDING_THREAD_ACTIVATION_PUBLICATION_PROGRESS_CAP: f32 = 0.96;
const PENDING_THREAD_ACTIVATION_PROGRESS_FILL_DURATION: Duration = Duration::from_millis(1800);
const PENDING_THREAD_ACTIVATION_PROGRESS_EPSILON: f32 = 0.002;

#[derive(Clone)]
pub(super) struct PendingThreadActivation {
    pub(super) label: String,
    thread_id: String,
    execution_target: WorkspaceId,
    source: ThreadNavigationActivationSource,
    started_at: Instant,
    progress: f32,
}

impl PendingThreadActivation {
    fn matches(&self, thread_id: &str, execution_target: &WorkspaceId) -> bool {
        self.thread_id == thread_id && &self.execution_target == execution_target
    }

    fn progress(&self) -> f32 {
        self.progress.clamp(0.0, 1.0)
    }

    fn timed_progress_target(&self, cap: f32, now: Instant) -> f32 {
        let cap = cap.clamp(PENDING_THREAD_ACTIVATION_INITIAL_PROGRESS, 1.0);
        let elapsed = now.saturating_duration_since(self.started_at);
        let progress_fraction = (elapsed.as_secs_f32()
            / PENDING_THREAD_ACTIVATION_PROGRESS_FILL_DURATION.as_secs_f32())
        .clamp(0.0, 1.0);
        PENDING_THREAD_ACTIVATION_INITIAL_PROGRESS
            + (cap - PENDING_THREAD_ACTIVATION_INITIAL_PROGRESS) * progress_fraction
    }

    fn advance_progress_to(&mut self, target: f32) -> bool {
        let target = target.clamp(self.progress, 1.0);
        if target - self.progress < PENDING_THREAD_ACTIVATION_PROGRESS_EPSILON {
            return false;
        }
        self.progress = target;
        true
    }
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
            started_at: Instant::now(),
            progress: PENDING_THREAD_ACTIVATION_INITIAL_PROGRESS,
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

    pub(super) fn pending_thread_activation_matches(
        &self,
        thread_id: &str,
        execution_target: &WorkspaceId,
    ) -> bool {
        self.pending_thread_activation
            .as_ref()
            .is_some_and(|pending| pending.matches(thread_id, execution_target))
    }

    pub(super) fn pending_thread_activation_progress(&self) -> Option<f32> {
        self.pending_thread_activation
            .as_ref()
            .map(PendingThreadActivation::progress)
    }

    pub(super) fn pending_activation_ui_state(&self) -> Option<PendingActivationUiState> {
        if self.pending_thread_activation.is_none()
            && self.staged_selected_thread_activation.is_none()
        {
            return None;
        }

        let pending = self.pending_thread_activation.as_ref();
        let staged = self.staged_selected_thread_activation.as_ref();
        let presentability = staged.map(|staged| {
            let summary = staged.presentability.summary();
            PendingActivationPresentabilityUiState {
                row_count: summary.row_count,
                presentable_rows: summary.presentable_rows,
                missing_full_detail_rows: summary.missing_full_detail_rows,
                markdown_plan_pending_rows: summary.markdown_plan_pending_rows,
                completed_media_pending_rows: summary.completed_media_pending_rows,
                terminal_fallback_media_items: summary.terminal_fallback_media_items,
                live_pending_placeholder_items: summary.live_pending_placeholder_items,
                structural_readiness_settled: staged.presentability.structural_readiness_settled(),
                presentable: staged.presentability.is_presentable(),
            }
        });
        Some(PendingActivationUiState {
            pending_label: pending.map(|pending| pending.label.clone()),
            pending_thread_id: pending.map(|pending| pending.thread_id.clone()),
            pending_progress: pending.map(PendingThreadActivation::progress),
            staged_thread_id: staged.map(|staged| staged.thread.summary().id),
            staged_source: staged.map(|staged| format!("{:?}", staged.source)),
            staged_history_turn_count: staged.map(|staged| staged.thread.turns.len()),
            ready_for_publication: staged
                .map(StagedSelectedThreadActivation::is_ready_for_publication),
            progress_cap: self.pending_thread_activation_progress_cap(),
            presentability,
            media_admission: None,
            prepublication_preparation: None,
        })
    }

    pub(super) fn poll_pending_thread_activation_progress(&mut self, now: Instant) -> bool {
        let Some(cap) = self.pending_thread_activation_progress_cap() else {
            return false;
        };
        let Some(pending) = self.pending_thread_activation.as_mut() else {
            return false;
        };
        pending.advance_progress_to(pending.timed_progress_target(cap, now))
    }

    fn pending_thread_activation_progress_cap(&self) -> Option<f32> {
        self.pending_thread_activation.as_ref()?;
        self.staged_selected_thread_activation
            .as_ref()
            .map(StagedSelectedThreadActivation::progress_cap)
            .or(Some(PENDING_THREAD_ACTIVATION_WORKER_PROGRESS_CAP))
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
        SelectedThreadPublisher::try_publish(self)
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
        publisher::publish_history_window(
            self,
            thread,
            history_window,
            image_resolver,
            SelectedThreadInitialViewportPolicy::Tail,
        );
    }
}

impl ShellView {
    pub(super) fn selected_thread_activation_pending(&self) -> bool {
        self.thread_activation_receiver.is_some()
            || self.conversation_surface().is_some_and(|surface| {
                surface.pending_thread_activation_label().is_some()
                    || surface.staged_selected_thread_activation.is_some()
            })
    }
}
