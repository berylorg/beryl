use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

use beryl_backend::{
    CheckedSteeringUserMessage, ClientUserMessageId, SteeringUserMessageSelection,
    UserMessageEchoLifecycle,
};
use beryl_home_store::HomeGeneration;
use beryl_model::CasItemId;
use syndic_storage::SyndicDeliveringSteeringInput;

use super::BROKER_POLL_INTERVAL;
use crate::cas_projection::connection::router::{
    ActiveSteeringAttemptPermit, ActiveSteeringAttemptStatus, LiveEventTargetCloseReason,
};

/// One checked delayed lifecycle at the app-owned boundary before route disposition.
#[derive(Debug)]
pub(in crate::cas_projection) struct CheckedSteeringLifecycle {
    route: SyndicDeliveringSteeringInput,
    home_generation: HomeGeneration,
    message: CheckedSteeringUserMessage,
}

impl CheckedSteeringLifecycle {
    pub(super) const fn new(
        route: SyndicDeliveringSteeringInput,
        home_generation: HomeGeneration,
        message: CheckedSteeringUserMessage,
    ) -> Self {
        Self {
            route,
            home_generation,
            message,
        }
    }

    pub(in crate::cas_projection) const fn route(&self) -> &SyndicDeliveringSteeringInput {
        &self.route
    }

    pub(in crate::cas_projection) const fn home_generation(&self) -> HomeGeneration {
        self.home_generation
    }

    pub(in crate::cas_projection) const fn message(&self) -> &CheckedSteeringUserMessage {
        &self.message
    }

