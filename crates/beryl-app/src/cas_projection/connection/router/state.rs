use std::sync::mpsc::{self, TrySendError};

use super::{
    EventRouter, LiveEventTargetCloseReason, LiveEventTargetError,
    LiveEventTargetRegistrationError, QueuedLiveEvent, RouteOutcome, RoutedLiveEvent, RouterState,
    TargetEntry, TargetRegistration, TargetTurn,
    classify::{EventScope, classify},
    target::{close_target, invalidation, reserve_bytes, retire_router_state},
};
use crate::cas_projection::ProjectionCoordinatorError;
use beryl_backend::TurnStreamEvent;
use beryl_model::{
    CasLoadedSessionGeneration, CasProcessGeneration, CasThreadId, CasTurnId, DynamicToolCallId,
    RuntimeId, SyndicThreadId,
};

impl EventRouter {
    pub(in crate::cas_projection) fn new(
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        connection_generation: u64,
    ) -> Result<Self, ProjectionCoordinatorError> {
        let process = super::process::acquire_process_projection(runtime_id, process_generation)?;
        process.register_connection(connection_generation)?;
        Ok(Self {
            runtime_id,
            process_generation,
            connection_generation,
            process,
            state: std::sync::Mutex::new(RouterState {
                revision: 0,
                next_registration: 0,
                retired: None,
                targets: std::collections::HashMap::new(),
                retired_thread_lanes: std::collections::HashSet::new(),
                routed_event_count: 0,
                unmatched_event_count: 0,
                rejected_event_count: 0,
                overflow_count: 0,
                quiet_poll_count: 0,
            }),
        })
    }

    pub(in crate::cas_projection) fn register(
        &self,
        key: super::LoadedThreadKey,
        owner: SyndicThreadId,
        loaded_generation: CasLoadedSessionGeneration,
        turn_id: Option<CasTurnId>,
    ) -> Result<TargetRegistration, LiveEventTargetRegistrationError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| LiveEventTargetRegistrationError::RouterPoisoned)?;
        if state.retired.is_some() {
            return Err(LiveEventTargetRegistrationError::ConnectionRetired);
        }
        if state.targets.contains_key(&key.cas_thread_id) {
            return Err(LiveEventTargetRegistrationError::TargetAlreadyRegistered {
                thread_id: key.cas_thread_id.clone(),
            });
        }
        if state.retired_thread_lanes.contains(&key.cas_thread_id) {
            return Err(LiveEventTargetRegistrationError::TargetGenerationRetired {
                thread_id: key.cas_thread_id.clone(),
            });
        }
        state.next_registration = state
            .next_registration
            .checked_add(1)
            .ok_or(LiveEventTargetRegistrationError::GenerationExhausted)?;
        let registration = state.next_registration;
        let (sender, receiver) = mpsc::sync_channel(super::LIVE_EVENT_TARGET_QUEUE_COUNT_LIMIT);
        let queued_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let queued_bytes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let terminal = std::sync::Arc::new(std::sync::Mutex::new(None));
        let start_dispatched = turn_id.is_some();
        state.targets.insert(
            key.cas_thread_id.clone(),
            TargetEntry {
                registration,
                key: key.clone(),
                owner,
                loaded_generation,
                turn_state: if turn_id.is_some() {
                    TargetTurn::Exact
                } else {
                    TargetTurn::AwaitingStart
                },
                turn_id,
                start_dispatched,
                dynamic_tool_requests: std::collections::HashMap::new(),
                sender,
                queued_count: std::sync::Arc::clone(&queued_count),
                queued_bytes: std::sync::Arc::clone(&queued_bytes),
                terminal: std::sync::Arc::clone(&terminal),
            },
        );
        advance_revision(&mut state);
        Ok(TargetRegistration {
            registration,
            key,
            owner,
            loaded_generation,
            receiver,
            queued_count,
            queued_bytes,
            terminal,
        })
    }

    pub(in crate::cas_projection) fn confirm_turn(
        &self,
        registration: &TargetRegistration,
        turn_id: CasTurnId,
    ) -> Result<(), LiveEventTargetError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| LiveEventTargetError::RouterPoisoned)?;
        let Some(target) = state.targets.get_mut(&registration.key.cas_thread_id) else {
            return Err(LiveEventTargetError::TargetClosed);
        };
        if target.registration != registration.registration {
            return Err(LiveEventTargetError::TargetClosed);
        }
        match target.turn_id.as_ref() {
            Some(observed) if observed != &turn_id => {
                let actual = observed.clone();
                let connection_retired = close_target(
                    &mut state,
                    &registration.key.cas_thread_id,
                    LiveEventTargetCloseReason::ConflictingTurnIdentity,
                );
                if connection_retired {
                    Err(LiveEventTargetError::ConnectionRetired)
                } else {
                    Err(LiveEventTargetError::ConflictingTurnIdentity {
                        expected: turn_id,
                        actual,
                    })
                }
            }
            Some(_) => Ok(()),
            None => {
                target.turn_state = TargetTurn::Exact;
                target.turn_id = Some(turn_id);
                advance_revision(&mut state);
                Ok(())
            }
        }
    }

    pub(in crate::cas_projection) fn unregister(
        &self,
        registration: &TargetRegistration,
        reason: LiveEventTargetCloseReason,
    ) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return true;
        };
        if state
            .targets
            .get(&registration.key.cas_thread_id)
            .is_some_and(|target| target.registration == registration.registration)
        {
            return close_target(&mut state, &registration.key.cas_thread_id, reason);
        }
        false
    }

    pub(in crate::cas_projection) fn retire(&self, reason: LiveEventTargetCloseReason) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let effective_reason = state.retired.unwrap_or(reason);
        retire_router_state(&mut state, effective_reason);
        drop(state);
        self.process
            .retire_connection(self.connection_generation, effective_reason);
    }

    pub(in crate::cas_projection) fn record_quiet_poll(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.quiet_poll_count = state.quiet_poll_count.saturating_add(1);
            advance_revision(&mut state);
        }
    }
}

