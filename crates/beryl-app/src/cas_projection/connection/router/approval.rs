use std::sync::mpsc::TrySendError;

use beryl_backend::{
    ApprovalInterruption, ApprovalRequest, OrderedTurnStreamRejection, OrderedTurnStreamSubmitCause,
};
use beryl_model::{CasItemId, CasLoadedSessionGeneration, CasThreadId, CasTurnId, SyndicThreadId};

use super::{
    EventRouter, LiveEventTargetCloseReason, QueuedTargetOperation, RouteOutcome, RoutedApproval,
    RoutedTargetOperation, RouterState, TargetInvalidation, TargetTurn,
    state::advance_revision,
    target::{close_target, invalidation},
};
use crate::cas_projection::stop::StopDispatchOwner;

pub(in crate::cas_projection) struct PreparedApprovalInterruption {
    interruption: ApprovalInterruption,
    primary: Option<StopDispatchOwner>,
}

impl PreparedApprovalInterruption {
    pub(in crate::cas_projection) const fn new(
        interruption: ApprovalInterruption,
        primary: Option<StopDispatchOwner>,
    ) -> Self {
        Self {
            interruption,
            primary,
        }
    }
}
pub(in crate::cas_projection) enum ApprovalRouteOutcome {
    Routed {
        interruption: ApprovalInterruption,
        obligation: Option<ApprovalInterruptionObligation>,
    },
    TargetFailed {
        request: ApprovalRequest,
        cause: OrderedTurnStreamSubmitCause,
        target: TargetInvalidation,
        obligation: Option<ApprovalInterruptionObligation>,
    },
    Rejected {
        request: ApprovalRequest,
        cause: OrderedTurnStreamSubmitCause,
        reason: LiveEventTargetCloseReason,
    },
}

/// One already-authorized, driver-owned permission interruption.
///
/// The exact target proof is retained independently of the presentation receiver so dropping the
/// target after approval acknowledgement cannot cancel the backend interruption.
pub(in crate::cas_projection) struct ApprovalInterruptionObligation {
    connection_generation: u64,
    target_registration: u64,
    key: super::LoadedThreadKey,
    owner: SyndicThreadId,
    loaded_generation: CasLoadedSessionGeneration,
    turn_id: CasTurnId,
    item_id: Option<CasItemId>,
    primary: Option<StopDispatchOwner>,
}

impl ApprovalInterruptionObligation {
    pub(in crate::cas_projection) fn thread_id(&self) -> &CasThreadId {
        &self.key.cas_thread_id
    }

    pub(in crate::cas_projection) fn turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }

    pub(in crate::cas_projection) fn matches_request(&self, request: &ApprovalRequest) -> bool {
        request.kind().separate_interruption_required()
            && request.thread_id() == Some(self.thread_id())
            && request.turn_id() == Some(self.turn_id())
            && request.item_id() == self.item_id.as_ref()
    }

    pub(in crate::cas_projection) fn take_primary(&mut self) -> Option<StopDispatchOwner> {
        self.primary.take()
    }
}

