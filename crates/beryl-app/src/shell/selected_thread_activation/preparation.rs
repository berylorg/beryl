use beryl_backend::{ThreadInfo, ThreadSessionMetadata};
use beryl_model::workspace::WorkspaceId;

use super::SelectedThreadActivationSource;

pub(in crate::shell) struct ActivationPreparer;

#[derive(Clone)]
pub(in crate::shell) struct StagedSelectedThreadActivation {
    pub(in crate::shell) execution_target: WorkspaceId,
    pub(in crate::shell) thread: ThreadInfo,
    pub(in crate::shell) session_metadata: Option<ThreadSessionMetadata>,
    pub(in crate::shell) source: SelectedThreadActivationSource,
}

impl ActivationPreparer {
    pub(in crate::shell) fn prepare(
        execution_target: WorkspaceId,
        thread: ThreadInfo,
        session_metadata: Option<ThreadSessionMetadata>,
        source: SelectedThreadActivationSource,
    ) -> StagedSelectedThreadActivation {
        StagedSelectedThreadActivation {
            execution_target,
            thread,
            session_metadata,
            source,
        }
    }
}

impl StagedSelectedThreadActivation {
    pub(in crate::shell) fn is_ready_for_publication(&self) -> bool {
        true
    }

    pub(in crate::shell) fn progress_cap(&self) -> f32 {
        super::PENDING_THREAD_ACTIVATION_PUBLICATION_PROGRESS_CAP
    }
}
