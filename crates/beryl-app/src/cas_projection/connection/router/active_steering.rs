use std::{sync::Arc, time::Duration};

use beryl_model::{CasLoadedSessionGeneration, CasThreadId, CasTurnId, SyndicThreadId};
use syndic_storage::SteeringTargetProof;

use super::{
    ActiveSteeringAttemptKey, EventRouter, LiveEventTargetCloseReason, RouterState,
    TargetAuthorizationFailure, TargetRegistrationProof, TargetTurn, state::advance_revision,
    target::close_target,
};

/// One connection-wide non-cloneable outbound steering attempt.
pub(in crate::cas_projection) struct ActiveSteeringAttemptPermit {
    pub(super) router: Arc<EventRouter>,
    pub(super) token: u64,
    pub(super) thread_id: CasThreadId,
    pub(super) turn_id: CasTurnId,
    pub(super) registration: u64,
    pub(super) owner: SyndicThreadId,
    pub(super) loaded_generation: CasLoadedSessionGeneration,
    pub(super) home_generation: u64,
    pub(super) command: Option<crate::cas_projection::LiveCommandPermit>,
    pub(super) finished: bool,
}

pub(in crate::cas_projection) struct ActiveSteeringCommandAuthorization {
    token: u64,
    thread_id: CasThreadId,
    registration: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum ActiveSteeringAttemptStatus {
    Active,
    Closed(LiveEventTargetCloseReason),
}

#[derive(Debug)]
pub(in crate::cas_projection) enum ActiveSteeringAttemptAcquireError {
    Busy,
    TargetClosed(LiveEventTargetCloseReason),
    TargetMismatch,
    GenerationExhausted,
    Router,
}

#[derive(Debug)]
pub(in crate::cas_projection) enum ActiveSteeringAttemptFinishError {
    Router,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum ActiveSteeringTargetLookupError {
    MissingOrStale,
    Router,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum ActiveSteeringAttemptFinishOutcome {
    Settled,
    TargetLossPending,
    ProvenTerminal,
}

impl ActiveSteeringAttemptPermit {
    pub(in crate::cas_projection) const fn syndic_thread_id(&self) -> SyndicThreadId {
        self.owner
    }

    pub(in crate::cas_projection) const fn cas_thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    pub(in crate::cas_projection) const fn cas_turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }

    pub(in crate::cas_projection) const fn loaded_generation(&self) -> CasLoadedSessionGeneration {
        self.loaded_generation
    }

    pub(in crate::cas_projection) fn home_generation(
        &self,
        home: &beryl_home_store::HomeStore,
    ) -> Option<beryl_home_store::HomeGeneration> {
        let health = home.health();
        (health.state() == beryl_home_store::HomeHealthState::Healthy)
            .then(|| health.generation())
            .flatten()
            .filter(|generation| generation.get() == self.home_generation)
    }

    pub(in crate::cas_projection) const fn home_generation_number(&self) -> u64 {
        self.home_generation
    }

    pub(in crate::cas_projection) fn matches_target(
        &self,
        owner: SyndicThreadId,
        target: &SteeringTargetProof,
        loaded_generation: CasLoadedSessionGeneration,
    ) -> bool {
        self.owner == owner
            && self.loaded_generation == loaded_generation
            && &self.thread_id == target.pending().cas_thread_id()
            && &self.turn_id == target.cas_turn_id()
    }

    pub(in crate::cas_projection) fn matches_delivering(
        &self,
        route: &syndic_storage::SyndicDeliveringSteeringInput,
    ) -> bool {
        self.matches_target(
            route.input().thread_id(),
            route.target(),
            route.loaded_generation(),
        )
    }

    pub(in crate::cas_projection) fn command_authorization(
        &self,
    ) -> ActiveSteeringCommandAuthorization {
        ActiveSteeringCommandAuthorization {
            token: self.token,
            thread_id: self.thread_id.clone(),
            registration: self.registration,
        }
    }

    pub(in crate::cas_projection) fn command_is_current(&self) -> bool {
        self.command
            .as_ref()
            .is_some_and(crate::cas_projection::LiveCommandPermit::is_current)
    }

    pub(in crate::cas_projection) fn observe_persistent_failure(&self) -> bool {
        let Some(command) = self.command.as_ref() else {
            return true;
        };
        let _ = command.observe_persistent_failure();
        !command.is_current()
    }

    pub(in crate::cas_projection) fn status(&self) -> ActiveSteeringAttemptStatus {
        self.router.active_steering_attempt_status(self)
    }

    pub(in crate::cas_projection) fn wait_for_status_change(
        &mut self,
        timeout: Duration,
    ) -> ActiveSteeringAttemptStatus {
        drop(self.command.take());
        self.router
            .wait_for_active_steering_attempt_status(self, timeout)
    }

    pub(in crate::cas_projection) fn finish(
        mut self,
    ) -> Result<ActiveSteeringAttemptFinishOutcome, ActiveSteeringAttemptFinishError> {
        let result = self.router.finish_active_steering_attempt(&self);
        self.finished = true;
        result
    }
}

impl Drop for ActiveSteeringAttemptPermit {
    fn drop(&mut self) {
        if !self.finished {
            self.router.fail_active_steering_attempt(self);
        }
    }
}

impl EventRouter {
    pub(in crate::cas_projection) fn active_steering_target_registration(
        &self,
        owner: SyndicThreadId,
        target_proof: &SteeringTargetProof,
        loaded_generation: CasLoadedSessionGeneration,
    ) -> Result<(TargetRegistrationProof, Duration), ActiveSteeringTargetLookupError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ActiveSteeringTargetLookupError::Router)?;
        if self.failure_observed() || state.retired.is_some() || state.persistent_failure.is_some()
        {
            return Err(ActiveSteeringTargetLookupError::MissingOrStale);
        }
        let Some(target) = state.targets.get(target_proof.pending().cas_thread_id()) else {
            return Err(ActiveSteeringTargetLookupError::MissingOrStale);
        };
        let registration = TargetRegistrationProof {
            registration: target.registration,
            key: target.key.clone(),
            owner: target.owner,
            loaded_generation: target.loaded_generation,
            terminal: Arc::clone(&target.terminal),
            loss_receipt: Arc::clone(&target.loss_receipt),
            compaction: target.compaction,
        };
        validate_target(target, &registration, target_proof, loaded_generation)
            .map_err(|_| ActiveSteeringTargetLookupError::MissingOrStale)?;
        if target.owner != owner {
            return Err(ActiveSteeringTargetLookupError::MissingOrStale);
        }
        Ok((registration, target.request_timeout))
    }

