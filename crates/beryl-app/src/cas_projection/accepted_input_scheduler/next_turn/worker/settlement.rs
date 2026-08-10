use super::{
    super::{super::failure, authority::LeaseValidationAuthority},
    execution::{projection_error_cut_correlated, projection_error_verification_pending},
};
use crate::cas_projection::{
    LoadedProjectionReleaseError, OrdinaryNotStartedProjection, OrdinaryTurnCaptureLoss,
    OrdinaryTurnExecutionError, OrdinaryTurnExecutionFailure, OrdinaryTurnExecutionOutcome,
};

pub(in crate::cas_projection::accepted_input_scheduler) fn settle_ordinary_outcome(
    validator: &LeaseValidationAuthority,
    outcome: Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionFailure>,
) -> OrdinaryTurnSettlement {
    let verification_pending =
        ordinary_outcome_verification_pending(&outcome, validator.home_generation());
    let cut_correlated = ordinary_outcome_cut_correlated(&outcome, validator.home_generation());
    match outcome {
        Ok(OrdinaryTurnExecutionOutcome::NotStarted {
            projection: OrdinaryNotStartedProjection::Retained(projection),
            ..
        }) => {
            release_or_retain_projection(validator, projection);
        }
        Ok(OrdinaryTurnExecutionOutcome::NotStarted {
            projection: OrdinaryNotStartedProjection::Unavailable { .. },
            ..
        }) => {}
        Ok(OrdinaryTurnExecutionOutcome::Terminal { projection, .. }) => {
            release_or_retain_projection(validator, projection);
        }
        Ok(OrdinaryTurnExecutionOutcome::ReacquisitionRequired { anchor, .. }) => {
            if validator.observe_persistent_failure() {
                validator.retain_failed_reacquisition_anchor(*anchor);
            }
        }
        Ok(OrdinaryTurnExecutionOutcome::LifecycleContinuationScheduled { .. }) => {}
        Ok(OrdinaryTurnExecutionOutcome::Incomplete { .. }) => {}
        Err(OrdinaryTurnExecutionFailure::PreActivation { projection, .. }) => {
            release_or_retain_projection(validator, projection);
        }
        Err(
            OrdinaryTurnExecutionFailure::Activation { .. }
            | OrdinaryTurnExecutionFailure::AfterActivation { .. },
        ) => {}
    }
    let settlement = ordinary_typed_settlement(verification_pending, cut_correlated);
    if settlement != OrdinaryTurnSettlement::Settled {
        let _ = validator.observe_persistent_failure();
    }
    settlement
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection::accepted_input_scheduler) enum OrdinaryTurnSettlement {
    Settled,
    VerificationPending,
    PersistentHomeFailure,
}

fn ordinary_typed_settlement(
    verification_pending: bool,
    cut_correlated: bool,
) -> OrdinaryTurnSettlement {
    if verification_pending {
        OrdinaryTurnSettlement::VerificationPending
    } else if cut_correlated {
        OrdinaryTurnSettlement::PersistentHomeFailure
    } else {
        OrdinaryTurnSettlement::Settled
    }
}

fn ordinary_outcome_verification_pending(
    outcome: &Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionFailure>,
    home_generation: beryl_home_store::HomeGeneration,
) -> bool {
    match outcome {
        Ok(OrdinaryTurnExecutionOutcome::Incomplete {
            reason: OrdinaryTurnCaptureLoss::StartAuthorityLost(source),
        }) => projection_error_verification_pending(source, home_generation),
        Err(OrdinaryTurnExecutionFailure::PreActivation { source, .. })
        | Err(OrdinaryTurnExecutionFailure::Activation { source })
        | Err(OrdinaryTurnExecutionFailure::AfterActivation { source }) => {
            ordinary_error_verification_pending(source, home_generation)
        }
        _ => false,
    }
}

