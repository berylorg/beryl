use beryl_home_store::{
    CommandError, DomainCallbackSource, DomainHandleError, HealthGateError, HomeGeneration,
    HomeHealthState, ReadError, SidecarError,
};
use beryl_state::{AssetReadError, BerylStateReacquireError};
use syndic_storage::SyndicReadError;

use super::{AcceptedInputSchedulerContext, WorkerDisposition};
use crate::cas_projection::{
    LiveCommandAdmissionError, LiveCommandPermit, PersistentFailureNotificationStatus,
    ProjectionCoordinatorError, ProjectionPublicationFailure, ScheduledOrdinaryAdmissionError,
    active_steering::{
        ActiveSteeringDeliveryError, ActiveSteeringDeliveryOutcome,
        ActiveSteeringPreparationFailure, ActiveSteeringRetryCause, ActiveSteeringUnknownCause,
    },
    connection::ProviderBrokerLossError,
    input_replay::AcceptedInputReplayError,
    persistent_failure::LiveCommandGateStatus,
};
use crate::input_admission::InputAdmissionBuildError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum AcceptedInputSchedulerExit {
    Clean,
    PersistentHomeFailure,
    Fatal,
}

impl AcceptedInputSchedulerExit {
    pub(in crate::cas_projection) const fn failed(self) -> bool {
        !matches!(self, Self::Clean)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SchedulerFailure {
    /// Exact current-generation verification is owned by the process supervisor. Scheduler work
    /// returns this nonterminal disposition so the scheduler can park without closing its gate.
    VerificationPending,
    PersistentHomeFailure,
    Fatal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SchedulerGateStatus {
    Open,
    OrdinaryShutdown,
    PersistentHomeFailure,
}

impl SchedulerFailure {
    pub(super) const fn merge(self, other: Self) -> Self {
        if matches!(self, Self::Fatal) || matches!(other, Self::Fatal) {
            Self::Fatal
        } else if matches!(self, Self::PersistentHomeFailure)
            || matches!(other, Self::PersistentHomeFailure)
        {
            Self::PersistentHomeFailure
        } else {
            Self::VerificationPending
        }
    }
}

pub(super) fn reconcile_failure(
    context: &AcceptedInputSchedulerContext,
    failure: SchedulerFailure,
) -> Option<SchedulerFailure> {
    debug_assert_ne!(failure, SchedulerFailure::VerificationPending);
    if failure == SchedulerFailure::VerificationPending {
        return Some(SchedulerFailure::Fatal);
    }
    if failure == SchedulerFailure::Fatal {
        return Some(SchedulerFailure::Fatal);
    }

    let authorizer = context.command_gate.authorizer();
    match authorizer.status_exact() {
        Ok(LiveCommandGateStatus::PersistentFailure) => {
            Some(SchedulerFailure::PersistentHomeFailure)
        }
        Ok(LiveCommandGateStatus::OrdinaryShutdown) => None,
        Ok(LiveCommandGateStatus::LocalFailure) => {
            if !authorizer.is_persistent_failure_cut() {
                let _ = authorizer.observe_persistent_failure();
            }
            Some(SchedulerFailure::Fatal)
        }
        Ok(LiveCommandGateStatus::Open) => {
            match authorizer.observe_persistent_failure() {
                PersistentFailureNotificationStatus::Signaled
                | PersistentFailureNotificationStatus::Joined => {}
                PersistentFailureNotificationStatus::VerificationSignaled
                | PersistentFailureNotificationStatus::VerificationJoined
                | PersistentFailureNotificationStatus::NotFailed
                | PersistentFailureNotificationStatus::Unavailable => {
                    return Some(SchedulerFailure::Fatal);
                }
            }
            match authorizer.status_exact() {
                Ok(LiveCommandGateStatus::PersistentFailure) => {
                    Some(SchedulerFailure::PersistentHomeFailure)
                }
                Ok(
                    LiveCommandGateStatus::Open
                    | LiveCommandGateStatus::OrdinaryShutdown
                    | LiveCommandGateStatus::LocalFailure,
                )
                | Err(_) => Some(SchedulerFailure::Fatal),
            }
        }
        Err(_) => Some(SchedulerFailure::Fatal),
    }
}

pub(super) fn authorize(
    context: &AcceptedInputSchedulerContext,
) -> Result<Option<LiveCommandPermit>, SchedulerFailure> {
    let authorizer = context.command_gate.authorizer();
    match authorizer.authorize() {
        Ok(command) => Ok(Some(command)),
        Err(LiveCommandAdmissionError::Closed) => match gate_status(context)? {
            SchedulerGateStatus::OrdinaryShutdown => Ok(None),
            SchedulerGateStatus::PersistentHomeFailure => {
                Err(SchedulerFailure::PersistentHomeFailure)
            }
            SchedulerGateStatus::Open => Err(SchedulerFailure::Fatal),
        },
        Err(LiveCommandAdmissionError::Unavailable) => Err(SchedulerFailure::Fatal),
    }
}

pub(super) fn gate_status(
    context: &AcceptedInputSchedulerContext,
) -> Result<SchedulerGateStatus, SchedulerFailure> {
    match context.command_gate.authorizer().status_exact() {
        Ok(LiveCommandGateStatus::Open) => Ok(SchedulerGateStatus::Open),
        Ok(LiveCommandGateStatus::OrdinaryShutdown) => Ok(SchedulerGateStatus::OrdinaryShutdown),
        Ok(LiveCommandGateStatus::PersistentFailure) => {
            Ok(SchedulerGateStatus::PersistentHomeFailure)
        }
        Ok(LiveCommandGateStatus::LocalFailure) | Err(_) => Err(SchedulerFailure::Fatal),
    }
}

pub(super) fn from_syndic_read(
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

pub(super) fn from_read(error: &ReadError, expected: HomeGeneration) -> SchedulerFailure {
    if is_verification_pending_read(error, expected) {
        SchedulerFailure::VerificationPending
    } else if is_cut_correlated_read(error, expected) {
        SchedulerFailure::PersistentHomeFailure
    } else {
        SchedulerFailure::Fatal
    }
}

pub(super) fn is_verification_pending_read(error: &ReadError, expected: HomeGeneration) -> bool {
    matches!(
        error,
        ReadError::HealthGate(gate)
            if gate.state() == HomeHealthState::Verifying && gate.generation() == expected
    )
}

fn is_verification_pending_gate(error: &HealthGateError, expected: HomeGeneration) -> bool {
    error.state() == HomeHealthState::Verifying && error.generation() == expected
}

fn is_cut_correlated_gate(error: &HealthGateError, expected: HomeGeneration) -> bool {
    error.state() == HomeHealthState::Failed && error.generation() == expected
}

pub(super) fn is_verification_pending_sidecar(
    error: &SidecarError,
    expected: HomeGeneration,
) -> bool {
    matches!(
        error,
        SidecarError::HealthGate(source) if is_verification_pending_gate(source, expected)
    )
}

pub(super) fn is_cut_correlated_sidecar(error: &SidecarError, expected: HomeGeneration) -> bool {
    matches!(
        error,
        SidecarError::HealthGate(source) if is_cut_correlated_gate(source, expected)
    )
}

pub(super) fn is_verification_pending_command(
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

pub(super) fn is_cut_correlated_command(error: &CommandError, expected: HomeGeneration) -> bool {
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

pub(super) fn is_cut_correlated_read(error: &ReadError, expected: HomeGeneration) -> bool {
    matches!(
        error,
        ReadError::HealthGate(gate)
            if gate.state() == HomeHealthState::Failed && gate.generation() == expected
    )
}

pub(super) fn from_coordinator(
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

pub(super) fn is_verification_pending_coordinator(
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

pub(super) fn is_cut_correlated_coordinator(
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

pub(super) fn is_cut_correlated_admission(
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

pub(super) fn from_admission(
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

pub(super) fn is_verification_pending_admission(
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

pub(super) fn is_cut_correlated_publication(
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

pub(super) fn is_verification_pending_publication(
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

pub(super) fn from_input_admission_build(
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

fn is_verification_pending_replay(
    error: &AcceptedInputReplayError,
    expected: HomeGeneration,
) -> bool {
    match error {
        AcceptedInputReplayError::HomeNotHealthy {
            state: HomeHealthState::Verifying,
            expected_home_id,
            actual_home_id,
            expected_generation,
            actual_generation: Some(actual_generation),
        } => {
            expected_home_id == actual_home_id
                && *expected_generation == expected
                && *actual_generation == expected
        }
        AcceptedInputReplayError::HomeGenerationMismatch {
            expected: bound,
            actual: Some(actual),
            state: HomeHealthState::Verifying,
        } => *bound == expected && *actual == expected,
        AcceptedInputReplayError::HomeRead(source) => {
            is_verification_pending_read(source, expected)
        }
        AcceptedInputReplayError::SyndicRead(SyndicReadError::Read(source)) => {
            is_verification_pending_read(source, expected)
        }
        AcceptedInputReplayError::AssetRead(AssetReadError::Read(source)) => {
            is_verification_pending_read(source, expected)
        }
        AcceptedInputReplayError::Sidecar(source) => {
            is_verification_pending_sidecar(source, expected)
        }
        _ => false,
    }
}

fn is_cut_correlated_replay(error: &AcceptedInputReplayError, expected: HomeGeneration) -> bool {
    match error {
        AcceptedInputReplayError::HomeNotHealthy {
            state: HomeHealthState::Failed,
            expected_home_id,
            actual_home_id,
            expected_generation,
            actual_generation: Some(actual_generation),
        } => {
            expected_home_id == actual_home_id
                && *expected_generation == expected
                && *actual_generation == expected
        }
        AcceptedInputReplayError::HomeGenerationMismatch {
            expected: bound,
            actual: Some(actual),
            state: HomeHealthState::Failed,
        } => *bound == expected && *actual == expected,
        AcceptedInputReplayError::HomeRead(source) => is_cut_correlated_read(source, expected),
        AcceptedInputReplayError::SyndicRead(SyndicReadError::Read(source)) => {
            is_cut_correlated_read(source, expected)
        }
        AcceptedInputReplayError::AssetRead(AssetReadError::Read(source)) => {
            is_cut_correlated_read(source, expected)
        }
        AcceptedInputReplayError::Sidecar(source) => is_cut_correlated_sidecar(source, expected),
        _ => false,
    }
}

fn is_verification_pending_steering_retry(
    cause: &ActiveSteeringRetryCause,
    expected: HomeGeneration,
) -> bool {
    let ActiveSteeringRetryCause::Preparation(source) = cause else {
        return false;
    };
    match source {
        ActiveSteeringPreparationFailure::State(BerylStateReacquireError::Domain {
            source: DomainHandleError::HealthGate(source),
            ..
        }) => is_verification_pending_gate(source, expected),
        ActiveSteeringPreparationFailure::Asset(source) => {
            is_verification_pending_read(source, expected)
        }
        ActiveSteeringPreparationFailure::Replay(source) => {
            is_verification_pending_replay(source, expected)
        }
        _ => false,
    }
}

fn is_cut_correlated_steering_retry(
    cause: &ActiveSteeringRetryCause,
    expected: HomeGeneration,
) -> bool {
    let ActiveSteeringRetryCause::Preparation(source) = cause else {
        return false;
    };
    match source {
        ActiveSteeringPreparationFailure::State(BerylStateReacquireError::Domain {
            source: DomainHandleError::HealthGate(source),
            ..
        }) => is_cut_correlated_gate(source, expected),
        ActiveSteeringPreparationFailure::Asset(source) => is_cut_correlated_read(source, expected),
        ActiveSteeringPreparationFailure::Replay(source) => {
            is_cut_correlated_replay(source, expected)
        }
        _ => false,
    }
}

fn is_verification_pending_broker_loss(
    error: &ProviderBrokerLossError,
    expected: HomeGeneration,
) -> bool {
    match error {
        ProviderBrokerLossError::Coordinator(source) => {
            is_verification_pending_coordinator(source, expected)
        }
        ProviderBrokerLossError::Read(SyndicReadError::Read(source)) => {
            is_verification_pending_read(source, expected)
        }
        ProviderBrokerLossError::Publication(source) => {
            is_verification_pending_publication(source, expected)
        }
        ProviderBrokerLossError::LiveSource(
            crate::cas_projection::live_source::LiveSourcePublicationError::Read(
                SyndicReadError::Read(source),
            ),
        ) => is_verification_pending_read(source, expected),
        ProviderBrokerLossError::LiveSource(
            crate::cas_projection::live_source::LiveSourcePublicationError::Publication(source),
        ) => is_verification_pending_publication(source, expected),
        ProviderBrokerLossError::Stop(crate::cas_projection::StopCoordinationError::Read(
            SyndicReadError::Read(source),
        )) => is_verification_pending_read(source, expected),
        _ => false,
    }
}

fn is_cut_correlated_broker_loss(
    error: &ProviderBrokerLossError,
    expected: HomeGeneration,
) -> bool {
    match error {
        ProviderBrokerLossError::Coordinator(source) => {
            is_cut_correlated_coordinator(source, expected)
        }
        ProviderBrokerLossError::Read(SyndicReadError::Read(source)) => {
            is_cut_correlated_read(source, expected)
        }
        ProviderBrokerLossError::Publication(source) => {
            is_cut_correlated_publication(source, expected)
        }
        ProviderBrokerLossError::LiveSource(
            crate::cas_projection::live_source::LiveSourcePublicationError::Read(
                SyndicReadError::Read(source),
            ),
        ) => is_cut_correlated_read(source, expected),
        ProviderBrokerLossError::LiveSource(
            crate::cas_projection::live_source::LiveSourcePublicationError::Publication(source),
        ) => is_cut_correlated_publication(source, expected),
        ProviderBrokerLossError::Stop(crate::cas_projection::StopCoordinationError::Read(
            SyndicReadError::Read(source),
        )) => is_cut_correlated_read(source, expected),
        _ => false,
    }
}

pub(super) fn classify_active_steering_delivery(
    result: &Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError>,
    home_generation: HomeGeneration,
) -> WorkerDisposition {
    match result {
        Ok(ActiveSteeringDeliveryOutcome::Retryable { cause })
            if is_verification_pending_steering_retry(cause, home_generation) =>
        {
            WorkerDisposition::VerificationPending
        }
        Ok(ActiveSteeringDeliveryOutcome::Retryable { cause })
            if is_cut_correlated_steering_retry(cause, home_generation) =>
        {
            WorkerDisposition::PersistentHomeFailure
        }
        Ok(ActiveSteeringDeliveryOutcome::Retryable { .. }) => WorkerDisposition::Parked,
        Err(ActiveSteeringDeliveryError::PersistentFailureCut) => WorkerDisposition::Parked,
        Ok(ActiveSteeringDeliveryOutcome::DeliveryUnknown {
            cause: ActiveSteeringUnknownCause::Disposition(error),
        }) if is_verification_pending_publication(error, home_generation) => {
            // Target-loss convergence has already consumed the attempt/lifecycle owners. The
            // durable delivery state remains authoritative, so the verified wake resumes durable
            // scheduler discovery without retaining an ownerless in-memory attempt.
            WorkerDisposition::VerificationPending
        }
        Ok(ActiveSteeringDeliveryOutcome::DeliveryUnknown {
            cause: ActiveSteeringUnknownCause::DeliveringRouteRead(SyndicReadError::Read(error)),
        }) if is_verification_pending_read(error, home_generation) => {
            WorkerDisposition::VerificationPending
        }
        Ok(ActiveSteeringDeliveryOutcome::DeliveryUnknown {
            cause: ActiveSteeringUnknownCause::Coordinator(error),
        }) if is_verification_pending_coordinator(error, home_generation) => {
            WorkerDisposition::VerificationPending
        }
        Ok(ActiveSteeringDeliveryOutcome::DeliveryUnknown {
            cause: ActiveSteeringUnknownCause::Disposition(error),
        }) if is_cut_correlated_publication(error, home_generation) => {
            WorkerDisposition::PersistentHomeFailure
        }
        Ok(ActiveSteeringDeliveryOutcome::DeliveryUnknown {
            cause: ActiveSteeringUnknownCause::DeliveringRouteRead(error),
        }) if matches!(
            from_syndic_read(error, home_generation),
            SchedulerFailure::PersistentHomeFailure
        ) =>
        {
            WorkerDisposition::PersistentHomeFailure
        }
        Ok(ActiveSteeringDeliveryOutcome::DeliveryUnknown {
            cause: ActiveSteeringUnknownCause::Coordinator(error),
        }) if is_cut_correlated_coordinator(error, home_generation) => {
            WorkerDisposition::PersistentHomeFailure
        }
        Ok(
            ActiveSteeringDeliveryOutcome::Delivered
            | ActiveSteeringDeliveryOutcome::SteeringRejected { .. }
            | ActiveSteeringDeliveryOutcome::ProjectionLost { .. }
            | ActiveSteeringDeliveryOutcome::DeliveryUnknown { .. },
        ) => WorkerDisposition::Settled,
        Ok(
            ActiveSteeringDeliveryOutcome::NotReady
            | ActiveSteeringDeliveryOutcome::Saturated { .. },
        ) => WorkerDisposition::Fatal,
        Err(ActiveSteeringDeliveryError::Coordinator(error))
            if is_verification_pending_coordinator(error, home_generation) =>
        {
            WorkerDisposition::VerificationPending
        }
        Err(ActiveSteeringDeliveryError::Read(SyndicReadError::Read(error)))
            if is_verification_pending_read(error, home_generation) =>
        {
            WorkerDisposition::VerificationPending
        }
        Err(ActiveSteeringDeliveryError::Publication(error))
            if is_verification_pending_publication(error, home_generation) =>
        {
            WorkerDisposition::VerificationPending
        }
        Err(ActiveSteeringDeliveryError::Loss(error))
            if is_verification_pending_broker_loss(error, home_generation) =>
        {
            WorkerDisposition::VerificationPending
        }
        Err(ActiveSteeringDeliveryError::Coordinator(error))
            if is_cut_correlated_coordinator(error, home_generation) =>
        {
            WorkerDisposition::PersistentHomeFailure
        }
        Err(ActiveSteeringDeliveryError::Read(error))
            if matches!(
                from_syndic_read(error, home_generation),
                SchedulerFailure::PersistentHomeFailure
            ) =>
        {
            WorkerDisposition::PersistentHomeFailure
        }
        Err(ActiveSteeringDeliveryError::Publication(error))
            if is_cut_correlated_publication(error, home_generation) =>
        {
            WorkerDisposition::PersistentHomeFailure
        }
        Err(ActiveSteeringDeliveryError::Loss(error))
            if is_cut_correlated_broker_loss(error, home_generation) =>
        {
            WorkerDisposition::PersistentHomeFailure
        }
        Err(_) => WorkerDisposition::Fatal,
    }
}