    pub(in crate::cas_projection) fn acquire_active_steering_attempt(
        self: &Arc<Self>,
        registration: &TargetRegistrationProof,
        target_proof: &SteeringTargetProof,
        loaded_generation: CasLoadedSessionGeneration,
    ) -> Result<ActiveSteeringAttemptPermit, ActiveSteeringAttemptAcquireError> {
        self.acquire_active_steering_attempt_inner(
            registration,
            target_proof,
            loaded_generation,
            false,
        )
    }

    pub(in crate::cas_projection) fn acquire_active_steering_attempt_or_arm(
        self: &Arc<Self>,
        registration: &TargetRegistrationProof,
        target_proof: &SteeringTargetProof,
        loaded_generation: CasLoadedSessionGeneration,
    ) -> Result<ActiveSteeringAttemptPermit, ActiveSteeringAttemptAcquireError> {
        self.acquire_active_steering_attempt_inner(
            registration,
            target_proof,
            loaded_generation,
            true,
        )
    }

    fn acquire_active_steering_attempt_inner(
        self: &Arc<Self>,
        registration: &TargetRegistrationProof,
        target_proof: &SteeringTargetProof,
        loaded_generation: CasLoadedSessionGeneration,
        arm_waiter: bool,
    ) -> Result<ActiveSteeringAttemptPermit, ActiveSteeringAttemptAcquireError> {
        let command = self
            .commands
            .authorize()
            .map_err(|_| ActiveSteeringAttemptAcquireError::Router)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| ActiveSteeringAttemptAcquireError::Router)?;
        let thread_id = &registration.key.cas_thread_id;
        let admission = command
            .commit_if_current(|| {
                if state.retired.is_some() || state.persistent_failure.is_some() {
                    return Err(ActiveSteeringAttemptAcquireError::Router);
                }
                if state.active_steering_attempt.is_some() || state.active_stop_election.is_some() {
                    if arm_waiter {
                        state.active_steering_attempt_waiter = true;
                    }
                    return Err(ActiveSteeringAttemptAcquireError::Busy);
                }
                let target = state
                    .targets
                    .get(thread_id)
                    .ok_or_else(|| closed_or_stopped(registration))?;
                validate_target(target, registration, target_proof, loaded_generation)?;
                let home_generation = target.home_generation;
                state.next_steering_attempt = state
                    .next_steering_attempt
                    .checked_add(1)
                    .ok_or(ActiveSteeringAttemptAcquireError::GenerationExhausted)?;
                let token = state.next_steering_attempt;
                state.active_steering_attempt = Some(ActiveSteeringAttemptKey {
                    token,
                    thread_id: thread_id.clone(),
                    registration: registration.registration,
                    command_dispatched: false,
                    loss_transferred: false,
                });
                advance_revision(&mut state);
                Ok((token, home_generation))
            })
            .unwrap_or(Err(ActiveSteeringAttemptAcquireError::Router))?;
        let permit = ActiveSteeringAttemptPermit {
            router: Arc::clone(self),
            token: admission.0,
            thread_id: thread_id.clone(),
            turn_id: target_proof.cas_turn_id().clone(),
            registration: registration.registration,
            owner: registration.owner,
            loaded_generation,
            home_generation: admission.1,
            command: Some(command),
            finished: false,
        };
        Ok(permit)
    }

    pub(in crate::cas_projection) fn authorize_active_steering_command(
        &self,
        command: &crate::cas_projection::LiveCommandPermit,
        authorization: &ActiveSteeringCommandAuthorization,
    ) -> Result<(), TargetAuthorizationFailure> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| TargetAuthorizationFailure::Router)?;
        command
            .commit_if_current(|| {
                if state.retired.is_some() || state.persistent_failure.is_some() {
                    return Err(TargetAuthorizationFailure::Router);
                }
                let Some(active) = state.active_steering_attempt.as_ref() else {
                    return Err(TargetAuthorizationFailure::Target(
                        LiveEventTargetCloseReason::SourcePublicationRouteUnavailable,
                    ));
                };
                if active.token != authorization.token
                    || active.thread_id != authorization.thread_id
                    || active.registration != authorization.registration
                    || active.command_dispatched
                    || active.loss_transferred
                {
                    return Err(TargetAuthorizationFailure::Target(
                        LiveEventTargetCloseReason::SourcePublicationRouteUnavailable,
                    ));
                }
                let target = state.targets.get(&authorization.thread_id).ok_or(
                    TargetAuthorizationFailure::Target(LiveEventTargetCloseReason::WorkerStopped),
                )?;
                if target.publication_closing.is_some() || target.loss_requested {
                    return Err(TargetAuthorizationFailure::Target(
                        target.publication_closing.unwrap_or(
                            LiveEventTargetCloseReason::SourcePublicationRouteUnavailable,
                        ),
                    ));
                }
                state
                    .active_steering_attempt
                    .as_mut()
                    .expect("validated steering attempt remains active")
                    .command_dispatched = true;
                advance_revision(&mut state);
                Ok(())
            })
            .unwrap_or(Err(TargetAuthorizationFailure::Router))
    }

    fn active_steering_attempt_status(
        &self,
        permit: &ActiveSteeringAttemptPermit,
    ) -> ActiveSteeringAttemptStatus {
        let state = match self.state.lock() {
            Ok(state) => state,
            Err(poison) => poison.into_inner(),
        };
        attempt_status(self, &state, permit)
    }

    fn wait_for_active_steering_attempt_status(
        &self,
        permit: &ActiveSteeringAttemptPermit,
        timeout: Duration,
    ) -> ActiveSteeringAttemptStatus {
        let state = match self.state.lock() {
            Ok(state) => state,
            Err(poison) => poison.into_inner(),
        };
        let status = attempt_status(self, &state, permit);
        if status != ActiveSteeringAttemptStatus::Active {
            return status;
        }
        let (state, _) = self
            .publication_changed
            .wait_timeout(state, timeout)
            .unwrap_or_else(|poison| poison.into_inner());
        attempt_status(self, &state, permit)
    }

    fn finish_active_steering_attempt(
        &self,
        permit: &ActiveSteeringAttemptPermit,
    ) -> Result<ActiveSteeringAttemptFinishOutcome, ActiveSteeringAttemptFinishError> {
        let settlement =
            self.settle_long_lived_authority(|state| finish_active_steering_locked(state, permit));
        let (outcome, wake_scheduler) = match settlement {
            super::LongLivedAuthoritySettlement::Settled(Ok(settled)) => settled,
            super::LongLivedAuthoritySettlement::Settled(Err(error)) => {
                self.publication_changed.notify_all();
                return Err(error);
            }
            super::LongLivedAuthoritySettlement::PreservedForPersistentFailure
            | super::LongLivedAuthoritySettlement::Unavailable => {
                return Err(ActiveSteeringAttemptFinishError::Router);
            }
        };
        self.publication_changed.notify_all();
        if wake_scheduler {
            self.scheduler_signal.wake(
                crate::cas_projection::accepted_input_scheduler::AcceptedInputWakeReason::AttemptReleased,
            );
        }
        Ok(outcome)
    }

    fn fail_active_steering_attempt(&self, permit: &ActiveSteeringAttemptPermit) {
        let settlement =
            self.settle_long_lived_authority(|state| fail_active_steering_locked(state, permit));
        let super::LongLivedAuthoritySettlement::Settled(Some(wake_scheduler)) = settlement else {
            return;
        };
        self.publication_changed.notify_all();
        if wake_scheduler {
            self.scheduler_signal.wake(
                crate::cas_projection::accepted_input_scheduler::AcceptedInputWakeReason::AttemptReleased,
            );
        }
    }
}

