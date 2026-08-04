use beryl_model::CasTurnId;

use super::{
    EventRouter, LiveEventTargetCloseReason, TargetRegistrationProof, TargetTurn,
    target::close_target,
};
use crate::cas_projection::PendingTurnActivation;

pub(in crate::cas_projection) enum TargetTurnRegistration {
    Pending(PendingTurnActivation),
    Active(CasTurnId),
    ContextCompaction(crate::cas_projection::context_compaction::ContextCompactionTargetAuthority),
}

pub(in crate::cas_projection::connection) struct ResponseActivationProof {
    home_generation: u64,
}

impl ResponseActivationProof {
    pub(in crate::cas_projection::connection) const fn home_generation(&self) -> u64 {
        self.home_generation
    }
}

#[derive(Debug)]
pub(in crate::cas_projection::connection) enum ResponseActivationProofError {
    Target(LiveEventTargetCloseReason),
    Router,
}

impl EventRouter {
    pub(in crate::cas_projection::connection) fn prove_response_activation(
        &self,
        registration: &TargetRegistrationProof,
        turn_id: &CasTurnId,
    ) -> Result<ResponseActivationProof, ResponseActivationProofError> {
        let admission = self
            .commands
            .authorize()
            .map_err(|_| ResponseActivationProofError::Router)?;
        let thread_id = &registration.key.cas_thread_id;
        let mut state = self
            .state
            .lock()
            .map_err(|_| ResponseActivationProofError::Router)?;
        admission
            .commit_if_current(|| validate_response_target(&state, registration).map(|_| ()))
            .unwrap_or(Err(ResponseActivationProofError::Router))?;
        drop(admission);
        loop {
            if !self.commands.is_open()
                || state.retired.is_some()
                || state.persistent_failure.is_some()
            {
                return Err(ResponseActivationProofError::Router);
            }
            let target = validate_response_target(&state, registration)?;
            if target.publication_in_flight.is_none() {
                break;
            }
            state = self
                .publication_changed
                .wait(state)
                .map_err(|_| ResponseActivationProofError::Router)?;
        }

        let command = self
            .commands
            .authorize()
            .map_err(|_| ResponseActivationProofError::Router)?;
        command
            .commit_if_current(|| {
                let target = validate_response_target(&state, registration)?;
                let failure = if let Some(reason) = target.publication_closing {
                    Some(reason)
                } else if target.turn_id.as_ref() != Some(turn_id) {
                    Some(LiveEventTargetCloseReason::ConflictingTurnIdentity)
                } else if !target.start_dispatched
                    || target.pending_activation.is_none()
                    || !target.activation_durable
                    || target.turn_state == TargetTurn::AwaitingStart
                {
                    Some(LiveEventTargetCloseReason::TurnActivationPublicationFailed)
                } else {
                    None
                };
                if let Some(reason) = failure {
                    let connection_retired = close_target(&mut state, thread_id, reason);
                    return if connection_retired {
                        Err(ResponseActivationProofError::Router)
                    } else {
                        Err(ResponseActivationProofError::Target(reason))
                    };
                }
                Ok(ResponseActivationProof {
                    home_generation: target.home_generation,
                })
            })
            .unwrap_or(Err(ResponseActivationProofError::Router))
    }
}

fn validate_response_target<'a>(
    state: &'a super::RouterState,
    registration: &TargetRegistrationProof,
) -> Result<&'a super::TargetEntry, ResponseActivationProofError> {
    let target = state
        .targets
        .get(&registration.key.cas_thread_id)
        .ok_or_else(|| {
            ResponseActivationProofError::Target(
                registration
                    .terminal_reason()
                    .unwrap_or(LiveEventTargetCloseReason::WorkerStopped),
            )
        })?;
    if target.registration != registration.registration
        || target.owner != registration.owner
        || target.loaded_generation != registration.loaded_generation
    {
        return Err(ResponseActivationProofError::Target(
            LiveEventTargetCloseReason::WorkerStopped,
        ));
    }
    if target.loss_requested {
        return Err(ResponseActivationProofError::Target(
            target
                .publication_closing
                .or_else(|| registration.terminal_reason())
                .unwrap_or(LiveEventTargetCloseReason::SourcePublicationRouteUnavailable),
        ));
    }
    Ok(target)
}
