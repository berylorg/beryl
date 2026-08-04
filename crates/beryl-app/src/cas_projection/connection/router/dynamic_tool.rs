use std::sync::{Arc, atomic::Ordering};

use beryl_backend::DynamicToolCall;
use beryl_model::{CasThreadId, CasTurnId, DynamicToolCallId};

use super::{
    EventRouter, LiveEventTargetCloseReason, QueuedTargetOperation, RoutedDynamicToolCall,
    RoutedDynamicToolResponse, RoutedTargetOperation, RouterState, TargetAuthorizationFailure,
    TargetInvalidation, TargetRegistrationProof, TargetTerminalSignal, TargetTurn,
    state::advance_revision,
    target::{close_target, invalidation},
};
use crate::conversation_tools::RoutedDynamicToolRequest;

pub(super) struct DynamicToolResponseAdmission {
    connection_generation: u64,
    registration: u64,
}

impl std::fmt::Debug for DynamicToolResponseAdmission {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DynamicToolResponseAdmission")
            .field("connection_generation", &self.connection_generation)
            .field("registration", &self.registration)
            .finish_non_exhaustive()
    }
}

pub(in crate::cas_projection::connection) struct DynamicToolTargetPermit {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    call_id: DynamicToolCallId,
    registration: u64,
    admission: Arc<DynamicToolResponseAdmission>,
    terminal: Arc<std::sync::Mutex<TargetTerminalSignal>>,
}

pub(in crate::cas_projection::connection) struct DynamicToolResponseAuthorization {
    connection_generation: u64,
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    call_id: DynamicToolCallId,
    registration: u64,
    pub(super) admission: Arc<DynamicToolResponseAdmission>,
}

pub(in crate::cas_projection::connection) enum DynamicToolTargetError {
    Unmatched,
    Target(TargetInvalidation),
    Router,
}

impl DynamicToolTargetPermit {
    pub(in crate::cas_projection::connection) fn is_terminal(&self) -> bool {
        self.terminal
            .lock()
            .map_or(true, |terminal| *terminal != TargetTerminalSignal::Open)
    }
}

impl EventRouter {
    pub(in crate::cas_projection::connection) fn reserve_dynamic_tool(
        &self,
        command: &crate::cas_projection::LiveCommandPermit,
        call: &DynamicToolCall,
    ) -> Result<DynamicToolTargetPermit, DynamicToolTargetError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| DynamicToolTargetError::Router)?;
        let thread_id = call.thread_id();
        let admission = command
            .commit_if_current(|| {
                if state.retired.is_some() || state.persistent_failure.is_some() {
                    return Err(DynamicToolTargetError::Router);
                }
                let Some(target) = state.targets.get(thread_id) else {
                    return Err(DynamicToolTargetError::Unmatched);
                };
                let reason = if call.is_sealed() {
                    Some(LiveEventTargetCloseReason::ConflictingDynamicToolIdentity)
                } else if target.turn_state == TargetTurn::AwaitingStart {
                    Some(LiveEventTargetCloseReason::EventBeforeTurnStart)
                } else if target.turn_state == TargetTurn::Terminal {
                    Some(LiveEventTargetCloseReason::EventAfterTurnCompletion)
                } else if target.turn_id.as_ref() != Some(call.turn_id()) {
                    Some(LiveEventTargetCloseReason::ConflictingTurnIdentity)
                } else if target.loss_requested
                    || target.sender.is_none()
                    || target.publication_closing.is_some()
                {
                    Some(LiveEventTargetCloseReason::ReceiverAbandoned)
                } else if target.dynamic_tool_responses.len()
                    >= super::TARGET_OPERATION_QUEUE_CAPACITY
                    || target.dynamic_tool_responses.contains_key(call.call_id())
                {
                    Some(LiveEventTargetCloseReason::ConflictingDynamicToolIdentity)
                } else {
                    None
                };
                if let Some(reason) = reason {
                    let invalidation = invalidation(target, reason);
                    if close_target(&mut state, thread_id, reason) {
                        return Err(DynamicToolTargetError::Router);
                    }
                    return Err(DynamicToolTargetError::Target(invalidation));
                }
                let target = state
                    .targets
                    .get_mut(thread_id)
                    .expect("validated dynamic-tool target remains registered");
                let admission = Arc::new(DynamicToolResponseAdmission {
                    connection_generation: self.connection_generation,
                    registration: target.registration,
                });
                target
                    .dynamic_tool_responses
                    .insert(call.call_id().clone(), Arc::clone(&admission));
                let retained = (
                    target.registration,
                    Arc::clone(&admission),
                    Arc::clone(&target.terminal),
                );
                advance_revision(&mut state);
                Ok(retained)
            })
            .unwrap_or(Err(DynamicToolTargetError::Router))?;
        let permit = DynamicToolTargetPermit {
            thread_id: thread_id.clone(),
            turn_id: call.turn_id().clone(),
            call_id: call.call_id().clone(),
            registration: admission.0,
            admission: admission.1,
            terminal: admission.2,
        };
        Ok(permit)
    }

    pub(in crate::cas_projection::connection) fn dynamic_tool_is_live(
        &self,
        command: &crate::cas_projection::LiveCommandPermit,
        permit: &DynamicToolTargetPermit,
    ) -> bool {
        if permit.is_terminal() {
            return false;
        }
        let Ok(state) = self.state.lock() else {
            return false;
        };
        command
            .commit_if_current(|| {
                state.retired.is_none()
                    && state.persistent_failure.is_none()
                    && state.targets.get(&permit.thread_id).is_some_and(|target| {
                        target.registration == permit.registration
                            && target.turn_state == TargetTurn::Exact
                            && target.turn_id.as_ref() == Some(&permit.turn_id)
                            && target.sender.is_some()
                            && !target.loss_requested
                            && target
                                .dynamic_tool_responses
                                .get(&permit.call_id)
                                .is_some_and(|admission| Arc::ptr_eq(admission, &permit.admission))
                    })
            })
            .unwrap_or(false)
    }

    pub(in crate::cas_projection::connection) fn abandon_dynamic_tool(
        &self,
        permit: &DynamicToolTargetPermit,
    ) {
        let _ = self.settle_long_lived_authority(|state| {
            let Some(target) = state.targets.get_mut(&permit.thread_id) else {
                return;
            };
            let exact = target.registration == permit.registration
                && target
                    .dynamic_tool_responses
                    .get(&permit.call_id)
                    .is_some_and(|admission| Arc::ptr_eq(admission, &permit.admission));
            if exact {
                target.dynamic_tool_responses.remove(&permit.call_id);
                advance_revision(state);
            }
        });
    }
}