fn ordinary_outcome_cut_correlated(
    outcome: &Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionFailure>,
    home_generation: beryl_home_store::HomeGeneration,
) -> bool {
    match outcome {
        Ok(OrdinaryTurnExecutionOutcome::Incomplete {
            reason: OrdinaryTurnCaptureLoss::StartAuthorityLost(source),
        }) => projection_error_cut_correlated(source, home_generation),
        Err(OrdinaryTurnExecutionFailure::PreActivation { source, .. })
        | Err(OrdinaryTurnExecutionFailure::Activation { source })
        | Err(OrdinaryTurnExecutionFailure::AfterActivation { source }) => {
            ordinary_error_cut_correlated(source, home_generation)
        }
        _ => false,
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn ordinary_error_verification_pending(
    error: &OrdinaryTurnExecutionError,
    home_generation: beryl_home_store::HomeGeneration,
) -> bool {
    match error {
        OrdinaryTurnExecutionError::Coordinator(source) => {
            failure::is_verification_pending_coordinator(source, home_generation)
        }
        OrdinaryTurnExecutionError::HomeRead(source) => {
            failure::is_verification_pending_read(source, home_generation)
        }
        OrdinaryTurnExecutionError::HomeCommand(source) => {
            failure::is_verification_pending_command(source, home_generation)
        }
        OrdinaryTurnExecutionError::InputReplayHomeNotHealthy {
            state: beryl_home_store::HomeHealthState::Verifying,
            expected_home_id,
            actual_home_id,
            expected_generation,
            actual_generation: Some(actual_generation),
        } => {
            expected_home_id == actual_home_id
                && *expected_generation == home_generation
                && *actual_generation == home_generation
        }
        OrdinaryTurnExecutionError::Read(syndic_storage::SyndicReadError::Read(source)) => {
            failure::is_verification_pending_read(source, home_generation)
        }
        OrdinaryTurnExecutionError::AssetRead(beryl_state::AssetReadError::Read(source)) => {
            failure::is_verification_pending_read(source, home_generation)
        }
        OrdinaryTurnExecutionError::TargetRegistration(
            crate::cas_projection::LiveEventTargetRegistrationError::ProjectionRegistry(source),
        ) => failure::is_verification_pending_coordinator(source, home_generation),
        OrdinaryTurnExecutionError::ProjectionExecution(source) => {
            projection_error_verification_pending(source, home_generation)
        }
        OrdinaryTurnExecutionError::ReacquisitionAnchor(
            LoadedProjectionReleaseError::Registry(source),
        ) => failure::is_verification_pending_coordinator(source, home_generation),
        OrdinaryTurnExecutionError::Publication(source) => {
            failure::is_verification_pending_publication(source, home_generation)
        }
        OrdinaryTurnExecutionError::InputAssetSidecar(source) => {
            failure::is_verification_pending_sidecar(source, home_generation)
        }
        _ => false,
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn ordinary_error_cut_correlated(
    error: &OrdinaryTurnExecutionError,
    home_generation: beryl_home_store::HomeGeneration,
) -> bool {
    match error {
        OrdinaryTurnExecutionError::Coordinator(source) => {
            failure::is_cut_correlated_coordinator(source, home_generation)
        }
        OrdinaryTurnExecutionError::HomeRead(source) => {
            failure::is_cut_correlated_read(source, home_generation)
        }
        OrdinaryTurnExecutionError::HomeCommand(source) => {
            failure::is_cut_correlated_command(source, home_generation)
        }
        OrdinaryTurnExecutionError::InputReplayHomeNotHealthy {
            state: beryl_home_store::HomeHealthState::Failed,
            expected_home_id,
            actual_home_id,
            expected_generation,
            actual_generation: Some(actual_generation),
        } => {
            expected_home_id == actual_home_id
                && *expected_generation == home_generation
                && *actual_generation == home_generation
        }
        OrdinaryTurnExecutionError::Read(syndic_storage::SyndicReadError::Read(source)) => {
            failure::is_cut_correlated_read(source, home_generation)
        }
        OrdinaryTurnExecutionError::AssetRead(beryl_state::AssetReadError::Read(source)) => {
            failure::is_cut_correlated_read(source, home_generation)
        }
        OrdinaryTurnExecutionError::TargetRegistration(
            crate::cas_projection::LiveEventTargetRegistrationError::ProjectionRegistry(source),
        ) => failure::is_cut_correlated_coordinator(source, home_generation),
        OrdinaryTurnExecutionError::ProjectionExecution(source) => {
            projection_error_cut_correlated(source, home_generation)
        }
        OrdinaryTurnExecutionError::ReacquisitionAnchor(
            LoadedProjectionReleaseError::Registry(source),
        ) => failure::is_cut_correlated_coordinator(source, home_generation),
        OrdinaryTurnExecutionError::Publication(source) => {
            failure::is_cut_correlated_publication(source, home_generation)
        }
        OrdinaryTurnExecutionError::InputAssetSidecar(source) => {
            failure::is_cut_correlated_sidecar(source, home_generation)
        }
        _ => false,
    }
}

fn release_or_retain_projection(
    validator: &LeaseValidationAuthority,
    projection: Box<crate::cas_projection::LoadedCasProjection>,
) {
    if validator.observe_persistent_failure() {
        validator.retain_failed_projection(*projection);
    } else {
        let _ = projection.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/ordinary_turn_settlement.rs"
    ));
}
