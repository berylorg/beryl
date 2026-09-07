use super::{DraftMarkerAdmissionLimitsV1, DraftMarkerAdmissionRetainedChargeV1};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(super) enum AdmissionLimitError {
    #[error("draft-marker admission operation exceeds its retained charge limit")]
    OperationTooLarge,
    #[error("draft-marker admission shared capacity is unavailable")]
    CapacityUnavailable,
    #[error("draft-marker admission retained charge is invalid")]
    InvalidCharge,
}

pub(super) fn check_operation_charge(
    operation: DraftMarkerAdmissionRetainedChargeV1,
    limits: DraftMarkerAdmissionLimitsV1,
) -> Result<(), AdmissionLimitError> {
    if operation.heads() != 1 {
        return Err(AdmissionLimitError::InvalidCharge);
    }
    if operation.associations() > limits.max_associations()
        || operation.encoded_bytes() > limits.max_encoded_bytes()
    {
        return Err(AdmissionLimitError::OperationTooLarge);
    }
    Ok(())
}

pub(super) fn check_aggregate_charge(
    aggregate: DraftMarkerAdmissionRetainedChargeV1,
    limits: DraftMarkerAdmissionLimitsV1,
) -> Result<(), AdmissionLimitError> {
    if !aggregate.fits(limits) {
        return Err(AdmissionLimitError::CapacityUnavailable);
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum DraftMarkerAdmissionStorageErrorV1 {
    #[error(transparent)]
    Read(#[from] crate::SyndicReadError),
    #[error(transparent)]
    Command(#[from] beryl_home_store::CommandError),
}

#[derive(Debug, thiserror::Error)]
pub enum DraftMarkerAdmissionCommittedUnavailableReasonV1 {
    #[error("draft-marker admission committed with a later storage failure")]
    LaterFailure,
    #[error(transparent)]
    LocalFinalization(beryl_home_store::CommittedLocalFinalizationError),
    #[error("draft-marker admission local custody is unavailable")]
    LocalCustodyUnavailable,
    #[error(transparent)]
    Receipt(beryl_home_store::CommitReceiptError),
    #[error("draft-marker admission commit receipt is no longer current")]
    ReceiptNotCurrent,
}

pub(super) fn committed_finalization_reason(
    result: Result<Result<(), ()>, beryl_home_store::CommittedLocalFinalizationError>,
) -> DraftMarkerAdmissionCommittedUnavailableReasonV1 {
    match result {
        Ok(Ok(())) => DraftMarkerAdmissionCommittedUnavailableReasonV1::LaterFailure,
        Ok(Err(())) => DraftMarkerAdmissionCommittedUnavailableReasonV1::LocalCustodyUnavailable,
        Err(error) => DraftMarkerAdmissionCommittedUnavailableReasonV1::LocalFinalization(error),
    }
}

pub(super) fn is_storage_command_error(error: &beryl_home_store::CommandError) -> bool {
    use beryl_home_store::CommandError;
    matches!(
        error,
        CommandError::ContributorAccess { .. }
            | CommandError::RevisionRead { .. }
            | CommandError::Commit { .. }
            | CommandError::Persistence { .. }
            | CommandError::PersistenceAfterCommitFailure { .. }
    )
}
