use beryl_home_store::HomeServiceReference;
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use beryl_backend::{ManagedBackendClientConnector, ManagedBackendError, ManagedBackendSession};
mod submission_execution;
use beryl_home_store::{HomeCloseError, HomeGeneration, HomeHealthState, HomeStore};
use beryl_model::{
    BerylHomeId, CasProcessGeneration, DomainRevision, ExecutionBinding, RuntimeId, SyndicThreadId,
    SyndicTurnId,
};
pub use submission_execution::SubmissionExecutionWake;
#[cfg(any(test, feature = "test-faults"))]
pub use submission_execution::SubmissionExecutionWakeTestProbe;
use syndic_storage::{
    StopAdmissionIneligibility, StopAdmissionRead, StopCause, SyndicPointReadLimit, SyndicStorage,
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
    AdmittedProjectionSession, NativeLineageRecoveryControl, ProjectionCoordinatorError,
    ProjectionRegistryKind, ProjectionSessionAdmissionError,
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

mod admission;
mod commands;
mod resolution_admission;
pub(in crate::cas_projection) use resolution_admission::admit_resolution;
pub use resolution_admission::{DiscussionResolutionOutcome, ScopedDiscussionResolutionOutcome};
mod compaction_work;
mod construction;
mod control_work;
mod flight_registry;
mod graceful_shutdown;
pub(crate) mod initial_preparation;
mod mutation_wake;
pub(crate) mod recovery_preparation;
pub(crate) mod recovery_retirement;
#[cfg(feature = "test-faults")]
pub use graceful_shutdown::GracefulShutdownProbe;
pub(crate) use graceful_shutdown::{
    ShutdownAttemptId, ShutdownCoordinatorError, ShutdownFailure, ShutdownProgress,
};
mod process_work;
pub use process_work::*;
mod runtime_failure;
mod runtime_retry;
pub use runtime_retry::{
    SelectedRuntimeRetryError, SelectedRuntimeRetryWorker, SelectedRuntimeUsability,
};
mod runtime_interest;
pub use runtime_failure::{RuntimeFailureReader, SelectedRuntimeFailureObservation};
mod runtime_preparation;
mod scheduling;
mod shutdown;
pub use shutdown::ProjectionConnectionServiceCloseFailure;
mod shutdown_connections;
mod shutdown_settlement;
pub(crate) use shutdown_settlement::{
    ShutdownThreadDisposition, ShutdownThreadSettlement, ShutdownThreadSettlementError,
};
mod stop_feedback;
mod stop_worker;
pub use stop_worker::ExactStopWorker;
pub use stop_worker::{ExactOperationOrigin, ExactParentState, ExactSelectedOperationSnapshot};
mod stop_work;
mod work_facts;
mod work_sources;
pub(in crate::cas_projection) use work_sources::ProcessWorkSources;

pub(super) use admission::ProjectionAdmissionContext;
pub(super) use flight_registry::ProjectionFlight;
pub(crate) use flight_registry::TerminalCompletionObserver;
pub(in crate::cas_projection) use flight_registry::TerminalCompletionPublisher;

struct PreparedProjectionSessionAdmission {
    command: super::LiveCommandPermit,
    home: Arc<HomeServiceReference>,
    worker_permits: ProjectionWorkerPermitPair,
    _acquisition: super::acquisition::ProjectionAcquisition,
}

/// Process-owned admission and shutdown boundary for projection connections.
pub struct ProjectionConnectionService {
    outage_inventory: Arc<super::outage_buffer::OutageInventory>,
    owned_home: Option<HomeStore>,
    initial_start: Arc<InitialStartGate>,
    home: Option<Arc<HomeServiceReference>>,
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
    connections: Arc<ProjectionServiceConnectionRegistry>,
    stop_coordinator: Arc<StopCoordinator>,
    resolution: Arc<super::process_tools::ResolutionAuthority>,
    context_compaction: Option<Arc<super::context_compaction::ContextCompactionCoordinator>>,
    scheduler: Option<AcceptedInputScheduler>,
    scheduler_signal: AcceptedInputSchedulerSignal,
    mutation_observer: beryl_home_store::HomeMutationObserver,
    mutation_wake: Arc<mutation_wake::HomeMutationWake>,
    native_lineage_recovery: NativeLineageRecoveryControl,
    scheduled_ordinary_provider: Option<Arc<Mutex<Box<dyn ScheduledOrdinaryExecutionProvider>>>>,
    runtime_interest: Option<Arc<super::runtime_interest::RuntimeInterestOwner>>,
    graceful_shutdown: Mutex<graceful_shutdown::ShutdownCoordinator>,
    settled: bool,
    shutdown_started: bool,
    close_retry_error: Option<ProjectionConnectionServiceCloseError>,
    close_auxiliary_error: Option<ProjectionConnectionServiceCloseError>,
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

impl Drop for LiveHomeCommand<'_> {
    fn drop(&mut self) {
        // Drop checking must retain the home borrow until the permit is released.
    }
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
pub enum ProjectionConnectionServiceCloseError {
    #[error("previous service disposal started but did not prove complete joined closure")]
    ShutdownIncomplete,
    #[error("one or more managed runtimes failed joined retirement")]
    RuntimeRetirement,
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
    #[error("the live-command gate could not confirm drained shutdown")]
    CommandDrain,
    #[error("persistent-failure authority could not be terminally disposed")]
    PersistentFailureDisposal,
    #[error("the projection service could not regain exclusive shutdown ownership")]
    ServiceOwnershipUnavailable,
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
mod home_ownership_tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/service_home_ownership.rs"
    ));
}

#[cfg(all(test, feature = "test-faults"))]
mod persistent_failure_tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/persistent_failure_cut.rs"
    ));
}
