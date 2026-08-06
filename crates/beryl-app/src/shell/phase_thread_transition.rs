use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, TryRecvError},
    },
    time::Instant,
};

use beryl_model::workspace::{BerylWorkspaceId, WorkspaceId};

use super::{
    execution_detail::UserInputFragment,
    phase_thread_preparation_core::{PhaseThreadPreparationOutcome, PhaseThreadPreparationRequest},
    phase_thread_preparation_worker::PhaseThreadPreparationUpdate,
    phase_thread_transition_deferred::DeferredPhaseThreadOutcome,
};

pub(crate) use super::phase_thread_transition_guard::{
    PhaseThreadCurrentIdentity, PhaseThreadResultGuardFailure,
    guard_phase_thread_preparation_result, phase_thread_request_registrations_are_available,
};

pub(crate) const PHASE_THREAD_TRANSITION_BUSY_MESSAGE: &str =
    "Beryl is preparing and activating the next lifecycle phase thread.";
const MAX_PHASE_THREAD_NOTICE_CHARS: usize = 1_200;
const MAX_RETAINED_CANCELLING_PHASE_THREAD_TASKS: usize = 8;

pub(crate) fn bounded_phase_thread_notice_detail(detail: impl AsRef<str>) -> String {
    let detail = detail.as_ref();
    if detail.chars().count() <= MAX_PHASE_THREAD_NOTICE_CHARS {
        return detail.to_string();
    }
    let mut bounded = detail
        .chars()
        .take(MAX_PHASE_THREAD_NOTICE_CHARS.saturating_sub(1))
        .collect::<String>();
    bounded.push('…');
    bounded
}

pub(crate) struct PhaseThreadPreparationTask {
    request: PhaseThreadPreparationRequest,
    resume_fragment: UserInputFragment,
    cancellation: Arc<AtomicBool>,
    receiver: Receiver<PhaseThreadPreparationUpdate>,
    state: PhaseThreadPreparationTaskState,
    retention_deadline: Option<Instant>,
}

impl PhaseThreadPreparationTask {
    pub(crate) fn new(
        request: PhaseThreadPreparationRequest,
        resume_fragment: UserInputFragment,
        cancellation: Arc<AtomicBool>,
        receiver: Receiver<PhaseThreadPreparationUpdate>,
    ) -> Self {
        Self {
            request,
            resume_fragment,
            cancellation,
            receiver,
            state: PhaseThreadPreparationTaskState::Active,
            retention_deadline: None,
        }
    }

    pub(crate) fn request(&self) -> &PhaseThreadPreparationRequest {
        &self.request
    }

    pub(crate) fn resume_fragment(&self) -> UserInputFragment {
        self.resume_fragment.clone()
    }

    pub(crate) fn try_recv(&self) -> Result<PhaseThreadPreparationOutcome, TryRecvError> {
        self.receiver.try_recv().map(|update| match update {
            PhaseThreadPreparationUpdate::Finished(outcome) => outcome,
        })
    }

    pub(crate) fn state(&self) -> PhaseThreadPreparationTaskState {
        self.state
    }

    pub(crate) fn cancel(&mut self, retention_deadline: Instant) {
        self.cancellation.store(true, Ordering::Release);
        self.state = PhaseThreadPreparationTaskState::Cancelling;
        self.retention_deadline = Some(retention_deadline);
    }

    fn retention_expired(&self, now: Instant) -> bool {
        self.retention_deadline
            .is_some_and(|deadline| now >= deadline)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PhaseThreadPreparationTaskState {
    Active,
    Cancelling,
}

impl PhaseThreadPreparationTaskState {
    pub(crate) fn blocks_controls(self) -> bool {
        matches!(self, Self::Active)
    }
}

pub(crate) enum PhaseThreadPreparationPoll {
    Outcome {
        task: PhaseThreadPreparationTask,
        outcome: PhaseThreadPreparationOutcome,
    },
    Disconnected {
        task: PhaseThreadPreparationTask,
    },
    RetentionExpired {
        task: PhaseThreadPreparationTask,
    },
}

pub(crate) struct PhaseThreadTransitionOwner {
    generation: u64,
    active: Option<PhaseThreadPreparationTask>,
    cancelling: Vec<PhaseThreadPreparationTask>,
    deferred_outcomes: VecDeque<DeferredPhaseThreadOutcome>,
}

impl Default for PhaseThreadTransitionOwner {
    fn default() -> Self {
        Self {
            generation: 0,
            active: None,
            cancelling: Vec::new(),
            deferred_outcomes: VecDeque::new(),
        }
    }
}

impl PhaseThreadTransitionOwner {
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.generation
    }

    pub(crate) fn install_active(&mut self, task: PhaseThreadPreparationTask) -> Result<(), ()> {
        if !self.can_install_active() {
            return Err(());
        }
        self.active = Some(task);
        Ok(())
    }

    pub(crate) fn can_install_active(&self) -> bool {
        self.active.is_none()
            && self
                .cancelling
                .len()
                .saturating_add(self.deferred_outcomes.len())
                < MAX_RETAINED_CANCELLING_PHASE_THREAD_TASKS
    }

    pub(crate) fn blocks_controls(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|task| task.state().blocks_controls())
    }

