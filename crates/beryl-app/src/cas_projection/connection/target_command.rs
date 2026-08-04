use std::time::Duration;

use beryl_backend::{
    ClientUserMessageId, CompactionAttemptCorrelation, DynamicToolCallResponse,
    ExactForegroundThread, ManagedBackendError, NonIdempotentRequestOutcome, StreamedInputSource,
    TurnStartOptions, TurnStartOutcome, TurnSteerOutcome,
};

use super::{
    ConnectionCommandOutcome, ConnectionGeneration, ConnectionRoutingFailure,
    ExactContextCompactionDispatch, LiveEventTargetCloseReason, ProjectionConnection,
    StreamedInputBrokerService, TargetRegistration, TargetRegistrationProof,
    provider_broker::ProviderBrokerResponseActivationFailure,
    router::{
        ActiveSteeringAttemptPermit, RoutedDynamicToolResponse, TargetAuthorizationFailure,
        TargetHandoffRequirement,
    },
};
use crate::cas_projection::{ProjectionCoordinatorError, ProjectionExecutionError};

#[derive(Debug)]
pub(in crate::cas_projection) struct TargetTurnStartOutcome {
    connection_generation: ConnectionGeneration,
    registration: u64,
    command: ConnectionCommandOutcome<TurnStartOutcome>,
    response_activation_failure: Option<TargetTurnStartActivationFailure>,
}

#[derive(Clone, Debug)]
pub(in crate::cas_projection) enum TargetTurnStartActivationFailure {
    Target(LiveEventTargetCloseReason),
    Router,
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

