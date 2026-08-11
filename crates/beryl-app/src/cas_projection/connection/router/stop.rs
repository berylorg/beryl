use std::{sync::Arc, time::Duration};

use beryl_model::{
    CasLoadedSessionGeneration, CasThreadId, CasTurnId, RuntimeId, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::StopOperationTarget;
use thiserror::Error;

use super::{
    ActiveStopElectionKey, EventRouter, TargetTurn, state::advance_revision, target::close_target,
};

/// Immutable router proof for the exact live target selected by a stop operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct StopTargetProof {
    pub(super) connection_generation: u64,
    pub(super) registration: u64,
    pub(super) runtime_id: RuntimeId,
    pub(super) loaded_generation: CasLoadedSessionGeneration,
    pub(super) syndic_thread_id: SyndicThreadId,
    pub(super) cas_thread_id: CasThreadId,
    pub(super) cas_turn_id: CasTurnId,
    pub(super) request_timeout: Duration,
}

impl StopTargetProof {
    pub(in crate::cas_projection) const fn syndic_thread_id(&self) -> SyndicThreadId {
        self.syndic_thread_id
    }

    pub(in crate::cas_projection) const fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    pub(in crate::cas_projection) fn matches(&self, target: &StopOperationTarget) -> bool {
        self.syndic_thread_id == target.thread_id()
            && self.runtime_id == target.runtime_id()
            && self.loaded_generation == target.loaded_generation()
            && self.cas_thread_id == *target.cas_thread_id()
            && self.cas_turn_id == *target.cas_turn_id()
    }
}

/// Non-cloneable ownership of the router's exact target-operation stop election.
pub(in crate::cas_projection) struct StopElectionPermit {
    router: Arc<EventRouter>,
    proof: Option<StopTargetProof>,
    token: u64,
    command: Option<crate::cas_projection::LiveCommandPermit>,
    finished: bool,
}

/// Result of revalidating the live-command gate after the exact stop election is held.
pub(in crate::cas_projection) enum StopElectionAdmission {
    Current {
        permit: StopElectionPermit,
        command: crate::cas_projection::LiveCommandPermit,
    },
    PersistentFailure(PersistentFailureStopAdmission),
    Closed,
}

/// Command-bound transfer that may preserve one exact pre-writer volatile admission proof.
pub(in crate::cas_projection) struct PersistentFailureStopAdmission {
    permit: StopElectionPermit,
    command: crate::cas_projection::LiveCommandPermit,
    failure_generation: crate::cas_projection::persistent_failure::PersistentFailureGeneration,
    syndic_turn_id: SyndicTurnId,
}

/// One non-cloneable exact failed-admission proof retained by the original target router.
#[derive(Debug)]
pub(super) struct VolatileStopAdmissionProof {
    pub(super) proof: StopTargetProof,
    pub(super) syndic_turn_id: SyndicTurnId,
    pub(super) token: u64,
    pub(super) service_generation:
        crate::cas_projection::persistent_failure::ProjectionServiceGeneration,
    pub(super) failure_generation:
        crate::cas_projection::persistent_failure::PersistentFailureGeneration,
}

impl StopElectionPermit {
    pub(in crate::cas_projection) fn admission(
        self,
        syndic_turn_id: SyndicTurnId,
    ) -> Result<StopElectionAdmission, crate::cas_projection::LiveCommandAdmissionError> {
        enum Disposition {
            Current,
            PersistentFailure(
                crate::cas_projection::persistent_failure::PersistentFailureGeneration,
            ),
            Closed,
        }

        let mut permit = self;
        let command = permit
            .command
            .take()
            .expect("an unsettled stop election retains its admitting live command");
        let disposition = command.commit_or_transfer_persistent_only(
            || Disposition::Current,
            Disposition::PersistentFailure,
            || Disposition::Closed,
        )?;
        Ok(match disposition {
            Disposition::Current => StopElectionAdmission::Current { permit, command },
            Disposition::PersistentFailure(failure_generation) => {
                StopElectionAdmission::PersistentFailure(PersistentFailureStopAdmission {
                    permit,
                    command,
                    failure_generation,
                    syndic_turn_id,
                })
            }
            Disposition::Closed => StopElectionAdmission::Closed,
        })
    }

