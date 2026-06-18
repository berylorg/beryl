use std::time::{Duration, Instant};

use beryl_backend::{ThreadStatus, ThreadSummary};
use beryl_model::workspace::WorkspaceId;
use tracing::debug;

use crate::gui_control_dynamic_tools::PendingActivationUiState;

use super::thread_navigation::ThreadNavigationActivationSource;
use super::{ConversationSurfaceState, ShellView};

#[path = "thread_activation/loader.rs"]
mod loader;
#[path = "thread_activation/preparation.rs"]
mod preparation;

pub(crate) use loader::{ExistingThreadActivationError, ThreadActivationLoader};
pub(super) use preparation::{
    ActivationPreparer, StagedSelectedThreadActivation,
    prepare_storage_backed_transcript_activation,
};

const PENDING_THREAD_ACTIVATION_INITIAL_PROGRESS: f32 = 0.06;
const PENDING_THREAD_ACTIVATION_WORKER_PROGRESS_CAP: f32 = 0.45;
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
    pub(super) prepared_transcript: super::syndic_transcript::PreparedTranscriptActivation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectedThreadActivationSource {
    StartupRestore,
    BackendReopenRefresh,
    Explicit(ThreadNavigationActivationSource),
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
        self.staged_thread_activation = None;
        self.clear_transcript_shell_transient_state();
        self.notices.clear_all();
        self.close_transcript_branch_menu();
        self.cancel_transcript_edit_mode();
    }

    pub(super) fn clear_pending_thread_activation(&mut self) {
        self.pending_thread_activation = None;
        self.staged_thread_activation = None;
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
        if self.pending_thread_activation.is_none() && self.staged_thread_activation.is_none() {
            return None;
        }

        let pending = self.pending_thread_activation.as_ref();
        let staged = self.staged_thread_activation.as_ref();
        Some(PendingActivationUiState {
            pending_label: pending.map(|pending| pending.label.clone()),
            pending_thread_id: pending.map(|pending| pending.thread_id.clone()),
            pending_progress: pending.map(PendingThreadActivation::progress),
            staged_thread_id: staged.map(|staged| staged.thread.summary().id),
            staged_source: staged.map(|staged| format!("{:?}", staged.source)),
            staged_metadata_turn_count: None,
            ready_for_publication: staged
                .map(StagedSelectedThreadActivation::is_ready_for_publication),
            progress_cap: self.pending_thread_activation_progress_cap(),
            presentability: None,
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
        self.staged_thread_activation
            .as_ref()
            .map(StagedSelectedThreadActivation::progress_cap)
            .or(Some(PENDING_THREAD_ACTIVATION_WORKER_PROGRESS_CAP))
    }

    pub(super) fn pending_thread_activation_source(
        &self,
        thread_id: &str,
        execution_target: &WorkspaceId,
    ) -> Option<SelectedThreadActivationSource> {
        let pending = self.pending_thread_activation.as_ref()?;
        pending
            .matches(thread_id, execution_target)
            .then_some(SelectedThreadActivationSource::Explicit(pending.source))
    }

    pub(super) fn stage_thread_activation(&mut self, activation: StagedSelectedThreadActivation) {
        self.staged_thread_activation = Some(activation);
    }

    pub(super) fn publish_staged_thread_activation(
        &mut self,
    ) -> Option<PublishedSelectedThreadActivation> {
        let staged = self.staged_thread_activation.as_ref()?;
        if !staged.is_ready_for_publication() {
            debug!(
                thread_id = staged.thread.summary().id.as_str(),
                "selected-thread activation remains staged pending publication"
            );
            return None;
        }

        let staged = self.staged_thread_activation.take()?;
        let summary = staged.thread.summary();
        let source = staged.source;
        let execution_target = staged.execution_target.clone();
        let activated_idle = matches!(staged.thread.status, ThreadStatus::Idle);
        let prepared_transcript = staged.prepared_transcript.clone();

        self.upsert_selected_thread(summary.clone());
        self.selected_thread_status = Some(staged.thread.status.clone());
        self.sync_thread_selector_active_thread();
        self.hard_stop_targets.clear_all();
        self.status_line.clear_session_metadata();
        self.active_turn_state.reset();
        self.clear_transcript_shell_transient_state();
        self.pending_thread_activation = None;
        self.staged_thread_activation = None;
        self.context_compaction_thread_id = None;
        self.close_transcript_branch_menu();
        self.cancel_transcript_edit_mode();
        self.pending_turn_input_queue = None;
        self.pending_active_turn_steering_queue = None;
        self.notices.clear_all();
        if let Some(metadata) = staged.session_metadata {
            self.set_thread_session_metadata(metadata);
        }

        debug!(
            thread_id = summary.id.as_str(),
            runtime = execution_target.runtime_mode().display_name(),
            activation_source = ?source,
            "published metadata-only selected-thread activation"
        );
        Some(PublishedSelectedThreadActivation {
            summary,
            execution_target,
            source,
            activated_idle,
            prepared_transcript,
        })
    }
}

impl ShellView {
    pub(super) fn thread_activation_pending(&self) -> bool {
        self.thread_activation_receiver.is_some()
            || self.conversation_surface().is_some_and(|surface| {
                surface.pending_thread_activation_label().is_some()
                    || surface.staged_thread_activation.is_some()
            })
    }
}
