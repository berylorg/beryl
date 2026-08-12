use syndic_storage::{ProviderObservationValidatorError, SyndicRecordError};

use super::*;

#[test]
fn staging_rejection_preserves_schema_and_infrastructure_taxonomy() {
    let schema = [
        ProviderObservationStagingError::Validation(
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
        ProviderObservationStagingError::Batch(
            ProviderObservationStageBatchError::InvalidTransition,
        ),
        ProviderObservationStagingError::Batch(
            ProviderObservationStageBatchError::FrontierOverflow,
        ),
        ProviderObservationStagingError::Batch(ProviderObservationStageBatchError::ReplayMismatch),
        ProviderObservationStagingError::Record(SyndicRecordError::LengthOverflow {
            kind: "provider observation",
        }),
    ];
    for error in infrastructure {
        assert_eq!(
            staging_rejection(&error),
            OrderedTurnStreamRejection::StagingConflict
        );
    }
}
