use std::sync::{Arc, Mutex, Weak};

use beryl_model::{CasThreadId, CasTurnId};
use syndic_storage::{StopOperationTarget, TurnTerminalOutcome};

use super::{StopCoordinationError, StopCoordinator};
use crate::cas_projection::connection::StopTargetProof;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactStopFeedbackState {
    Waiting,
    DurableNondispatch,
    VolatileNondispatch,
    RequestNotAdmitted,
    Interrupted,
    Completed,
    Failed,
    UnknownTerminal,
    AuthorityLost,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactStopAttemptKind {
    AdmissionPending,
    Durable,
    Volatile,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactStopFeedbackSnapshot {
    pub revision: u64,
    pub state: ExactStopFeedbackState,
    pub attempt: ExactStopAttemptKind,
}

#[derive(Clone)]
pub struct ExactStopFeedback {
    pub(in crate::cas_projection) inner: Arc<FeedbackRecord>,
}

#[derive(Clone)]
pub(crate) struct ExactStopFeedbackIdentity(Weak<FeedbackRecord>);

impl ExactStopFeedbackIdentity {
    pub(crate) fn is_live(&self) -> bool {
        self.0.strong_count() != 0
    }
    pub(crate) fn matches(&self, feedback: &ExactStopFeedback) -> bool {
        self.0.ptr_eq(&Arc::downgrade(&feedback.inner))
    }
}

impl std::fmt::Debug for ExactStopFeedback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExactStopFeedback")
            .field("latest", &self.snapshot())
            .finish()
    }
}

impl PartialEq for ExactStopFeedback {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for ExactStopFeedback {}

impl ExactStopFeedback {
    pub(crate) fn weak_identity(&self) -> ExactStopFeedbackIdentity {
        ExactStopFeedbackIdentity(Arc::downgrade(&self.inner))
    }

    #[cfg(feature = "test-faults")]
    pub fn test_resolve_projected_feedback(&self, state: ExactStopFeedbackState) {
        assert_ne!(state, ExactStopFeedbackState::Waiting);
        self.inner.update(state, None);
    }
    pub fn operation_origin(&self) -> Option<crate::cas_projection::ExactOperationOrigin> {
        self.inner
            .origin
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub(in crate::cas_projection) fn associate_origin(
        &self,
        origin: crate::cas_projection::ExactOperationOrigin,
    ) {
        let mut current = self.inner.origin.lock().unwrap_or_else(|p| p.into_inner());
        if current.is_none() {
            *current = Some(origin);
        }
    }

    pub fn snapshot(&self) -> ExactStopFeedbackSnapshot {
        *self.inner.latest.lock().unwrap_or_else(|p| p.into_inner())
    }
}

pub enum ExactSoftStopAvailability {
    Eligible(ExactSoftStopEligibility),
    Unavailable(ExactSoftStopUnavailable),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactSoftStopUnavailable {
    NoExactTarget,
    RequestInProgress,
    AuthorityUnavailable,
}

#[derive(Clone)]
pub struct ExactSoftStopEligibility {
    pub(in crate::cas_projection) inner: Arc<EligibilityRecord>,
}

impl ExactSoftStopEligibility {
    pub fn operation_origin(&self) -> crate::cas_projection::ExactOperationOrigin {
        self.inner.origin.clone()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_projected_feedback(
        &self,
        state: ExactStopFeedbackState,
    ) -> Result<ExactStopFeedback, ExactStopRequestError> {
        let owner = self
            .inner
            .owner
            .upgrade()
            .ok_or(ExactStopRequestError::Revoked)?;
        let (feedback, _) =
            owner.reserve_feedback(&self.inner.target, &self.inner.proof, self.inner.epoch)?;
        feedback.associate_origin(self.inner.origin.clone());
        feedback.inner.update(state, None);
        Ok(feedback)
    }
}

pub(in crate::cas_projection) struct EligibilityRecord {
    pub(in crate::cas_projection) origin: crate::cas_projection::ExactOperationOrigin,
    pub(in crate::cas_projection) owner: Weak<StopCoordinator>,
    pub(in crate::cas_projection) target: StopOperationTarget,
    pub(in crate::cas_projection) proof: StopTargetProof,
    pub(in crate::cas_projection) epoch: u64,
    pub(in crate::cas_projection) consumed: Mutex<Option<Weak<FeedbackRecord>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum ExactStopRequestError {
    #[error("the exact soft-stop eligibility was revoked")]
    Revoked,
    #[error("the bounded exact-stop feedback capacity is exhausted")]
    Capacity,
}

#[derive(Debug)]
pub(in crate::cas_projection) struct FeedbackRecord {
    _presentation: Arc<()>,
    origin: Mutex<Option<crate::cas_projection::ExactOperationOrigin>>,
    target: StopOperationTarget,
    proof: StopTargetProof,
    latest: Mutex<ExactStopFeedbackSnapshot>,
}

impl FeedbackRecord {
    pub(super) fn matches_target(&self, target: &StopOperationTarget) -> bool {
        self.target == *target
    }

