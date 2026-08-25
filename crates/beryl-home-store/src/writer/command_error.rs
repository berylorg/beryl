use std::error::Error;

use crate::{
    CodecOperation, CommandError, ContributorCallbackStage, ReadError, ReadStage,
    domain::callback::{ErasedCallbackError, callback_failure_severity},
    health::{ClassifiedFjallError, FailureSeverity},
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
        | CommandError::InvalidSuccessorProtocol
        | CommandError::RevisionExhausted { .. }
        | CommandError::Metadata { .. } => None,
        CommandError::ContributorAccess { source, .. } => callback_failure_severity(source),
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
                FailureSeverity::Structural => 0,
            }),
        CommandError::RevisionRead { source } => read_failure_severity(source),
        CommandError::WriterPoisoned
        | CommandError::GenerationPoisoned
        | CommandError::DomainRegistrationInvariant { .. } => Some(FailureSeverity::Structural),
    }
}

/// Classifies only evidence that is independently structural when a command's
/// commit state is already indeterminate. The indeterminate outcome itself is
/// reconciled at domain scope and must not poison the whole home merely because
/// its underlying storage error was I/O or durability related.
pub(super) fn indeterminate_failure_severity(error: &CommandError) -> Option<FailureSeverity> {
    match error {
        CommandError::Commit { source } | CommandError::Persistence { source } => {
            independently_structural_storage_failure(source.as_ref())
        }
        CommandError::PersistenceAfterCommitFailure {
            commit,
            persistence,
        } => indeterminate_failure_severity(commit)
            .or_else(|| indeterminate_failure_severity(persistence)),
        CommandError::RevisionRead { source } => independently_structural_read_failure(source),
        CommandError::WriterPoisoned
        | CommandError::GenerationPoisoned
        | CommandError::DomainRegistrationInvariant { .. } => Some(FailureSeverity::Structural),
        _ => None,
    }
}

fn independently_structural_storage_failure(
    source: &(dyn Error + Send + Sync + 'static),
) -> Option<FailureSeverity> {
    if source.downcast_ref::<BatchAccountingOverflow>().is_some() {
        return None;
    }
    if source.downcast_ref::<std::io::Error>().is_some() {
        return None;
    }
    match source.downcast_ref::<ClassifiedFjallError>() {
        Some(source) if source.is_independently_structural() => Some(FailureSeverity::Structural),
        Some(_) => None,
        None => Some(FailureSeverity::Structural),
    }
}

fn independently_structural_read_failure(error: &ReadError) -> Option<FailureSeverity> {
    match error {
        ReadError::Storage { source, .. } => {
            independently_structural_storage_failure(source.as_ref())
        }
        ReadError::GenerationPoisoned
        | ReadError::InvalidStoredKeySize { .. }
        | ReadError::InvalidStoredValueSize { .. }
        | ReadError::UnsupportedRecordVersion { .. }
        | ReadError::MalformedRecord { .. }
        | ReadError::InvalidRevisionMetadata { .. } => Some(FailureSeverity::Structural),
        ReadError::Codec { operation, .. }
            if matches!(
                operation,
                CodecOperation::DecodeKey | CodecOperation::DecodeValue
            ) =>
        {
            Some(FailureSeverity::Structural)
        }
        _ => None,
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
        None => Some(FailureSeverity::Structural),
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