    pub(in crate::cas_projection) fn response_activation_failure(
        &self,
    ) -> Option<&TargetTurnStartActivationFailure> {
        self.response_activation_failure.as_ref()
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
    pub(in crate::cas_projection) fn compact_target(
        &self,
        registration: &TargetRegistrationProof,
        attempt: CompactionAttemptCorrelation,
        timeout: Duration,
    ) -> Result<
        Result<
            ConnectionCommandOutcome<ExactContextCompactionDispatch>,
            TargetAuthorizationFailure,
        >,
        ProjectionCoordinatorError,
    > {
        let proof = registration.clone();
        let target = ExactForegroundThread::new(
            registration.key().runtime_id,
            registration.loaded_generation(),
            registration.key().cas_thread_id.clone(),
        );
        self.with_runtime(|runtime| {
            runtime.driver.call_classified_checked(
                move |router, _| router.authorize_context_compaction_command(&proof),
                move |session| session.compact_exact_foreground_thread(target, attempt, timeout),
                ExactContextCompactionDispatch::invalidates_connection,
            )
        })
    }

    pub(in crate::cas_projection) fn steer_target_streamed_input(
        &self,
        registration: &TargetRegistrationProof,
        attempt: &ActiveSteeringAttemptPermit,
        correlation: ClientUserMessageId,
        timeout: Duration,
        service: impl StreamedInputBrokerService,
    ) -> Result<
        Result<ConnectionCommandOutcome<TurnSteerOutcome>, TargetAuthorizationFailure>,
        ProjectionCoordinatorError,
    > {
        let authorization = attempt.command_authorization();
        let thread_id = attempt.cas_thread_id().clone();
        let turn_id = attempt.cas_turn_id().clone();
        let command = self.with_runtime(|runtime| {
            runtime.driver.call_classified_checked_with_source(
                move |router, command| {
                    router.authorize_active_steering_command(command, &authorization)
                },
                move |source| {
                    let input: Box<dyn StreamedInputSource> = Box::new(source);
                    move |session| {
                        session.steer_turn_with_streamed_input(
                            &thread_id,
                            &turn_id,
                            &correlation,
                            input,
                            timeout,
                        )
                    }
                },
                |broker, outcome| {
                    if matches!(
                        outcome,
                        NonIdempotentRequestOutcome::ProvenNotDispatched { .. }
                    ) {
                        broker.seal_checked_steering_proven_nondispatch();
                    }
                },
                service,
                turn_steer_invalidates_connection,
            )
        })?;
        let command = match command {
            Ok(command) => command,
            Err(failure) => return Ok(Err(failure)),
        };
        let ((outcome, ()), routing_failure) = command.into_parts();
        let mut command = ConnectionCommandOutcome::new(outcome, routing_failure);
        self.record_exact_target_terminal_proof(registration, &mut command);
        Ok(Ok(command))
    }

    pub(in crate::cas_projection) fn start_target_streamed_turn(
        &self,
        registration: &TargetRegistration,
        options: TurnStartOptions,
        timeout: Duration,
        service: impl StreamedInputBrokerService,
    ) -> Result<TargetTurnStartOutcome, ProjectionExecutionError> {
        let proof = registration.proof();
        let response_proof = proof.clone();
        let thread_id = registration.key().cas_thread_id.clone();
        let command = self.with_runtime(|runtime| {
            runtime.driver.call_classified_checked_with_source(
                move |router, _| router.authorize_turn_start(&proof),
                move |source| {
                    let input: Box<dyn StreamedInputSource> = Box::new(source);
                    move |session| {
                        session.start_turn_with_streamed_input(&thread_id, input, options, timeout)
                    }
                },
                move |broker, outcome| match outcome {
                    NonIdempotentRequestOutcome::ExactResponse { response } => {
                        broker.prove_response_activation(&response_proof, response.turn_id())
                    }
                    NonIdempotentRequestOutcome::ExactRejection { .. }
                    | NonIdempotentRequestOutcome::ProvenNotDispatched { .. }
                    | NonIdempotentRequestOutcome::CompletionUnknown { .. } => Ok(()),
                },
                service,
                turn_start_invalidates_connection,
            )
        })?;
        let (command, response_activation_failure) = match command {
            Ok(command) => {
                let ((outcome, classification), routing_failure) = command.into_parts();
                (
                    Ok(ConnectionCommandOutcome::new(outcome, routing_failure)),
                    classification.err(),
                )
            }
            Err(error) => (Err(error), None),
        };
        self.complete_target_turn_start(registration, command, response_activation_failure)
    }

    fn complete_target_turn_start(
        &self,
        registration: &TargetRegistration,
        command: Result<ConnectionCommandOutcome<TurnStartOutcome>, TargetAuthorizationFailure>,
        response_activation_failure: Option<ProviderBrokerResponseActivationFailure>,
    ) -> Result<TargetTurnStartOutcome, ProjectionExecutionError> {
        let mut command = match command {
            Ok(command) => command,
            Err(failure) => {
                self.publish_target_authorization(registration, Err(failure))?;
                unreachable!("failed target authorization returns an execution error")
            }
        };
        let response_activation_failure = response_activation_failure.map(|failure| {
            self.record_response_activation_failure(registration, &mut command, failure)
        });
        self.record_exact_target_terminal(registration, &mut command);
        Ok(TargetTurnStartOutcome {
            connection_generation: self.authority.generation,
            registration: registration.registration(),
            command,
            response_activation_failure,
        })
    }

    pub(in crate::cas_projection) fn respond_target_dynamic_tool_call(
        &self,
        registration: &TargetRegistration,
        routed: RoutedDynamicToolResponse,
        response: DynamicToolCallResponse,
    ) -> Result<(), ProjectionExecutionError> {
        let proof = registration.proof();
        let (authorization, write) = routed.into_parts();
        let command = self.with_runtime(|runtime| {
            runtime.driver.call_classified_checked(
                move |router, command| {
                    router.authorize_dynamic_tool_response(command, &proof, &authorization)
                },
                move |session| session.respond_dynamic_tool_call(write.call(), &response),
                |result| {
                    result
                        .as_ref()
                        .is_err_and(ManagedBackendError::invalidates_connection_authority)
                },
            )
        })?;
        let mut command = match command {
            Ok(command) => command,
            Err(failure) => {
                self.publish_target_authorization(registration, Err(failure))?;
                unreachable!("failed target authorization returns an execution error")
            }
        };
        self.record_exact_target_terminal(registration, &mut command);
        self.publish_ordered_result(command)
    }

    pub(in crate::cas_projection) fn handoff_target(
        &self,
        registration: &TargetRegistration,
        requirement: TargetHandoffRequirement,
    ) -> Result<(), super::LiveEventTargetHandoffError> {
        self.current_router()
            .map_err(|_| super::LiveEventTargetHandoffError::RouterPoisoned)?
            .handoff_target(registration, requirement)
    }

    fn record_response_activation_failure(
        &self,
        registration: &TargetRegistration,
        command: &mut ConnectionCommandOutcome<TurnStartOutcome>,
        failure: ProviderBrokerResponseActivationFailure,
    ) -> TargetTurnStartActivationFailure {
        let (routing_failure, failure) = match failure {
            ProviderBrokerResponseActivationFailure::Target(reason) => {
                self.invalidate_target_generation(registration);
                (
                    ConnectionRoutingFailure::Target {
                        thread_id: registration.key().cas_thread_id.clone(),
                        reason,
                    },
                    TargetTurnStartActivationFailure::Target(reason),
                )
            }
            ProviderBrokerResponseActivationFailure::Router => {
                self.retire();
                (
                    ConnectionRoutingFailure::Router,
                    TargetTurnStartActivationFailure::Router,
                )
            }
        };
        command.record_routing_failure(routing_failure);
        failure
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

    fn record_exact_target_terminal<T>(
        &self,
        registration: &TargetRegistration,
        command: &mut ConnectionCommandOutcome<T>,
    ) {
        self.record_exact_target_terminal_proof(&registration.proof(), command);
    }

    fn record_exact_target_terminal_proof<T>(
        &self,
        registration: &TargetRegistrationProof,
        command: &mut ConnectionCommandOutcome<T>,
    ) {
        if let Some(reason) = registration.terminal_reason() {
            command.record_routing_failure(ConnectionRoutingFailure::Target {
                thread_id: registration.key().cas_thread_id.clone(),
                reason,
            });
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

fn turn_steer_invalidates_connection(outcome: &TurnSteerOutcome) -> bool {
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
