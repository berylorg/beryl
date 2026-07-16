use std::time::Duration;

use beryl_backend::NonIdempotentRequestOutcome;
use beryl_home_store::HomeStore;
use beryl_model::{BindingRevision, SyndicExecutionSnapshotId};
use syndic_storage::{
    PublishActiveCasTurn, StaleCasBinding, SyndicPointReadLimit, SyndicStorage,
    TurnIncompleteReason,
};

use super::cleanup::{abandon_and_close_incomplete, abandon_without_cas_turn};
use crate::cas_projection::connection::TargetTurnStartOutcome;
use crate::cas_projection::ordinary::{
    OrdinaryDynamicToolContext, OrdinaryDynamicToolHandler, OrdinaryTurnCaptureLoss,
    OrdinaryTurnExecutionError, OrdinaryTurnExecutionOutcome, capture::LiveCapture,
    converge::converge_terminal_history, preflight::PendingOrdinaryExecution,
};
use crate::cas_projection::{LiveEventPoll, LiveEventTarget, publication};

const LIVE_POLL_INTERVAL: Duration = Duration::from_millis(100);

pub(super) enum StartIdentityEvidence {
    Response,
    RoutedStartEvent,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn begin_capture(
    store: &HomeStore,
    storage: SyndicStorage,
    target: LiveEventTarget,
    start: TargetTurnStartOutcome,
    pending: PendingOrdinaryExecution,
    active_binding_revision: BindingRevision,
    active_gate_revision: beryl_model::InputGateRevision,
    snapshot_id: SyndicExecutionSnapshotId,
    cas_turn_id: beryl_model::CasTurnId,
    identity_evidence: StartIdentityEvidence,
    stale: StaleCasBinding,
    tools: &mut impl OrdinaryDynamicToolHandler,
    limit: SyndicPointReadLimit,
) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionError> {
    let confirmation = match identity_evidence {
        StartIdentityEvidence::Response => Some(target.confirm_turn(cas_turn_id.clone())),
        StartIdentityEvidence::RoutedStartEvent => None,
    };
    let active_turn = PublishActiveCasTurn::new(
        pending.thread_id,
        active_binding_revision,
        active_gate_revision,
        snapshot_id,
        target.cas_thread_id().clone(),
        cas_turn_id.clone(),
        stale.observed_at(),
    );
    let published_gate_revision =
        match publication::publish_active_turn(store, storage, &active_turn, limit) {
            Ok(revision) => revision,
            Err(primary) => {
                abandon_without_cas_turn(
                    store,
                    storage,
                    &pending,
                    active_binding_revision,
                    active_gate_revision,
                    stale,
                    TurnIncompleteReason::AuthorityLost,
                    limit,
                )?;
                drop(target);
                return Err(primary.into());
            }
        };
    let mut capture = LiveCapture::new(
        OrdinaryDynamicToolContext::new(pending.thread_id, pending.turn_id),
        target.cas_thread_id().clone(),
        cas_turn_id,
        pending.item_id,
        pending.input,
        pending.state_revision,
        published_gate_revision,
        pending.minimum_observed_at,
    );
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
    if let Some(Err(error)) = confirmation {
        abandon_and_close_incomplete(
            store,
            storage,
            &mut capture,
            &pending,
            active_binding_revision,
            stale,
            TurnIncompleteReason::CompletionMismatch,
            limit,
        )?;
        drop(target);
        return Ok(OrdinaryTurnExecutionOutcome::Incomplete {
            reason: completion_unknown.map_or(
                OrdinaryTurnCaptureLoss::TargetConfirmationFailed(error),
                OrdinaryTurnCaptureLoss::StartCompletionUnknown,
            ),
        });
    }
    if let Err(primary) = capture.activate(store, storage, limit) {
        abandon_and_close_incomplete(
            store,
            storage,
            &mut capture,
            &pending,
            active_binding_revision,
            stale,
            TurnIncompleteReason::AuthorityLost,
            limit,
        )?;
        drop(target);
        return Err(primary);
    }
    run_capture(
        store,
        storage,
        target,
        capture,
        pending,
        active_binding_revision,
        stale,
        completion_unknown,
        tools,
        limit,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn wait_for_start_evidence(
    store: &HomeStore,
    storage: SyndicStorage,
    target: LiveEventTarget,
    start: TargetTurnStartOutcome,
    pending: PendingOrdinaryExecution,
    active_binding_revision: BindingRevision,
    active_gate_revision: beryl_model::InputGateRevision,
    snapshot_id: SyndicExecutionSnapshotId,
    stale: StaleCasBinding,
    tools: &mut impl OrdinaryDynamicToolHandler,
    limit: SyndicPointReadLimit,
) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionError> {
    loop {
        match target.poll(LIVE_POLL_INTERVAL) {
            LiveEventPoll::Event(event) => {
                if !matches!(
                    event.event(),
                    beryl_backend::TurnStreamEvent::TurnStarted { .. }
                ) {
                    abandon_without_cas_turn(
                        store,
                        storage,
                        &pending,
                        active_binding_revision,
                        active_gate_revision,
                        stale,
                        TurnIncompleteReason::CompletionMismatch,
                        limit,
                    )?;
                    drop(target);
                    return Err(OrdinaryTurnExecutionError::Invariant(
                        "a possibly started target delivered work before turn/started",
                    ));
                }
                let Some(cas_turn_id) = event.turn_id().cloned() else {
                    abandon_without_cas_turn(
                        store,
                        storage,
                        &pending,
                        active_binding_revision,
                        active_gate_revision,
                        stale,
                        TurnIncompleteReason::CompletionMismatch,
                        limit,
                    )?;
                    drop(target);
                    return Err(OrdinaryTurnExecutionError::Invariant(
                        "turn/started lacks its routed CAS turn identity",
                    ));
                };
                return begin_capture(
                    store,
                    storage,
                    target,
                    start,
                    pending,
                    active_binding_revision,
                    active_gate_revision,
                    snapshot_id,
                    cas_turn_id,
                    StartIdentityEvidence::RoutedStartEvent,
                    stale,
                    tools,
                    limit,
                );
            }
            LiveEventPoll::Quiet => {}
            LiveEventPoll::Closed(reason) => {
                let (outcome, _) = start.into_parts();
                let NonIdempotentRequestOutcome::CompletionUnknown { error } = outcome else {
                    return Err(OrdinaryTurnExecutionError::Invariant(
                        "start-evidence wait lost its completion-unknown outcome",
                    ));
                };
                abandon_without_cas_turn(
                    store,
                    storage,
                    &pending,
                    active_binding_revision,
                    active_gate_revision,
                    stale,
                    TurnIncompleteReason::StreamLost,
                    limit,
                )?;
                drop(target);
                let _ = reason;
                return Ok(OrdinaryTurnExecutionOutcome::Incomplete {
                    reason: OrdinaryTurnCaptureLoss::StartCompletionUnknown(error),
                });
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_capture(
    store: &HomeStore,
    storage: SyndicStorage,
    target: LiveEventTarget,
    mut capture: LiveCapture,
    pending: PendingOrdinaryExecution,
    active_binding_revision: BindingRevision,
    stale: StaleCasBinding,
    completion_unknown: Option<Box<beryl_backend::ManagedBackendError>>,
    tools: &mut impl OrdinaryDynamicToolHandler,
    limit: SyndicPointReadLimit,
) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionError> {
    loop {
        match target.poll(LIVE_POLL_INTERVAL) {
            LiveEventPoll::Event(event) => {
                if event.thread_id() != target.cas_thread_id()
                    || event
                        .turn_id()
                        .is_some_and(|turn| turn != capture.cas_turn_id())
                {
                    abandon_and_close_incomplete(
                        store,
                        storage,
                        &mut capture,
                        &pending,
                        active_binding_revision,
                        stale,
                        TurnIncompleteReason::CompletionMismatch,
                        limit,
                    )?;
                    drop(target);
                    return Err(OrdinaryTurnExecutionError::Invariant(
                        "routed event escaped its exact live target",
                    ));
                }
                match capture.handle_event(
                    store,
                    storage,
                    &target,
                    event.into_event(),
                    tools,
                    limit,
                ) {
                    Ok(Some(status)) => {
                        let terminal_observed_at = capture.minimum_observed_at();
                        let valid_revision =
                            active_binding_revision.checked_next().map_err(|_| {
                                OrdinaryTurnExecutionError::Invariant(
                                    "terminal binding revision exhausted",
                                )
                            })?;
                        let projection = target
                            .into_proven_terminal_projection()?
                            .with_binding_revision(valid_revision);
                        converge_terminal_history(
                            store,
                            storage,
                            pending.thread_id,
                            pending.turn_id,
                            terminal_observed_at,
                            limit,
                        )?;
                        return Ok(OrdinaryTurnExecutionOutcome::Terminal {
                            projection: Box::new(projection),
                            status,
                        });
                    }
                    Ok(None) => {}
                    Err(primary) => {
                        abandon_and_close_incomplete(
                            store,
                            storage,
                            &mut capture,
                            &pending,
                            active_binding_revision,
                            stale,
                            TurnIncompleteReason::WorkerStopped,
                            limit,
                        )?;
                        drop(target);
                        return Err(primary);
                    }
                }
            }
            LiveEventPoll::Quiet => {}
            LiveEventPoll::Closed(reason) => {
                abandon_and_close_incomplete(
                    store,
                    storage,
                    &mut capture,
                    &pending,
                    active_binding_revision,
                    stale,
                    TurnIncompleteReason::StreamLost,
                    limit,
                )?;
                drop(target);
                return Ok(OrdinaryTurnExecutionOutcome::Incomplete {
                    reason: completion_unknown.map_or(
                        OrdinaryTurnCaptureLoss::TargetClosed(reason),
                        OrdinaryTurnCaptureLoss::StartCompletionUnknown,
                    ),
                });
            }
        }
    }
}