    pub(crate) fn has_poll_work(&self) -> bool {
        self.active.is_some() || !self.cancelling.is_empty()
    }

    pub(crate) fn cancel_active(&mut self, retention_deadline: Instant) -> bool {
        let cancelled = self.active.take().map(|mut task| {
            task.cancel(retention_deadline);
            self.cancelling.push(task);
        });
        self.next_generation();
        cancelled.is_some()
    }

    pub(crate) fn cancel_all(&mut self, retention_deadline: Instant) {
        self.cancel_active(retention_deadline);
        for task in &mut self.cancelling {
            task.cancellation.store(true, Ordering::Release);
            task.retention_deadline = Some(
                task.retention_deadline
                    .map_or(retention_deadline, |deadline| {
                        deadline.min(retention_deadline)
                    }),
            );
        }
    }

    pub(crate) fn invalidate_for_accepted_workspace_replacement(
        &mut self,
        replaces_active_workspace: bool,
        retention_deadline: Instant,
    ) -> bool {
        if !replaces_active_workspace {
            return false;
        }
        self.cancel_active(retention_deadline);
        true
    }

    pub(crate) fn owns_execution_target(&self, target: &WorkspaceId) -> bool {
        self.active
            .iter()
            .chain(self.cancelling.iter())
            .any(|task| task.request().execution_target() == target)
    }

    pub(crate) fn has_task_for_workspace(&self, workspace_id: &BerylWorkspaceId) -> bool {
        self.active
            .iter()
            .chain(self.cancelling.iter())
            .any(|task| task.request().workspace_id() == workspace_id)
    }

    pub(crate) fn active_request(&self) -> Option<&PhaseThreadPreparationRequest> {
        self.active
            .as_ref()
            .map(PhaseThreadPreparationTask::request)
    }

