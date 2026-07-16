use beryl_backend::{NonIdempotentRequestOutcome, UserInput};
use beryl_home_store::HomeStore;
use beryl_model::{BindingRevision, SyndicExecutionSnapshotId};
use syndic_storage::{
    AbandonActiveBinding, ActivateBinding, CancelBindingActivation, SyndicPointReadLimit,
    SyndicStorage, TurnIncompleteReason,
};

use super::{
    capture_loop::{StartIdentityEvidence, begin_capture, wait_for_start_evidence},
    cleanup::abandon_without_cas_turn,
    identity::{execution_snapshot_id, point_limit, stale_binding},
};
use crate::cas_projection::connection::TargetTurnStartOutcome;
use crate::cas_projection::ordinary::{
    OrdinaryDynamicToolHandler, OrdinaryNotStartedProjection, OrdinaryTurnCaptureLoss,
    OrdinaryTurnExecutionError, OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionRequest,
    OrdinaryTurnNotStarted, capture::system_timestamp_at_least,
    preflight::PendingOrdinaryExecution,
};
use crate::cas_projection::{
    CasProjectionCoordinator, LiveEventTarget, LoadedCasProjection, publication,
};

pub(super) fn execute(
    coordinator: &CasProjectionCoordinator,
    store: &HomeStore,
    storage: SyndicStorage,
    projection: LoadedCasProjection,
    request: &OrdinaryTurnExecutionRequest,
    tools: &mut impl OrdinaryDynamicToolHandler,
) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionError> {
    coordinator.ensure_home(store)?;
    if projection.home_id() != coordinator.home_id()
        || projection.home_generation() != coordinator.home_generation()
    {
        return Err(OrdinaryTurnExecutionError::ProjectionMismatch {
            thread_id: projection.syndic_thread_id(),
        });
    }
    let _flight = coordinator.begin_projection(projection.syndic_thread_id())?;
    let limit = point_limit();
    let pending = PendingOrdinaryExecution::read(store, storage, &projection, limit)?;
    let input = pending.assemble_input(store, storage)?;
    let started_at = system_timestamp_at_least(pending.minimum_observed_at)?;
    let snapshot_id = execution_snapshot_id(coordinator, &projection, &pending);
    let activation = ActivateBinding::new(
        pending.thread_id,
        pending.binding_revision,
        pending.gate_revision,
        pending.selected_path,
        snapshot_id,
        pending.turn_id,
        projection.loaded_session_generation(),
        started_at,
    );
    let (active_binding_revision, active_gate_revision) =
        publication::activate(store, storage, &activation, limit)?;
    let stale = stale_binding(&projection, &pending, started_at)?;
    let target = match projection.into_pending_live_event_target() {
        Ok(target) => target,
        Err(error) => {
            publication::abandon_active(
                store,
                storage,
                &AbandonActiveBinding::new(
                    pending.thread_id,
                    active_binding_revision,
                    active_gate_revision,
                    pending.selected_path,
                    stale,
                ),
                limit,
            )?;
            return Err(error.into());
        }
    };
    let start = match target.start_turn(
        vec![UserInput::text(input)],
        request.start_options().clone(),
        request.request_timeout(),
    ) {
        Ok(start) => start,
        Err(error) => {
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
            return Ok(OrdinaryTurnExecutionOutcome::Incomplete {
                reason: OrdinaryTurnCaptureLoss::StartAuthorityLost(Box::new(error)),
            });
        }
    };
    match start.outcome() {
        NonIdempotentRequestOutcome::ExactRejection { .. }
        | NonIdempotentRequestOutcome::ProvenNotDispatched { .. } => finish_not_started(
            store,
            storage,
            target,
            start,
            pending,
            active_binding_revision,
            active_gate_revision,
            snapshot_id,
            limit,
        ),
        NonIdempotentRequestOutcome::ExactResponse { response } => {
            let cas_turn_id = response.turn_id().clone();
            begin_capture(
                store,
                storage,
                target,
                start,
                pending,
                active_binding_revision,
                active_gate_revision,
                snapshot_id,
                cas_turn_id,
                StartIdentityEvidence::Response,
                stale,
                tools,
                limit,
            )
        }
        NonIdempotentRequestOutcome::CompletionUnknown { .. } => wait_for_start_evidence(
            store,
            storage,
            target,
            start,
            pending,
            active_binding_revision,
            active_gate_revision,
            snapshot_id,
            stale,
            tools,
            limit,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn finish_not_started(
    store: &HomeStore,
    storage: SyndicStorage,
    target: LiveEventTarget,
    start: TargetTurnStartOutcome,
    pending: PendingOrdinaryExecution,
    active_binding_revision: BindingRevision,
    active_gate_revision: beryl_model::InputGateRevision,
    snapshot_id: SyndicExecutionSnapshotId,
    limit: SyndicPointReadLimit,
) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionError> {
    let cancellation = CancelBindingActivation::new(
        pending.thread_id,
        active_binding_revision,
        active_gate_revision,
        pending.selected_path,
        snapshot_id,
        pending.turn_id,
    );
    let (valid_revision, _) = publication::cancel_activation(store, storage, &cancellation, limit)?;
    let projection = match target.into_not_started_projection(&start) {
        Ok(projection) => OrdinaryNotStartedProjection::Retained(Box::new(
            projection.with_binding_revision(valid_revision),
        )),
        Err(error) => OrdinaryNotStartedProjection::Unavailable {
            reason: error.to_string().into_boxed_str(),
        },
    };
    let (outcome, _) = start.into_parts();
    let reason = match outcome {
        NonIdempotentRequestOutcome::ExactRejection { error } => {
            OrdinaryTurnNotStarted::ExactRejection(error)
        }
        NonIdempotentRequestOutcome::ProvenNotDispatched { error } => {
            OrdinaryTurnNotStarted::ProvenNotDispatched(error)
        }
        NonIdempotentRequestOutcome::ExactResponse { .. }
        | NonIdempotentRequestOutcome::CompletionUnknown { .. } => {
            return Err(OrdinaryTurnExecutionError::Invariant(
                "not-started handoff accepted a possibly dispatched outcome",
            ));
        }
    };
    Ok(OrdinaryTurnExecutionOutcome::NotStarted { projection, reason })
}