fn finish_active_steering_locked(
    state: &mut RouterState,
    permit: &ActiveSteeringAttemptPermit,
) -> Result<(ActiveSteeringAttemptFinishOutcome, bool), ActiveSteeringAttemptFinishError> {
    if !matching_attempt(state, permit) {
        return Err(ActiveSteeringAttemptFinishError::Router);
    }
    let wake_scheduler = std::mem::take(&mut state.active_steering_attempt_waiter);
    let outcome = state
        .targets
        .get(&permit.thread_id)
        .map(|target| {
            if target.turn_state == TargetTurn::Terminal {
                ActiveSteeringAttemptFinishOutcome::ProvenTerminal
            } else if target.loss_requested
                || target
                    .publication_closing
                    .or(state.retired)
                    .is_some_and(|reason| reason != LiveEventTargetCloseReason::ReceiverAbandoned)
            {
                ActiveSteeringAttemptFinishOutcome::TargetLossPending
            } else {
                ActiveSteeringAttemptFinishOutcome::Settled
            }
        })
        .ok_or(ActiveSteeringAttemptFinishError::Router)?;
    state.active_steering_attempt = None;
    let deferred_close = state.targets.get(&permit.thread_id).and_then(|target| {
        (target.turn_state != TargetTurn::Terminal && !target.loss_requested)
            .then_some(target.publication_closing.or(state.retired))
            .flatten()
    });
    if let Some(reason) = deferred_close {
        close_target(state, &permit.thread_id, reason);
    } else {
        advance_revision(state);
    }
    Ok((outcome, wake_scheduler))
}

