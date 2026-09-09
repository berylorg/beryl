use std::{sync::Arc, time::Duration};

use beryl_backend::NonIdempotentRequestOutcome;
use beryl_home_store::HomeStore;
use beryl_model::BindingRevision;
use syndic_storage::{SyndicPointReadLimit, SyndicStorage, TurnIncompleteReason};

use crate::cas_projection::connection::{
    LiveEventTargetLossOutcome, TargetTurnStartActivationFailure, TargetTurnStartOutcome,
};
use crate::cas_projection::ordinary::{
    OrdinaryDynamicToolContext, OrdinaryDynamicToolHandlers, OrdinaryTurnCaptureLoss,
    OrdinaryTurnExecutionError, OrdinaryTurnExecutionOutcome, converge::converge_terminal_history,
    preflight::PendingOrdinaryExecution,
};
use crate::cas_projection::stop::StopCoordinator;
use crate::cas_projection::{ContextCompactionTimeoutPolicy, LiveEventPoll, LiveEventTarget};

const LIVE_POLL_INTERVAL: Duration = Duration::from_millis(100);

struct OrdinaryLifecycleGuard {
    stop: Arc<StopCoordinator>,
    thread_id: beryl_model::SyndicThreadId,
    turn_id: beryl_model::SyndicTurnId,
    transferred: bool,
}

