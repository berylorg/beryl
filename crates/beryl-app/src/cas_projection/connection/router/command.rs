use beryl_backend::DynamicToolCallRequest;
use beryl_model::{CasThreadId, CasTurnId, DynamicToolCallId};

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
        let mut state = self
            .state
            .lock()
            .map_err(|_| TargetAuthorizationFailure::Router)?;
        if state.retired.is_some() {
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
            TargetTurn::AwaitingStart | TargetTurn::Exact => {
                LiveEventTargetCloseReason::DuplicateTurnStart
            }
        };
        close_target(&mut state, &registration.key.cas_thread_id, reason);
        Err(TargetAuthorizationFailure::Target(reason))
    }

    pub(in crate::cas_projection) fn authorize_dynamic_tool_response(
        &self,
        registration: &TargetRegistrationProof,
        request: &DynamicToolCallRequest,
    ) -> Result<(), TargetAuthorizationFailure> {
        let request_thread = CasThreadId::new(request.thread_id()).ok();
        let request_turn = CasTurnId::new(request.turn_id()).ok();
        let request_call = DynamicToolCallId::new(request.call_id()).ok();
        let mut state = self
            .state
            .lock()
            .map_err(|_| TargetAuthorizationFailure::Router)?;
        if state.retired.is_some() {
            return Err(TargetAuthorizationFailure::Router);
        }
        authorize_exact_target(&state, registration)?;
        let target = state
            .targets
            .get_mut(&registration.key.cas_thread_id)
            .expect("authorized target remains registered");
        let reason = if target.turn_state == TargetTurn::Terminal {
            Some(LiveEventTargetCloseReason::EventAfterTurnCompletion)
        } else if request_thread.as_ref() != Some(&registration.key.cas_thread_id)
            || request_turn.as_ref() != target.turn_id.as_ref()
        {
            Some(LiveEventTargetCloseReason::ConflictingTurnIdentity)
        } else {
            match request_call.and_then(|call_id| target.dynamic_tool_requests.remove(&call_id)) {
                Some(expected) if expected == *request => None,
                _ => Some(LiveEventTargetCloseReason::ConflictingDynamicToolIdentity),
            }
        };
        if let Some(reason) = reason {
            close_target(&mut state, &registration.key.cas_thread_id, reason);
            return Err(TargetAuthorizationFailure::Target(reason));
        }
        advance_revision(&mut state);
        Ok(())
    }

    pub(in crate::cas_projection) fn handoff_target(
        &self,
        registration: &TargetRegistration,
        requirement: TargetHandoffRequirement,
    ) -> Result<(), LiveEventTargetHandoffError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| LiveEventTargetHandoffError::RouterPoisoned)?;
        if state.retired.is_some() {
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
                if target.turn_state != TargetTurn::AwaitingStart || !target.start_dispatched =>
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
        let count = target
            .queued_count
            .load(std::sync::atomic::Ordering::Acquire);
        let bytes = target
            .queued_bytes
            .load(std::sync::atomic::Ordering::Acquire);
        if count != 0 || bytes != 0 {
            return Err(LiveEventTargetHandoffError::QueuedEvents { count, bytes });
        }
        if !target.dynamic_tool_requests.is_empty() {
            return Err(LiveEventTargetHandoffError::DynamicToolResponsesPending);
        }
        state.targets.remove(&registration.key.cas_thread_id);
        advance_revision(&mut state);
        Ok(())
    }
}