impl EventRouter {
    pub(in crate::cas_projection) fn route(
        &self,
        event: TurnStreamEvent,
        approximate_retained_bytes: usize,
    ) -> RouteOutcome {
        let scope = match classify(&event) {
            Ok(scope) => scope,
            Err(()) => {
                if let Ok(mut state) = self.state.lock() {
                    state.rejected_event_count = state.rejected_event_count.saturating_add(1);
                    advance_revision(&mut state);
                }
                return RouteOutcome::RetireConnection(
                    LiveEventTargetCloseReason::InvalidEventIdentity,
                );
            }
        };
        if matches!(scope, EventScope::ProtocolError) {
            return RouteOutcome::RetireConnection(LiveEventTargetCloseReason::ProtocolError);
        }
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => {
                return RouteOutcome::RetireConnection(LiveEventTargetCloseReason::StreamFailure);
            }
        };
        if state.retired.is_some() {
            state.rejected_event_count = state.rejected_event_count.saturating_add(1);
            advance_revision(&mut state);
            return RouteOutcome::Continue;
        }
        match scope {
            EventScope::Account(rate_limits) => {
                match self
                    .process
                    .publish_account(self.connection_generation, rate_limits)
                {
                    Ok(true) => RouteOutcome::Continue,
                    Ok(false) | Err(_) => {
                        RouteOutcome::RetireConnection(LiveEventTargetCloseReason::StreamFailure)
                    }
                }
            }
            EventScope::Thread {
                thread_id,
                closes_target,
            } => self.route_target_locked(
                &mut state,
                event,
                approximate_retained_bytes,
                thread_id,
                None,
                false,
                false,
                closes_target,
            ),
            EventScope::Turn {
                thread_id,
                turn_id,
                starts_turn,
                completes_turn,
            } => self.route_target_locked(
                &mut state,
                event,
                approximate_retained_bytes,
                thread_id,
                Some(turn_id),
                starts_turn,
                completes_turn,
                false,
            ),
            EventScope::ProtocolError => unreachable!("protocol errors return before routing"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn route_target_locked(
        &self,
        state: &mut RouterState,
        event: TurnStreamEvent,
        approximate_retained_bytes: usize,
        thread_id: CasThreadId,
        turn_id: Option<CasTurnId>,
        starts_turn: bool,
        completes_turn: bool,
        closes_target: bool,
    ) -> RouteOutcome {
        let Some(target) = state.targets.get_mut(&thread_id) else {
            state.unmatched_event_count = state.unmatched_event_count.saturating_add(1);
            advance_revision(state);
            return RouteOutcome::Continue;
        };
        if target.turn_state == TargetTurn::Terminal {
            let invalidation =
                invalidation(target, LiveEventTargetCloseReason::EventAfterTurnCompletion);
            if close_target(
                state,
                &thread_id,
                LiveEventTargetCloseReason::EventAfterTurnCompletion,
            ) {
                return RouteOutcome::RetireConnection(
                    LiveEventTargetCloseReason::RetiredThreadLaneCapacity,
                );
            }
            return RouteOutcome::InvalidateTarget(invalidation);
        }
        if let Some(actual_turn) = turn_id.as_ref() {
            match (&target.turn_state, target.turn_id.as_ref()) {
                (TargetTurn::AwaitingStart, None) if starts_turn => {
                    target.turn_state = TargetTurn::Exact;
                    target.turn_id = Some(actual_turn.clone());
                }
                (TargetTurn::AwaitingStart, None) => {
                    let invalidation =
                        invalidation(target, LiveEventTargetCloseReason::EventBeforeTurnStart);
                    if close_target(
                        state,
                        &thread_id,
                        LiveEventTargetCloseReason::EventBeforeTurnStart,
                    ) {
                        return RouteOutcome::RetireConnection(
                            LiveEventTargetCloseReason::RetiredThreadLaneCapacity,
                        );
                    }
                    return RouteOutcome::InvalidateTarget(invalidation);
                }
                (TargetTurn::Exact | TargetTurn::Terminal, Some(expected))
                    if expected == actual_turn => {}
                (_, Some(_)) => {
                    let invalidation =
                        invalidation(target, LiveEventTargetCloseReason::ConflictingTurnIdentity);
                    if close_target(
                        state,
                        &thread_id,
                        LiveEventTargetCloseReason::ConflictingTurnIdentity,
                    ) {
                        return RouteOutcome::RetireConnection(
                            LiveEventTargetCloseReason::RetiredThreadLaneCapacity,
                        );
                    }
                    return RouteOutcome::InvalidateTarget(invalidation);
                }
                _ => {
                    return RouteOutcome::RetireConnection(
                        LiveEventTargetCloseReason::StreamFailure,
                    );
                }
            }
        }
        let dynamic_tool_request = match &event {
            TurnStreamEvent::DynamicToolCallRequested(request) => Some(request.clone()),
            _ => None,
        };
        let retained_bytes = approximate_retained_bytes.max(1);
        if !reserve_bytes(&target.queued_bytes, retained_bytes) {
            let invalidation = invalidation(target, LiveEventTargetCloseReason::QueueOverflow);
            state.overflow_count = state.overflow_count.saturating_add(1);
            if close_target(state, &thread_id, LiveEventTargetCloseReason::QueueOverflow) {
                return RouteOutcome::RetireConnection(
                    LiveEventTargetCloseReason::RetiredThreadLaneCapacity,
                );
            }
            return RouteOutcome::InvalidateTarget(invalidation);
        }
        target
            .queued_count
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let routed = RoutedLiveEvent {
            thread_id: thread_id.clone(),
            turn_id,
            event: Box::new(event),
            approximate_retained_bytes: retained_bytes,
        };
        match target.sender.try_send(QueuedLiveEvent {
            event: routed,
            retained_bytes,
        }) {
            Ok(()) => {
                if let Some(request) = dynamic_tool_request {
                    let call_id = DynamicToolCallId::new(request.call_id())
                        .expect("classified dynamic-tool call identity remains valid");
                    if target.dynamic_tool_requests.len()
                        >= super::LIVE_EVENT_TARGET_QUEUE_COUNT_LIMIT
                        || target
                            .dynamic_tool_requests
                            .insert(call_id, request)
                            .is_some()
                    {
                        let invalidation = invalidation(
                            target,
                            LiveEventTargetCloseReason::ConflictingDynamicToolIdentity,
                        );
                        if close_target(
                            state,
                            &thread_id,
                            LiveEventTargetCloseReason::ConflictingDynamicToolIdentity,
                        ) {
                            return RouteOutcome::RetireConnection(
                                LiveEventTargetCloseReason::RetiredThreadLaneCapacity,
                            );
                        }
                        return RouteOutcome::InvalidateTarget(invalidation);
                    }
                }
                if completes_turn {
                    target.turn_state = TargetTurn::Terminal;
                }
                state.routed_event_count = state.routed_event_count.saturating_add(1);
                advance_revision(state);
                if closes_target {
                    let target = state
                        .targets
                        .get(&thread_id)
                        .expect("routed target remains registered");
                    let invalidation =
                        invalidation(target, LiveEventTargetCloseReason::ThreadClosed);
                    if close_target(state, &thread_id, LiveEventTargetCloseReason::ThreadClosed) {
                        RouteOutcome::RetireConnection(
                            LiveEventTargetCloseReason::RetiredThreadLaneCapacity,
                        )
                    } else {
                        RouteOutcome::InvalidateTarget(invalidation)
                    }
                } else {
                    RouteOutcome::Continue
                }
            }
            Err(TrySendError::Full(_)) => {
                target
                    .queued_count
                    .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
                target
                    .queued_bytes
                    .fetch_sub(retained_bytes, std::sync::atomic::Ordering::AcqRel);
                let invalidation = invalidation(target, LiveEventTargetCloseReason::QueueOverflow);
                state.overflow_count = state.overflow_count.saturating_add(1);
                if close_target(state, &thread_id, LiveEventTargetCloseReason::QueueOverflow) {
                    RouteOutcome::RetireConnection(
                        LiveEventTargetCloseReason::RetiredThreadLaneCapacity,
                    )
                } else {
                    RouteOutcome::InvalidateTarget(invalidation)
                }
            }
            Err(TrySendError::Disconnected(_)) => {
                target
                    .queued_count
                    .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
                target
                    .queued_bytes
                    .fetch_sub(retained_bytes, std::sync::atomic::Ordering::AcqRel);
                let invalidation =
                    invalidation(target, LiveEventTargetCloseReason::ReceiverAbandoned);
                if close_target(
                    state,
                    &thread_id,
                    LiveEventTargetCloseReason::ReceiverAbandoned,
                ) {
                    RouteOutcome::RetireConnection(
                        LiveEventTargetCloseReason::RetiredThreadLaneCapacity,
                    )
                } else {
                    RouteOutcome::InvalidateTarget(invalidation)
                }
            }
        }
    }
}

pub(super) fn advance_revision(state: &mut RouterState) {
    state.revision = state.revision.saturating_add(1);
}
