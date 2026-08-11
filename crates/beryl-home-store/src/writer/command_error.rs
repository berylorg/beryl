use std::error::Error;

use crate::{
    domain::callback::{callback_failure_severity, ErasedCallbackError},
    health::{ClassifiedFjallError, FailureSeverity},
    CodecOperation, CommandError, ContributorCallbackStage, ReadError, ReadStage,
};

#[derive(Debug, thiserror::Error)]
#[error("encoded command batch totals exceed the supported accounting range")]
struct BatchAccountingOverflow;

pub(super) fn callback_command_error(
    domain: &'static str,
    stage: ContributorCallbackStage,
    source: ErasedCallbackError,
) -> CommandError {
    match source {
        ErasedCallbackError::Access(source) => CommandError::ContributorAccess {
            domain,
            stage,
            source,
        },
        ErasedCallbackError::Rejected(source) => match stage {
            ContributorCallbackStage::Reservation => {
                CommandError::ContributorReservation { domain, source }
            }
            ContributorCallbackStage::Validation => {
                CommandError::ContributorValidation { domain, source }
            }
            ContributorCallbackStage::Contribution => {
                CommandError::ContributorAssembly { domain, source }
            }
        },
    }
}

pub(super) fn batch_accounting_overflow() -> CommandError {
    CommandError::Commit {
        source: Box::new(BatchAccountingOverflow),
    }
}

pub(super) fn commit_fjall_error(source: fjall::Error) -> CommandError {
    CommandError::Commit {
        source: classified_fjall_source(source),
    }
}

pub(super) fn persistence_fjall_error(source: fjall::Error) -> CommandError {
    CommandError::Persistence {
        source: classified_fjall_source(source),
    }
}

pub(super) fn revision_snapshot_error(stage: ReadStage, source: fjall::Error) -> CommandError {
    CommandError::RevisionRead {
        source: ReadError::Storage {
            stage,
            source: classified_fjall_source(source),
        },
    }
}

fn classified_fjall_source(source: fjall::Error) -> Box<dyn Error + Send + Sync> {
    Box::new(ClassifiedFjallError::direct(source))
}

pub(super) fn command_failure_severity(error: &CommandError) -> Option<FailureSeverity> {
    match error {
        CommandError::HealthGate(_)
        | CommandError::CancelledBeforeAdmission
        | CommandError::ReentrantWriter
        | CommandError::EmptyCommand
        | CommandError::ValidationOnlyCommand
        | CommandError::ForeignDomain { .. }
        | CommandError::ForeignSidecar
        | CommandError::Conflict { .. }
        | CommandError::ContributorReservation { .. }
        | CommandError::ContributorValidation { .. }
        | CommandError::ContributorAssembly { .. }
        | CommandError::EmptyContribution { .. }
        | CommandError::ReconciliationCapacity
        | CommandError::ReconciliationDescriptorTooLarge { .. }
        | CommandError::ReconciliationReservationMismatch { .. }
        | CommandError::RevisionExhausted { .. }
        | CommandError::Metadata { .. } => None,
        CommandError::ContributorAccess { source, .. } => Some(callback_failure_severity(source)),
        CommandError::Commit { source } | CommandError::Persistence { source } => {
            erased_storage_failure_severity(source.as_ref())
        }
        CommandError::PersistenceAfterCommitFailure {
            commit,
            persistence,
        } => command_failure_severity(commit)
            .into_iter()
            .chain(command_failure_severity(persistence))
            .max_by_key(|severity| match severity {
                FailureSeverity::Verify => 0,
                FailureSeverity::Structural => 1,
            }),
        CommandError::RevisionRead { source } => read_failure_severity(source),
        CommandError::WriterPoisoned
        | CommandError::GenerationPoisoned
        | CommandError::DomainRegistrationInvariant { .. } => Some(FailureSeverity::Structural),
    }
}

fn erased_storage_failure_severity(
    source: &(dyn Error + Send + Sync + 'static),
) -> Option<FailureSeverity> {
    if source.downcast_ref::<BatchAccountingOverflow>().is_some() {
        return None;
    }
    match source.downcast_ref::<ClassifiedFjallError>() {
        Some(source) => source.severity(),
        None => Some(FailureSeverity::Verify),
    }
}

fn read_failure_severity(error: &ReadError) -> Option<FailureSeverity> {
    match error {
        ReadError::HealthGate(_)
        | ReadError::ForeignDomain { .. }
        | ReadError::UnknownFamily { .. }
        | ReadError::CodecTypeMismatch { .. }
        | ReadError::InvalidCodecContract { .. }
        | ReadError::InvalidKeySize { .. }
        | ReadError::ReversedRange { .. }
        | ReadError::BoundExceeded { .. } => None,
        ReadError::Storage { source, .. } => erased_storage_failure_severity(source.as_ref()),
        ReadError::GenerationPoisoned
        | ReadError::InvalidStoredKeySize { .. }
        | ReadError::InvalidStoredValueSize { .. }
        | ReadError::UnsupportedRecordVersion { .. }
        | ReadError::MalformedRecord { .. }
        | ReadError::InvalidRevisionMetadata { .. } => Some(FailureSeverity::Structural),
        ReadError::Codec { operation, .. } => match operation {
            CodecOperation::DecodeKey | CodecOperation::DecodeValue => {
                Some(FailureSeverity::Structural)
            }
            CodecOperation::EncodeKey | CodecOperation::EncodeValue => None,
        },
    }
}
