use beryl_backend::{ThreadInfo, ThreadSessionMetadata};
use beryl_model::workspace::WorkspaceId;

use super::super::execution_detail::TranscriptImagePathResolver;
use super::super::transcript_history::TranscriptHistoryWindow;
use super::super::transcript_presentability::TranscriptPresentabilityWindow;
use super::{SelectedThreadActivationSource, SelectedThreadInitialViewportPolicy};

pub(in crate::shell) struct ActivationPreparer;

#[derive(Clone)]
pub(in crate::shell) struct StagedSelectedThreadActivation {
    pub(in crate::shell) execution_target: WorkspaceId,
    pub(in crate::shell) thread: ThreadInfo,
    pub(in crate::shell) history_window: TranscriptHistoryWindow,
    pub(in crate::shell) image_resolver: TranscriptImagePathResolver,
    pub(in crate::shell) session_metadata: Option<ThreadSessionMetadata>,
    pub(in crate::shell) source: SelectedThreadActivationSource,
    pub(in crate::shell) initial_viewport_policy: SelectedThreadInitialViewportPolicy,
    pub(in crate::shell) presentability: TranscriptPresentabilityWindow,
}

impl ActivationPreparer {
    pub(in crate::shell) fn prepare(
        execution_target: WorkspaceId,
        thread: ThreadInfo,
        history_window: TranscriptHistoryWindow,
        image_resolver: TranscriptImagePathResolver,
        session_metadata: Option<ThreadSessionMetadata>,
        source: SelectedThreadActivationSource,
        initial_viewport_policy: SelectedThreadInitialViewportPolicy,
    ) -> StagedSelectedThreadActivation {
        let presentability = TranscriptPresentabilityWindow::from_selected_thread_activation(
            &thread,
            &image_resolver,
        );
        StagedSelectedThreadActivation {
            execution_target,
            thread,
            history_window,
            image_resolver,
            session_metadata,
            source,
            initial_viewport_policy,
            presentability,
        }
    }
}

impl StagedSelectedThreadActivation {
    pub(in crate::shell) fn is_ready_for_publication(&self) -> bool {
        self.presentability.structural_readiness_settled()
    }

    pub(in crate::shell) fn progress_cap(&self) -> f32 {
        let presentability = self.presentability.summary();

        let structural_progress = if presentability.row_count == 0 {
            1.0
        } else {
            let pending_rows = presentability
                .missing_full_detail_rows
                .saturating_add(presentability.markdown_plan_pending_rows);
            progress_for_completed_count(
                presentability.row_count.saturating_sub(pending_rows),
                presentability.row_count,
            )
        };

        super::PENDING_THREAD_ACTIVATION_STAGED_PROGRESS_BASE
            + (super::PENDING_THREAD_ACTIVATION_PUBLICATION_PROGRESS_CAP
                - super::PENDING_THREAD_ACTIVATION_STAGED_PROGRESS_BASE)
                * structural_progress
    }
}

fn progress_for_completed_count(completed: usize, total: usize) -> f32 {
    if total == 0 {
        return 1.0;
    }
    (completed.min(total) as f32 / total as f32).clamp(0.0, 1.0)
}
