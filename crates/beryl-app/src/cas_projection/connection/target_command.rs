use std::time::Duration;

use beryl_backend::{
    DynamicToolCallRequest, DynamicToolCallResponse, ManagedBackendError,
    NonIdempotentRequestOutcome, TurnStartOptions, TurnStartOutcome, UserInput,
};

use super::{
    ConnectionCommandOutcome, ConnectionGeneration, ConnectionRoutingFailure,
    LiveEventTargetCloseReason, ProjectionConnection, TargetRegistration,
    router::{LiveEventTargetError, TargetAuthorizationFailure, TargetHandoffRequirement},
};
use crate::cas_projection::ProjectionExecutionError;

#[derive(Debug)]
pub(in crate::cas_projection) struct TargetTurnStartOutcome {
    connection_generation: ConnectionGeneration,
    registration: u64,
    command: ConnectionCommandOutcome<TurnStartOutcome>,
}

impl TargetTurnStartOutcome {
    pub(in crate::cas_projection) const fn outcome(&self) -> &TurnStartOutcome {
        self.command.operation()
    }

    pub(in crate::cas_projection) fn into_parts(
        self,
    ) -> (TurnStartOutcome, Option<ConnectionRoutingFailure>) {
        self.command.into_parts()
    }

    pub(super) const fn belongs_to(
        &self,
        connection_generation: ConnectionGeneration,
        registration: u64,
    ) -> bool {
        self.connection_generation.get() == connection_generation.get()
            && self.registration == registration
    }
}

impl ProjectionConnection {
    pub(in crate::cas_projection) fn start_target_turn(
        &self,
        registration: &TargetRegistration,
        input: Vec<UserInput>,
        options: TurnStartOptions,
        timeout: Duration,
    ) -> Result<TargetTurnStartOutcome, ProjectionExecutionError> {
        let proof = registration.proof();
        let thread_id = registration.key().cas_thread_id.clone();
        let command = self.driver.call_classified_checked(
            move |router| router.authorize_turn_start(&proof),
            move |session| session.start_turn_with_user_input(&thread_id, input, options, timeout),
            turn_start_invalidates_connection,
        )?;
        let mut command = match command {
            Ok(command) => command,
            Err(failure) => {
                self.publish_target_authorization(registration, Err(failure))?;
                unreachable!("failed target authorization returns an execution error")
            }
        };
        if let NonIdempotentRequestOutcome::ExactResponse { response } = command.operation()
            && let Err(error) = self.confirm_target_turn(registration, response.turn_id().clone())
        {
            let failure = match error {
                LiveEventTargetError::ConflictingTurnIdentity { .. } => {
                    ConnectionRoutingFailure::Target {
                        thread_id: registration.key().cas_thread_id.clone(),
                        reason: LiveEventTargetCloseReason::ConflictingTurnIdentity,
                    }
                }
                LiveEventTargetError::TargetClosed => ConnectionRoutingFailure::Target {
                    thread_id: registration.key().cas_thread_id.clone(),
                    reason: registration
                        .terminal_reason()
                        .unwrap_or(LiveEventTargetCloseReason::WorkerStopped),
                },
                LiveEventTargetError::ConnectionRetired | LiveEventTargetError::RouterPoisoned => {
                    ConnectionRoutingFailure::Router
                }
            };
            command.record_routing_failure(failure);
        }
        Ok(TargetTurnStartOutcome {
            connection_generation: self.authority.generation,
            registration: registration.registration(),
            command,
        })
    }

    pub(in crate::cas_projection) fn respond_target_dynamic_tool_call(
        &self,
        registration: &TargetRegistration,
        request: &DynamicToolCallRequest,
        response: &DynamicToolCallResponse,
    ) -> Result<(), ProjectionExecutionError> {
        let proof = registration.proof();
        let authorization_request = request.clone();
        let request = request.clone();
        let response = response.clone();
        let command = self.driver.call_classified_checked(
            move |router| router.authorize_dynamic_tool_response(&proof, &authorization_request),
            move |session| session.respond_dynamic_tool_call(&request, &response),
            |result| {
                result
                    .as_ref()
                    .is_err_and(ManagedBackendError::invalidates_connection_authority)
            },
        )?;
        let command = match command {
            Ok(command) => command,
            Err(failure) => {
                self.publish_target_authorization(registration, Err(failure))?;
                unreachable!("failed target authorization returns an execution error")
            }
        };
        self.publish_ordered_result(command)
    }

    pub(in crate::cas_projection) fn handoff_target(
        &self,
        registration: &TargetRegistration,
        requirement: TargetHandoffRequirement,
    ) -> Result<(), super::LiveEventTargetHandoffError> {
        self.router.handoff_target(registration, requirement)
    }

    fn publish_target_authorization(
        &self,
        registration: &TargetRegistration,
        authorization: Result<(), TargetAuthorizationFailure>,
    ) -> Result<(), ProjectionExecutionError> {
        match authorization {
            Ok(()) => Ok(()),
            Err(TargetAuthorizationFailure::Target(reason)) => {
                self.invalidate_target_generation(registration);
                Err(ProjectionExecutionError::LiveEventRouting {
                    thread_id: registration.key().cas_thread_id.clone(),
                    reason,
                })
            }
            Err(TargetAuthorizationFailure::Router) => {
                self.retire();
                Err(self.unavailable().into())
            }
        }
    }
}

fn turn_start_invalidates_connection(outcome: &TurnStartOutcome) -> bool {
    match outcome {
        NonIdempotentRequestOutcome::ProvenNotDispatched { error }
        | NonIdempotentRequestOutcome::CompletionUnknown { error } => {
            error.invalidates_connection_authority()
        }
        NonIdempotentRequestOutcome::ExactResponse { .. }
        | NonIdempotentRequestOutcome::ExactRejection { .. } => false,
    }
}

pub(in crate::cas_projection) fn turn_start_allows_not_started(outcome: &TurnStartOutcome) -> bool {
    matches!(
        outcome,
        NonIdempotentRequestOutcome::ExactRejection { .. }
            | NonIdempotentRequestOutcome::ProvenNotDispatched { .. }
    )
}
