use std::sync::{Arc, atomic::Ordering};

use beryl_model::{CasLoadedSessionGeneration, CasThreadId, CasTurnId, SyndicThreadId};

use super::{
    ActiveSteeringAttemptPermit, EventRouter, LiveEventTargetCloseReason, PendingTurnActivation,
    ProvenTerminalOutcome, TargetInvalidation, TargetPublication, TargetRegistrationProof,
    TargetTerminalSignal, TargetTurn,
    state::advance_revision,
    target::{fence_thread_lane, invalidation, set_terminal},
};

pub(in crate::cas_projection) enum TargetLossAcquisition {
    Authority(TargetLossPublicationAuthority),
    Incomplete,
    ProvenTerminal(ProvenTerminalOutcome),
}

#[derive(Debug)]
pub(in crate::cas_projection) enum TargetLossRequestError {
    TargetClosed,
    Router,
}

#[derive(Debug)]
pub(in crate::cas_projection) enum TargetLossFinishError {
    Router,
}

/// Sole non-cloneable authority to converge one abnormal target loss.
pub(in crate::cas_projection) struct TargetLossPublicationAuthority {
    router: Arc<EventRouter>,
    cas_thread_id: CasThreadId,
    cas_turn_id: Option<CasTurnId>,
    registration: u64,
    owner: SyndicThreadId,
    loaded_generation: CasLoadedSessionGeneration,
    home_generation: u64,
    activation: Option<PendingTurnActivation>,
    activation_durable: bool,
    steering_token: Option<u64>,
    finished: bool,
}

impl TargetLossPublicationAuthority {
    #[allow(clippy::too_many_arguments)]
    fn new(
        router: Arc<EventRouter>,
        cas_thread_id: CasThreadId,
        cas_turn_id: Option<CasTurnId>,
        registration: u64,
        owner: SyndicThreadId,
        loaded_generation: CasLoadedSessionGeneration,
        home_generation: u64,
        activation: Option<PendingTurnActivation>,
        activation_durable: bool,
        steering_token: Option<u64>,
    ) -> Self {
        Self {
            router,
            cas_thread_id,
            cas_turn_id,
            registration,
            owner,
            loaded_generation,
            home_generation,
            activation,
            activation_durable,
            steering_token,
            finished: false,
        }
    }

    pub(in crate::cas_projection) const fn syndic_thread_id(&self) -> SyndicThreadId {
        self.owner
    }

    pub(in crate::cas_projection) const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }

    pub(in crate::cas_projection) const fn cas_turn_id(&self) -> Option<&CasTurnId> {
        self.cas_turn_id.as_ref()
    }

    pub(in crate::cas_projection) const fn loaded_generation(&self) -> CasLoadedSessionGeneration {
        self.loaded_generation
    }

    pub(in crate::cas_projection) const fn activation(&self) -> &PendingTurnActivation {
        self.activation
            .as_ref()
            .expect("ordinary loss authority retains pending activation")
    }

    pub(in crate::cas_projection) const fn activation_durable(&self) -> bool {
        self.activation_durable
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

    pub(in crate::cas_projection) fn active_turn_request(
        &self,
    ) -> Option<syndic_storage::PublishActiveCasTurn> {
        self.cas_turn_id.as_ref().and_then(|turn_id| {
            self.activation.as_ref().map(|activation| {
                activation.active_turn(self.cas_thread_id.clone(), turn_id.clone())
            })
        })
    }

    pub(in crate::cas_projection) fn activation_event(
        &self,
        published_gate_revision: beryl_model::InputGateRevision,
    ) -> Result<Option<syndic_storage::LiveSourceEvent>, syndic_storage::SyndicRecordError> {
        self.cas_turn_id
            .as_ref()
            .zip(self.activation.as_ref())
            .map(|(turn_id, activation)| {
                activation.activation_event(
                    self.cas_thread_id.clone(),
                    turn_id.clone(),
                    published_gate_revision,
                )
            })
            .transpose()
    }

    pub(in crate::cas_projection) fn finish(
        mut self,
    ) -> Result<(TargetInvalidation, bool), TargetLossFinishError> {
        let result = self.router.finish_target_loss(&self, true);
        self.finished = true;
        result
    }

    pub(in crate::cas_projection) fn fail(
        mut self,
    ) -> Result<(TargetInvalidation, bool), TargetLossFinishError> {
        let result = self.router.finish_target_loss(&self, false);
        self.finished = true;
        result
    }
}

