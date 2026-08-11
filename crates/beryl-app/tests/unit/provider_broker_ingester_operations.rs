use std::{error::Error, fmt};

#[cfg(feature = "test-faults")]
use beryl_home_store::HomeHealthState;
use syndic_storage::{ProviderObservationValidatorError, SyndicRecordError};

use super::*;

#[cfg(feature = "test-faults")]
#[test]
fn provider_seal_rejects_terminal_and_foreign_health_gates_after_verified_current() {
    const EXPECTED_GENERATION: u64 = 41;

    assert!(
        super::super::super::consumer::health_gate_values_match(
            HomeHealthState::Verifying,
            EXPECTED_GENERATION,
            EXPECTED_GENERATION,
        )
    );
    assert!(
        [HomeHealthState::Failed, HomeHealthState::Reopening]
            .into_iter()
            .all(|state| !super::super::super::consumer::health_gate_values_match(
                state,
                EXPECTED_GENERATION,
                EXPECTED_GENERATION,
            ))
    );
    assert!(!super::super::super::consumer::health_gate_values_match(
        HomeHealthState::Verifying,
        EXPECTED_GENERATION + 1,
        EXPECTED_GENERATION,
    ));
}

#[derive(Debug)]
struct PhysicalFailure;

impl fmt::Display for PhysicalFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("physical staging failure")
    }
}

impl Error for PhysicalFailure {}

#[test]
fn staging_rejection_preserves_schema_and_infrastructure_taxonomy() {
    let schema = [
        ProviderObservationStagingError::<PhysicalFailure>::Validation(
            ProviderObservationValidatorError::StructureMismatch,
        ),
        ProviderObservationStagingError::Batch(ProviderObservationStageBatchError::EmptyFragment),
        ProviderObservationStagingError::Batch(
            ProviderObservationStageBatchError::FragmentTooLarge { actual: 65_537 },
        ),
    ];
    for error in schema {
        assert_eq!(
            staging_rejection(&error),
            OrderedTurnStreamRejection::SchemaMismatch
        );
    }

    let infrastructure = [
        ProviderObservationStagingError::<PhysicalFailure>::Batch(
            ProviderObservationStageBatchError::InvalidTransition,
        ),
        ProviderObservationStagingError::Batch(
            ProviderObservationStageBatchError::FrontierOverflow,
        ),
        ProviderObservationStagingError::Batch(ProviderObservationStageBatchError::ReplayMismatch),
        ProviderObservationStagingError::Record(SyndicRecordError::LengthOverflow {
            kind: "provider observation",
        }),
        ProviderObservationStagingError::Callback(PhysicalFailure),
    ];
    for error in infrastructure {
        assert_eq!(
            staging_rejection(&error),
            OrderedTurnStreamRejection::StagingConflict
        );
    }
}