impl Drop for OrdinaryLifecycleGuard {
    fn drop(&mut self) {
        if !self.transferred {
            self.stop
                .release_ordinary_lifecycle_yield(self.thread_id, self.turn_id);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn begin_capture(
    store: &HomeStore,
    storage: &SyndicStorage,
    target: LiveEventTarget,
    start: TargetTurnStartOutcome,
    pending: PendingOrdinaryExecution,
    active_binding_revision: BindingRevision,
    cas_turn_id: beryl_model::CasTurnId,
    tools: &mut OrdinaryDynamicToolHandlers<'_>,
    limit: SyndicPointReadLimit,
    context_compaction_timeout: &ContextCompactionTimeoutPolicy,
) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionError> {
    if let Some(failure) = start.response_activation_failure().cloned() {
        let (cause, reason) = match failure {
            TargetTurnStartActivationFailure::Target(reason) => (
                if reason == crate::cas_projection::LiveEventTargetCloseReason::TurnActivationPublicationFailed {
                    TurnIncompleteReason::AuthorityLost
                } else {
                    TurnIncompleteReason::CompletionMismatch
                },
                OrdinaryTurnCaptureLoss::TargetClosed(reason),
            ),
            TargetTurnStartActivationFailure::Router => (
                TurnIncompleteReason::AuthorityLost,
                OrdinaryTurnCaptureLoss::TargetClosed(
                    crate::cas_projection::LiveEventTargetCloseReason::WorkerStopped,
                ),
            ),
        };
        if let Some(outcome) = converge_target_loss(
            store,
            storage,
            target,
            &pending,
            active_binding_revision,
            cause,
            limit,
            context_compaction_timeout,
            None,
        )? {
            return Ok(outcome);
        }
        return Ok(OrdinaryTurnExecutionOutcome::Incomplete { reason });
    }
    let completion_unknown = match start.into_parts().0 {
        NonIdempotentRequestOutcome::CompletionUnknown { error } => Some(error),
        NonIdempotentRequestOutcome::ExactResponse { .. } => None,
        NonIdempotentRequestOutcome::ExactRejection { .. }
        | NonIdempotentRequestOutcome::ProvenNotDispatched { .. } => {
            return Err(OrdinaryTurnExecutionError::Invariant(
                "live capture received a not-started outcome",
            ));
        }
    };
    run_capture(
        store,
        storage,
        target,
        cas_turn_id,
        pending,
        active_binding_revision,
        completion_unknown,
        tools,
        limit,
        context_compaction_timeout,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn converge_completion_unknown_start(
    store: &HomeStore,
    storage: &SyndicStorage,
    target: LiveEventTarget,
    start: TargetTurnStartOutcome,
    pending: PendingOrdinaryExecution,
    active_binding_revision: BindingRevision,
    limit: SyndicPointReadLimit,
    context_compaction_timeout: &ContextCompactionTimeoutPolicy,
) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionError> {
    let (outcome, _) = start.into_parts();
    let NonIdempotentRequestOutcome::CompletionUnknown { error } = outcome else {
        return Err(OrdinaryTurnExecutionError::Invariant(
            "completion-unknown convergence received a known start outcome",
        ));
    };
    if let Some(outcome) = converge_target_loss(
        store,
        storage,
        target,
        &pending,
        active_binding_revision,
        TurnIncompleteReason::StreamLost,
        limit,
        context_compaction_timeout,
        None,
    )? {
        return Ok(outcome);
    }
    Ok(OrdinaryTurnExecutionOutcome::Incomplete {
        reason: OrdinaryTurnCaptureLoss::StartCompletionUnknown(error),
    })
}

#[allow(clippy::too_many_arguments)]
fn run_capture(
    store: &HomeStore,
    storage: &SyndicStorage,
    target: LiveEventTarget,
    cas_turn_id: beryl_model::CasTurnId,
    pending: PendingOrdinaryExecution,
    active_binding_revision: BindingRevision,
    completion_unknown: Option<Box<beryl_backend::ManagedBackendError>>,
    tools: &mut OrdinaryDynamicToolHandlers<'_>,
    limit: SyndicPointReadLimit,
    context_compaction_timeout: &ContextCompactionTimeoutPolicy,
) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionError> {
    let mut lifecycle = OrdinaryLifecycleGuard {
        stop: target.stop_coordinator()?,
        thread_id: pending.thread_id,
        turn_id: pending.turn_id,
        transferred: false,
    };
    let context = lifecycle
        .stop
        .dynamic_tool_context(pending.thread_id, pending.turn_id);
    let result = (|| loop {
        match target.poll(LIVE_POLL_INTERVAL) {
            LiveEventPoll::Approval(approval) => {
                if approval.thread_id() != target.cas_thread_id()
                    || approval.turn_id() != &cas_turn_id
                {
                    if let Some(outcome) = converge_target_loss(
                        store,
                        storage,
                        target,
                        &pending,
                        active_binding_revision,
                        TurnIncompleteReason::CompletionMismatch,
                        limit,
                        context_compaction_timeout,
                        Some(&lifecycle.stop),
                    )? {
                        return Ok(outcome);
                    }
                    return Err(OrdinaryTurnExecutionError::Invariant(
                        "routed approval escaped its exact live target",
                    ));
                }
            }
            LiveEventPoll::DynamicTool(call) => {
                if call.thread_id() != target.cas_thread_id() || call.turn_id() != &cas_turn_id {
                    if let Some(outcome) = converge_target_loss(
                        store,
                        storage,
                        target,
                        &pending,
                        active_binding_revision,
                        TurnIncompleteReason::CompletionMismatch,
                        limit,
                        context_compaction_timeout,
                        Some(&lifecycle.stop),
                    )? {
                        return Ok(outcome);
                    }
                    return Err(OrdinaryTurnExecutionError::Invariant(
                        "routed dynamic tool escaped its exact live target",
                    ));
                }
                if let Err(primary) = handle_dynamic_tool(&target, context, call, tools) {
                    if let Some(outcome) = converge_target_loss(
                        store,
                        storage,
                        target,
                        &pending,
                        active_binding_revision,
                        TurnIncompleteReason::WorkerStopped,
                        limit,
                        context_compaction_timeout,
                        Some(&lifecycle.stop),
                    )? {
                        return Ok(outcome);
                    }
                    return Err(primary);
                }
            }
            LiveEventPoll::ProvenTerminal(outcome) => {
                return finish_proven_terminal(
                    store,
                    storage,
                    target,
                    &pending,
                    active_binding_revision,
                    outcome,
                    limit,
                    context_compaction_timeout,
                    Some(&lifecycle.stop),
                );
            }
            LiveEventPoll::Quiet => {}
            LiveEventPoll::Closed(reason) => {
                if let Some(outcome) = converge_target_loss(
                    store,
                    storage,
                    target,
                    &pending,
                    active_binding_revision,
                    TurnIncompleteReason::StreamLost,
                    limit,
                    context_compaction_timeout,
                    Some(&lifecycle.stop),
                )? {
                    return Ok(outcome);
                }
                return Ok(OrdinaryTurnExecutionOutcome::Incomplete {
                    reason: completion_unknown.map_or(
                        OrdinaryTurnCaptureLoss::TargetClosed(reason),
                        OrdinaryTurnCaptureLoss::StartCompletionUnknown,
                    ),
                });
            }
        }
    })();
    lifecycle.transferred = matches!(
        &result,
        Ok(OrdinaryTurnExecutionOutcome::LifecycleContinuationScheduled { .. })
    );
    result
}

fn handle_dynamic_tool(
    target: &LiveEventTarget,
    context: OrdinaryDynamicToolContext,
    call: crate::cas_projection::connection::RoutedDynamicToolCall,
    tools: &mut OrdinaryDynamicToolHandlers<'_>,
) -> Result<(), OrdinaryTurnExecutionError> {
    let (response_owner, request) = call.into_parts();
    let response = match request {
        crate::conversation_tools::RoutedDynamicToolRequest::LifecycleYield(request) => {
            tools.respond_lifecycle_yield(context, request)
        }
        crate::conversation_tools::RoutedDynamicToolRequest::BranchDiscussionResolution(
            request,
        ) => tools.respond_branch_discussion_resolution(context, request),
        crate::conversation_tools::RoutedDynamicToolRequest::Rejected(rejection) => {
            rejection.response()
        }
    };
    target.respond_dynamic_tool_call(response_owner, response)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn converge_target_loss(
    store: &HomeStore,
    storage: &SyndicStorage,
    target: LiveEventTarget,
    pending: &PendingOrdinaryExecution,
    active_binding_revision: BindingRevision,
    cause: TurnIncompleteReason,
    limit: SyndicPointReadLimit,
    context_compaction_timeout: &ContextCompactionTimeoutPolicy,
    lifecycle_stop: Option<&Arc<StopCoordinator>>,
) -> Result<Option<OrdinaryTurnExecutionOutcome>, OrdinaryTurnExecutionError> {
    let accepted_next_ready = target.accepted_next_ready_notifier();
    match target.converge_source_loss(cause)? {
        LiveEventTargetLossOutcome::Incomplete => {
            converge_terminal_history(
                store,
                storage,
                pending.thread_id,
                pending.turn_id,
                pending.minimum_observed_at,
                limit,
            )?;
            if let Some(stop) = lifecycle_stop {
                stop.observe_terminal_lifecycle_yield(pending.thread_id, pending.turn_id);
            }
            accepted_next_ready.notify();
            Ok(None)
        }
        LiveEventTargetLossOutcome::ProvenTerminal { target, outcome } => finish_proven_terminal(
            store,
            storage,
            target,
            pending,
            active_binding_revision,
            outcome,
            limit,
            context_compaction_timeout,
            lifecycle_stop,
        )
        .map(Some),
    }
}

fn finish_proven_terminal(
    store: &HomeStore,
    storage: &SyndicStorage,
    mut target: LiveEventTarget,
    pending: &PendingOrdinaryExecution,
    active_binding_revision: BindingRevision,
    outcome: crate::cas_projection::connection::ProvenTerminalOutcome,
    limit: SyndicPointReadLimit,
    context_compaction_timeout: &ContextCompactionTimeoutPolicy,
    lifecycle_stop: Option<&Arc<StopCoordinator>>,
) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionError> {
    let binding = storage
        .current_binding(store, pending.thread_id, limit)?
        .ok_or(OrdinaryTurnExecutionError::Invariant(
            "terminal publication did not leave a current valid binding",
        ))?;
    let expected_revision = active_binding_revision.checked_next().map_err(|_| {
        OrdinaryTurnExecutionError::Invariant("terminal binding revision exhausted")
    })?;
    if binding.binding().revision() != expected_revision
        || !matches!(
            binding.binding().state(),
            syndic_storage::BindingState::Valid(_)
        )
    {
        return Err(OrdinaryTurnExecutionError::Invariant(
            "terminal outcome disagreed with the durable valid binding",
        ));
    }
    let accepted_next_ready = target.accepted_next_ready_notifier();
    let stop_coordinator = match lifecycle_stop {
        Some(stop) => Arc::clone(stop),
        None => target.stop_coordinator()?,
    };
    let context_compaction = target.context_compaction_coordinator()?;
    let projection = target
        .into_proven_terminal_projection()?
        .with_binding_revision(binding.binding().revision());
    converge_terminal_history(
        store,
        storage,
        pending.thread_id,
        pending.turn_id,
        outcome.observed_at(),
        limit,
    )?;
    stop_coordinator.observe_terminal_lifecycle_yield(pending.thread_id, pending.turn_id);
    accepted_next_ready.notify();
    match context_compaction.begin_lifecycle_continuation(
        projection,
        pending.turn_id,
        context_compaction_timeout,
    ) {
        Ok(crate::cas_projection::context_compaction::LifecycleCompactionAdmission::Launched) => {
            return Ok(
                OrdinaryTurnExecutionOutcome::LifecycleContinuationScheduled {
                    status: outcome.status(),
                },
            );
        }
        Ok(
            crate::cas_projection::context_compaction::LifecycleCompactionAdmission::NotLaunched(
                projection,
            ),
        ) => {
            return Ok(OrdinaryTurnExecutionOutcome::Terminal {
                projection: Box::new(projection),
                status: outcome.status(),
            });
        }
        Err(error) => {
            let _ =
                stop_coordinator.take_terminal_lifecycle_yield(pending.thread_id, pending.turn_id);
            return Err(error.into());
        }
    }
}