impl Drop for TargetLossPublicationAuthority {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.router.finish_target_loss(self, false);
        }
    }
}

impl ActiveSteeringAttemptPermit {
    /// Atomically replaces this connection-wide attempt with exact loss-publication authority.
    pub(in crate::cas_projection) fn transfer_to_target_loss(
        mut self,
    ) -> Result<TargetLossPublicationAuthority, TargetLossRequestError> {
        let router = Arc::clone(&self.router);
        let command = self.command.take().ok_or(TargetLossRequestError::Router)?;
        let result = router.transfer_active_steering_to_loss(command, &self);
        if result.is_ok() {
            self.finished = true;
        }
        result
    }
}

impl EventRouter {
    #[cfg(test)]
    pub(in crate::cas_projection) fn target_loss_requested_for_test(
        &self,
        registration: &TargetRegistrationProof,
    ) -> bool {
        self.state
            .lock()
            .expect("router state is usable")
            .targets
            .get(&registration.key.cas_thread_id)
            .is_some_and(|target| {
                target.registration == registration.registration && target.loss_requested
            })
    }

    pub(in crate::cas_projection) fn acquire_target_loss(
        self: &Arc<Self>,
        command: crate::cas_projection::LiveCommandPermit,
        registration: &TargetRegistrationProof,
    ) -> Result<TargetLossAcquisition, TargetLossRequestError> {
        if registration.loss_was_published() {
            return Ok(TargetLossAcquisition::Incomplete);
        }
        let thread_id = &registration.key.cas_thread_id;
        let mut state = self
            .state
            .lock()
            .map_err(|_| TargetLossRequestError::Router)?;
        let requested = command
            .commit_if_current(|| {
                if state.persistent_failure.is_some() {
                    return Err(TargetLossRequestError::Router);
                }
                let Some(target) = state.targets.get_mut(thread_id) else {
                    return if registration.loss_was_published() {
                        Ok(false)
                    } else {
                        Err(TargetLossRequestError::TargetClosed)
                    };
                };
                if target.registration != registration.registration
                    || target.owner != registration.owner
                    || target.loaded_generation != registration.loaded_generation
                {
                    return Err(TargetLossRequestError::TargetClosed);
                }
                if target.loss_requested {
                    return Ok(false);
                }
                target.loss_requested = true;
                advance_revision(&mut state);
                Ok(true)
            })
            .unwrap_or(Err(TargetLossRequestError::Router))?;
        if requested {
            self.publication_changed.notify_all();
        }
        drop(command);
        loop {
            if !self.commands.is_open() || state.persistent_failure.is_some() {
                return Err(TargetLossRequestError::Router);
            }
            let Some(target) = state.targets.get(thread_id) else {
                return if registration.loss_was_published() {
                    Ok(TargetLossAcquisition::Incomplete)
                } else {
                    Err(TargetLossRequestError::TargetClosed)
                };
            };
            if target.registration != registration.registration
                || target.owner != registration.owner
                || target.loaded_generation != registration.loaded_generation
            {
                return if registration.loss_was_published() {
                    Ok(TargetLossAcquisition::Incomplete)
                } else {
                    Err(TargetLossRequestError::TargetClosed)
                };
            }
            let steering_attempt_in_flight =
                state
                    .active_steering_attempt
                    .as_ref()
                    .is_some_and(|attempt| {
                        attempt.registration == registration.registration
                            && &attempt.thread_id == thread_id
                    });
            if target.publication_in_flight.is_none() && !steering_attempt_in_flight {
                break;
            }
            state = self
                .publication_changed
                .wait(state)
                .map_err(|_| TargetLossRequestError::Router)?;
        }
        let target = state
            .targets
            .get(thread_id)
            .expect("loss target remains registered after publication wait");
        let terminal = if target.turn_state == TargetTurn::Terminal {
            Some(
                *target
                    .terminal
                    .lock()
                    .map_err(|_| TargetLossRequestError::Router)?,
            )
        } else {
            None
        };
        let activation = target.pending_activation.clone();
        if terminal.is_none() && activation.is_none() && target.compaction.is_none() {
            return Err(TargetLossRequestError::TargetClosed);
        }
        let final_command = self
            .commands
            .authorize()
            .map_err(|_| TargetLossRequestError::Router)?;
        final_command
            .commit_if_current(|| {
                let target = state
                    .targets
                    .get_mut(thread_id)
                    .ok_or(TargetLossRequestError::TargetClosed)?;
                if target.registration != registration.registration
                    || target.owner != registration.owner
                    || target.loaded_generation != registration.loaded_generation
                    || !target.loss_requested
                    || target.publication_in_flight.is_some()
                {
                    return Err(TargetLossRequestError::TargetClosed);
                }
                if let Some(terminal) = terminal {
                    target.loss_requested = false;
                    return match terminal {
                        TargetTerminalSignal::Proven(outcome) => {
                            Ok(TargetLossAcquisition::ProvenTerminal(outcome))
                        }
                        TargetTerminalSignal::Open | TargetTerminalSignal::Closed(_) => {
                            Err(TargetLossRequestError::Router)
                        }
                    };
                }
                target.publication_in_flight = Some(TargetPublication::Loss);
                let authority = TargetLossPublicationAuthority::new(
                    Arc::clone(self),
                    thread_id.clone(),
                    target.turn_id.clone(),
                    target.registration,
                    target.owner,
                    target.loaded_generation,
                    target.home_generation,
                    activation,
                    target.activation_durable,
                    None,
                );
                advance_revision(&mut state);
                Ok(TargetLossAcquisition::Authority(authority))
            })
            .unwrap_or(Err(TargetLossRequestError::Router))
    }