    pub(crate) fn poll_next(&mut self, now: Instant) -> Option<PhaseThreadPreparationPoll> {
        if let Some(update) = self
            .active
            .as_ref()
            .map(PhaseThreadPreparationTask::try_recv)
        {
            match update {
                Ok(outcome) => {
                    let task = self.active.take()?;
                    return Some(PhaseThreadPreparationPoll::Outcome { task, outcome });
                }
                Err(TryRecvError::Disconnected) => {
                    return self
                        .active
                        .take()
                        .map(|task| PhaseThreadPreparationPoll::Disconnected { task });
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        for index in 0..self.cancelling.len() {
            match self.cancelling[index].try_recv() {
                Ok(outcome) => {
                    let task = self.cancelling.remove(index);
                    return Some(PhaseThreadPreparationPoll::Outcome { task, outcome });
                }
                Err(TryRecvError::Disconnected) => {
                    let task = self.cancelling.remove(index);
                    return Some(PhaseThreadPreparationPoll::Disconnected { task });
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        let expired = self
            .cancelling
            .iter()
            .position(|task| task.retention_expired(now))?;
        let task = self.cancelling.remove(expired);
        Some(PhaseThreadPreparationPoll::RetentionExpired { task })
    }

    pub(crate) fn defer_outcome(&mut self, outcome: DeferredPhaseThreadOutcome) {
        self.deferred_outcomes.push_back(outcome);
        debug_assert!(
            self.cancelling
                .len()
                .saturating_add(self.deferred_outcomes.len())
                <= MAX_RETAINED_CANCELLING_PHASE_THREAD_TASKS,
            "admitted phase-thread truth must remain within its reserved shared bound"
        );
    }

    pub(crate) fn take_deferred_outcomes(
        &mut self,
        workspace_id: &BerylWorkspaceId,
    ) -> Vec<DeferredPhaseThreadOutcome> {
        let mut retained = VecDeque::with_capacity(self.deferred_outcomes.len());
        let mut selected = Vec::new();
        while let Some(outcome) = self.deferred_outcomes.pop_front() {
            if outcome.workspace_id() == workspace_id {
                selected.push(outcome);
            } else {
                retained.push_back(outcome);
            }
        }
        self.deferred_outcomes = retained;
        selected
    }

    pub(crate) fn restore_deferred_outcomes(
        &mut self,
        outcomes: impl IntoIterator<Item = DeferredPhaseThreadOutcome>,
    ) {
        let outcomes = outcomes.into_iter().collect::<Vec<_>>();
        for outcome in outcomes.into_iter().rev() {
            self.deferred_outcomes.push_front(outcome);
        }
        debug_assert!(
            self.cancelling
                .len()
                .saturating_add(self.deferred_outcomes.len())
                <= MAX_RETAINED_CANCELLING_PHASE_THREAD_TASKS,
            "restored phase-thread truth must retain its original shared reservation"
        );
    }

    #[cfg(test)]
    pub(crate) fn deferred_notice_count(&self) -> usize {
        self.deferred_outcomes.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PhaseThreadCompletionKind {
    Prepared,
    DefinitiveForkFailure,
    IndeterminateFork,
    CancelledBeforeFork,
    KnownChildCleaned,
    KnownChildRemaining,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PhaseThreadCompletionNotice {
    None,
    DefinitiveFailure,
    Indeterminate,
    Cancelled,
    KnownOrphan,
    PreparedNotActivated,
    Stale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PhaseThreadCompletionDecision {
    pub(crate) activate_prepared: bool,
    pub(crate) register_prepared: bool,
    pub(crate) refresh_original_workspace: bool,
    pub(crate) notice: PhaseThreadCompletionNotice,
}

pub(crate) fn reduce_phase_thread_completion(
    task_state: PhaseThreadPreparationTaskState,
    returned_request_matches: bool,
    current_identity_matches: bool,
    original_workspace_is_current: bool,
    kind: PhaseThreadCompletionKind,
) -> PhaseThreadCompletionDecision {
    let exact_active = task_state == PhaseThreadPreparationTaskState::Active
        && returned_request_matches
        && current_identity_matches;
    if exact_active {
        return match kind {
            PhaseThreadCompletionKind::Prepared => PhaseThreadCompletionDecision {
                activate_prepared: true,
                register_prepared: false,
                refresh_original_workspace: false,
                notice: PhaseThreadCompletionNotice::None,
            },
            PhaseThreadCompletionKind::DefinitiveForkFailure
            | PhaseThreadCompletionKind::KnownChildCleaned => PhaseThreadCompletionDecision {
                activate_prepared: false,
                register_prepared: false,
                refresh_original_workspace: false,
                notice: PhaseThreadCompletionNotice::DefinitiveFailure,
            },
            PhaseThreadCompletionKind::IndeterminateFork => PhaseThreadCompletionDecision {
                activate_prepared: false,
                register_prepared: false,
                refresh_original_workspace: true,
                notice: PhaseThreadCompletionNotice::Indeterminate,
            },
            PhaseThreadCompletionKind::CancelledBeforeFork => PhaseThreadCompletionDecision {
                activate_prepared: false,
                register_prepared: false,
                refresh_original_workspace: false,
                notice: PhaseThreadCompletionNotice::Cancelled,
            },
            PhaseThreadCompletionKind::KnownChildRemaining => PhaseThreadCompletionDecision {
                activate_prepared: false,
                register_prepared: false,
                refresh_original_workspace: true,
                notice: PhaseThreadCompletionNotice::KnownOrphan,
            },
        };
    }

    match kind {
        PhaseThreadCompletionKind::Prepared => PhaseThreadCompletionDecision {
            activate_prepared: false,
            register_prepared: returned_request_matches && original_workspace_is_current,
            refresh_original_workspace: true,
            notice: PhaseThreadCompletionNotice::PreparedNotActivated,
        },
        PhaseThreadCompletionKind::IndeterminateFork => PhaseThreadCompletionDecision {
            activate_prepared: false,
            register_prepared: false,
            refresh_original_workspace: true,
            notice: PhaseThreadCompletionNotice::Indeterminate,
        },
        PhaseThreadCompletionKind::KnownChildRemaining => PhaseThreadCompletionDecision {
            activate_prepared: false,
            register_prepared: false,
            refresh_original_workspace: true,
            notice: PhaseThreadCompletionNotice::KnownOrphan,
        },
        PhaseThreadCompletionKind::DefinitiveForkFailure
        | PhaseThreadCompletionKind::CancelledBeforeFork
        | PhaseThreadCompletionKind::KnownChildCleaned
            if task_state == PhaseThreadPreparationTaskState::Cancelling =>
        {
            PhaseThreadCompletionDecision {
                activate_prepared: false,
                register_prepared: false,
                refresh_original_workspace: false,
                notice: PhaseThreadCompletionNotice::None,
            }
        }
        PhaseThreadCompletionKind::DefinitiveForkFailure
        | PhaseThreadCompletionKind::CancelledBeforeFork
        | PhaseThreadCompletionKind::KnownChildCleaned => PhaseThreadCompletionDecision {
            activate_prepared: false,
            register_prepared: false,
            refresh_original_workspace: false,
            notice: PhaseThreadCompletionNotice::Stale,
        },
    }
}

pub(crate) fn reduce_phase_thread_disconnect(
    _original_workspace_is_current: bool,
) -> PhaseThreadCompletionDecision {
    PhaseThreadCompletionDecision {
        activate_prepared: false,
        register_prepared: false,
        refresh_original_workspace: true,
        notice: PhaseThreadCompletionNotice::Indeterminate,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PhaseThreadContinuationStartDecision {
    Started,
    LeaveSelectedIdle,
}

pub(crate) fn reduce_phase_thread_continuation_start(
    started: bool,
) -> PhaseThreadContinuationStartDecision {
    if started {
        PhaseThreadContinuationStartDecision::Started
    } else {
        PhaseThreadContinuationStartDecision::LeaveSelectedIdle
    }
}