    pub(super) fn into_message(self) -> CheckedSteeringUserMessage {
        self.message
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SteeringSequenceProof {
    owner_id: u64,
    route: SyndicDeliveringSteeringInput,
    home_generation: HomeGeneration,
    item_id: CasItemId,
    correlation: ClientUserMessageId,
}

impl SteeringSequenceProof {
    pub(super) fn new(
        owner_id: u64,
        route: SyndicDeliveringSteeringInput,
        home_generation: HomeGeneration,
        selection: &SteeringUserMessageSelection,
    ) -> Self {
        Self {
            owner_id,
            route,
            home_generation,
            item_id: selection.item_id().clone(),
            correlation: selection.client_user_message_id().clone(),
        }
    }

    pub(super) const fn route(&self) -> &SyndicDeliveringSteeringInput {
        &self.route
    }

    pub(super) const fn home_generation(&self) -> HomeGeneration {
        self.home_generation
    }

    pub(super) const fn item_id(&self) -> &CasItemId {
        &self.item_id
    }

    pub(super) const fn correlation(&self) -> &ClientUserMessageId {
        &self.correlation
    }

    fn matches_checked(&self, result: &CheckedSteeringLifecycle) -> bool {
        &self.route == result.route()
            && self.home_generation == result.home_generation()
            && self.item_id() == result.message().item_id()
            && self.correlation() == result.message().client_user_message_id()
    }

    fn matches_selection(
        &self,
        route: &SyndicDeliveringSteeringInput,
        home_generation: HomeGeneration,
        selection: &SteeringUserMessageSelection,
    ) -> bool {
        &self.route == route
            && self.home_generation == home_generation
            && &self.item_id == selection.item_id()
            && &self.correlation == selection.client_user_message_id()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SteeringArmProof {
    owner_id: u64,
    route: SyndicDeliveringSteeringInput,
    home_generation: HomeGeneration,
    correlation: ClientUserMessageId,
}

impl SteeringArmProof {
    fn matches_selection(
        &self,
        route: &SyndicDeliveringSteeringInput,
        home_generation: HomeGeneration,
        selection: &SteeringUserMessageSelection,
    ) -> bool {
        &self.route == route
            && self.home_generation == home_generation
            && &self.correlation == selection.client_user_message_id()
    }
}

enum SteeringSequence {
    Idle,
    Armed(SteeringArmProof),
    Sealed(SteeringArmProof),
    SelectedStarted(SteeringArmProof),
    AwaitingCompleted(SteeringSequenceProof),
    SelectedCompleted(SteeringSequenceProof),
    Completed(SteeringSequenceProof),
}

struct SteeringResultState {
    next_owner_id: u64,
    sequence: SteeringSequence,
    ready: VecDeque<CheckedSteeringLifecycle>,
    closed: bool,
}

impl Default for SteeringResultState {
    fn default() -> Self {
        Self {
            next_owner_id: 0,
            sequence: SteeringSequence::Idle,
            ready: VecDeque::with_capacity(2),
            closed: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SteeringSelectionReservationError {
    Closed,
    CapacityFull,
    InvalidSequence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum CheckedSteeringLifecycleArmError {
    Busy,
    Closed,
    GenerationExhausted,
    TargetMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum CheckedSteeringLifecycleWaitError {
    BrokerClosed,
    LifecycleAlreadyReserved,
    LifecycleMissing,
    OwnershipLost,
    Sealed,
    TargetClosed(LiveEventTargetCloseReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecycleOwnerPhase {
    AwaitingStarted,
    AwaitingCompleted,
    Sealed,
}

/// Sole outbound owner of one exact armed Started-to-Completed lifecycle sequence.
pub(in crate::cas_projection) struct CheckedSteeringLifecycleOwner {
    slot: Arc<SteeringResultSlot>,
    owner_id: u64,
    phase: LifecycleOwnerPhase,
    released: bool,
}

/// Bounded two-result cell plus the compact Started-to-Completed proof.
#[derive(Default)]
pub(super) struct SteeringResultSlot {
    state: Mutex<SteeringResultState>,
    changed: Condvar,
}

impl SteeringResultSlot {
    pub(super) fn arm(
        self: &Arc<Self>,
        route: SyndicDeliveringSteeringInput,
        home_generation: HomeGeneration,
        correlation: ClientUserMessageId,
    ) -> Result<CheckedSteeringLifecycleOwner, CheckedSteeringLifecycleArmError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.closed {
            return Err(CheckedSteeringLifecycleArmError::Closed);
        }
        if !matches!(state.sequence, SteeringSequence::Idle) || !state.ready.is_empty() {
            return Err(CheckedSteeringLifecycleArmError::Busy);
        }
        state.next_owner_id = state
            .next_owner_id
            .checked_add(1)
            .ok_or(CheckedSteeringLifecycleArmError::GenerationExhausted)?;
        let owner_id = state.next_owner_id;
        state.sequence = SteeringSequence::Armed(SteeringArmProof {
            owner_id,
            route,
            home_generation,
            correlation,
        });
        Ok(CheckedSteeringLifecycleOwner {
            slot: Arc::clone(self),
            owner_id,
            phase: LifecycleOwnerPhase::AwaitingStarted,
            released: false,
        })
    }

    pub(super) fn reserve(
        &self,
        route: &SyndicDeliveringSteeringInput,
        home_generation: HomeGeneration,
        selection: &SteeringUserMessageSelection,
    ) -> Result<SteeringSequenceProof, SteeringSelectionReservationError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.closed {
            return Err(SteeringSelectionReservationError::Closed);
        }
        if state.ready.len() >= 2 {
            return Err(SteeringSelectionReservationError::CapacityFull);
        }
        match (&state.sequence, selection.lifecycle()) {
            (SteeringSequence::Armed(arm), UserMessageEchoLifecycle::Started)
                if arm.matches_selection(route, home_generation, selection) =>
            {
                let arm = arm.clone();
                let proof = SteeringSequenceProof::new(
                    arm.owner_id,
                    route.clone(),
                    home_generation,
                    selection,
                );
                state.sequence = SteeringSequence::SelectedStarted(arm);
                Ok(proof)
            }
            (
                SteeringSequence::AwaitingCompleted(expected),
                UserMessageEchoLifecycle::Completed,
            ) if expected.matches_selection(route, home_generation, selection) => {
                let proof = expected.clone();
                state.sequence = SteeringSequence::SelectedCompleted(proof.clone());
                Ok(proof)
            }
            _ => Err(SteeringSelectionReservationError::InvalidSequence),
        }
    }

    /// Called only from the router's exact-target linearization callback.
    pub(super) fn publish(&self, result: CheckedSteeringLifecycle, proof: SteeringSequenceProof) {
        let lifecycle = result.message().lifecycle();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        assert!(
            state.ready.len() < 2,
            "bounded steering-result sequence was overfilled"
        );
        match (&state.sequence, lifecycle) {
            (SteeringSequence::SelectedStarted(arm), UserMessageEchoLifecycle::Started)
                if arm.owner_id == proof.owner_id =>
            {
                state.sequence = SteeringSequence::AwaitingCompleted(proof);
            }
            (
                SteeringSequence::SelectedCompleted(expected),
                UserMessageEchoLifecycle::Completed,
            ) if expected == &proof => {
                state.sequence = SteeringSequence::Completed(proof);
            }
            _ => panic!("checked steering lifecycle disagreed with its reservation"),
        }
        state.ready.push_back(result);
        drop(state);
        self.changed.notify_all();
    }

    pub(super) fn abandon_selection(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.closed = true;
        state.sequence = match std::mem::replace(&mut state.sequence, SteeringSequence::Idle) {
            SteeringSequence::SelectedStarted(arm) => SteeringSequence::Armed(arm),
            SteeringSequence::SelectedCompleted(proof) => {
                SteeringSequence::AwaitingCompleted(proof)
            }
            sequence => sequence,
        };
        drop(state);
        self.changed.notify_all();
    }

    #[cfg(test)]
    pub(super) fn take(&self) -> Option<CheckedSteeringLifecycle> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let result = state.ready.pop_front();
        if let Some(result) = &result {
            let proof = match &state.sequence {
                SteeringSequence::AwaitingCompleted(proof) | SteeringSequence::Completed(proof) => {
                    proof
                }
                _ => panic!("ready checked steering lifecycle has no sequence proof"),
            };
            assert!(
                proof.matches_checked(result),
                "ready checked steering lifecycle disagreed with its sequence proof"
            );
        }
        drop(state);
        if result.is_some() {
            self.changed.notify_all();
        }
        result
    }

    pub(super) fn has_ready(&self) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        debug_assert!(match &state.sequence {
            SteeringSequence::AwaitingCompleted(proof)
            | SteeringSequence::SelectedCompleted(proof)
            | SteeringSequence::Completed(proof) => {
                state
                    .ready
                    .iter()
                    .all(|result| proof.matches_checked(result))
            }
            _ => state.ready.is_empty(),
        });
        !state.ready.is_empty()
    }

    pub(super) fn wait_while_ready(&self, timeout: Duration) {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if !state.ready.is_empty() {
            drop(
                self.changed
                    .wait_timeout(state, timeout)
                    .unwrap_or_else(|poison| poison.into_inner()),
            );
        }
    }

    /// Seals the armed no-lifecycle branch after the driver proves nondispatch.
    pub(super) fn seal_armed_after_proven_nondispatch(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let SteeringSequence::Armed(arm) = &state.sequence else {
            return;
        };
        debug_assert!(state.ready.is_empty());
        state.sequence = SteeringSequence::Sealed(arm.clone());
        drop(state);
        self.changed.notify_all();
    }

    pub(super) fn close(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.closed = true;
        drop(state);
        self.changed.notify_all();
    }
}

impl CheckedSteeringLifecycleOwner {
    pub(in crate::cas_projection) fn wait_started(
        &mut self,
        attempt: &ActiveSteeringAttemptPermit,
    ) -> Result<CheckedSteeringLifecycle, CheckedSteeringLifecycleWaitError> {
        if self.phase != LifecycleOwnerPhase::AwaitingStarted {
            return Err(CheckedSteeringLifecycleWaitError::OwnershipLost);
        }
        let result = self.wait_for(UserMessageEchoLifecycle::Started, attempt)?;
        self.phase = LifecycleOwnerPhase::AwaitingCompleted;
        Ok(result)
    }

    pub(in crate::cas_projection) fn wait_completed(
        &mut self,
        attempt: &ActiveSteeringAttemptPermit,
    ) -> Result<CheckedSteeringLifecycle, CheckedSteeringLifecycleWaitError> {
        if self.phase != LifecycleOwnerPhase::AwaitingCompleted {
            return Err(CheckedSteeringLifecycleWaitError::OwnershipLost);
        }
        self.wait_for(UserMessageEchoLifecycle::Completed, attempt)
    }

    /// Excludes a Started reservation before selecting a no-lifecycle disposition.
    pub(in crate::cas_projection) fn seal_without_lifecycle(
        &mut self,
    ) -> Result<(), CheckedSteeringLifecycleWaitError> {
        if self.phase != LifecycleOwnerPhase::AwaitingStarted {
            return Err(CheckedSteeringLifecycleWaitError::OwnershipLost);
        }
        let mut state = self
            .slot
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if sequence_has_owner(&state.sequence, self.owner_id)
            && (!state.ready.is_empty()
                || matches!(
                    state.sequence,
                    SteeringSequence::SelectedStarted(_)
                        | SteeringSequence::AwaitingCompleted(_)
                        | SteeringSequence::SelectedCompleted(_)
                        | SteeringSequence::Completed(_)
                ))
        {
            return Err(CheckedSteeringLifecycleWaitError::LifecycleAlreadyReserved);
        }
        if matches!(
            &state.sequence,
            SteeringSequence::Sealed(arm) if arm.owner_id == self.owner_id
        ) {
            self.phase = LifecycleOwnerPhase::Sealed;
            return Ok(());
        }
        if state.closed {
            return Err(CheckedSteeringLifecycleWaitError::BrokerClosed);
        }
        let SteeringSequence::Armed(arm) = &state.sequence else {
            return Err(CheckedSteeringLifecycleWaitError::OwnershipLost);
        };
        if arm.owner_id != self.owner_id {
            return Err(CheckedSteeringLifecycleWaitError::OwnershipLost);
        }
        state.sequence = SteeringSequence::Sealed(arm.clone());
        self.phase = LifecycleOwnerPhase::Sealed;
        drop(state);
        self.slot.changed.notify_all();
        Ok(())
    }

    pub(in crate::cas_projection) fn release_after_disposition(
        mut self,
    ) -> Result<(), CheckedSteeringLifecycleWaitError> {
        let mut state = self
            .slot
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let releasable = match (&state.sequence, self.phase) {
            (SteeringSequence::Sealed(arm), LifecycleOwnerPhase::Sealed) => {
                arm.owner_id == self.owner_id
            }
            (SteeringSequence::Completed(proof), LifecycleOwnerPhase::AwaitingCompleted) => {
                proof.owner_id == self.owner_id && state.ready.is_empty()
            }
            _ => false,
        };
        if !releasable {
            return Err(CheckedSteeringLifecycleWaitError::LifecycleMissing);
        }
        state.sequence = SteeringSequence::Idle;
        self.released = true;
        drop(state);
        self.slot.changed.notify_all();
        Ok(())
    }

    pub(in crate::cas_projection) fn release_after_loss(
        mut self,
    ) -> Result<(), CheckedSteeringLifecycleWaitError> {
        let mut state = self
            .slot
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        while sequence_selection_in_flight(&state.sequence, self.owner_id) && !state.closed {
            state = self
                .slot
                .changed
                .wait(state)
                .unwrap_or_else(|poison| poison.into_inner());
        }
        if !sequence_has_owner(&state.sequence, self.owner_id) {
            return Err(CheckedSteeringLifecycleWaitError::OwnershipLost);
        }
        state.ready.clear();
        state.sequence = SteeringSequence::Idle;
        self.released = true;
        drop(state);
        self.slot.changed.notify_all();
        Ok(())
    }

    fn wait_for(
        &self,
        expected: UserMessageEchoLifecycle,
        attempt: &ActiveSteeringAttemptPermit,
    ) -> Result<CheckedSteeringLifecycle, CheckedSteeringLifecycleWaitError> {
        loop {
            let mut state = self
                .slot
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if let Some(result) = take_expected_ready(&mut state, self.owner_id, expected)? {
                drop(state);
                self.slot.changed.notify_all();
                return Ok(result);
            }
            if !sequence_waits_for(&state.sequence, self.owner_id, expected) {
                return Err(if matches!(&state.sequence, SteeringSequence::Sealed(_)) {
                    CheckedSteeringLifecycleWaitError::Sealed
                } else {
                    CheckedSteeringLifecycleWaitError::LifecycleMissing
                });
            }
            if state.closed {
                return Err(CheckedSteeringLifecycleWaitError::BrokerClosed);
            }
            let (state_after_wait, _) = self
                .slot
                .changed
                .wait_timeout(state, BROKER_POLL_INTERVAL)
                .unwrap_or_else(|poison| poison.into_inner());
            drop(state_after_wait);
            match attempt.status() {
                ActiveSteeringAttemptStatus::Active => {}
                ActiveSteeringAttemptStatus::Closed(reason) => {
                    let mut state = self
                        .slot
                        .state
                        .lock()
                        .unwrap_or_else(|poison| poison.into_inner());
                    if let Some(result) = take_expected_ready(&mut state, self.owner_id, expected)?
                    {
                        drop(state);
                        self.slot.changed.notify_all();
                        return Ok(result);
                    }
                    return Err(CheckedSteeringLifecycleWaitError::TargetClosed(reason));
                }
            }
        }
    }
}

impl Drop for CheckedSteeringLifecycleOwner {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        let mut state = self
            .slot
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if sequence_has_owner(&state.sequence, self.owner_id) {
            state.closed = true;
        }
        drop(state);
        self.slot.changed.notify_all();
    }
}

fn ready_proof(sequence: &SteeringSequence) -> Option<&SteeringSequenceProof> {
    match sequence {
        SteeringSequence::AwaitingCompleted(proof)
        | SteeringSequence::SelectedCompleted(proof)
        | SteeringSequence::Completed(proof) => Some(proof),
        _ => None,
    }
}

fn take_expected_ready(
    state: &mut SteeringResultState,
    owner_id: u64,
    expected: UserMessageEchoLifecycle,
) -> Result<Option<CheckedSteeringLifecycle>, CheckedSteeringLifecycleWaitError> {
    let Some(result) = state.ready.front() else {
        return Ok(None);
    };
    let proof =
        ready_proof(&state.sequence).ok_or(CheckedSteeringLifecycleWaitError::OwnershipLost)?;
    if proof.owner_id != owner_id
        || result.message().lifecycle() != expected
        || !proof.matches_checked(result)
    {
        return Err(CheckedSteeringLifecycleWaitError::OwnershipLost);
    }
    Ok(state.ready.pop_front())
}

fn sequence_has_owner(sequence: &SteeringSequence, owner_id: u64) -> bool {
    match sequence {
        SteeringSequence::Armed(arm)
        | SteeringSequence::Sealed(arm)
        | SteeringSequence::SelectedStarted(arm) => arm.owner_id == owner_id,
        SteeringSequence::AwaitingCompleted(proof)
        | SteeringSequence::SelectedCompleted(proof)
        | SteeringSequence::Completed(proof) => proof.owner_id == owner_id,
        SteeringSequence::Idle => false,
    }
}

fn sequence_selection_in_flight(sequence: &SteeringSequence, owner_id: u64) -> bool {
    match sequence {
        SteeringSequence::SelectedStarted(arm) => arm.owner_id == owner_id,
        SteeringSequence::SelectedCompleted(proof) => proof.owner_id == owner_id,
        _ => false,
    }
}

fn sequence_waits_for(
    sequence: &SteeringSequence,
    owner_id: u64,
    lifecycle: UserMessageEchoLifecycle,
) -> bool {
    match (sequence, lifecycle) {
        (
            SteeringSequence::Armed(arm) | SteeringSequence::SelectedStarted(arm),
            UserMessageEchoLifecycle::Started,
        ) => arm.owner_id == owner_id,
        (
            SteeringSequence::AwaitingCompleted(proof)
            | SteeringSequence::SelectedCompleted(proof)
            | SteeringSequence::Completed(proof),
            UserMessageEchoLifecycle::Completed,
        ) => proof.owner_id == owner_id,
        _ => false,
    }
}
