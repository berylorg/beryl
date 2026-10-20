use syndic_storage::{SelectedPathProof, SyndicTimestamp};

use super::{
    super::{
        super::failure,
        authority::{LeaseValidationAuthority, expected_coordinator_drift},
    },
    settlement::{OrdinaryTurnSettlement, execute_retained_projection, settle_ordinary_outcome},
};
use crate::cas_projection::{
    CasProjectionCoordinator, CasProjectionRequest, LoadedProjectionReleaseError,
    NativeLineageHistoryRecovery, NativeLineageRecoveryControl, NativeLineageRecoveryDecision,
    ProjectionCancellationToken, ProjectionCoordinatorError, ProjectionExecutionError,
    ScheduledOrdinaryExecutionLease,
};

#[cfg(all(test, feature = "test-faults"))]
mod source_advance_tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/pending_projection_drift.rs"
    ));
}

pub(in crate::cas_projection::accepted_input_scheduler) enum PendingTurnExecutionDisposition {
    Settled,
    ExpectedInterruption,
    SourceAdvanced,
    RetainedProjectionUnavailable,
    PersistentHomeFailure,
    ProjectionRefused,
    NativeLineageCapacityBlocked,
    ParkNativeLineage {
        decision: Box<NativeLineageRecoveryDecision>,
        history_recovery: NativeLineageHistoryRecovery,
    },
}

