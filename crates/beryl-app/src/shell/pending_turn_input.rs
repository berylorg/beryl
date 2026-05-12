use beryl_backend::TurnStartOptions;
use beryl_model::workspace::WorkspaceId;

use super::execution_detail::UserInputFragment;

pub(super) const PENDING_TURN_INPUT_MAX_FRAGMENTS: usize = 64;
pub(super) const PENDING_TURN_INPUT_MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
pub(super) const PENDING_ACTIVE_TURN_STEERING_MAX_FRAGMENTS: usize = 64;
pub(super) const PENDING_ACTIVE_TURN_STEERING_MAX_PAYLOAD_BYTES: usize = 1024 * 1024;

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
pub(super) enum PendingInputAdmissionError {
    TooManyFragments { max_fragments: usize },
    TooManyBytes { max_bytes: usize },
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
    #[allow(dead_code)]
    pub(super) fn new(
        thread_id: String,
        execution_target: WorkspaceId,
        automatic_title_generation_allowed: bool,
        turn_options: TurnStartOptions,
        turn_index: usize,
        first_fragment: UserInputFragment,
    ) -> Self {
        Self::try_new(
            thread_id,
            execution_target,
            automatic_title_generation_allowed,
            turn_options,
            turn_index,
            first_fragment,
        )
        .expect("pending turn input queue constructor received over-budget first fragment")
    }