    pub(super) fn mark_durable(&self) {
        self.update(
            ExactStopFeedbackState::Waiting,
            Some(ExactStopAttemptKind::Durable),
        );
    }
    pub(in crate::cas_projection) fn update(
        &self,
        state: ExactStopFeedbackState,
        attempt: Option<ExactStopAttemptKind>,
    ) {
        let mut latest = self.latest.lock().unwrap_or_else(|p| p.into_inner());
        if latest.state != ExactStopFeedbackState::Waiting {
            return;
        }
        let next = ExactStopFeedbackSnapshot {
            revision: latest.revision,
            state,
            attempt: attempt.unwrap_or(latest.attempt),
        };
        if *latest != next {
            *latest = ExactStopFeedbackSnapshot {
                revision: latest
                    .revision
                    .checked_add(1)
                    .expect("bounded feedback transitions"),
                ..next
            };
        }
    }
}

impl StopCoordinator {
    pub(in crate::cas_projection) fn has_waiting_feedback(
        &self,
        target: &StopOperationTarget,
    ) -> bool {
        self.state
            .lock()
            .map(|state| {
                state
                    .feedback
                    .iter()
                    .filter_map(Weak::upgrade)
                    .any(|record| {
                        record.target == *target
                            && record
                                .latest
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .state
                                == ExactStopFeedbackState::Waiting
                    })
            })
            .unwrap_or(true)
    }
    pub(in crate::cas_projection) fn feedback_eligibility_epoch(
        &self,
    ) -> Result<u64, StopCoordinationError> {
        self.state
            .lock()
            .map(|state| state.feedback_epoch)
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)
    }

    pub(in crate::cas_projection) fn reserve_feedback(
        &self,
        target: &StopOperationTarget,
        proof: &StopTargetProof,
        epoch: u64,
    ) -> Result<(ExactStopFeedback, bool), ExactStopRequestError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ExactStopRequestError::Revoked)?;
        state.feedback.retain(|record| record.strong_count() != 0);
        for record in state.feedback.iter().filter_map(Weak::upgrade) {
            if record.target == *target
                && record.proof == *proof
                && record
                    .latest
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .state
                    == ExactStopFeedbackState::Waiting
            {
                return Ok((ExactStopFeedback { inner: record }, false));
            }
        }
        if state.feedback_epoch != epoch || state.persistent_failure.is_some() {
            return Err(ExactStopRequestError::Revoked);
        }
        let presentation = self
            .feedback_budget
            .reserve()
            .ok_or(ExactStopRequestError::Capacity)?;
        let next = state
            .feedback_epoch
            .checked_add(1)
            .ok_or(ExactStopRequestError::Revoked)?;
        let inner = Arc::new(FeedbackRecord {
            _presentation: presentation,
            origin: Mutex::new(None),
            target: target.clone(),
            proof: proof.clone(),
            latest: Mutex::new(ExactStopFeedbackSnapshot {
                revision: 0,
                state: ExactStopFeedbackState::Waiting,
                attempt: ExactStopAttemptKind::AdmissionPending,
            }),
        });
        state.feedback_epoch = next;
        state.feedback.push(Arc::downgrade(&inner));
        Ok((ExactStopFeedback { inner }, true))
    }

    pub(in crate::cas_projection) fn feedback_for_target(
        &self,
        target: &StopOperationTarget,
        outcome: ExactStopFeedbackState,
        attempt: Option<ExactStopAttemptKind>,
    ) {
        if let Ok(state) = self.state.lock() {
            for record in state.feedback.iter().filter_map(Weak::upgrade) {
                if record.target == *target {
                    record.update(outcome, attempt);
                }
            }
        }
    }

