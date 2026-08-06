use std::time::Instant;

use beryl_model::conversation::ConversationThreadId;
use beryl_model::workspace::BerylWorkspaceId;

use super::{
    phase_thread_transition::{PhaseThreadTransitionOwner, bounded_phase_thread_notice_detail},
    phase_thread_transition_applicator::{
        PhaseThreadCompletionHost, apply_deferred_prepared_registration,
    },
    phase_thread_transition_deferred::DeferredPhaseThreadOutcome,
};

pub(crate) trait PhaseThreadWorkspaceDeletionHost: PhaseThreadCompletionHost {
    fn phase_thread_task_pending_for_workspace(&self, workspace_id: &BerylWorkspaceId) -> bool;

    fn take_deferred_phase_thread_outcomes_for_workspace(
        &mut self,
        workspace_id: &BerylWorkspaceId,
    ) -> Vec<DeferredPhaseThreadOutcome>;

    fn publish_released_phase_thread_outcomes(
        &mut self,
        workspace_id: &BerylWorkspaceId,
        released: &ReleasedPhaseThreadWorkspaceDeletionOutcomes,
    );

    fn capture_persistence_barrier_and_start_delete_worker(
        &mut self,
        workspace_id: &BerylWorkspaceId,
    ) -> Result<(), String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PhaseThreadWorkspaceDeletionPoll {
    WaitingForPhaseThread,
    WorkerStarted,
    WorkerStartFailed(String),
}

pub(crate) struct PhaseThreadWorkspaceDeletionDrain {
    workspace_id: BerylWorkspaceId,
    worker_started: bool,
}

impl PhaseThreadWorkspaceDeletionDrain {
    pub(crate) fn accept(
        workspace_id: BerylWorkspaceId,
        transition: &mut PhaseThreadTransitionOwner,
        retention_deadline: Instant,
    ) -> Self {
        transition.invalidate_for_accepted_workspace_replacement(true, retention_deadline);
        Self::new(workspace_id)
    }

    pub(crate) fn new(workspace_id: BerylWorkspaceId) -> Self {
        Self {
            workspace_id,
            worker_started: false,
        }
    }

    pub(crate) fn workspace_id(&self) -> &BerylWorkspaceId {
        &self.workspace_id
    }

    fn begin_worker_if_drained(&mut self, phase_thread_task_pending: bool) -> bool {
        if self.worker_started || phase_thread_task_pending {
            return false;
        }
        self.worker_started = true;
        true
    }
}

pub(crate) fn poll_phase_thread_workspace_deletion<H: PhaseThreadWorkspaceDeletionHost>(
    deletion: &mut PhaseThreadWorkspaceDeletionDrain,
    host: &mut H,
) -> PhaseThreadWorkspaceDeletionPoll {
    let task_pending = host.phase_thread_task_pending_for_workspace(deletion.workspace_id());
    if !deletion.begin_worker_if_drained(task_pending) {
        return PhaseThreadWorkspaceDeletionPoll::WaitingForPhaseThread;
    }

    let outcomes = host.take_deferred_phase_thread_outcomes_for_workspace(deletion.workspace_id());
    let released = replay_and_release_phase_thread_workspace_deletion_outcomes(host, outcomes);
    host.publish_released_phase_thread_outcomes(deletion.workspace_id(), &released);
    match host.capture_persistence_barrier_and_start_delete_worker(deletion.workspace_id()) {
        Ok(()) => PhaseThreadWorkspaceDeletionPoll::WorkerStarted,
        Err(error) => PhaseThreadWorkspaceDeletionPoll::WorkerStartFailed(error),
    }
}

pub(crate) struct ReleasedPhaseThreadWorkspaceDeletionOutcomes {
    refresh_inventory: bool,
    known_remaining_child_ids: Vec<ConversationThreadId>,
    final_notice: Option<(String, String)>,
}

impl ReleasedPhaseThreadWorkspaceDeletionOutcomes {
    pub(crate) fn refresh_inventory(&self) -> bool {
        self.refresh_inventory
    }

    pub(crate) fn known_remaining_child_ids(&self) -> &[ConversationThreadId] {
        &self.known_remaining_child_ids
    }

    pub(crate) fn final_notice(&self) -> Option<(&str, &str)> {
        self.final_notice
            .as_ref()
            .map(|(title, detail)| (title.as_str(), detail.as_str()))
    }
}

pub(crate) fn replay_and_release_phase_thread_workspace_deletion_outcomes<
    H: PhaseThreadCompletionHost,
>(
    host: &mut H,
    outcomes: Vec<DeferredPhaseThreadOutcome>,
) -> ReleasedPhaseThreadWorkspaceDeletionOutcomes {
    let mut refresh_inventory = false;
    let mut known_remaining_child_ids = Vec::new();
    let mut final_notice = None;
    for outcome in outcomes {
        refresh_inventory |= outcome.refresh_inventory();
        if let Some(registration) = outcome.prepared_registration()
            && let Err(error) =
                apply_deferred_prepared_registration(host, outcome.request(), registration)
        {
            let child_thread_id = registration.child_thread_id().clone();
            final_notice = Some((
                "Lifecycle phase child remains in backend".to_string(),
                bounded_phase_thread_notice_detail(format!(
                    "Workspace deletion released retained backend child {} without activation. The exact child remains in the backend and was not hidden or rebound. {error}",
                    child_thread_id.as_str(),
                )),
            ));
            known_remaining_child_ids.push(child_thread_id);
        } else {
            final_notice = Some((outcome.title().to_string(), outcome.detail().to_string()));
        }
    }
    ReleasedPhaseThreadWorkspaceDeletionOutcomes {
        refresh_inventory,
        known_remaining_child_ids,
        final_notice,
    }
}

pub(crate) fn complete_phase_thread_workspace_deletion(
    deletion: &mut Option<PhaseThreadWorkspaceDeletionDrain>,
) -> bool {
    deletion.take().is_some()
}