fn fail_active_steering_locked(
    state: &mut RouterState,
    permit: &ActiveSteeringAttemptPermit,
) -> Option<bool> {
    if !matching_attempt(state, permit) {
        return None;
    }
    let wake_scheduler = std::mem::take(&mut state.active_steering_attempt_waiter);
    state.active_steering_attempt = None;
    if state
        .targets
        .get(&permit.thread_id)
        .is_some_and(|target| target.turn_state == TargetTurn::Terminal)
    {
        // An exact proven-terminal source owner is stronger than cleanup of an abandoned
        // steering permit. Preserve its handoff and wake any loss waiter to observe it.
        advance_revision(state);
    } else {
        close_target(
            state,
            &permit.thread_id,
            LiveEventTargetCloseReason::SourcePublicationFailed,
        );
    }
    Some(wake_scheduler)
}

fn validate_target(
    target: &super::TargetEntry,
    registration: &TargetRegistrationProof,
    proof: &SteeringTargetProof,
    loaded_generation: CasLoadedSessionGeneration,
) -> Result<(), ActiveSteeringAttemptAcquireError> {
    if target.registration != registration.registration
        || target.owner != registration.owner
        || target.loaded_generation != registration.loaded_generation
    {
        return Err(ActiveSteeringAttemptAcquireError::TargetMismatch);
    }
    if target.publication_closing.is_some()
        || target.publication_in_flight.is_some()
        || target.loss_requested
    {
        return Err(ActiveSteeringAttemptAcquireError::TargetClosed(
            target
                .publication_closing
                .unwrap_or(LiveEventTargetCloseReason::SourcePublicationRouteUnavailable),
        ));
    }
    if target.turn_state != TargetTurn::Exact
        || target.turn_id.as_ref() != Some(proof.cas_turn_id())
        || !target.start_dispatched
        || !target.activation_durable
        || target.loaded_generation != loaded_generation
        || &target.key.cas_thread_id != proof.pending().cas_thread_id()
    {
        return Err(ActiveSteeringAttemptAcquireError::TargetMismatch);
    }
    let activation = target
        .pending_activation
        .as_ref()
        .ok_or(ActiveSteeringAttemptAcquireError::TargetMismatch)?;
    if activation.thread_id() != target.owner
        || activation.turn_id() != proof.pending().active_turn_id()
        || activation.binding_revision() != proof.pending().binding_revision()
        || activation.snapshot_id() != proof.pending().snapshot_id()
    {
        return Err(ActiveSteeringAttemptAcquireError::TargetMismatch);
    }
    Ok(())
}