    pub(in crate::cas_projection) fn feedback_volatile_result(
        &self,
        witness: &crate::cas_projection::connection::PersistentFailureTargetWitness,
        result: crate::cas_projection::connection::PersistentFailureDriverResult,
    ) {
        use crate::cas_projection::connection::{
            PersistentFailureDriverResult as R, PersistentFailureInterruptDisposition as D,
        };
        let outcome = match result {
            R::NoDispatch(_)
            | R::Attempted {
                disposition: D::ProvenNotDispatched | D::RejectedBeforeCoreInterrupt,
                ..
            } => ExactStopFeedbackState::VolatileNondispatch,
            R::Attempted { .. } => ExactStopFeedbackState::Waiting,
        };
        if let Ok(state) = self.state.lock() {
            for record in state.feedback.iter().filter_map(Weak::upgrade) {
                if record.target.thread_id() == witness.syndic_thread_id()
                    && Some(record.target.turn_id()) == witness.syndic_turn_id()
                    && record.proof.loaded_generation() == witness.loaded_generation()
                    && record.proof.registration() == witness.registration()
                    && record.proof.connection_generation()
                        == witness.connection().connection_generation()
                    && record.target.cas_thread_id() == witness.cas_thread_id()
                    && Some(record.target.cas_turn_id()) == witness.cas_turn_id()
                    && record
                        .latest
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .attempt
                        == ExactStopAttemptKind::Volatile
                {
                    record.update(outcome, None);
                }
            }
        }
    }

    pub(in crate::cas_projection) fn feedback_passive_terminal(
        &self,
        connection: u64,
        thread: &CasThreadId,
        turn: &CasTurnId,
        status: beryl_backend::NormalTurnTerminalStatus,
    ) {
        let outcome = match status {
            beryl_backend::NormalTurnTerminalStatus::Completed => ExactStopFeedbackState::Completed,
            beryl_backend::NormalTurnTerminalStatus::Interrupted => {
                ExactStopFeedbackState::Interrupted
            }
            beryl_backend::NormalTurnTerminalStatus::Failed => ExactStopFeedbackState::Failed,
        };
        if let Ok(state) = self.state.lock() {
            if state.persistent_failure.is_none() {
                return;
            }
            for record in state.feedback.iter().filter_map(Weak::upgrade) {
                if record.proof.connection_generation() == connection
                    && record.target.cas_thread_id() == thread
                    && record.target.cas_turn_id() == turn
                {
                    record.update(outcome, None);
                }
            }
        }
    }

    pub(in crate::cas_projection) fn feedback_published_terminal(
        &self,
        permit: &crate::cas_projection::connection::SourcePublicationPermit,
        status: TurnTerminalOutcome,
    ) {
        let (connection, registration, loaded) = permit.stop_feedback_binding();
        if let Ok(state) = self.state.lock() {
            for record in state.feedback.iter().filter_map(Weak::upgrade) {
                if record.proof.connection_generation() == connection
                    && record.proof.registration() == registration
                    && record.proof.loaded_generation() == loaded
                    && record.target.thread_id() == permit.syndic_thread_id()
                    && record.target.cas_thread_id() == permit.cas_thread_id()
                    && record.target.cas_turn_id() == permit.cas_turn_id()
                {
                    record.update(terminal_state(status), None);
                }
            }
        }
    }

    pub(in crate::cas_projection) fn dispose_feedback(&self) {
        if let Ok(mut state) = self.state.lock() {
            for record in state.feedback.iter().filter_map(Weak::upgrade) {
                record.update(ExactStopFeedbackState::AuthorityLost, None);
            }
            state.feedback.clear();
            state.feedback_epoch = state.feedback_epoch.saturating_add(1);
        }
    }

    pub(in crate::cas_projection) fn feedback_connection_retired(&self, connection: u64) {
        if let Ok(state) = self.state.lock() {
            for record in state.feedback.iter().filter_map(Weak::upgrade) {
                if record.proof.connection_generation() == connection {
                    record.update(ExactStopFeedbackState::AuthorityLost, None);
                }
            }
        }
    }
}

fn terminal_state(outcome: TurnTerminalOutcome) -> ExactStopFeedbackState {
    match outcome {
        TurnTerminalOutcome::Interrupted => ExactStopFeedbackState::Interrupted,
        TurnTerminalOutcome::Complete => ExactStopFeedbackState::Completed,
        TurnTerminalOutcome::Failed => ExactStopFeedbackState::Failed,
        TurnTerminalOutcome::Incomplete | TurnTerminalOutcome::UnknownTerminal => {
            ExactStopFeedbackState::UnknownTerminal
        }
    }
}
