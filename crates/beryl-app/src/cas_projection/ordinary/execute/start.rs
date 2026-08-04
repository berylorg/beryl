use beryl_backend::NonIdempotentRequestOutcome;
use beryl_home_store::HomeStore;
use beryl_model::{BindingRevision, SyndicExecutionSnapshotId};
use beryl_state::AssetState;
use syndic_storage::{
    ActivateBinding, CancelBindingActivation, SyndicPointReadLimit, SyndicStorage,
    TurnIncompleteReason,
};

use super::{
    capture_loop::{begin_capture, converge_completion_unknown_start, converge_target_loss},
    identity::{execution_snapshot_id, system_timestamp_at_least},
};
use crate::cas_projection::connection::TargetTurnStartOutcome;
use crate::cas_projection::ordinary::{
    OrdinaryDynamicToolHandlers, OrdinaryNotStartedProjection, OrdinaryTurnCaptureLoss,
    OrdinaryTurnExecutionError, OrdinaryTurnExecutionFailure, OrdinaryTurnExecutionOutcome,
    OrdinaryTurnExecutionRequest, OrdinaryTurnNotStarted, preflight::PendingOrdinaryExecution,
};
use crate::cas_projection::{
    CasProjectionCoordinator, LiveEventTarget, LoadedCasProjection, PendingTurnActivation,
    ProjectionCancellationToken, ProjectionPublicationFailure,
    input_replay::{
        InputReplayContext, InputReplayFactory, InputReplayRecord, check_cancelled, point_limit,
    },
    publication,
};

#[allow(
    clippy::too_many_arguments,
    reason = "ordinary execution keeps its durable authorities and request-local handlers explicit"
)]
pub(super) fn execute(
    coordinator: &CasProjectionCoordinator,
    store: &HomeStore,
    storage: SyndicStorage,
    assets: AssetState,
    projection: LoadedCasProjection,
    cancellation: &ProjectionCancellationToken,
    request: &OrdinaryTurnExecutionRequest,
    tools: &mut OrdinaryDynamicToolHandlers<'_>,
) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionFailure> {
    let thread_id = projection.syndic_thread_id();
    let flight = match coordinator.begin_projection(thread_id) {
        Ok(flight) => flight,
        Err(source) => return Err(pre_activation(projection, source.into())),
    };
    execute_in_flight(
        coordinator,
        store,
        storage,
        assets,
        projection,
        cancellation,
        request,
        tools,
        &flight,
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "ordinary execution keeps its durable authorities and request-local handlers explicit"
)]
pub(super) fn execute_in_flight(
    coordinator: &CasProjectionCoordinator,
    store: &HomeStore,
    storage: SyndicStorage,
    assets: AssetState,
    projection: LoadedCasProjection,
    cancellation: &ProjectionCancellationToken,
    request: &OrdinaryTurnExecutionRequest,
    tools: &mut OrdinaryDynamicToolHandlers<'_>,
    flight: &crate::cas_projection::service::ProjectionFlight,
) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionFailure> {
    macro_rules! retain_projection {
        ($result:expr) => {
            match $result {
                Ok(value) => value,
                Err(source) => {
                    let source: OrdinaryTurnExecutionError = source.into();
                    return Err(pre_activation(projection, source));
                }
            }
        };
    }

    retain_projection!(check_cancelled(cancellation));
    retain_projection!(coordinator.ensure_home(store));
    if projection.home_id() != coordinator.home_id()
        || projection.home_generation() != coordinator.home_generation()
    {
        let thread_id = projection.syndic_thread_id();
        return Err(pre_activation(
            projection,
            OrdinaryTurnExecutionError::ProjectionMismatch { thread_id },
        ));
    }
    retain_projection!(coordinator.ensure_projection_flight(flight, projection.syndic_thread_id()));
    let limit = point_limit();
    let pending = retain_projection!(PendingOrdinaryExecution::read(
        store,
        storage,
        assets,
        &projection,
        limit,
    ));
    let prepared = retain_projection!(InputReplayFactory::prepare(
        store,
        storage,
        assets,
        InputReplayContext::from_projection(&projection),
        InputReplayRecord::submitted(pending.thread_id, pending.item_id),
        pending.input,
        pending.asset_reference_set,
        pending.asset_owner_head.clone(),
        cancellation,
        #[cfg(feature = "test-faults")]
        request.input_replay_diagnostics(),
    ));
    let started_at = retain_projection!(system_timestamp_at_least(pending.minimum_observed_at));
    retain_projection!(check_cancelled(cancellation));
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
        match activate(store, storage, &activation, limit) {
            ActivationAttempt::Activated {
                binding_revision,
                gate_revision,
            } => (binding_revision, gate_revision),
            ActivationAttempt::ProvenPrior(source) => {
                return Err(pre_activation(projection, source.into()));
            }
            ActivationAttempt::AuthorityUncertain(source) => {
                return Err(activation_failure(source.into()));
            }
        };
    let activation = PendingTurnActivation::new(
        pending.thread_id,
        pending.turn_id,
        active_binding_revision,
        active_gate_revision,
        pending.state_revision,
        snapshot_id,
        started_at,
    );
    let target = match projection.into_pending_live_event_target(activation) {
        Ok(target) => target,
        Err(error) => return Err(activation_failure(error.into())),
    };
    #[cfg(feature = "test-faults")]
    let target = {
        let mut target = target;
        crate::cas_projection::test_faults::abandon_live_event_target_if_requested(&mut target);
        target
    };
    let mut replay = prepared.fresh_source();
    let start = match target.start_streamed_turn(
        request.start_options().clone(),
        request.request_timeout(),
        replay.service(store, storage, cancellation),
    ) {
        Ok(start) => start,
        Err(error) => {
            if let Some(outcome) = converge_target_loss(
                store,
                storage,
                target,
                &pending,
                active_binding_revision,
                TurnIncompleteReason::AuthorityLost,
                limit,
            )
            .map_err(after_activation)?
            {
                return Ok(outcome);
            }
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
        )
        .map_err(after_activation),
        NonIdempotentRequestOutcome::ExactResponse { response } => {
            let cas_turn_id = response.turn_id().clone();
            begin_capture(
                store,
                storage,
                target,
                start,
                pending,
                active_binding_revision,
                cas_turn_id,
                tools,
                limit,
                request.context_compaction_timeout(),
            )
            .map_err(after_activation)
        }
        NonIdempotentRequestOutcome::CompletionUnknown { .. } => converge_completion_unknown_start(
            store,
            storage,
            target,
            start,
            pending,
            active_binding_revision,
            limit,
        )
        .map_err(after_activation),
    }
}