pub(super) fn matching_attempt(state: &RouterState, permit: &ActiveSteeringAttemptPermit) -> bool {
    state
        .active_steering_attempt
        .as_ref()
        .is_some_and(|active| {
            active.token == permit.token
                && active.thread_id == permit.thread_id
                && active.registration == permit.registration
        })
}

fn attempt_status(
    router: &EventRouter,
    state: &RouterState,
    permit: &ActiveSteeringAttemptPermit,
) -> ActiveSteeringAttemptStatus {
    if router.failure_observed() || state.persistent_failure.is_some() {
        return ActiveSteeringAttemptStatus::Closed(LiveEventTargetCloseReason::StreamFailure);
    }
    if !matching_attempt(state, permit) {
        return ActiveSteeringAttemptStatus::Closed(
            LiveEventTargetCloseReason::SourcePublicationRouteUnavailable,
        );
    }
    let Some(target) = state.targets.get(&permit.thread_id) else {
        return ActiveSteeringAttemptStatus::Closed(LiveEventTargetCloseReason::WorkerStopped);
    };
    let delayed_lifecycle_in_flight = target.publication_in_flight
        == Some(super::TargetPublication::DelayedSteering)
        && state
            .active_steering_attempt
            .as_ref()
            .is_some_and(|attempt| !attempt.loss_transferred);
    if delayed_lifecycle_in_flight {
        return ActiveSteeringAttemptStatus::Active;
    }
    if let Some(reason) = state.retired.or(target.publication_closing) {
        return ActiveSteeringAttemptStatus::Closed(reason);
    }
    if target.loss_requested {
        return ActiveSteeringAttemptStatus::Closed(
            LiveEventTargetCloseReason::SourcePublicationRouteUnavailable,
        );
    }
    ActiveSteeringAttemptStatus::Active
}

fn closed_or_stopped(registration: &TargetRegistrationProof) -> ActiveSteeringAttemptAcquireError {
    ActiveSteeringAttemptAcquireError::TargetClosed(
        registration
            .terminal_reason()
            .unwrap_or(LiveEventTargetCloseReason::WorkerStopped),
    )
}
