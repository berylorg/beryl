use beryl_home_store::{HomeGeneration, HomeHealthState};
use beryl_state::AssetReadError;
use syndic_storage::SyndicReadError;

use crate::cas_projection::{
    ProjectionCoordinatorError, ProjectionPublicationFailure, ScheduledOrdinaryAdmissionError,
};
use crate::input_admission::InputAdmissionBuildError;

use super::{
    health::{
        from_syndic_read, is_cut_correlated_command, is_cut_correlated_read,
        is_verification_pending_command, is_verification_pending_read,
    },
    types::SchedulerFailure,
};

pub(in crate::cas_projection::accepted_input_scheduler) fn from_coordinator(
    error: &ProjectionCoordinatorError,
    expected: HomeGeneration,
) -> SchedulerFailure {
    if is_verification_pending_coordinator(error, expected) {
        SchedulerFailure::VerificationPending
    } else if is_cut_correlated_coordinator(error, expected) {
        SchedulerFailure::PersistentHomeFailure
    } else {
        SchedulerFailure::Fatal
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn is_verification_pending_coordinator(
    error: &ProjectionCoordinatorError,
    expected: HomeGeneration,
) -> bool {
    match error {
        ProjectionCoordinatorError::HomeNotHealthy {
            state: HomeHealthState::Verifying,
            generation: Some(actual),
        } => *actual == expected,
        ProjectionCoordinatorError::HomeGenerationMismatch {
            expected: bound,
            actual: Some(actual),
            state: HomeHealthState::Verifying,
        } => *bound == expected && *actual == expected,
        ProjectionCoordinatorError::SyndicRevisionUnavailable { source } => {
            is_verification_pending_read(source, expected)
        }
        _ => false,
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn is_cut_correlated_coordinator(
    error: &ProjectionCoordinatorError,
    expected: HomeGeneration,
) -> bool {
    match error {
        ProjectionCoordinatorError::HomeNotHealthy {
            state: HomeHealthState::Failed,
            generation: Some(actual),
        } => *actual == expected,
        ProjectionCoordinatorError::HomeGenerationMismatch {
            expected: bound,
            actual: Some(actual),
            state: HomeHealthState::Failed,
        } => *bound == expected && *actual == expected,
        ProjectionCoordinatorError::LiveCommandPersistentHomeFailure { generation } => {
            *generation == expected
        }
        ProjectionCoordinatorError::SyndicRevisionUnavailable { source } => {
            is_cut_correlated_read(source, expected)
        }
        _ => false,
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn is_cut_correlated_admission(
    error: &ScheduledOrdinaryAdmissionError,
    expected: HomeGeneration,
) -> bool {
    match error {
        ScheduledOrdinaryAdmissionError::Authority(error) => {
            is_cut_correlated_coordinator(error, expected)
        }
        ScheduledOrdinaryAdmissionError::AssetAuthority { source } => {
            is_cut_correlated_read(source, expected)
        }
        _ => false,
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn from_admission(
    error: &ScheduledOrdinaryAdmissionError,
    expected: HomeGeneration,
) -> SchedulerFailure {
    if is_verification_pending_admission(error, expected) {
        SchedulerFailure::VerificationPending
    } else if is_cut_correlated_admission(error, expected) {
        SchedulerFailure::PersistentHomeFailure
    } else {
        SchedulerFailure::Fatal
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn is_verification_pending_admission(
    error: &ScheduledOrdinaryAdmissionError,
    expected: HomeGeneration,
) -> bool {
    match error {
        ScheduledOrdinaryAdmissionError::Authority(error) => {
            is_verification_pending_coordinator(error, expected)
        }
        ScheduledOrdinaryAdmissionError::AssetAuthority { source } => {
            is_verification_pending_read(source, expected)
        }
        _ => false,
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn is_cut_correlated_publication(
    error: &ProjectionPublicationFailure,
    expected: HomeGeneration,
) -> bool {
    match error {
        ProjectionPublicationFailure::HomeRead(source) => is_cut_correlated_read(source, expected),
        ProjectionPublicationFailure::Command(source) => {
            is_cut_correlated_command(source, expected)
        }
        ProjectionPublicationFailure::Reconciliation(source) => matches!(
            from_syndic_read(source, expected),
            SchedulerFailure::PersistentHomeFailure
        ),
        ProjectionPublicationFailure::HomeAuthorityLost(source) => {
            is_cut_correlated_coordinator(source, expected)
        }
        _ => false,
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn is_verification_pending_publication(
    error: &ProjectionPublicationFailure,
    expected: HomeGeneration,
) -> bool {
    match error {
        ProjectionPublicationFailure::HomeRead(source) => {
            is_verification_pending_read(source, expected)
        }
        ProjectionPublicationFailure::Command(source) => {
            is_verification_pending_command(source, expected)
        }
        ProjectionPublicationFailure::Reconciliation(SyndicReadError::Read(source)) => {
            is_verification_pending_read(source, expected)
        }
        ProjectionPublicationFailure::HomeAuthorityLost(source) => {
            is_verification_pending_coordinator(source, expected)
        }
        _ => false,
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn from_input_admission_build(
    error: &InputAdmissionBuildError,
    expected: HomeGeneration,
) -> SchedulerFailure {
    let cut_correlated = match error {
        InputAdmissionBuildError::Read(source) => is_cut_correlated_read(source, expected),
        InputAdmissionBuildError::SyndicRead(source) => matches!(
            from_syndic_read(source, expected),
            SchedulerFailure::PersistentHomeFailure
        ),
        InputAdmissionBuildError::AssetRead(AssetReadError::Read(source)) => {
            is_cut_correlated_read(source, expected)
        }
        _ => false,
    };
    let verification_pending = match error {
        InputAdmissionBuildError::Read(source) => is_verification_pending_read(source, expected),
        InputAdmissionBuildError::SyndicRead(SyndicReadError::Read(source)) => {
            is_verification_pending_read(source, expected)
        }
        InputAdmissionBuildError::AssetRead(AssetReadError::Read(source)) => {
            is_verification_pending_read(source, expected)
        }
        _ => false,
    };
    if verification_pending {
        SchedulerFailure::VerificationPending
    } else if cut_correlated {
        SchedulerFailure::PersistentHomeFailure
    } else {
        SchedulerFailure::Fatal
    }
}
