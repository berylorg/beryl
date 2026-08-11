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
        SyndicReadError::Read(error) if is_current_health_loss_read(error, expected) => {
            SchedulerFailure::PersistentHomeFailure
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
    if is_current_health_loss_read(error, expected) {
        SchedulerFailure::PersistentHomeFailure
    } else if is_cut_correlated_read(error, expected) {
        SchedulerFailure::PersistentHomeFailure
    } else {
        SchedulerFailure::Fatal
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn is_current_health_loss_read(
    error: &ReadError,
    expected: HomeGeneration,
) -> bool {
    matches!(
        error,
        ReadError::HealthGate(gate)
            if gate.state() != HomeHealthState::Healthy && gate.generation() == expected
    )
}

pub(super) fn is_current_health_loss_gate(
    error: &HealthGateError,
    expected: HomeGeneration,
) -> bool {
    error.state() != HomeHealthState::Healthy && error.generation() == expected
}

pub(super) fn is_cut_correlated_gate(error: &HealthGateError, expected: HomeGeneration) -> bool {
    error.state() == HomeHealthState::Failed && error.generation() == expected
}

pub(in crate::cas_projection::accepted_input_scheduler) fn is_current_health_loss_sidecar(
    error: &SidecarError,
    expected: HomeGeneration,
) -> bool {
    matches!(
        error,
        SidecarError::HealthGate(source) if is_current_health_loss_gate(source, expected)
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

pub(in crate::cas_projection::accepted_input_scheduler) fn is_current_health_loss_command(
    error: &CommandError,
    expected: HomeGeneration,
) -> bool {
    match error {
        CommandError::HealthGate(source) => is_current_health_loss_gate(source, expected),
        CommandError::RevisionRead { source } => is_current_health_loss_read(source, expected),
        CommandError::ContributorAccess {
            source: DomainCallbackSource::Read(source),
            ..
        } => is_current_health_loss_read(source, expected),
        CommandError::ContributorAccess {
            source: DomainCallbackSource::Sidecar(source),
            ..
        } => is_current_health_loss_sidecar(source, expected),
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
