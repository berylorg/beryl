use std::sync::Arc;

use beryl_model::{CasLoadedSessionGeneration, CasThreadId, CasTurnId, SyndicThreadId};

use super::{
    EventRouter, LiveEventTargetCloseReason, TargetInvalidation, TargetPublication, TargetTurn,
    state::advance_revision,
    target::{close_target, invalidation, retire_router_state},
};

/// Non-cloneable authority for one delayed steering lifecycle publication.
pub(in crate::cas_projection) struct DelayedSteeringLifecyclePermit {
    router: Arc<EventRouter>,
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    registration: u64,
    owner: SyndicThreadId,
    loaded_generation: CasLoadedSessionGeneration,
    home_generation: u64,
    finished: bool,
}

impl DelayedSteeringLifecyclePermit {
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

    pub(in crate::cas_projection) fn matches_target(
        &self,
        owner: SyndicThreadId,
        target: &syndic_storage::SteeringTargetProof,
        loaded_generation: CasLoadedSessionGeneration,
    ) -> bool {
        self.owner == owner
            && self.loaded_generation == loaded_generation
            && &self.thread_id == target.pending().cas_thread_id()
            && &self.turn_id == target.cas_turn_id()
    }

    /// Publishes the caller's in-memory result at the exact router publication point.
    pub(in crate::cas_projection) fn finish_with(
        mut self,
        publish: impl FnOnce(),
    ) -> Result<(), DelayedSteeringLifecycleFinishError> {
        let result = self
            .router
            .finish_delayed_steering_lifecycle(&self, publish);
        self.finished = true;
        result
    }

    pub(in crate::cas_projection) fn fail(
        mut self,
    ) -> Result<TargetInvalidation, DelayedSteeringLifecycleFinishError> {
        let result = self.router.fail_delayed_steering_lifecycle(&self);
        self.finished = true;
        result
    }
}

impl Drop for DelayedSteeringLifecyclePermit {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.router.fail_delayed_steering_lifecycle(self);
        }
    }
}

#[derive(Debug)]
pub(in crate::cas_projection) enum DelayedSteeringLifecyclePermitError {
    Unmatched,
    Target(TargetInvalidation),
    Router,
}

#[derive(Debug)]
pub(in crate::cas_projection) enum DelayedSteeringLifecycleFinishError {
    Target(TargetInvalidation),
    Router,
}

impl EventRouter {
    pub(in crate::cas_projection) fn acquire_delayed_steering_lifecycle(
        self: &Arc<Self>,
        command: &crate::cas_projection::LiveCommandPermit,
        thread_id: &CasThreadId,
        turn_id: &CasTurnId,
    ) -> Result<DelayedSteeringLifecyclePermit, DelayedSteeringLifecyclePermitError> {
        use DelayedSteeringLifecyclePermitError::{Router, Target, Unmatched};

        let mut state = self.state.lock().map_err(|_| Router)?;
        let admission = command
            .commit_if_current(|| {
                if state.retired.is_some() || state.persistent_failure.is_some() {
                    return Err(Router);
                }
                let Some(target) = state.targets.get_mut(thread_id) else {
                    return Err(Unmatched);
                };
                if target.loss_requested {
                    return Err(Unmatched);
                }
                let close_reason = match target.turn_state {
                    TargetTurn::AwaitingStart if !target.start_dispatched => {
                        Some(LiveEventTargetCloseReason::EventBeforeTurnStart)
                    }
                    TargetTurn::AwaitingCompactionTurn => {
                        Some(LiveEventTargetCloseReason::SourcePublicationRouteUnavailable)
                    }
                    TargetTurn::AwaitingStart => {
                        Some(LiveEventTargetCloseReason::TurnActivationPublicationFailed)
                    }
                    TargetTurn::Terminal => {
                        Some(LiveEventTargetCloseReason::EventAfterTurnCompletion)
                    }
                    TargetTurn::Exact if target.turn_id.as_ref() != Some(turn_id) => {
                        Some(LiveEventTargetCloseReason::ConflictingTurnIdentity)
                    }
                    TargetTurn::Exact if !target.start_dispatched || !target.activation_durable => {
                        Some(LiveEventTargetCloseReason::TurnActivationPublicationFailed)
                    }
                    TargetTurn::Exact
                        if target.publication_closing.is_some()
                            || target.publication_in_flight.is_some() =>
                    {
                        Some(LiveEventTargetCloseReason::SourcePublicationRouteUnavailable)
                    }
                    TargetTurn::Exact => None,
                };
                if let Some(reason) = close_reason {
                    let invalidation = invalidation(target, reason);
                    close_target(&mut state, thread_id, reason);
                    return Err(Target(invalidation));
                }
                target.publication_in_flight = Some(TargetPublication::DelayedSteering);
                let admission = (
                    target.registration,
                    target.owner,
                    target.loaded_generation,
                    target.home_generation,
                );
                advance_revision(&mut state);
                Ok(admission)
            })
            .unwrap_or(Err(Router))?;
        let permit = DelayedSteeringLifecyclePermit {
            router: Arc::clone(self),
            thread_id: thread_id.clone(),
            turn_id: turn_id.clone(),
            registration: admission.0,
            owner: admission.1,
            loaded_generation: admission.2,
            home_generation: admission.3,
            finished: false,
        };
        Ok(permit)
    }

