use std::{error::Error, fmt};

use syndic_storage::{ProviderObservationValidatorError, SyndicRecordError};

use super::*;

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
