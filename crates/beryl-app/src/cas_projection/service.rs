use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use beryl_backend::{ManagedBackendClientConnector, ManagedBackendError, ManagedBackendSession};
use beryl_home_store::{
    CommandError, CommitReceipt, FreeSpaceOutcome, HomeCloseError, HomeGeneration, HomeHealthState,
    HomeStore,
};
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
        ActiveSteeringCancellationLifecycle, StartupRecoveryDiagnostics,
    },
    connection::ProjectionConnection,
    initial_start::InitialStartGate,
    persistent_failure::{
        LiveCommandAuthorizer, MasterCommandGate, MasterCommandGateCloseOwner,
        PersistentFailureCoordinator, PersistentFailureCutCompletion, PersistentFailureCutSnapshot,
        PersistentFailureCutState, PersistentFailureNotification,
        PersistentFailureTerminalEvidence, ProjectionServiceGeneration,
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
    stop::{
        StopCoordinationError, StopCoordinationOutcome, StopCoordinator, StopOwnership,
        WindowCloseStopBarrier, WindowCloseStopOutcome,
    },
};
use crate::input_admission::{InputAdmissionBuildError, PreparedAcceptedInputAdmission};
#[cfg(test)]
use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    mpsc::{Receiver, SyncSender, sync_channel},
};

mod admission;
mod commands;
mod construction;
mod flight_registry;
mod scheduling;
mod shutdown;

pub(super) use flight_registry::ProjectionFlight;

#[cfg(test)]
static NEXT_ADMISSION_RECONCILIATION_PAUSE: AtomicU64 = AtomicU64::new(1);

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
    startup_storage_revision: DomainRevision,
    config: ProjectionServiceConfig,
    workers: ProjectionWorkerPool,
    service_generation: ProjectionServiceGeneration,
    command_gate: MasterCommandGate,
    command_authorizer: LiveCommandAuthorizer,
    persistent_failure: Option<PersistentFailureCoordinator>,
    #[cfg(test)]
    admission_reconciliation_failures: AtomicUsize,
    #[cfg(test)]
    admission_reconciliation_pause: Arc<Mutex<Option<PendingAdmissionReconciliationPause>>>,
    connections: Arc<ProjectionServiceConnectionRegistry>,
    stop_coordinator: Arc<StopCoordinator>,
    context_compaction: Option<Arc<super::context_compaction::ContextCompactionCoordinator>>,
    scheduler: Option<AcceptedInputScheduler>,
    scheduler_signal: AcceptedInputSchedulerSignal,
    scheduled_ordinary_provider: Option<Arc<Mutex<Box<dyn ScheduledOrdinaryExecutionProvider>>>>,
    settled: bool,
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
#[must_use = "persistent failure returns bounded terminal evidence"]
#[derive(Debug)]
pub enum ProjectionConnectionServiceCloseOutcome {
    /// Ordinary shutdown won and explicitly closed the owned home.
    Closed,
    /// Persistent failure won and the failed generation was terminally disposed.
    PersistentFailure(PersistentFailureTerminalEvidence),
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
    #[error("accepted-input publication committed before a later failure: {later_failure}")]
    CommandCommitted {
        receipt: CommitReceipt,
        #[source]
        later_failure: CommandError,
    },
    #[error("accepted-input publication has an indeterminate durable outcome: {failure}")]
    CommandIndeterminate {
        #[source]
        failure: CommandError,
    },
    #[error("accepted-input publication reconciliation failed; the service was closed: {0}")]
    Reconciliation(#[from] SyndicReadError),
    #[error(
        "accepted-input publication reported success without exact durable admission; the service was closed"
    )]
    ImpossibleReconciliation,
    #[error("accepted-input publication collided with current durable state")]
    Collision,
}

/// Failure while making a direct idle submission durable.
#[derive(Debug, Error)]
pub enum IdleSubmissionExecutionError {
    #[error("the projection connection service is closed")]
    ServiceClosed,
    #[error(transparent)]
    Authority(#[from] ProjectionCoordinatorError),
    #[error(transparent)]
    Build(#[from] InputAdmissionBuildError),
    #[error("turn-start free-space admission denied: {0:?}")]
    FreeSpace(FreeSpaceOutcome),
    #[error("idle submission did not commit: {0}")]
    Command(#[source] CommandError),
    #[error("idle submission committed before a later failure: {later_failure}")]
    CommandCommitted {
        receipt: CommitReceipt,
        #[source]
        later_failure: CommandError,
    },
    #[error("idle submission has an indeterminate durable outcome: {failure}")]
    CommandIndeterminate {
        #[source]
        failure: CommandError,
    },
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
    #[error("persistent-failure authority could not be terminally disposed")]
    PersistentFailureDisposal,
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
}