impl EventRouter {
    pub(in crate::cas_projection::connection) fn seal_dynamic_tool(
        &self,
        command: &crate::cas_projection::LiveCommandPermit,
        permit: DynamicToolTargetPermit,
        call: DynamicToolCall,
        request: RoutedDynamicToolRequest,
    ) -> Result<(), DynamicToolTargetError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| DynamicToolTargetError::Router)?;
        command
            .commit_if_current(|| {
                if state.retired.is_some() || state.persistent_failure.is_some() {
                    return Err(DynamicToolTargetError::Router);
                }
                let exact_call = call.thread_id() == &permit.thread_id
                    && call.turn_id() == &permit.turn_id
                    && call.call_id() == &permit.call_id;
                let valid = exact_call
                    && state.targets.get(&permit.thread_id).is_some_and(|target| {
                        target.registration == permit.registration
                            && target.turn_state == TargetTurn::Exact
                            && target.turn_id.as_ref() == Some(&permit.turn_id)
                            && target.sender.is_some()
                            && !target.loss_requested
                            && target
                                .dynamic_tool_responses
                                .get(&permit.call_id)
                                .is_some_and(|admission| Arc::ptr_eq(admission, &permit.admission))
                    });
                if !valid {
                    return Err(self.fail_dynamic_locked(
                        &mut state,
                        &permit,
                        LiveEventTargetCloseReason::ConflictingDynamicToolIdentity,
                    ));
                }
                let target = state
                    .targets
                    .get_mut(&permit.thread_id)
                    .expect("validated dynamic-tool target remains registered");
                target.queued_operations.fetch_add(1, Ordering::AcqRel);
                let authorization = DynamicToolResponseAuthorization {
                    connection_generation: self.connection_generation,
                    thread_id: permit.thread_id.clone(),
                    turn_id: permit.turn_id.clone(),
                    call_id: permit.call_id.clone(),
                    registration: permit.registration,
                    admission: Arc::clone(&permit.admission),
                };
                let routed = RoutedDynamicToolCall {
                    response: RoutedDynamicToolResponse {
                        call,
                        authorization,
                    },
                    request,
                };
                let delivery = target
                    .sender
                    .as_ref()
                    .expect("validated dynamic-tool target retains its sender")
                    .try_send(QueuedTargetOperation {
                        operation: RoutedTargetOperation::DynamicTool(routed),
                    });
                match delivery {
                    Ok(()) => {
                        state.routed_operation_count =
                            state.routed_operation_count.saturating_add(1);
                        advance_revision(&mut state);
                        Ok(())
                    }
                    Err(std::sync::mpsc::TrySendError::Full(_)) => {
                        let target = state
                            .targets
                            .get_mut(&permit.thread_id)
                            .expect("failed dynamic-tool target remains registered");
                        release_queue_count(target);
                        state.queue_pressure_count = state.queue_pressure_count.saturating_add(1);
                        Err(self.fail_dynamic_locked(
                            &mut state,
                            &permit,
                            LiveEventTargetCloseReason::QueueOverflow,
                        ))
                    }
                    Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                        let target = state
                            .targets
                            .get_mut(&permit.thread_id)
                            .expect("failed dynamic-tool target remains registered");
                        release_queue_count(target);
                        Err(self.fail_dynamic_locked(
                            &mut state,
                            &permit,
                            LiveEventTargetCloseReason::ReceiverAbandoned,
                        ))
                    }
                }
            })
            .unwrap_or(Err(DynamicToolTargetError::Router))
    }

    pub(in crate::cas_projection::connection) fn authorize_dynamic_tool_response(
        &self,
        command: &crate::cas_projection::LiveCommandPermit,
        registration: &TargetRegistrationProof,
        authorization: &DynamicToolResponseAuthorization,
    ) -> Result<(), TargetAuthorizationFailure> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| TargetAuthorizationFailure::Router)?;
        command
            .commit_if_current(|| {
                if state.retired.is_some()
                    || state.persistent_failure.is_some()
                    || authorization.connection_generation != self.connection_generation
                {
                    return Err(TargetAuthorizationFailure::Router);
                }
                let Some(target) = state.targets.get(&authorization.thread_id) else {
                    return Err(TargetAuthorizationFailure::Target(
                        registration
                            .terminal_reason()
                            .unwrap_or(LiveEventTargetCloseReason::WorkerStopped),
                    ));
                };
                let valid = target.registration == registration.registration
                    && target.registration == authorization.registration
                    && target.owner == registration.owner
                    && target.loaded_generation == registration.loaded_generation
                    && registration.key.cas_thread_id == authorization.thread_id
                    && target.turn_state != TargetTurn::Terminal
                    && target.turn_id.as_ref() == Some(&authorization.turn_id)
                    && target
                        .dynamic_tool_responses
                        .get(&authorization.call_id)
                        .is_some_and(|admission| {
                            Arc::ptr_eq(admission, &authorization.admission)
                                && admission.connection_generation
                                    == authorization.connection_generation
                                && admission.registration == authorization.registration
                        });
                if !valid {
                    let reason = LiveEventTargetCloseReason::ConflictingDynamicToolIdentity;
                    close_target(&mut state, &authorization.thread_id, reason);
                    return Err(TargetAuthorizationFailure::Target(reason));
                }
                state
                    .targets
                    .get_mut(&authorization.thread_id)
                    .expect("authorized dynamic-tool target remains registered")
                    .dynamic_tool_responses
                    .remove(&authorization.call_id);
                advance_revision(&mut state);
                Ok(())
            })
            .unwrap_or(Err(TargetAuthorizationFailure::Router))
    }

    fn fail_dynamic_locked(
        &self,
        state: &mut RouterState,
        permit: &DynamicToolTargetPermit,
        reason: LiveEventTargetCloseReason,
    ) -> DynamicToolTargetError {
        let Some(target) = state.targets.get_mut(&permit.thread_id) else {
            return DynamicToolTargetError::Unmatched;
        };
        let invalidation = invalidation(target, reason);
        if target
            .dynamic_tool_responses
            .get(&permit.call_id)
            .is_some_and(|admission| Arc::ptr_eq(admission, &permit.admission))
        {
            target.dynamic_tool_responses.remove(&permit.call_id);
        }
        if close_target(state, &permit.thread_id, reason) {
            DynamicToolTargetError::Router
        } else {
            DynamicToolTargetError::Target(invalidation)
        }
    }
}

fn release_queue_count(target: &super::TargetEntry) {
    target.queued_operations.fetch_sub(1, Ordering::AcqRel);
}