    fn finish_delayed_steering_lifecycle(
        &self,
        permit: &DelayedSteeringLifecyclePermit,
        publish: impl FnOnce(),
    ) -> Result<(), DelayedSteeringLifecycleFinishError> {
        let settlement = self.settle_long_lived_authority(|state| {
            finish_delayed_steering_locked(state, permit, publish)
        });
        match settlement {
            super::LongLivedAuthoritySettlement::Settled(result) => {
                self.publication_changed.notify_all();
                result
            }
            super::LongLivedAuthoritySettlement::PreservedForPersistentFailure
            | super::LongLivedAuthoritySettlement::Unavailable => {
                Err(DelayedSteeringLifecycleFinishError::Router)
            }
        }
    }

    fn fail_delayed_steering_lifecycle(
        &self,
        permit: &DelayedSteeringLifecyclePermit,
    ) -> Result<TargetInvalidation, DelayedSteeringLifecycleFinishError> {
        let settlement =
            self.settle_long_lived_authority(|state| fail_delayed_steering_locked(state, permit));
        match settlement {
            super::LongLivedAuthoritySettlement::Settled(result) => {
                self.publication_changed.notify_all();
                result
            }
            super::LongLivedAuthoritySettlement::PreservedForPersistentFailure
            | super::LongLivedAuthoritySettlement::Unavailable => {
                Err(DelayedSteeringLifecycleFinishError::Router)
            }
        }
    }
}

fn finish_delayed_steering_locked(
    state: &mut super::RouterState,
    permit: &DelayedSteeringLifecyclePermit,
    publish: impl FnOnce(),
) -> Result<(), DelayedSteeringLifecycleFinishError> {
    if !valid_permit_target(state, permit) {
        clear_matching_publication(state, permit);
        retire_router_state(state, LiveEventTargetCloseReason::StreamFailure);
        return Err(DelayedSteeringLifecycleFinishError::Router);
    }
    let target = state
        .targets
        .get(&permit.thread_id)
        .expect("validated delayed-steering target remains registered");
    let protected_by_active_attempt =
        state
            .active_steering_attempt
            .as_ref()
            .is_some_and(|attempt| {
                attempt.thread_id == permit.thread_id
                    && attempt.registration == permit.registration
                    && !attempt.loss_transferred
            });
    let failure = if let Some(reason) = target
        .publication_closing
        .filter(|_| !protected_by_active_attempt)
    {
        Some(reason)
    } else if target.loss_requested && !protected_by_active_attempt {
        Some(LiveEventTargetCloseReason::SourcePublicationRouteUnavailable)
    } else {
        match target.turn_state {
            TargetTurn::AwaitingStart | TargetTurn::AwaitingCompactionTurn => {
                Some(LiveEventTargetCloseReason::TurnActivationPublicationFailed)
            }
            TargetTurn::Terminal => Some(LiveEventTargetCloseReason::EventAfterTurnCompletion),
            TargetTurn::Exact if target.turn_id.as_ref() != Some(&permit.turn_id) => {
                Some(LiveEventTargetCloseReason::ConflictingTurnIdentity)
            }
            TargetTurn::Exact if !target.start_dispatched || !target.activation_durable => {
                Some(LiveEventTargetCloseReason::TurnActivationPublicationFailed)
            }
            TargetTurn::Exact => None,
        }
    };
    if let Some(reason) = failure {
        let invalidation = {
            let target = state
                .targets
                .get_mut(&permit.thread_id)
                .expect("validated delayed-steering target remains registered");
            let invalidation = invalidation(target, reason);
            target.publication_in_flight = None;
            invalidation
        };
        close_target(state, &permit.thread_id, reason);
        return Err(DelayedSteeringLifecycleFinishError::Target(invalidation));
    }
    publish();
    state
        .targets
        .get_mut(&permit.thread_id)
        .expect("stable delayed-steering target remains registered")
        .publication_in_flight = None;
    advance_revision(state);
    Ok(())
}

fn fail_delayed_steering_locked(
    state: &mut super::RouterState,
    permit: &DelayedSteeringLifecyclePermit,
) -> Result<TargetInvalidation, DelayedSteeringLifecycleFinishError> {
    if !valid_permit_target(state, permit) {
        clear_matching_publication(state, permit);
        retire_router_state(state, LiveEventTargetCloseReason::StreamFailure);
        return Err(DelayedSteeringLifecycleFinishError::Router);
    }
    let (reason, invalidation) = {
        let target = state
            .targets
            .get_mut(&permit.thread_id)
            .expect("validated delayed-steering target remains registered");
        let reason = target
            .publication_closing
            .unwrap_or(LiveEventTargetCloseReason::SourcePublicationFailed);
        let invalidation = invalidation(target, reason);
        target.publication_in_flight = None;
        (reason, invalidation)
    };
    close_target(state, &permit.thread_id, reason);
    Ok(invalidation)
}

fn valid_permit_target(
    state: &super::RouterState,
    permit: &DelayedSteeringLifecyclePermit,
) -> bool {
    state.targets.get(&permit.thread_id).is_some_and(|target| {
        target.registration == permit.registration
            && target.owner == permit.owner
            && target.loaded_generation == permit.loaded_generation
            && target.home_generation == permit.home_generation
            && target.publication_in_flight == Some(TargetPublication::DelayedSteering)
    })
}

fn clear_matching_publication(
    state: &mut super::RouterState,
    permit: &DelayedSteeringLifecyclePermit,
) {
    if let Some(target) = state.targets.get_mut(&permit.thread_id)
        && target.registration == permit.registration
        && target.publication_in_flight == Some(TargetPublication::DelayedSteering)
    {
        target.publication_in_flight = None;
    }
}