pub(in crate::cas_projection::accepted_input_scheduler) fn execute_pending_turn(
    validator: &LeaseValidationAuthority,
    storage: &syndic_storage::SyndicStorage,
    cancellation: &ProjectionCancellationToken,
    observed_at: SyndicTimestamp,
    selected_path: SelectedPathProof,
    native_lineage_recovery: &NativeLineageRecoveryControl,
    lease: &mut ScheduledOrdinaryExecutionLease,
) -> PendingTurnExecutionDisposition {
    let thread_id = lease.thread_id();
    let execution_binding = lease.execution_binding().clone();
    let coordinator = match CasProjectionCoordinator::for_healthy_home(&validator.home) {
        Ok(coordinator) => coordinator,
        Err(error)
            if failure::is_cut_correlated_coordinator(&error, validator.home_generation()) =>
        {
            if let Some(projection) = lease.take_projection() {
                validator.retain_failed_projection(projection);
            }
            return PendingTurnExecutionDisposition::PersistentHomeFailure;
        }
        Err(error) if expected_coordinator_drift(&error) => {
            return PendingTurnExecutionDisposition::ExpectedInterruption;
        }
        Err(_) => return PendingTurnExecutionDisposition::ProjectionRefused,
    };
    let mut retained = lease.take_projection();
    let outcome =
        lease.with_execution_authority(|session, policy, assets, handoff, mut tools, flight| {
            let projection_request = CasProjectionRequest::new(
                thread_id,
                selected_path,
                execution_binding,
                policy.thread_options().clone(),
                policy.model_context_window_tokens(),
                observed_at,
                policy.projection_timeout(),
            );
            let projection = match coordinator.obtain_projection_in_flight(
                &validator.home,
                storage,
                session,
                &projection_request,
                cancellation,
                flight,
                native_lineage_recovery,
                &mut retained,
            ) {
                Ok(projection) => projection,
                Err(ProjectionExecutionError::NativeLineageRecoveryRequired { decision }) => {
                    let history_recovery = NativeLineageHistoryRecovery::from_preflight(
                        coordinator.validate_native_lineage_recovery_in_flight(
                            &validator.home,
                            storage,
                            session,
                            &decision,
                            cancellation,
                        ),
                    );
                    return Err(PendingTurnExecutionDisposition::ParkNativeLineage {
                        decision,
                        history_recovery,
                    });
                }
                Err(ProjectionExecutionError::NativeLineageRouteCapacityFull { .. }) => {
                    return Err(PendingTurnExecutionDisposition::NativeLineageCapacityBlocked);
                }
                Err(error) => {
                    return Err(classify_projection_error(
                        error,
                        validator.home_generation(),
                    ));
                }
            };
            Ok(execute_retained_projection(projection, |projection| {
                coordinator.execute_ordinary_turn_in_flight(
                    &validator.home,
                    storage,
                    assets,
                    handoff,
                    projection,
                    cancellation,
                    policy.turn(),
                    tools.reborrow(),
                    flight,
                )
            }))
        });
    if let Some(projection) = retained
        && let Some(projection) =
            super::settlement::return_or_retain_projection(validator, Box::new(projection))
    {
        lease.return_projection(*projection);
    }
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(disposition) => return disposition,
    };
    let (settlement, returned) = settle_ordinary_outcome(validator, outcome);
    if let Some(projection) = returned {
        lease.return_projection(*projection);
    }
    match settlement {
        OrdinaryTurnSettlement::Settled => PendingTurnExecutionDisposition::Settled,
        OrdinaryTurnSettlement::PersistentHomeFailure => {
            PendingTurnExecutionDisposition::PersistentHomeFailure
        }
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn classify_projection_error(
    error: ProjectionExecutionError,
    home_generation: beryl_home_store::HomeGeneration,
) -> PendingTurnExecutionDisposition {
    classify_projection_error_ref(&error, home_generation)
}

pub(in crate::cas_projection::accepted_input_scheduler) fn classify_projection_error_ref(
    error: &ProjectionExecutionError,
    home_generation: beryl_home_store::HomeGeneration,
) -> PendingTurnExecutionDisposition {
    if projection_error_cut_correlated(&error, home_generation) {
        return PendingTurnExecutionDisposition::PersistentHomeFailure;
    }
    match error {
        ProjectionExecutionError::RetainedProjectionReleaseBeforeAcquisition(_) => {
            PendingTurnExecutionDisposition::RetainedProjectionUnavailable
        }
        ProjectionExecutionError::NativeContextSeedSourceAdvanced { .. }
        | ProjectionExecutionError::ProjectionSourceAdvancedBeforeInjection { .. } => {
            PendingTurnExecutionDisposition::SourceAdvanced
        }
        ProjectionExecutionError::Cancelled => {
            PendingTurnExecutionDisposition::ExpectedInterruption
        }
        ProjectionExecutionError::Coordinator(error)
            if failure::is_cut_correlated_coordinator(&error, home_generation) =>
        {
            PendingTurnExecutionDisposition::PersistentHomeFailure
        }
        ProjectionExecutionError::Coordinator(error) if expected_coordinator_drift(&error) => {
            PendingTurnExecutionDisposition::ExpectedInterruption
        }
        ProjectionExecutionError::Coordinator(
            ProjectionCoordinatorError::ProjectionConnectionUnavailable { .. },
        ) => PendingTurnExecutionDisposition::ExpectedInterruption,
        _ => PendingTurnExecutionDisposition::ProjectionRefused,
    }
}

pub(super) fn projection_error_cut_correlated(
    error: &ProjectionExecutionError,
    home_generation: beryl_home_store::HomeGeneration,
) -> bool {
    match error {
        ProjectionExecutionError::Coordinator(source) => {
            failure::is_cut_correlated_coordinator(source, home_generation)
        }
        ProjectionExecutionError::SyndicRead(syndic_storage::SyndicReadError::Read(source))
        | ProjectionExecutionError::NativePlanning(syndic_storage::NativeProjectionError::Read(
            source,
        ))
        | ProjectionExecutionError::RecoveryProjection(
            syndic_storage::RecoveryProjectionError::Read(source),
        ) => failure::is_cut_correlated_read(source, home_generation),
        ProjectionExecutionError::Publication(source) => {
            failure::is_cut_correlated_publication(source, home_generation)
        }
        ProjectionExecutionError::LeaseRelease(source)
        | ProjectionExecutionError::RetainedProjectionReleaseBeforeAcquisition(source) => matches!(
            source.as_ref(),
            LoadedProjectionReleaseError::Registry(source)
                if failure::is_cut_correlated_coordinator(source, home_generation)
        ),
        ProjectionExecutionError::AbandonmentFailed {
            primary,
            release,
            publication,
        } => {
            projection_error_cut_correlated(primary, home_generation)
                || release.as_deref().is_some_and(|source| {
                    matches!(
                        source,
                        LoadedProjectionReleaseError::Registry(source)
                            if failure::is_cut_correlated_coordinator(source, home_generation)
                    )
                })
                || publication.as_deref().is_some_and(|source| {
                    failure::is_cut_correlated_publication(source, home_generation)
                })
        }
        _ => false,
    }
}
