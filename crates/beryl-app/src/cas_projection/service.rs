use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use beryl_backend::{ManagedBackendClientConnector, ManagedBackendError, ManagedBackendSession};
use beryl_home_store::{CommandError, HomeCloseError, HomeGeneration, HomeHealthState, HomeStore};
#[cfg(test)]
use beryl_model::SyndicAcceptedInputId;
use beryl_model::{
    BerylHomeId, CasProcessGeneration, DomainRevision, ExecutionBinding, RuntimeId, SyndicThreadId,
    SyndicTurnId,
};
use syndic_storage::{
    AcceptedInputAdmission, InputAdmissionStatus, StopAdmissionIneligibility, StopAdmissionRead,
    StopCause, SyndicPointReadLimit, SyndicReadError, SyndicStorage,
};
use thiserror::Error;

#[cfg(all(test, feature = "test-faults"))]
use super::LoadedCasProjection;
#[cfg(any(test, feature = "test-faults"))]
use super::runtime::LifecycleTestCompatibilityFacts;
#[cfg(test)]
use super::{
    ActiveSteeringDeliveryError, ActiveSteeringDeliveryOutcome, LiveEventTarget,
    ProjectionCancellationToken, active_steering,
};
use super::{
    AdmittedProjectionSession, ProjectionCoordinatorError, ProjectionRegistryKind,
    ProjectionSessionAdmissionError,
    accepted_input_scheduler::{
        AcceptedInputScheduler, AcceptedInputSchedulerContext, AcceptedInputSchedulerDiagnostics,
        AcceptedInputSchedulerExit, AcceptedInputSchedulerSignal, AcceptedInputWakeReason,
        ActiveSteeringCancellationLifecycle, RecoveredProjectionLane, RecoveredProjectionLaneParts,
        RecoveredProjectionLaneStageError, StartupRecoveryDiagnostics,
    },
    connection::ProjectionConnection,
    persistent_failure::{
        LiveCommandAuthorizer, MasterCommandGate, MasterCommandGateCloseOwner,
        PersistentFailureCoordinator, PersistentFailureCutCompletion, PersistentFailureCutHandoff,
        PersistentFailureCutSnapshot, PersistentFailureCutState, PersistentFailureNotification,
        PersistentFailureRetainedService, ProjectionServiceGeneration,
        persistent_failure_notification_channel,
    },
    scheduled_ordinary::{
        ScheduledOrdinaryAdmission, ScheduledOrdinaryAdmissionError,
        ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryExecutionLease,
        ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionUnavailable,
    },
    service_config::{
        ProjectionServiceConfig, ProjectionWorkerPermitError, ProjectionWorkerPermitPair,
        ProjectionWorkerPool, ProjectionWorkerPoolDiagnostics,
    },
    service_registry::ProjectionServiceConnectionRegistry,
    service_startup::ServiceStartupGate,
    stop::{
        BoundedHardStopResult, StopCoordinationError, StopCoordinationOutcome, StopCoordinator,
        StopOwnership, WindowCloseStopBarrier, WindowCloseStopOutcome,
    },
};
use crate::input_admission::PreparedAcceptedInputAdmission;
#[cfg(test)]
use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    mpsc::{Receiver, SyncSender, sync_channel},
};

mod admission;
mod adoption;
mod commands;
mod construction;
mod flight_registry;
mod scheduling;
mod shutdown;

pub use adoption::{
    AdoptedProjectionCandidateReauthenticationLedger,
    AdoptedUnpublishedProjectionConnectionService,
    CandidateSetConvergedAdoptedProjectionConnectionService, PersistentFailureServiceAdoptionError,
    PersistentFailureServiceAdoptionMetadata, PersistentFailureServiceAdoptionReason,
    ProjectionCandidateDispositionOutcome, ProjectionCandidateId,
    ProjectionCandidateLedgerAccessError, ProjectionCandidateLedgerMetadata,
    ProjectionCandidateLedgerSealError, ProjectionCandidateLedgerSealFailure,
    ProjectionCandidateLedgerSealReason, ProjectionCandidateMetadata,
    ProjectionCandidateReauthenticationOutcome, ProjectionCandidateReauthenticationReason,
    ProjectionCandidateReauthenticationStatus, RecoveredProjectionCandidateMetadata,
    RecoveredServicePublicationError, RecoveredServicePublicationMetadata,
    RecoveredServicePublicationReason, TerminalAdoptedProjectionConnectionService,
    TerminalAdoptedProjectionConnectionServiceReason, UnpublishedProjectionConnectionService,
    UnpublishedProjectionConnectionServiceBuildError,
    UnpublishedProjectionConnectionServiceMetadata,
};

pub(super) use flight_registry::ProjectionFlight;

#[cfg(test)]
static NEXT_ADMISSION_RECONCILIATION_PAUSE: AtomicU64 = AtomicU64::new(1);

/// Synchronous outcome of one deliberate hard-stop request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HardStopCoordinationOutcome {
    /// The caller received the immutable bounded result shared by this stop operation.
    Finished(BoundedHardStopResult),
    /// No exact selected provider operation was eligible when the request was admitted.
    Ineligible(StopAdmissionIneligibility),
}

struct PreparedProjectionSessionAdmission {
    command: super::LiveCommandPermit,
    home: Arc<HomeStore>,
    worker_permits: ProjectionWorkerPermitPair,
}

#[cfg(test)]
struct PendingAdmissionReconciliationPause {
    token: u64,
    input_id: SyndicAcceptedInputId,
    arrived: SyncSender<()>,
    release: Receiver<()>,
}