    pub(super) fn try_new(
        thread_id: String,
        execution_target: WorkspaceId,
        automatic_title_generation_allowed: bool,
        turn_options: TurnStartOptions,
        turn_index: usize,
        first_fragment: UserInputFragment,
    ) -> Result<Self, PendingInputAdmissionError> {
        validate_fragment_count(1, PENDING_TURN_INPUT_MAX_FRAGMENTS)?;
        let retained_bytes = queue_base_payload_bytes_lower_bound(&thread_id, &execution_target)
            .saturating_add(first_fragment.retained_payload_bytes_lower_bound());
        validate_payload_bytes(retained_bytes, PENDING_TURN_INPUT_MAX_PAYLOAD_BYTES)?;
        Ok(Self {
            thread_id,
            execution_target,
            automatic_title_generation_allowed,
            turn_options,
            turn_index,
            fragments: vec![first_fragment],
        })
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

    #[allow(dead_code)]
    pub(super) fn append(&mut self, fragment: UserInputFragment) -> usize {
        self.try_append(fragment)
            .expect("pending turn input queue append received over-budget fragment")
    }

    pub(super) fn try_append(
        &mut self,
        fragment: UserInputFragment,
    ) -> Result<usize, PendingInputAdmissionError> {
        self.validate_append(&fragment)?;
        self.fragments.push(fragment);
        Ok(self.fragments.len() - 1)
    }

    pub(super) fn validate_append(
        &self,
        fragment: &UserInputFragment,
    ) -> Result<(), PendingInputAdmissionError> {
        validate_fragment_count(
            self.fragments.len().saturating_add(1),
            PENDING_TURN_INPUT_MAX_FRAGMENTS,
        )?;
        validate_payload_bytes(
            self.payload_bytes_lower_bound()
                .saturating_add(fragment.retained_payload_bytes_lower_bound()),
            PENDING_TURN_INPUT_MAX_PAYLOAD_BYTES,
        )
    }

    pub(super) fn fragment_count(&self) -> usize {
        self.fragments.len()
    }

    pub(super) fn payload_bytes_lower_bound(&self) -> usize {
        queue_base_payload_bytes_lower_bound(&self.thread_id, &self.execution_target)
            .saturating_add(
                self.fragments
                    .iter()
                    .map(UserInputFragment::retained_payload_bytes_lower_bound)
                    .sum::<usize>(),
            )
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

pub(super) fn validate_pending_turn_input_fragments(
    existing: Option<&PendingTurnInputQueue>,
    thread_id: &str,
    execution_target: &WorkspaceId,
    automatic_title_generation_allowed: bool,
    turn_options: &TurnStartOptions,
    new_turn_index: usize,
    fragments: &[UserInputFragment],
) -> Result<bool, PendingInputAdmissionError> {
    let mut simulated = existing.cloned();
    let mut accepted_any = false;
    for fragment in fragments {
        match PendingTurnInputQueue::submission_plan(simulated.as_ref(), thread_id) {
            Some(PendingTurnInputSubmissionPlan::AppendToQueue { .. }) => {
                let Some(queue) = simulated.as_mut() else {
                    debug_assert!(false, "append plan without simulated pending queue");
                    return Ok(false);
                };
                queue.try_append(fragment.clone())?;
                accepted_any = true;
            }
            Some(PendingTurnInputSubmissionPlan::StartQueue) => {
                simulated = Some(PendingTurnInputQueue::try_new(
                    thread_id.to_string(),
                    execution_target.clone(),
                    automatic_title_generation_allowed,
                    turn_options.clone(),
                    new_turn_index,
                    fragment.clone(),
                )?);
                accepted_any = true;
            }
            None => return Ok(false),
        }
    }
    Ok(accepted_any)
}

impl PendingInputAdmissionError {
    pub(super) fn user_message(self) -> String {
        match self {
            Self::TooManyFragments { max_fragments } => {
                format!(
                    "Beryl can queue at most {max_fragments} pending input fragments for one turn."
                )
            }
            Self::TooManyBytes { max_bytes } => {
                format!(
                    "Beryl can queue at most {} MiB of pending input for one turn.",
                    max_bytes / (1024 * 1024)
                )
            }
        }
    }
}

fn queue_base_payload_bytes_lower_bound(thread_id: &str, execution_target: &WorkspaceId) -> usize {
    thread_id
        .len()
        .saturating_add(execution_target.runtime_mode().display_name().len())
        .saturating_add(execution_target.canonical_path().to_string_lossy().len())
}

fn validate_fragment_count(
    fragment_count: usize,
    max_fragments: usize,
) -> Result<(), PendingInputAdmissionError> {
    if fragment_count > max_fragments {
        return Err(PendingInputAdmissionError::TooManyFragments { max_fragments });
    }
    Ok(())
}

fn validate_payload_bytes(
    payload_bytes: usize,
    max_bytes: usize,
) -> Result<(), PendingInputAdmissionError> {
    if payload_bytes > max_bytes {
        return Err(PendingInputAdmissionError::TooManyBytes { max_bytes });
    }
    Ok(())
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

    #[allow(dead_code)]
    pub(super) fn append(&mut self, fragment: F) {
        self.fragments.push(fragment);
    }

    pub(super) fn try_append(
        &mut self,
        fragment: F,
        fragment_payload_bytes: impl Fn(&F) -> usize,
    ) -> Result<(), PendingInputAdmissionError> {
        self.validate_append(&fragment, fragment_payload_bytes)?;
        self.fragments.push(fragment);
        Ok(())
    }

    pub(super) fn validate_append(
        &self,
        fragment: &F,
        fragment_payload_bytes: impl Fn(&F) -> usize,
    ) -> Result<(), PendingInputAdmissionError> {
        validate_fragment_count(
            self.fragments.len().saturating_add(1),
            PENDING_ACTIVE_TURN_STEERING_MAX_FRAGMENTS,
        )?;
        let retained_bytes = self
            .payload_bytes_lower_bound(&fragment_payload_bytes)
            .saturating_add(fragment_payload_bytes(fragment));
        validate_payload_bytes(
            retained_bytes,
            PENDING_ACTIVE_TURN_STEERING_MAX_PAYLOAD_BYTES,
        )
    }

    pub(super) fn fragment_count(&self) -> usize {
        self.fragments.len()
    }

    pub(super) fn fragments(&self) -> &[F] {
        &self.fragments
    }

    pub(super) fn payload_bytes_lower_bound(
        &self,
        fragment_payload_bytes: impl Fn(&F) -> usize,
    ) -> usize {
        self.thread_id.len().saturating_add(
            self.fragments
                .iter()
                .map(fragment_payload_bytes)
                .sum::<usize>(),
        )
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