impl EventRouter {
    pub(in crate::cas_projection) fn route_approval(
        &self,
        command: &crate::cas_projection::LiveCommandPermit,
        request: ApprovalRequest,
        prepared: Option<PreparedApprovalInterruption>,
    ) -> ApprovalRouteOutcome {
        let thread_id = request.thread_id().cloned();
        let turn_id = request.turn_id().cloned();
        let request = std::cell::RefCell::new(Some(request));
        let prepared = std::cell::RefCell::new(Some(prepared));
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => {
                return ApprovalRouteOutcome::Rejected {
                    request: request
                        .borrow_mut()
                        .take()
                        .expect("uncommitted approval request remains available"),
                    cause: OrderedTurnStreamSubmitCause::Unavailable,
                    reason: LiveEventTargetCloseReason::StreamFailure,
                };
            }
        };
        let committed = command.commit_if_current(|| {
            let request = request
                .borrow_mut()
                .take()
                .expect("one exact gate branch consumes the approval request");
            let prepared = prepared
                .borrow_mut()
                .take()
                .expect("one exact gate branch consumes prepared approval authority");
            let (Some(thread_id), Some(turn_id)) = (thread_id, turn_id) else {
                state.rejected_operation_count = state.rejected_operation_count.saturating_add(1);
                advance_revision(&mut state);
                return ApprovalRouteOutcome::Rejected {
                    request,
                    cause: OrderedTurnStreamSubmitCause::Rejected(
                        OrderedTurnStreamRejection::InvalidControl,
                    ),
                    reason: LiveEventTargetCloseReason::InvalidEventIdentity,
                };
            };
            if let Some(reason) = state.retired {
                state.rejected_operation_count = state.rejected_operation_count.saturating_add(1);
                advance_revision(&mut state);
                return ApprovalRouteOutcome::Rejected {
                    request,
                    cause: OrderedTurnStreamSubmitCause::Unavailable,
                    reason,
                };
            }
            let Some(target) = state.targets.get(&thread_id) else {
                state.unmatched_operation_count = state.unmatched_operation_count.saturating_add(1);
                advance_revision(&mut state);
                return ApprovalRouteOutcome::Rejected {
                    request,
                    cause: OrderedTurnStreamSubmitCause::Unavailable,
                    reason: LiveEventTargetCloseReason::InvalidEventIdentity,
                };
            };
            if target.loss_requested {
                let obligation =
                    approval_obligation(self.connection_generation, target, &request, prepared);
                return target_failed(
                    &mut state,
                    &thread_id,
                    request,
                    OrderedTurnStreamSubmitCause::Unavailable,
                    LiveEventTargetCloseReason::SourcePublicationRouteUnavailable,
                    obligation,
                );
            }
            if target.sender.is_none() {
                let reason = target
                    .publication_closing
                    .unwrap_or(LiveEventTargetCloseReason::WorkerStopped);
                let obligation =
                    approval_obligation(self.connection_generation, target, &request, prepared);
                return target_failed(
                    &mut state,
                    &thread_id,
                    request,
                    OrderedTurnStreamSubmitCause::Unavailable,
                    reason,
                    obligation,
                );
            }
            let route_failure = match (&target.turn_state, target.turn_id.as_ref()) {
                (TargetTurn::AwaitingStart | TargetTurn::AwaitingCompactionTurn, _) => {
                    Some(LiveEventTargetCloseReason::EventBeforeTurnStart)
                }
                (TargetTurn::Terminal, _) => {
                    Some(LiveEventTargetCloseReason::EventAfterTurnCompletion)
                }
                (TargetTurn::Exact, Some(expected)) if expected == &turn_id => None,
                (TargetTurn::Exact, _) => Some(LiveEventTargetCloseReason::ConflictingTurnIdentity),
            };
            if let Some(reason) = route_failure {
                let obligation =
                    approval_obligation(self.connection_generation, target, &request, prepared);
                return target_failed(
                    &mut state,
                    &thread_id,
                    request,
                    OrderedTurnStreamSubmitCause::Rejected(
                        OrderedTurnStreamRejection::SchemaMismatch,
                    ),
                    reason,
                    obligation,
                );
            }

            let interruption = prepared
                .as_ref()
                .map_or(ApprovalInterruption::NotRequired, |prepared| {
                    prepared.interruption.clone()
                });
            let obligation =
                approval_obligation(self.connection_generation, target, &request, prepared);
            let delivery = {
                let target = state
                    .targets
                    .get_mut(&thread_id)
                    .expect("validated approval target remains registered");
                target
                    .queued_operations
                    .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                target
                    .sender
                    .as_ref()
                    .expect("validated approval target retains its sender")
                    .try_send(QueuedTargetOperation {
                        operation: RoutedTargetOperation::Approval(RoutedApproval {
                            request,
                            interruption: interruption.clone(),
                        }),
                    })
            };
            match delivery {
                Ok(()) => {
                    state.routed_operation_count = state.routed_operation_count.saturating_add(1);
                    advance_revision(&mut state);
                    ApprovalRouteOutcome::Routed {
                        interruption,
                        obligation,
                    }
                }
                Err(TrySendError::Full(queued)) => {
                    release_queue_count(&state, &thread_id);
                    state.queue_pressure_count = state.queue_pressure_count.saturating_add(1);
                    let request = approval_request(queued);
                    target_failed(
                        &mut state,
                        &thread_id,
                        request,
                        OrderedTurnStreamSubmitCause::CapacityFull,
                        LiveEventTargetCloseReason::QueueOverflow,
                        obligation,
                    )
                }
                Err(TrySendError::Disconnected(queued)) => {
                    release_queue_count(&state, &thread_id);
                    let request = approval_request(queued);
                    target_failed(
                        &mut state,
                        &thread_id,
                        request,
                        OrderedTurnStreamSubmitCause::ReceiverLost,
                        LiveEventTargetCloseReason::ReceiverAbandoned,
                        obligation,
                    )
                }
            }
        });
        committed.unwrap_or_else(|_| ApprovalRouteOutcome::Rejected {
            request: request
                .borrow_mut()
                .take()
                .expect("cut approval request was not consumed"),
            cause: OrderedTurnStreamSubmitCause::Unavailable,
            reason: LiveEventTargetCloseReason::StreamFailure,
        })
    }

    pub(in crate::cas_projection) fn fail_approval_interruption(
        &self,
        command: &crate::cas_projection::LiveCommandPermit,
        obligation: &ApprovalInterruptionObligation,
    ) -> RouteOutcome {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => {
                return RouteOutcome::RetireConnection(
                    LiveEventTargetCloseReason::ApprovalInterruptionFailed,
                );
            }
        };
        command
            .commit_if_current(|| {
                if state.retired.is_some()
                    || state.persistent_failure.is_some()
                    || obligation.connection_generation != self.connection_generation
                {
                    return RouteOutcome::Continue;
                }
                let Some(target) = state.targets.get(obligation.thread_id()) else {
                    return RouteOutcome::Continue;
                };
                if target.registration != obligation.target_registration
                    || target.key != obligation.key
                    || target.owner != obligation.owner
                    || target.loaded_generation != obligation.loaded_generation
                {
                    return RouteOutcome::Continue;
                }
                let target = invalidation(
                    target,
                    LiveEventTargetCloseReason::ApprovalInterruptionFailed,
                );
                if close_target(
                    &mut state,
                    obligation.thread_id(),
                    LiveEventTargetCloseReason::ApprovalInterruptionFailed,
                ) {
                    return RouteOutcome::RetireConnection(
                        LiveEventTargetCloseReason::RetiredThreadLaneCapacity,
                    );
                }
                RouteOutcome::InvalidateTarget(target)
            })
            .unwrap_or(RouteOutcome::Continue)
    }
}

