use beryl_backend::TurnStartOptions;
use beryl_model::workspace::WorkspaceId;

use super::execution_detail::UserInputFragment;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingTurnInputQueue {
    thread_id: String,
    execution_target: WorkspaceId,
    automatic_title_generation_allowed: bool,
    turn_options: TurnStartOptions,
    turn_index: usize,
    fragments: Vec<UserInputFragment>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PendingTurnInputSubmissionPlan {
    StartQueue,
    AppendToQueue {
        turn_index: usize,
        fragment_index: usize,
    },
}

impl PendingTurnInputQueue {
    pub(super) fn new(
        thread_id: String,
        execution_target: WorkspaceId,
        automatic_title_generation_allowed: bool,
        turn_options: TurnStartOptions,
        turn_index: usize,
        first_fragment: UserInputFragment,
    ) -> Self {
        Self {
            thread_id,
            execution_target,
            automatic_title_generation_allowed,
            turn_options,
            turn_index,
            fragments: vec![first_fragment],
        }
    }

    pub(super) fn thread_id(&self) -> &str {
        self.thread_id.as_str()
    }

    pub(super) fn execution_target(&self) -> &WorkspaceId {
        &self.execution_target
    }

    pub(super) fn automatic_title_generation_allowed(&self) -> bool {
        self.automatic_title_generation_allowed
    }

    pub(super) fn turn_options(&self) -> &TurnStartOptions {
        &self.turn_options
    }

    pub(super) fn turn_index(&self) -> usize {
        self.turn_index
    }

    pub(super) fn append(&mut self, fragment: UserInputFragment) -> usize {
        self.fragments.push(fragment);
        self.fragments.len() - 1
    }

    pub(super) fn into_fragments(self) -> Vec<UserInputFragment> {
        self.fragments
    }

    pub(super) fn is_for_thread(&self, thread_id: &str) -> bool {
        self.thread_id == thread_id
    }

    pub(super) fn submission_plan(
        existing: Option<&Self>,
        thread_id: &str,
    ) -> Option<PendingTurnInputSubmissionPlan> {
        match existing {
            Some(queue) if queue.is_for_thread(thread_id) => {
                Some(PendingTurnInputSubmissionPlan::AppendToQueue {
                    turn_index: queue.turn_index,
                    fragment_index: queue.fragments.len(),
                })
            }
            Some(_) => None,
            None => Some(PendingTurnInputSubmissionPlan::StartQueue),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingActiveTurnSteeringQueue<F> {
    thread_id: String,
    turn_index: usize,
    fragments: Vec<F>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PendingActiveTurnSteeringSubmissionPlan {
    StartQueue,
    AppendToQueue,
}

impl<F> PendingActiveTurnSteeringQueue<F> {
    pub(super) fn new(thread_id: String, turn_index: usize, first_fragment: F) -> Self {
        Self {
            thread_id,
            turn_index,
            fragments: vec![first_fragment],
        }
    }

    pub(super) fn append(&mut self, fragment: F) {
        self.fragments.push(fragment);
    }

    pub(super) fn is_for_turn(&self, thread_id: &str, turn_index: usize) -> bool {
        self.thread_id == thread_id && self.turn_index == turn_index
    }

    pub(super) fn into_fragments(self) -> Vec<F> {
        self.fragments
    }

    pub(super) fn submission_plan(
        existing: Option<&Self>,
        thread_id: &str,
        turn_index: usize,
    ) -> Option<PendingActiveTurnSteeringSubmissionPlan> {
        match existing {
            Some(queue) if queue.is_for_turn(thread_id, turn_index) => {
                Some(PendingActiveTurnSteeringSubmissionPlan::AppendToQueue)
            }
            Some(_) => None,
            None => Some(PendingActiveTurnSteeringSubmissionPlan::StartQueue),
        }
    }
}
