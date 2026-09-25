use super::{
    super::{super::failure, authority::LeaseValidationAuthority},
    execution::projection_error_cut_correlated,
};
use crate::cas_projection::{
    LoadedCasProjection, LoadedProjectionReleaseError, OrdinaryNotStartedProjection,
    OrdinaryTurnCaptureLoss, OrdinaryTurnExecutionError, OrdinaryTurnExecutionFailure,
    OrdinaryTurnExecutionOutcome,
};

pub(in crate::cas_projection::accepted_input_scheduler) fn execute_retained_projection(
    mut projection: LoadedCasProjection,
    mut execute: impl FnMut(
        LoadedCasProjection,
    ) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionFailure>,
) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionFailure> {
    loop {
        match execute(projection) {
            Err(OrdinaryTurnExecutionFailure::PreActivation {
                projection: retained,
                source,
            }) if matches!(
                source,
                OrdinaryTurnExecutionError::ConcurrentChange { .. }
                    | OrdinaryTurnExecutionError::Read(
                        syndic_storage::SyndicReadError::ConcurrentChange { .. }
                    )
                    | OrdinaryTurnExecutionError::ActivityEnrollment(
                        crate::runtime_activity_enrollment::ActivityEnrollmentCustodyError::Read(
                            syndic_storage::SyndicReadError::ConcurrentChange { .. }
                        )
                    )
                    | OrdinaryTurnExecutionError::HomeCommandNotCommitted(
                        beryl_home_store::CommandError::Conflict { .. }
                    )
            ) =>
            {
                projection = *retained;
            }
            outcome => return outcome,
        }
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn settle_ordinary_outcome(
    validator: &LeaseValidationAuthority,
    outcome: Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionFailure>,
) -> OrdinaryTurnSettlement {
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
    let settlement = ordinary_typed_settlement(cut_correlated);
    if settlement == OrdinaryTurnSettlement::PersistentHomeFailure {
        let _ = validator.observe_persistent_failure();
    }
    settlement
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection::accepted_input_scheduler) enum OrdinaryTurnSettlement {
    Settled,
    PersistentHomeFailure,
}

fn ordinary_typed_settlement(cut_correlated: bool) -> OrdinaryTurnSettlement {
    if cut_correlated {
        OrdinaryTurnSettlement::PersistentHomeFailure
    } else {
        OrdinaryTurnSettlement::Settled
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

pub(in crate::cas_projection::accepted_input_scheduler) fn ordinary_error_cut_correlated(
    error: &OrdinaryTurnExecutionError,
    home_generation: beryl_home_store::HomeGeneration,
) -> bool {
    match error {
        OrdinaryTurnExecutionError::Coordinator(source) => {
            failure::is_cut_correlated_coordinator(source, home_generation)
        }
        OrdinaryTurnExecutionError::HomeRead(source)
        | OrdinaryTurnExecutionError::ContextCompaction(
            crate::cas_projection::ContextCompactionError::HomeRead(source),
        ) => failure::is_cut_correlated_read(source, home_generation),
        OrdinaryTurnExecutionError::HomeCommandNotCommitted(source) => {
            failure::is_cut_correlated_command(source, home_generation)
        }
        OrdinaryTurnExecutionError::HandoffSettlement(source) => {
            use crate::discussion_settlement::DiscussionSettlementError;
            match source {
                DiscussionSettlementError::Read(source)
                | DiscussionSettlementError::Syndic(syndic_storage::SyndicReadError::Read(
                    source,
                ))
                | DiscussionSettlementError::SyndicMutation(
                    syndic_storage::SyndicMutationError::Read(source),
                )
                | DiscussionSettlementError::State(beryl_state::DurableJobMutationError::Read(
                    source,
                )) => failure::is_cut_correlated_read(source, home_generation),
                DiscussionSettlementError::Command(source) => {
                    failure::is_cut_correlated_command(source, home_generation)
                }
                _ => false,
            }
        }
        OrdinaryTurnExecutionError::HomeCommandCommitted { later_failure, .. } => {
            failure::is_cut_correlated_command(later_failure, home_generation)
        }
        OrdinaryTurnExecutionError::HomeCommandIndeterminate {
            failure: source, ..
        } => failure::is_cut_correlated_command(source, home_generation),
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
        OrdinaryTurnExecutionError::ActivityEnrollment(
            crate::runtime_activity_enrollment::ActivityEnrollmentCustodyError::Read(
                syndic_storage::SyndicReadError::Read(source),
            ),
        ) => failure::is_cut_correlated_read(source, home_generation),
        OrdinaryTurnExecutionError::AssetRead(beryl_state::AssetReadError::Read(source)) => {
            failure::is_cut_correlated_read(source, home_generation)
        }
        OrdinaryTurnExecutionError::TargetRegistration(
            crate::cas_projection::LiveEventTargetRegistrationError::ProjectionRegistry(source),
        ) => failure::is_cut_correlated_coordinator(source, home_generation),
        OrdinaryTurnExecutionError::ProjectionExecution(source) => {
            projection_error_cut_correlated(source, home_generation)
        }
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
