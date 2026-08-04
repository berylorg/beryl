use super::{
    EventRouter, LiveEventTargetCloseReason, LiveEventTargetHandoffError, RouterState,
    TargetAuthorizationFailure, TargetHandoffRequirement, TargetRegistration,
    TargetRegistrationProof, TargetTurn, state::advance_revision, target::close_target,
};

fn authorize_exact_target(
    state: &RouterState,
    registration: &TargetRegistrationProof,
) -> Result<(), TargetAuthorizationFailure> {
    let Some(target) = state.targets.get(&registration.key.cas_thread_id) else {
        return Err(TargetAuthorizationFailure::Target(
            registration
                .terminal_reason()
                .unwrap_or(LiveEventTargetCloseReason::WorkerStopped),
        ));
    };
    if target.registration != registration.registration
        || target.owner != registration.owner
        || target.loaded_generation != registration.loaded_generation
    {
        return Err(TargetAuthorizationFailure::Target(
            LiveEventTargetCloseReason::ReceiverAbandoned,
        ));
    }
    Ok(())
}

impl EventRouter {
    pub(in crate::cas_projection) fn authorize_turn_start(
        &self,
        registration: &TargetRegistrationProof,
    ) -> Result<(), TargetAuthorizationFailure> {
        let command = self
            .commands
            .authorize()
            .map_err(|_| TargetAuthorizationFailure::Router)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| TargetAuthorizationFailure::Router)?;
        command
            .commit_if_current(|| {
                if state.retired.is_some() || state.persistent_failure.is_some() {
                    return Err(TargetAuthorizationFailure::Router);
                }
                authorize_exact_target(&state, registration)?;
                let target = state
                    .targets
                    .get_mut(&registration.key.cas_thread_id)
                    .expect("authorized target remains registered");
                if target.turn_state == TargetTurn::AwaitingStart && !target.start_dispatched {
                    target.start_dispatched = true;
                    advance_revision(&mut state);
                    return Ok(());
                }
                let reason = match target.turn_state {
                    TargetTurn::Terminal => LiveEventTargetCloseReason::EventAfterTurnCompletion,
                    TargetTurn::AwaitingStart
                    | TargetTurn::AwaitingCompactionTurn
                    | TargetTurn::Exact => LiveEventTargetCloseReason::DuplicateTurnStart,
                };
                close_target(&mut state, &registration.key.cas_thread_id, reason);
                Err(TargetAuthorizationFailure::Target(reason))
            })
            .unwrap_or(Err(TargetAuthorizationFailure::Router))
    }

    pub(in crate::cas_projection) fn handoff_target(
        &self,
        registration: &TargetRegistration,
        requirement: TargetHandoffRequirement,
    ) -> Result<(), LiveEventTargetHandoffError> {
        let command = self
            .commands
            .authorize()
            .map_err(|_| LiveEventTargetHandoffError::ConnectionRetired)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| LiveEventTargetHandoffError::RouterPoisoned)?;
        command
            .commit_if_current(|| {
                if state.retired.is_some() || state.persistent_failure.is_some() {
                    return Err(LiveEventTargetHandoffError::ConnectionRetired);
                }
                let Some(target) = state.targets.get(&registration.key.cas_thread_id) else {
                    return Err(LiveEventTargetHandoffError::TargetClosed);
                };
                if target.registration != registration.registration {
                    return Err(LiveEventTargetHandoffError::TargetClosed);
                }
                match requirement {
                    TargetHandoffRequirement::NotStarted
                        if target.turn_state != TargetTurn::AwaitingStart
                            || !target.start_dispatched =>
                    {
                        return Err(LiveEventTargetHandoffError::TargetMayHaveStarted);
                    }
                    TargetHandoffRequirement::CompactionNotDispatched
                        if target.turn_state != TargetTurn::AwaitingCompactionTurn
                            || !target.start_dispatched
                            || target.compaction.is_none() =>
                    {
                        return Err(LiveEventTargetHandoffError::TargetMayHaveStarted);
                    }
                    TargetHandoffRequirement::ProvenTerminal
                        if target.turn_state != TargetTurn::Terminal =>
                    {
                        return Err(LiveEventTargetHandoffError::TargetNotTerminal);
                    }
                    _ => {}
                }
                if requirement == TargetHandoffRequirement::ProvenTerminal
                    && registration.proven_terminal().is_none()
                {
                    return Err(LiveEventTargetHandoffError::TerminalOutcomeUnavailable);
                }
                let count = target
                    .queued_operations
                    .load(std::sync::atomic::Ordering::Acquire);
                if count != 0 {
                    return Err(LiveEventTargetHandoffError::QueuedOperations { count });
                }
                if !target.dynamic_tool_responses.is_empty() {
                    return Err(LiveEventTargetHandoffError::DynamicToolResponsesPending);
                }
                state.targets.remove(&registration.key.cas_thread_id);
                advance_revision(&mut state);
                Ok(())
            })
            .unwrap_or(Err(LiveEventTargetHandoffError::ConnectionRetired))
    }
}