enum ActivationAttempt {
    Activated {
        binding_revision: BindingRevision,
        gate_revision: beryl_model::InputGateRevision,
    },
    ProvenPrior(ProjectionPublicationFailure),
    AuthorityUncertain(ProjectionPublicationFailure),
}

fn activate(
    store: &HomeStore,
    storage: SyndicStorage,
    activation: &ActivateBinding,
    limit: SyndicPointReadLimit,
) -> ActivationAttempt {
    match publication::activate(store, storage, activation, limit) {
        Ok((binding_revision, gate_revision)) => ActivationAttempt::Activated {
            binding_revision,
            gate_revision,
        },
        Err(source @ ProjectionPublicationFailure::Prior)
        | Err(source @ ProjectionPublicationFailure::Command(_)) => {
            ActivationAttempt::ProvenPrior(source)
        }
        Err(source) => ActivationAttempt::AuthorityUncertain(source),
    }
}

fn pre_activation(
    projection: LoadedCasProjection,
    source: OrdinaryTurnExecutionError,
) -> OrdinaryTurnExecutionFailure {
    OrdinaryTurnExecutionFailure::PreActivation {
        projection: Box::new(projection),
        source,
    }
}

fn activation_failure(source: OrdinaryTurnExecutionError) -> OrdinaryTurnExecutionFailure {
    OrdinaryTurnExecutionFailure::Activation { source }
}

fn after_activation(source: OrdinaryTurnExecutionError) -> OrdinaryTurnExecutionFailure {
    OrdinaryTurnExecutionFailure::AfterActivation { source }
}

#[allow(clippy::too_many_arguments)]
fn finish_not_started(
    store: &HomeStore,
    storage: SyndicStorage,
    mut target: LiveEventTarget,
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
