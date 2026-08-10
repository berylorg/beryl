use beryl_home_store::{
    CommandError, DomainCallbackSource, HealthGateError, HomeGeneration, HomeHealthState,
    ReadError, SidecarError,
};
use syndic_storage::SyndicReadError;

use super::types::SchedulerFailure;

pub(in crate::cas_projection::accepted_input_scheduler) fn from_syndic_read(
    error: &SyndicReadError,
    expected: HomeGeneration,
) -> SchedulerFailure {
    match error {
        SyndicReadError::Read(error) if is_verification_pending_read(error, expected) => {
            SchedulerFailure::VerificationPending
        }
        SyndicReadError::Read(error) if is_cut_correlated_read(error, expected) => {
            SchedulerFailure::PersistentHomeFailure
        }
        _ => SchedulerFailure::Fatal,
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn from_read(
    error: &ReadError,
    expected: HomeGeneration,
) -> SchedulerFailure {
    if is_verification_pending_read(error, expected) {
        SchedulerFailure::VerificationPending
    } else if is_cut_correlated_read(error, expected) {
        SchedulerFailure::PersistentHomeFailure
    } else {
        SchedulerFailure::Fatal
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn is_verification_pending_read(
    error: &ReadError,
    expected: HomeGeneration,
) -> bool {
    matches!(
        error,
        ReadError::HealthGate(gate)
            if gate.state() == HomeHealthState::Verifying && gate.generation() == expected
    )
}

pub(super) fn is_verification_pending_gate(
    error: &HealthGateError,
    expected: HomeGeneration,
) -> bool {
    error.state() == HomeHealthState::Verifying && error.generation() == expected
}

pub(super) fn is_cut_correlated_gate(error: &HealthGateError, expected: HomeGeneration) -> bool {
    error.state() == HomeHealthState::Failed && error.generation() == expected
}

pub(in crate::cas_projection::accepted_input_scheduler) fn is_verification_pending_sidecar(
    error: &SidecarError,
    expected: HomeGeneration,
) -> bool {
    matches!(
        error,
        SidecarError::HealthGate(source) if is_verification_pending_gate(source, expected)
    )
}

pub(in crate::cas_projection::accepted_input_scheduler) fn is_cut_correlated_sidecar(
    error: &SidecarError,
    expected: HomeGeneration,
) -> bool {
    matches!(
        error,
        SidecarError::HealthGate(source) if is_cut_correlated_gate(source, expected)
    )
}

pub(in crate::cas_projection::accepted_input_scheduler) fn is_verification_pending_command(
    error: &CommandError,
    expected: HomeGeneration,
) -> bool {
    match error {
        CommandError::HealthGate(source) => is_verification_pending_gate(source, expected),
        CommandError::RevisionRead { source } => is_verification_pending_read(source, expected),
        CommandError::ContributorAccess {
            source: DomainCallbackSource::Read(source),
            ..
        } => is_verification_pending_read(source, expected),
        CommandError::ContributorAccess {
            source: DomainCallbackSource::Sidecar(source),
            ..
        } => is_verification_pending_sidecar(source, expected),
        _ => false,
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn is_cut_correlated_command(
    error: &CommandError,
    expected: HomeGeneration,
) -> bool {
    match error {
        CommandError::HealthGate(source) => is_cut_correlated_gate(source, expected),
        CommandError::RevisionRead { source } => is_cut_correlated_read(source, expected),
        CommandError::ContributorAccess {
            source: DomainCallbackSource::Read(source),
            ..
        } => is_cut_correlated_read(source, expected),
        CommandError::ContributorAccess {
            source: DomainCallbackSource::Sidecar(source),
            ..
        } => is_cut_correlated_sidecar(source, expected),
        _ => false,
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn is_cut_correlated_read(
    error: &ReadError,
    expected: HomeGeneration,
) -> bool {
    matches!(
        error,
        ReadError::HealthGate(gate)
            if gate.state() == HomeHealthState::Failed && gate.generation() == expected
    )
}