fn target_failed(
    state: &mut RouterState,
    thread_id: &CasThreadId,
    request: ApprovalRequest,
    cause: OrderedTurnStreamSubmitCause,
    reason: LiveEventTargetCloseReason,
    obligation: Option<ApprovalInterruptionObligation>,
) -> ApprovalRouteOutcome {
    let target = state
        .targets
        .get(thread_id)
        .expect("target-local approval failure retains its exact target");
    let target = invalidation(target, reason);
    if close_target(state, thread_id, reason) {
        return ApprovalRouteOutcome::Rejected {
            request,
            cause: OrderedTurnStreamSubmitCause::Unavailable,
            reason: LiveEventTargetCloseReason::RetiredThreadLaneCapacity,
        };
    }
    ApprovalRouteOutcome::TargetFailed {
        request,
        cause,
        target,
        obligation,
    }
}

fn approval_obligation(
    connection_generation: u64,
    target: &super::TargetEntry,
    request: &ApprovalRequest,
    prepared: Option<PreparedApprovalInterruption>,
) -> Option<ApprovalInterruptionObligation> {
    if !request.kind().separate_interruption_required() {
        return None;
    }
    let prepared = prepared.expect("permission approval retains durable stop ownership");
    Some(ApprovalInterruptionObligation {
        connection_generation,
        target_registration: target.registration,
        key: target.key.clone(),
        owner: target.owner,
        loaded_generation: target.loaded_generation,
        turn_id: request
            .turn_id()
            .expect("validated permission approval retains its turn identity")
            .clone(),
        item_id: request.item_id().cloned(),
        primary: prepared.primary,
    })
}

fn release_queue_count(state: &RouterState, thread_id: &CasThreadId) {
    let target = state
        .targets
        .get(thread_id)
        .expect("failed approval delivery retains its exact target");
    target
        .queued_operations
        .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
}

fn approval_request(queued: QueuedTargetOperation) -> ApprovalRequest {
    match queued.operation {
        RoutedTargetOperation::Approval(approval) => approval.into_parts().0,
        RoutedTargetOperation::DynamicTool(_) => {
            unreachable!("approval delivery returned another target operation")
        }
    }
}