    pub(in crate::cas_projection) fn finish(mut self) {
        self.router.finish_stop_election(&self);
        self.finished = true;
    }
}

impl PersistentFailureStopAdmission {
    pub(in crate::cas_projection) const fn syndic_turn_id(&self) -> SyndicTurnId {
        self.syndic_turn_id
    }

    pub(in crate::cas_projection) fn preserve(self) -> Result<(), StopElectionAcquireError> {
        let Self {
            permit,
            command,
            failure_generation,
            syndic_turn_id,
        } = self;
        let service_generation = command.service_generation();
        let result = permit.preserve_volatile_admission(
            service_generation,
            failure_generation,
            syndic_turn_id,
        );
        command.release_after_authority_loss();
        result
    }
}

impl Drop for StopElectionPermit {
    fn drop(&mut self) {
        if !self.finished {
            self.router.finish_stop_election(self);
        }
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub(in crate::cas_projection) enum StopElectionAcquireError {
    #[error("the exact stop target is missing or stale")]
    TargetMismatch,
    #[error("another exact target operation did not settle before the stop deadline")]
    Busy,
    #[error("the stop-election generation is exhausted")]
    GenerationExhausted,
    #[error("the live-event router is unavailable")]
    Router,
}

impl EventRouter {
    pub(in crate::cas_projection) fn approval_stop_target(
        &self,
        request: &beryl_backend::ApprovalRequest,
    ) -> Result<StopTargetProof, StopElectionAcquireError> {
        if !request.kind().separate_interruption_required() {
            return Err(StopElectionAcquireError::TargetMismatch);
        }
        let cas_thread_id = request
            .thread_id()
            .ok_or(StopElectionAcquireError::TargetMismatch)?;
        let cas_turn_id = request
            .turn_id()
            .ok_or(StopElectionAcquireError::TargetMismatch)?;
        let owner = {
            let state = self
                .state
                .lock()
                .map_err(|_| StopElectionAcquireError::Router)?;
            state
                .targets
                .get(cas_thread_id)
                .map(|target| target.owner)
                .ok_or(StopElectionAcquireError::TargetMismatch)?
        };
        self.stop_target(owner, cas_thread_id, cas_turn_id)
    }

    pub(in crate::cas_projection) fn stop_target(
        &self,
        syndic_thread_id: SyndicThreadId,
        cas_thread_id: &CasThreadId,
        cas_turn_id: &CasTurnId,
    ) -> Result<StopTargetProof, StopElectionAcquireError> {
        let state = self
            .state
            .lock()
            .map_err(|_| StopElectionAcquireError::Router)?;
        if self.failure_observed() || state.retired.is_some() || state.persistent_failure.is_some()
        {
            return Err(StopElectionAcquireError::Router);
        }
        let target = state
            .targets
            .get(cas_thread_id)
            .ok_or(StopElectionAcquireError::TargetMismatch)?;
        if target.owner != syndic_thread_id
            || target.key.runtime_id != self.runtime_id
            || target.loaded_generation.process() != self.process_generation
            || target.turn_state != TargetTurn::Exact
            || target.turn_id.as_ref() != Some(cas_turn_id)
            || !target.start_dispatched
            || !target.activation_durable
            || target.publication_closing.is_some()
            || target.loss_requested
        {
            return Err(StopElectionAcquireError::TargetMismatch);
        }
        Ok(StopTargetProof {
            connection_generation: self.connection_generation,
            registration: target.registration,
            runtime_id: self.runtime_id,
            loaded_generation: target.loaded_generation,
            syndic_thread_id,
            cas_thread_id: cas_thread_id.clone(),
            cas_turn_id: cas_turn_id.clone(),
            request_timeout: target.request_timeout,
        })
    }

    pub(in crate::cas_projection) fn acquire_stop_election(
        self: &Arc<Self>,
        command: crate::cas_projection::LiveCommandPermit,
        proof: &StopTargetProof,
    ) -> Result<StopElectionPermit, StopElectionAcquireError> {
        let deadline = std::time::Instant::now()
            .checked_add(proof.request_timeout)
            .ok_or(StopElectionAcquireError::Busy)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopElectionAcquireError::Router)?;
        command
            .commit_if_current(|| validate_proof(self, &state, proof))
            .unwrap_or(Err(StopElectionAcquireError::Router))?;
        drop(command);
        loop {
            if !self.commands.is_open()
                || state.retired.is_some()
                || state.persistent_failure.is_some()
            {
                return Err(StopElectionAcquireError::Router);
            }
            validate_proof(self, &state, proof)?;
            if state.active_steering_attempt.is_none()
                && state.active_stop_election.is_none()
                && state
                    .targets
                    .get(&proof.cas_thread_id)
                    .is_some_and(|target| target.publication_in_flight.is_none())
            {
                break;
            }
            #[cfg(test)]
            self.publish_stop_election_wait_for_test();
            let now = std::time::Instant::now();
            let Some(remaining) = deadline.checked_duration_since(now) else {
                return Err(StopElectionAcquireError::Busy);
            };
            let (next, wait) = self
                .publication_changed
                .wait_timeout(state, remaining)
                .map_err(|_| StopElectionAcquireError::Router)?;
            state = next;
            if wait.timed_out() {
                return Err(StopElectionAcquireError::Busy);
            }
        }
        let final_command = self
            .commands
            .authorize()
            .map_err(|_| StopElectionAcquireError::Router)?;
        let token = final_command
            .commit_if_current(|| {
                validate_proof(self, &state, proof)?;
                if state.active_steering_attempt.is_some()
                    || state.active_stop_election.is_some()
                    || state
                        .targets
                        .get(&proof.cas_thread_id)
                        .is_none_or(|target| target.publication_in_flight.is_some())
                {
                    return Err(StopElectionAcquireError::Busy);
                }
                state.next_stop_election = state
                    .next_stop_election
                    .checked_add(1)
                    .ok_or(StopElectionAcquireError::GenerationExhausted)?;
                let token = state.next_stop_election;
                state.active_stop_election = Some(ActiveStopElectionKey {
                    token,
                    thread_id: proof.cas_thread_id.clone(),
                    registration: proof.registration,
                });
                advance_revision(&mut state);
                Ok(token)
            })
            .unwrap_or(Err(StopElectionAcquireError::Router))?;
        Ok(StopElectionPermit {
            router: Arc::clone(self),
            proof: Some(proof.clone()),
            token,
            command: Some(final_command),
            finished: false,
        })
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn observe_next_stop_election_wait_for_test(
        &self,
        observer: std::sync::mpsc::SyncSender<()>,
    ) {
        *self
            .stop_election_wait_observer
            .lock()
            .expect("stop-election test observer mutex remains healthy") = Some(observer);
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn activate_stop_target_for_test(
        &self,
        cas_thread_id: &CasThreadId,
        cas_turn_id: CasTurnId,
    ) {
        let mut state = self.state.lock().expect("test router remains healthy");
        let target = state
            .targets
            .get_mut(cas_thread_id)
            .expect("test stop target remains registered");
        target.turn_state = TargetTurn::Exact;
        target.turn_id = Some(cas_turn_id);
        target.start_dispatched = true;
        target.activation_durable = true;
        advance_revision(&mut state);
    }

    #[cfg(test)]
    fn publish_stop_election_wait_for_test(&self) {
        let observer = self
            .stop_election_wait_observer
            .lock()
            .expect("stop-election test observer mutex remains healthy")
            .take();
        if let Some(observer) = observer {
            let _ = observer.send(());
        }
    }

    fn finish_stop_election(&self, permit: &StopElectionPermit) {
        let settlement =
            self.settle_long_lived_authority(|state| finish_stop_election_locked(state, permit));
        if let super::LongLivedAuthoritySettlement::Settled(settled) = settlement {
            self.publication_changed.notify_all();
            if settled {
                self.scheduler_signal.wake(
                    crate::cas_projection::accepted_input_scheduler::AcceptedInputWakeReason::AttemptReleased,
                );
            }
        }
    }

    fn preserve_volatile_stop_admission(
        &self,
        permit: &mut StopElectionPermit,
        service_generation: crate::cas_projection::persistent_failure::ProjectionServiceGeneration,
        failure_generation: crate::cas_projection::persistent_failure::PersistentFailureGeneration,
        syndic_turn_id: SyndicTurnId,
    ) -> Result<(), StopElectionAcquireError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopElectionAcquireError::Router)?;
        let proof = permit
            .proof
            .as_ref()
            .ok_or(StopElectionAcquireError::TargetMismatch)?;
        validate_proof(self, &state, proof)?;
        let election_matches = state.active_stop_election.as_ref().is_some_and(|active| {
            active.token == permit.token
                && active.thread_id == proof.cas_thread_id
                && active.registration == proof.registration
        });
        if !election_matches || state.volatile_stop_admission.is_some() {
            return Err(StopElectionAcquireError::TargetMismatch);
        }
        state.volatile_stop_admission = Some(VolatileStopAdmissionProof {
            proof: permit
                .proof
                .take()
                .expect("validated volatile admission retains its exact target"),
            syndic_turn_id,
            token: permit.token,
            service_generation,
            failure_generation,
        });
        state.active_stop_election = None;
        advance_revision(&mut state);
        permit.finished = true;
        drop(state);
        self.publication_changed.notify_all();
        Ok(())
    }
}

impl StopElectionPermit {
    fn preserve_volatile_admission(
        mut self,
        service_generation: crate::cas_projection::persistent_failure::ProjectionServiceGeneration,
        failure_generation: crate::cas_projection::persistent_failure::PersistentFailureGeneration,
        syndic_turn_id: SyndicTurnId,
    ) -> Result<(), StopElectionAcquireError> {
        let router = Arc::clone(&self.router);
        router.preserve_volatile_stop_admission(
            &mut self,
            service_generation,
            failure_generation,
            syndic_turn_id,
        )
    }
}

fn finish_stop_election_locked(
    state: &mut super::RouterState,
    permit: &StopElectionPermit,
) -> bool {
    let matches = state.active_stop_election.as_ref().is_some_and(|active| {
        let Some(proof) = permit.proof.as_ref() else {
            return false;
        };
        active.token == permit.token
            && active.thread_id == proof.cas_thread_id
            && active.registration == proof.registration
    });
    if !matches {
        return false;
    }
    state.active_stop_election = None;
    let deferred_close = state
        .targets
        .get(
            &permit
                .proof
                .as_ref()
                .expect("an unfinished stop election retains its target")
                .cas_thread_id,
        )
        .and_then(|target| {
            (target.turn_state != TargetTurn::Terminal && target.publication_in_flight.is_none())
                .then_some(target.publication_closing.or(state.retired))
                .flatten()
        });
    if let Some(reason) = deferred_close {
        close_target(
            state,
            &permit
                .proof
                .as_ref()
                .expect("an unfinished stop election retains its target")
                .cas_thread_id,
            reason,
        );
    } else {
        advance_revision(state);
    }
    true
}

fn validate_proof(
    router: &EventRouter,
    state: &super::RouterState,
    proof: &StopTargetProof,
) -> Result<(), StopElectionAcquireError> {
    if proof.connection_generation != router.connection_generation
        || proof.runtime_id != router.runtime_id
        || proof.loaded_generation.process() != router.process_generation
    {
        return Err(StopElectionAcquireError::TargetMismatch);
    }
    let target = state
        .targets
        .get(&proof.cas_thread_id)
        .ok_or(StopElectionAcquireError::TargetMismatch)?;
    if target.registration != proof.registration
        || target.owner != proof.syndic_thread_id
        || target.key.runtime_id != proof.runtime_id
        || target.loaded_generation != proof.loaded_generation
        || target.turn_state != TargetTurn::Exact
        || target.turn_id.as_ref() != Some(&proof.cas_turn_id)
        || !target.start_dispatched
        || !target.activation_durable
        || target.publication_closing.is_some()
        || target.loss_requested
    {
        return Err(StopElectionAcquireError::TargetMismatch);
    }
    Ok(())
}
