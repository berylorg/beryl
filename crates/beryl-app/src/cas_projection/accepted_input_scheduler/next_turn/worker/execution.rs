use syndic_storage::{SelectedPathProof, SyndicTimestamp};

use super::{
    super::{
        super::failure,
        authority::{LeaseValidationAuthority, expected_coordinator_drift},
    },
    settlement::{OrdinaryTurnSettlement, settle_ordinary_outcome},
};
use crate::cas_projection::{
    CasProjectionCoordinator, CasProjectionRequest, LoadedProjectionReleaseError,
    ProjectionCancellationToken, ProjectionCoordinatorError, ProjectionExecutionError,
    ScheduledOrdinaryExecutionLease,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection::accepted_input_scheduler) enum PendingTurnExecutionDisposition {
    Settled,
    ExpectedInterruption,
    PersistentHomeFailure,
    ProjectionRefused,
}

pub(in crate::cas_projection::accepted_input_scheduler) fn execute_pending_turn(
    validator: &LeaseValidationAuthority,
    storage: syndic_storage::SyndicStorage,
    cancellation: &ProjectionCancellationToken,
    observed_at: SyndicTimestamp,
    selected_path: SelectedPathProof,
    lease: &mut ScheduledOrdinaryExecutionLease,
) -> PendingTurnExecutionDisposition {
    let thread_id = lease.thread_id();
    let execution_binding = lease.execution_binding().clone();
    let coordinator = match CasProjectionCoordinator::for_healthy_home(&validator.home) {
        Ok(coordinator) => coordinator,
        Err(error)
            if failure::is_cut_correlated_coordinator(&error, validator.home_generation()) =>
        {
            return PendingTurnExecutionDisposition::PersistentHomeFailure;
        }
        Err(error) if expected_coordinator_drift(&error) => {
            return PendingTurnExecutionDisposition::ExpectedInterruption;
        }
        Err(_) => return PendingTurnExecutionDisposition::ProjectionRefused,
    };
    lease.with_execution_authority(|session, policy, assets, tools, flight| {
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
        ) {
            Ok(projection) => projection,
            Err(error) => return classify_projection_error(error, validator.home_generation()),
        };
        let outcome = coordinator.execute_ordinary_turn_in_flight(
            &validator.home,
            storage,
            assets,
            projection,
            cancellation,
            policy.turn(),
            tools,
            flight,
        );
        match settle_ordinary_outcome(validator, outcome) {
            OrdinaryTurnSettlement::Settled => PendingTurnExecutionDisposition::Settled,
            OrdinaryTurnSettlement::PersistentHomeFailure => {
                PendingTurnExecutionDisposition::PersistentHomeFailure
            }
        }
    })
}
fn classify_projection_error(
    error: ProjectionExecutionError,
    home_generation: beryl_home_store::HomeGeneration,
) -> PendingTurnExecutionDisposition {
    if projection_error_cut_correlated(&error, home_generation) {
        return PendingTurnExecutionDisposition::PersistentHomeFailure;
    }
    match error {
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
        ProjectionExecutionError::LeaseRelease(source) => matches!(
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