#[cfg(test)]
pub(in crate::cas_projection) struct AdmissionReconciliationPauseController {
    slot: Arc<Mutex<Option<PendingAdmissionReconciliationPause>>>,
    token: u64,
    arrived: Receiver<()>,
    release: SyncSender<()>,
}

/// Process-owned admission and shutdown boundary for projection connections.
pub struct ProjectionConnectionService {
    home: Option<Arc<HomeStore>>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    startup: ProjectionServiceStartupState,
    config: ProjectionServiceConfig,
    workers: ProjectionWorkerPool,
    service_generation: ProjectionServiceGeneration,
    command_gate: MasterCommandGate,
    command_authorizer: LiveCommandAuthorizer,
    persistent_failure: Option<PersistentFailureCoordinator>,
    persistent_failure_escrow:
        Option<super::persistent_failure::PersistentFailureServiceEscrowReservation>,
    #[cfg(test)]
    admission_reconciliation_failures: AtomicUsize,
    #[cfg(test)]
    admission_reconciliation_pause: Arc<Mutex<Option<PendingAdmissionReconciliationPause>>>,
    connections: Arc<ProjectionServiceConnectionRegistry>,
    stop_coordinator: Arc<StopCoordinator>,
    context_compaction: Option<Arc<super::context_compaction::ContextCompactionCoordinator>>,
    scheduler: Option<AcceptedInputScheduler>,
    scheduler_signal: AcceptedInputSchedulerSignal,
    recovered_projection_lane: RecoveredProjectionLane,
    scheduled_ordinary_provider: Option<Arc<Mutex<Box<dyn ScheduledOrdinaryExecutionProvider>>>>,
    settled: bool,
}

enum ProjectionServiceStartupState {
    Dormant,
    Ready { storage_revision: DomainRevision },
}

enum ProjectionServiceStartupPreparation {
    Dormant,
    Ready {
        storage_revision: DomainRevision,
        recovery: StartupRecoveryDiagnostics,
    },
}

impl ProjectionServiceStartupState {
    const fn storage_revision(&self) -> DomainRevision {
        match self {
            Self::Ready { storage_revision } => *storage_revision,
            Self::Dormant => {
                panic!("a dormant unpublished service has no converged storage revision")
            }
        }
    }
}

/// One scoped process-shell capability for the service-owned Beryl home.
///
/// The capability retains a master live-command permit for as long as the
/// borrowed home can be reached. Persistent-failure target election therefore
/// cannot begin until every store-dependent caller has released its borrow.
pub struct LiveHomeCommand<'a> {
    home: &'a HomeStore,
    _permit: super::persistent_failure::LiveCommandPermit,
}

/// Consuming close result for one projection-service generation.
#[must_use = "persistent failure returns the sole process recovery handoff"]
#[derive(Debug)]
pub enum ProjectionConnectionServiceCloseOutcome {
    /// Ordinary shutdown won and explicitly closed the owned home.
    Closed,
    /// Persistent failure won and preserved the failed service without destructive shutdown.
    PersistentFailure(PersistentFailureCutHandoff),
}

impl LiveHomeCommand<'_> {
    /// Returns the exact service-owned home while this command remains admitted.
    #[must_use]
    pub const fn home(&self) -> &HomeStore {
        self.home
    }
}

#[derive(Debug, Error)]
pub enum AcceptedInputAdmissionExecutionError {
    #[error("the projection connection service is closed")]
    ServiceClosed,
    #[error(transparent)]
    Authority(#[from] ProjectionCoordinatorError),
    #[error("accepted-input publication did not commit: {0}")]
    Command(#[source] CommandError),
    #[error("accepted-input publication reconciliation failed; the service was closed: {0}")]
    Reconciliation(#[from] SyndicReadError),
    #[error(
        "accepted-input publication reported success without exact durable admission; the service was closed"
    )]
    ImpossibleReconciliation,
    #[error("accepted-input publication collided with current durable state")]
    Collision,
}

#[derive(Debug, Error)]
pub enum ProjectionConnectionServiceCloseError {
    #[error("one or more projection connection workers failed during shutdown")]
    ConnectionShutdown,
    #[error("the active-steering scheduler failed or panicked before shutdown")]
    SchedulerShutdown,
    #[error("the scheduled ordinary execution provider remained shared after scheduler shutdown")]
    ExecutionProviderShutdown,
    #[error("the context-compaction coordinator failed or panicked before shutdown")]
    ContextCompactionShutdown,
    #[error("the persistent-failure cut worker failed or panicked during shutdown")]
    PersistentFailureWorkerShutdown,
    #[error("a joined projection connection still owns the Beryl home")]
    HomeOwnershipLeaked,
    #[error("the owned Beryl home failed explicit close: {0}")]
    HomeClose(#[source] HomeCloseError),
}

/// App-owned coordinator for projection work against one exact healthy home generation.
///
/// The coordinator serializes only competing work for the same Syndic thread.
/// Different threads remain independent. Its mutexes protect bounded in-memory
/// registry mutations only and are never retained across backend or storage
/// work.
pub struct CasProjectionCoordinator {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
}

#[cfg(test)]
mod lifecycle_test_admission_tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/lifecycle_test_admission.rs"
    ));
}

#[cfg(all(test, feature = "test-faults"))]
mod persistent_failure_tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/persistent_failure_cut.rs"
    ));
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/service_epoch_adoption.rs"
    ));
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/projection_candidate_reauthentication.rs"
    ));

    mod forwarding_order {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/unit/service_epoch_adoption/forwarding_order.rs"
        ));
    }
}