    fn transfer_active_steering_to_loss(
        self: &Arc<Self>,
        command: crate::cas_projection::LiveCommandPermit,
        permit: &ActiveSteeringAttemptPermit,
    ) -> Result<TargetLossPublicationAuthority, TargetLossRequestError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| TargetLossRequestError::Router)?;
        let requested = command
            .commit_if_current(|| {
                if state.persistent_failure.is_some()
                    || !super::active_steering::matching_attempt(&state, permit)
                {
                    return Err(TargetLossRequestError::Router);
                }
                let target_matches = state.targets.get(&permit.thread_id).is_some_and(|target| {
                    target.registration == permit.registration
                        && target.owner == permit.owner
                        && target.loaded_generation == permit.loaded_generation
                        && target.home_generation == permit.home_generation
                });
                if !target_matches {
                    return Err(TargetLossRequestError::TargetClosed);
                }
                let target = state
                    .targets
                    .get_mut(&permit.thread_id)
                    .expect("validated steering target remains registered");
                if target.loss_requested {
                    return Ok(false);
                }
                target.loss_requested = true;
                advance_revision(&mut state);
                Ok(true)
            })
            .unwrap_or(Err(TargetLossRequestError::Router))?;
        if requested {
            self.publication_changed.notify_all();
        }
        drop(command);
        loop {
            if !self.commands.is_open() || state.persistent_failure.is_some() {
                return Err(TargetLossRequestError::Router);
            }
            let target = state
                .targets
                .get(&permit.thread_id)
                .ok_or(TargetLossRequestError::TargetClosed)?;
            if target.registration != permit.registration
                || target.owner != permit.owner
                || target.loaded_generation != permit.loaded_generation
                || target.home_generation != permit.home_generation
                || !super::active_steering::matching_attempt(&state, permit)
            {
                return Err(TargetLossRequestError::TargetClosed);
            }
            if target.publication_in_flight.is_none() {
                break;
            }
            state = self
                .publication_changed
                .wait(state)
                .map_err(|_| TargetLossRequestError::Router)?;
        }
        let final_command = self
            .commands
            .authorize()
            .map_err(|_| TargetLossRequestError::Router)?;
        let authority = final_command
            .commit_if_current(|| {
                if !super::active_steering::matching_attempt(&state, permit) {
                    return Err(TargetLossRequestError::TargetClosed);
                }
                let target = state
                    .targets
                    .get_mut(&permit.thread_id)
                    .ok_or(TargetLossRequestError::TargetClosed)?;
                if target.registration != permit.registration
                    || target.owner != permit.owner
                    || target.loaded_generation != permit.loaded_generation
                    || target.home_generation != permit.home_generation
                    || !target.loss_requested
                    || target.publication_in_flight.is_some()
                    || target.turn_state == TargetTurn::Terminal
                {
                    return Err(TargetLossRequestError::TargetClosed);
                }
                let activation = target
                    .pending_activation
                    .clone()
                    .ok_or(TargetLossRequestError::TargetClosed)?;
                target.publication_in_flight = Some(TargetPublication::Loss);
                let authority = TargetLossPublicationAuthority::new(
                    Arc::clone(self),
                    permit.thread_id.clone(),
                    target.turn_id.clone(),
                    target.registration,
                    target.owner,
                    target.loaded_generation,
                    target.home_generation,
                    Some(activation),
                    target.activation_durable,
                    Some(permit.token),
                );
                state
                    .active_steering_attempt
                    .as_mut()
                    .expect("validated steering attempt remains reserved during loss")
                    .loss_transferred = true;
                advance_revision(&mut state);
                Ok(authority)
            })
            .unwrap_or(Err(TargetLossRequestError::Router))?;
        drop(state);
        self.publication_changed.notify_all();
        Ok(authority)
    }

    fn finish_target_loss(
        &self,
        authority: &TargetLossPublicationAuthority,
        published: bool,
    ) -> Result<(TargetInvalidation, bool), TargetLossFinishError> {
        let settlement = self.settle_long_lived_authority(|state| {
            finish_target_loss_locked(state, authority, published)
        });
        match settlement {
            super::LongLivedAuthoritySettlement::Settled(result) => {
                self.publication_changed.notify_all();
                result
            }
            super::LongLivedAuthoritySettlement::PreservedForPersistentFailure
            | super::LongLivedAuthoritySettlement::Unavailable => {
                Err(TargetLossFinishError::Router)
            }
        }
    }
}

fn finish_target_loss_locked(
    state: &mut super::RouterState,
    authority: &TargetLossPublicationAuthority,
    published: bool,
) -> Result<(TargetInvalidation, bool), TargetLossFinishError> {
    let valid = state
        .targets
        .get(&authority.cas_thread_id)
        .is_some_and(|target| {
            target.registration == authority.registration
                && target.owner == authority.owner
                && target.loaded_generation == authority.loaded_generation
                && target.home_generation == authority.home_generation
                && target.publication_in_flight == Some(TargetPublication::Loss)
                && target.loss_requested
        })
        && match authority.steering_token {
            Some(token) => state
                .active_steering_attempt
                .as_ref()
                .is_some_and(|attempt| {
                    attempt.token == token
                        && attempt.registration == authority.registration
                        && attempt.thread_id == authority.cas_thread_id
                        && attempt.loss_transferred
                }),
            None => !state
                .active_steering_attempt
                .as_ref()
                .is_some_and(|attempt| {
                    attempt.registration == authority.registration
                        && attempt.thread_id == authority.cas_thread_id
                }),
        };
    if !valid {
        return Err(TargetLossFinishError::Router);
    }
    let target = state
        .targets
        .get(&authority.cas_thread_id)
        .expect("validated loss target remains registered");
    if published {
        target.loss_receipt.store(true, Ordering::Release);
    }
    let invalidation = invalidation(
        target,
        if published {
            target
                .publication_closing
                .unwrap_or(LiveEventTargetCloseReason::ReceiverAbandoned)
        } else {
            LiveEventTargetCloseReason::SourcePublicationFailed
        },
    );
    let terminal_reason = invalidation.reason;
    if authority.steering_token.is_some() {
        state.active_steering_attempt = None;
    }
    let target = state
        .targets
        .remove(&authority.cas_thread_id)
        .expect("validated loss target remains removable");
    set_terminal(&target.terminal, terminal_reason);
    let connection_retired = fence_thread_lane(state, &authority.cas_thread_id);
    if !connection_retired {
        advance_revision(state);
    }
    Ok((invalidation, connection_retired))
}
