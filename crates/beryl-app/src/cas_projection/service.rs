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
#[cfg(test)]
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

mod adoption;

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

enum PreparedStop {
    Exact {
        target: syndic_storage::StopOperationTarget,
        connection: Arc<ProjectionConnection>,
        proof: crate::cas_projection::connection::StopTargetProof,
    },
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

impl ProjectionConnectionService {
    /// Takes sole process ownership of one healthy registered home.
    ///
    /// `scheduled_ordinary_provider` is the required process-shell boundary for
    /// issuing complete no-backlog ordinary-execution leases. Service shutdown
    /// invokes its shutdown fence after scheduler workers join and before the
    /// owned home is closed. Construction first walks bounded durable recovery
    /// pages and converges possible-dispatch authority synchronously. Only then
    /// does it start the scheduler and publish the one-way recovery handoff, so
    /// newer same-process admission cannot leapfrog pre-existing work. Scheduled
    /// execution binding is read from Syndic's immutable per-thread authority.
    pub fn new(
        home: HomeStore,
        storage: SyndicStorage,
        config: ProjectionServiceConfig,
        scheduled_ordinary_provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
    ) -> Result<Self, ProjectionCoordinatorError> {
        let home = Arc::new(home);
        let health = home.health();
        if health.state() != HomeHealthState::Healthy {
            return Err(ProjectionCoordinatorError::HomeNotHealthy {
                state: health.state(),
                generation: health.generation(),
            });
        }
        let Some(home_generation) = health.generation() else {
            return Err(ProjectionCoordinatorError::HealthyHomeGenerationMissing);
        };
        let recovery = super::accepted_delivery_recovery::recover_startup(
            &home,
            home.home_id(),
            home_generation,
            storage,
        )?;
        let storage_revision = storage
            .revision(&home)
            .map_err(|source| ProjectionCoordinatorError::SyndicRevisionUnavailable { source })?;
        Self::construct_with_startup_gate(
            home,
            storage,
            config,
            scheduled_ordinary_provider,
            ServiceStartupGate::open_gate(),
            ProjectionServiceStartupPreparation::Ready {
                storage_revision,
                recovery,
            },
        )
    }

    pub(in crate::cas_projection) fn new_dormant_with_startup_gate(
        home: Arc<HomeStore>,
        storage: SyndicStorage,
        config: ProjectionServiceConfig,
        scheduled_ordinary_provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
        startup_gate: Arc<ServiceStartupGate>,
    ) -> Result<Self, ProjectionCoordinatorError> {
        Self::construct_with_startup_gate(
            home,
            storage,
            config,
            scheduled_ordinary_provider,
            startup_gate,
            ProjectionServiceStartupPreparation::Dormant,
        )
    }

    fn construct_with_startup_gate(
        home: Arc<HomeStore>,
        storage: SyndicStorage,
        config: ProjectionServiceConfig,
        scheduled_ordinary_provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
        startup_gate: Arc<ServiceStartupGate>,
        startup: ProjectionServiceStartupPreparation,
    ) -> Result<Self, ProjectionCoordinatorError> {
        let health = home.health();
        if health.state() != HomeHealthState::Healthy {
            return Err(ProjectionCoordinatorError::HomeNotHealthy {
                state: health.state(),
                generation: health.generation(),
            });
        }
        let Some(home_generation) = health.generation() else {
            return Err(ProjectionCoordinatorError::HealthyHomeGenerationMissing);
        };
        let service_generation = ProjectionServiceGeneration::allocate()
            .map_err(|_| ProjectionCoordinatorError::ProjectionServiceGenerationExhausted)?;
        let persistent_failure_escrow =
            super::persistent_failure::PersistentFailureServiceEscrowReservation::reserve(
                home.home_id(),
                home_generation,
                service_generation,
            )?;
        let (failure_notification, failure_receiver) = persistent_failure_notification_channel(
            &home,
            home.home_id(),
            home_generation,
            service_generation,
        );
        let command_gate =
            MasterCommandGate::new(service_generation, Some(failure_notification.clone()));
        let command_authorizer = command_gate.authorizer();
        let connections = ProjectionServiceConnectionRegistry::new(service_generation);
        let stop_coordinator = Arc::new(StopCoordinator::new(
            &home,
            home.home_id(),
            home_generation,
            storage,
            command_authorizer.clone(),
        ));
        let persistent_failure = PersistentFailureCoordinator::start_with_startup_gate(
            Arc::clone(&home),
            home.home_id(),
            home_generation,
            service_generation,
            command_gate.clone(),
            failure_notification,
            failure_receiver,
            Arc::clone(&stop_coordinator),
            Arc::clone(&connections),
            Arc::clone(&startup_gate),
        )
        .map_err(
            |error| ProjectionCoordinatorError::PersistentFailureWorkerSpawn {
                message: error.to_string(),
            },
        )?;
        let scheduler_signal = AcceptedInputSchedulerSignal::new();
        let context_compaction =
            super::context_compaction::ContextCompactionCoordinator::new_with_startup_gate(
                Arc::clone(&home),
                home.home_id(),
                home_generation,
                storage,
                Arc::clone(&connections),
                Arc::clone(&stop_coordinator),
                command_authorizer.clone(),
                scheduler_signal.clone(),
                Arc::clone(&startup_gate),
            )
            .map_err(|_| ProjectionCoordinatorError::ContextCompactionCoordinatorUnavailable)?;
        let scheduled_ordinary_provider = Arc::new(Mutex::new(scheduled_ordinary_provider));
        let recovered_projection_lane = RecoveredProjectionLane::new(
            config.worker_capacity().get(),
            Arc::clone(&startup_gate),
            scheduler_signal.clone(),
        );
        let workers = ProjectionWorkerPool::new_with_scheduler(
            config.worker_capacity(),
            scheduler_signal.clone(),
        );
        let scheduler = AcceptedInputScheduler::start_with_startup_gate(
            AcceptedInputSchedulerContext::new(
                Arc::clone(&home),
                home.home_id(),
                home_generation,
                storage,
                workers.clone(),
                Arc::clone(&connections),
                Arc::clone(&scheduled_ordinary_provider),
                command_gate.clone(),
                persistent_failure.projection_retainer(home.home_id(), home_generation),
                ActiveSteeringCancellationLifecycle::new(),
                scheduler_signal.clone(),
                recovered_projection_lane.clone(),
            ),
            startup_gate,
        )?;
        let startup = match startup {
            ProjectionServiceStartupPreparation::Dormant => ProjectionServiceStartupState::Dormant,
            ProjectionServiceStartupPreparation::Ready {
                storage_revision,
                recovery,
            } => {
                scheduler_signal.hand_off_recovery(recovery);
                ProjectionServiceStartupState::Ready { storage_revision }
            }
        };
        Ok(Self {
            home_id: home.home_id(),
            home_generation,
            home: Some(home),
            storage,
            startup,
            config,
            workers,
            service_generation,
            command_gate,
            command_authorizer,
            persistent_failure: Some(persistent_failure),
            persistent_failure_escrow: Some(persistent_failure_escrow),
            #[cfg(test)]
            admission_reconciliation_failures: AtomicUsize::new(0),
            #[cfg(test)]
            admission_reconciliation_pause: Arc::new(Mutex::new(None)),
            connections,
            stop_coordinator,
            context_compaction: Some(context_compaction),
            scheduler: Some(scheduler),
            scheduler_signal,
            recovered_projection_lane,
            scheduled_ordinary_provider: Some(scheduled_ordinary_provider),
            settled: false,
        })
    }

    /// Creates, initializes, probes, and admits one exact foreground candidate.
    ///
    /// The service selects the candidate's home, healthy generation, registered
    /// storage, process identity, immutable foreground configuration, and worker
    /// permit pair before the connector performs the authenticated WebSocket handshake.
    pub fn admit(
        &self,
        connector: &ManagedBackendClientConnector,
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        config_cwd: &Path,
        timeout: Duration,
    ) -> Result<AdmittedProjectionSession, ProjectionSessionAdmissionError> {
        if connector.launch_identity().is_some_and(|identity| {
            identity.runtime_id() != runtime_id
                || identity.process_generation() != process_generation
                || config_cwd.to_str() != Some(identity.working_directory().as_str())
        }) {
            return Err(ProjectionSessionAdmissionError::compatibility(
                runtime_id,
                process_generation,
                ManagedBackendError::ManagedLaunchIdentityMismatch,
            ));
        }
        let prepared = self.prepare_session_admission(runtime_id, process_generation)?;
        let mut backend = self.connect_and_initialize_candidate(
            connector,
            runtime_id,
            process_generation,
            timeout,
        )?;
        let compatibility_report =
            backend
                .probe_compatibility(config_cwd, timeout)
                .map_err(|source| {
                    ProjectionSessionAdmissionError::compatibility(
                        runtime_id,
                        process_generation,
                        source,
                    )
                })?;
        if connector.launch_identity() != compatibility_report.launch_identity() {
            return Err(ProjectionSessionAdmissionError::compatibility(
                runtime_id,
                process_generation,
                ManagedBackendError::ManagedLaunchIdentityMismatch,
            ));
        }
        let connection =
            self.finish_session_admission(backend, runtime_id, process_generation, prepared)?;
        Ok(AdmittedProjectionSession::from_production_connection(
            connection,
            compatibility_report,
        ))
    }

    /// Admits one lifecycle fixture without manufacturing managed-launch authority.
    #[cfg(test)]
    #[doc(hidden)]
    pub(in crate::cas_projection) fn admit_lifecycle_test_candidate(
        &self,
        connector: &ManagedBackendClientConnector,
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        config_cwd: &Path,
        timeout: Duration,
    ) -> Result<AdmittedProjectionSession, ProjectionSessionAdmissionError> {
        if connector.launch_identity().is_some() {
            return Err(ProjectionSessionAdmissionError::compatibility(
                runtime_id,
                process_generation,
                ManagedBackendError::ManagedLaunchIdentityMismatch,
            ));
        }
        let prepared = self.prepare_session_admission(runtime_id, process_generation)?;
        let mut backend = self.connect_and_initialize_candidate(
            connector,
            runtime_id,
            process_generation,
            timeout,
        )?;
        let (probe_successes, config_defaults, thread_branch_capabilities) = backend
            .probe_non_authorizing_compatibility_for_lifecycle_test(config_cwd, timeout)
            .map_err(|source| {
                ProjectionSessionAdmissionError::compatibility(
                    runtime_id,
                    process_generation,
                    source,
                )
            })?;
        let connection =
            self.finish_session_admission(backend, runtime_id, process_generation, prepared)?;
        Ok(AdmittedProjectionSession::from_lifecycle_test_connection(
            connection,
            LifecycleTestCompatibilityFacts::new(
                probe_successes,
                config_defaults,
                thread_branch_capabilities,
            ),
        ))
    }

    fn prepare_session_admission(
        &self,
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
    ) -> Result<PreparedProjectionSessionAdmission, ProjectionSessionAdmissionError> {
        let command = self.command_authorizer.authorize().map_err(|_| {
            ProjectionSessionAdmissionError::service_closed(runtime_id, process_generation)
        })?;
        self.ensure_current().map_err(|source| {
            ProjectionSessionAdmissionError::connection_ownership(
                runtime_id,
                process_generation,
                source,
            )
        })?;
        self.connections.reap_finished_ordinary_retirements();
        let worker_permits = self.workers.try_acquire_pair().map_err(|error| {
            let source = match error {
                ProjectionWorkerPermitError::CapacityFull { available } => {
                    ProjectionCoordinatorError::ProjectionWorkerCapacityFull { available }
                }
                ProjectionWorkerPermitError::Poisoned => {
                    ProjectionCoordinatorError::ProjectionWorkerPoolPoisoned
                }
            };
            ProjectionSessionAdmissionError::connection_ownership(
                runtime_id,
                process_generation,
                source,
            )
        })?;
        Ok(PreparedProjectionSessionAdmission {
            command,
            home: Arc::clone(self.home.as_ref().expect("open service owns its home")),
            worker_permits,
        })
    }

    fn connect_and_initialize_candidate(
        &self,
        connector: &ManagedBackendClientConnector,
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        timeout: Duration,
    ) -> Result<ManagedBackendSession, ProjectionSessionAdmissionError> {
        let mut backend = connector
            .connect_foreground_candidate(self.config.foreground(), timeout)
            .map_err(|source| {
                ProjectionSessionAdmissionError::candidate_connection(
                    runtime_id,
                    process_generation,
                    source,
                )
            })?;
        backend.initialize_foreground(timeout).map_err(|source| {
            ProjectionSessionAdmissionError::initialization(runtime_id, process_generation, source)
        })?;
        Ok(backend)
    }

    fn finish_session_admission(
        &self,
        backend: ManagedBackendSession,
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        prepared: PreparedProjectionSessionAdmission,
    ) -> Result<Arc<ProjectionConnection>, ProjectionSessionAdmissionError> {
        let connection = ProjectionConnection::new(
            backend,
            runtime_id,
            process_generation,
            prepared.home,
            self.home_id,
            self.home_generation,
            self.storage,
            prepared.worker_permits,
            self.scheduler_signal.clone(),
            Arc::clone(&self.stop_coordinator),
            Arc::clone(
                self.context_compaction
                    .as_ref()
                    .expect("open service retains its context-compaction coordinator"),
            ),
            self.command_authorizer.clone(),
            self.persistent_failure
                .as_ref()
                .expect("open service retains its persistent-failure coordinator")
                .notification(),
            self.persistent_failure
                .as_ref()
                .expect("open service retains its persistent-failure coordinator")
                .projection_retainer(self.home_id, self.home_generation),
        )
        .map_err(|source| {
            ProjectionSessionAdmissionError::connection_ownership(
                runtime_id,
                process_generation,
                source,
            )
        })?;
        self.register_connection(&connection);
        if !prepared.command.is_current() {
            connection.retire();
            return Err(ProjectionSessionAdmissionError::service_closed(
                runtime_id,
                process_generation,
            ));
        }
        Ok(connection)
    }

    /// Executes and reconciles one exact accepted-input publication.
    ///
    /// Exact durable acceptance wakes automatic steering only after receipt
    /// reconciliation has completed.
    pub fn execute_accepted_input_admission(
        &self,
        prepared: PreparedAcceptedInputAdmission,
    ) -> Result<(), AcceptedInputAdmissionExecutionError> {
        let command = self
            .command_authorizer
            .authorize()
            .map_err(|_| AcceptedInputAdmissionExecutionError::ServiceClosed)?;
        self.ensure_current()?;
        if prepared.home_id != self.home_id {
            return Err(ProjectionCoordinatorError::HomeIdentityMismatch {
                expected: self.home_id,
                actual: prepared.home_id,
            }
            .into());
        }
        if prepared.home_generation != self.home_generation {
            return Err(ProjectionCoordinatorError::HomeGenerationMismatch {
                expected: self.home_generation,
                actual: Some(prepared.home_generation),
                state: HomeHealthState::Healthy,
            }
            .into());
        }
        let home = self
            .home
            .as_deref()
            .ok_or(ProjectionCoordinatorError::HomeOwnershipLeaked)?;
        if !command.is_current() {
            return Err(AcceptedInputAdmissionExecutionError::ServiceClosed);
        }
        self.ensure_current()?;
        let dispatch = home.execute(prepared.command);
        #[cfg(test)]
        self.pause_admission_reconciliation_if_requested(prepared.admission.accepted_input_id());
        let status = match self.accepted_input_status(home, &prepared.admission) {
            Ok(status) => status,
            Err(_) => {
                if let Err(source) = self.ensure_current() {
                    let verification_pending = matches!(
                        &source,
                        ProjectionCoordinatorError::HomeGenerationMismatch {
                            expected,
                            actual: Some(actual),
                            state: HomeHealthState::Verifying,
                        } if *expected == self.home_generation && *actual == self.home_generation
                    );
                    if verification_pending {
                        let _ = command.observe_persistent_failure();
                    } else {
                        self.fail_closed_admission_boundary();
                    }
                    return Err(source.into());
                }
                match self.accepted_input_status(home, &prepared.admission) {
                    Ok(status) => status,
                    Err(source) => {
                        self.fail_closed_admission_boundary();
                        return Err(AcceptedInputAdmissionExecutionError::Reconciliation(source));
                    }
                }
            }
        };
        match status {
            InputAdmissionStatus::ExactAccepted => {
                self.scheduler_signal
                    .wake(AcceptedInputWakeReason::AcceptedReady);
                self.scheduler_signal
                    .wake(AcceptedInputWakeReason::AcceptedNextReady);
                Ok(())
            }
            InputAdmissionStatus::Absent => match dispatch {
                Ok(_) => {
                    self.fail_closed_admission_boundary();
                    Err(AcceptedInputAdmissionExecutionError::ImpossibleReconciliation)
                }
                Err(source) => Err(AcceptedInputAdmissionExecutionError::Command(source)),
            },
            InputAdmissionStatus::ExactSubmitted | InputAdmissionStatus::Collision
                if dispatch.is_err() =>
            {
                Err(AcceptedInputAdmissionExecutionError::Collision)
            }
            InputAdmissionStatus::ExactSubmitted | InputAdmissionStatus::Collision => {
                self.fail_closed_admission_boundary();
                Err(AcceptedInputAdmissionExecutionError::ImpossibleReconciliation)
            }
        }
    }

    /// Runs one bounded exact steering-delivery attempt on the caller's non-GPUI worker.
    #[cfg(test)]
    pub(in crate::cas_projection) fn deliver_active_steering_input(
        &self,
        target: &LiveEventTarget,
        input_id: SyndicAcceptedInputId,
        cancellation: &ProjectionCancellationToken,
        request_timeout: Duration,
    ) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
        let _command = self
            .command_authorizer
            .authorize()
            .map_err(|_| ActiveSteeringDeliveryError::ServiceClosed)?;
        self.ensure_current()?;
        let home = self
            .home
            .as_deref()
            .ok_or(ProjectionCoordinatorError::HomeOwnershipLeaked)?;
        active_steering::deliver(
            home,
            self.home_id,
            self.home_generation,
            self.storage,
            &self.workers,
            target,
            input_id,
            cancellation,
            request_timeout,
        )
    }

    /// Records the first lifecycle-yield outcome for one exact executing Syndic turn.
    ///
    /// The state is owned by the healthy-home process service rather than a window. A phase-
    /// continuation outcome is refused when the same exact turn is already durably stopping.
    pub fn record_lifecycle_yield_outcome(
        &self,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        outcome: crate::LifecycleYieldOutcome,
    ) -> Result<bool, StopCoordinationError> {
        self.stop_coordinator
            .record_lifecycle_yield(thread_id, turn_id, outcome)
    }

    /// Admits or joins one exact durable context-compaction operation.
    ///
    /// The caller must be a non-GPUI worker. Expiring the shared deadline returns
    /// `StillRunning` while the process coordinator continues exact convergence.
    pub fn compact_thread(
        &self,
        request: super::ContextCompactionRequest,
    ) -> Result<super::ContextCompactionOutcome, super::ContextCompactionError> {
        let _command = self
            .command_authorizer
            .authorize()
            .map_err(|_| super::ContextCompactionError::Unavailable)?;
        self.context_compaction
            .as_ref()
            .ok_or(super::ContextCompactionError::Unavailable)?
            .compact_thread(request)
    }

    /// Consumes one process-owned lifecycle-yield outcome after exact terminal observation.
    ///
    /// Stop admission removes only a matching automatic phase continuation. Other terminal
    /// notification outcomes remain available to the later GUI integration phase.
    pub fn take_terminal_lifecycle_yield_outcome(
        &self,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    ) -> Result<Option<crate::LifecycleYieldOutcome>, StopCoordinationError> {
        self.stop_coordinator
            .take_terminal_lifecycle_yield(thread_id, turn_id)
    }

    /// Admits or joins deliberate control of one exact selected provider operation.
    ///
    /// This synchronous boundary performs storage and transport waits and must run on a non-GPUI
    /// worker. Matching acceptance remains [`StopCoordinationOutcome::Stopping`] until ordinary
    /// terminal or authority-loss convergence consumes the durable stop.
    pub fn stop_selected_operation(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<StopCoordinationOutcome, StopCoordinationError> {
        self.coordinate_stop(thread_id, StopCause::SelectedOperationControl)
            .map(|(outcome, _)| outcome)
    }

    /// Admits or joins the diagnostic cause for one exact selected provider operation.
    pub fn stop_selected_operation_for_diagnostics(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<StopCoordinationOutcome, StopCoordinationError> {
        self.coordinate_stop(thread_id, StopCause::DiagnosticControl)
            .map(|(outcome, _)| outcome)
    }

    /// Admits, attaches, and completes one bounded hard escalation for the selected operation.
    ///
    /// The primary interruption and every admitted hard target run synchronously on the same
    /// foreground driver. Duplicate callers join the immutable result for the durable stop
    /// operation and never repeat either request.
    pub fn hard_stop_selected_operation(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<HardStopCoordinationOutcome, StopCoordinationError> {
        self.coordinate_hard_stop(thread_id, StopCause::SelectedOperationControl)
    }

    /// Performs the diagnostic form of the same exact bounded hard-stop operation.
    pub fn hard_stop_selected_operation_for_diagnostics(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<HardStopCoordinationOutcome, StopCoordinationError> {
        self.coordinate_hard_stop(thread_id, StopCause::DiagnosticControl)
    }

    /// Admits or joins healthy-home window-close ownership of one exact selected operation.
    ///
    /// A waiting result owns an exact non-cloneable barrier. The non-GUI caller must retain its
    /// thread claim until that barrier reports terminal-history or authority-loss convergence.
    pub fn stop_selected_operation_for_window_close(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<WindowCloseStopOutcome, StopCoordinationError> {
        let (outcome, target) =
            self.coordinate_stop(thread_id, StopCause::HealthyHomeWindowClose)?;
        Ok(match outcome {
            StopCoordinationOutcome::Stopping {
                operation_id,
                primary_owner,
            } => WindowCloseStopOutcome::Waiting(WindowCloseStopBarrier::new(
                Arc::clone(&self.stop_coordinator),
                operation_id,
                target
                    .as_ref()
                    .expect("a stopping outcome retains its exact target")
                    .turn_id(),
                primary_owner,
            )),
            StopCoordinationOutcome::Abandoned { operation_id } => {
                WindowCloseStopOutcome::Waiting(WindowCloseStopBarrier::new(
                    Arc::clone(&self.stop_coordinator),
                    operation_id,
                    target
                        .as_ref()
                        .expect("an abandoned stop outcome retains its exact target")
                        .turn_id(),
                    true,
                ))
            }
            StopCoordinationOutcome::SafelyReopened { operation_id } => {
                WindowCloseStopOutcome::SafelyReopened { operation_id }
            }
            StopCoordinationOutcome::Ineligible(reason) => {
                WindowCloseStopOutcome::Ineligible(reason)
            }
        })
    }

    fn coordinate_stop(
        &self,
        thread_id: SyndicThreadId,
        cause: StopCause,
    ) -> Result<
        (
            StopCoordinationOutcome,
            Option<syndic_storage::StopOperationTarget>,
        ),
        StopCoordinationError,
    > {
        let (target, connection, proof) = match self.prepare_stop(thread_id)? {
            PreparedStop::Exact {
                target,
                connection,
                proof,
            } => (target, connection, proof),
            PreparedStop::Ineligible(reason) => {
                return Ok((StopCoordinationOutcome::Ineligible(reason), None));
            }
        };
        let outcome = match connection.coordinate_stop(&self.stop_coordinator, proof, cause)? {
            StopOwnership::Primary(owner) => connection.dispatch_exact_stop(owner),
            StopOwnership::Joined {
                operation_id,
                interruption: _,
            } => Ok(StopCoordinationOutcome::Stopping {
                operation_id,
                primary_owner: false,
            }),
        }?;
        Ok((outcome, Some(target)))
    }

    fn coordinate_hard_stop(
        &self,
        thread_id: SyndicThreadId,
        cause: StopCause,
    ) -> Result<HardStopCoordinationOutcome, StopCoordinationError> {
        let (connection, proof) = match self.prepare_stop(thread_id)? {
            PreparedStop::Exact {
                target: _,
                connection,
                proof,
            } => (connection, proof),
            PreparedStop::Ineligible(reason) => {
                return Ok(HardStopCoordinationOutcome::Ineligible(reason));
            }
        };
        let continuation_proof = proof.clone();
        let (attachment, continuation) =
            match connection.coordinate_stop(&self.stop_coordinator, proof, cause)? {
                StopOwnership::Primary(owner) => {
                    let operation_id = owner.operation_id();
                    let admission = self.stop_coordinator.attach_hard_stop(operation_id)?;
                    let parts = admission.into_parts();
                    debug_assert!(parts.1.is_none());
                    // The hard result is a separately frozen outcome. Driver or durable-settlement
                    // failure must still let the admitted waiter observe that bounded result.
                    let _ = connection.dispatch_exact_stop(owner);
                    parts
                }
                StopOwnership::Joined {
                    operation_id,
                    interruption: _,
                } => self
                    .stop_coordinator
                    .attach_hard_stop(operation_id)?
                    .into_parts(),
            };
        if let Some(owner) = continuation {
            let _ = connection.dispatch_exact_hard_stop(owner, continuation_proof);
        }
        attachment.wait().map(HardStopCoordinationOutcome::Finished)
    }

    fn prepare_stop(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<PreparedStop, StopCoordinationError> {
        let command = self
            .command_authorizer
            .authorize()
            .map_err(|_| StopCoordinationError::HomeAuthorityLost)?;
        self.ensure_current()
            .map_err(|_| StopCoordinationError::HomeAuthorityLost)?;
        let home = self
            .home
            .as_deref()
            .ok_or(StopCoordinationError::HomeAuthorityLost)?;
        let read = self.storage.stop_admission_read(
            home,
            thread_id,
            SyndicPointReadLimit::new(1_000_000)
                .expect("stop coordination point-read bound is nonzero"),
        )?;
        let target = match read {
            StopAdmissionRead::Admissible(candidate) => candidate.target().clone(),
            StopAdmissionRead::Stopping(live) => live.target().clone(),
            StopAdmissionRead::Ineligible(reason) => {
                return Ok(PreparedStop::Ineligible(reason));
            }
        };
        let (connection, proof) = self.stop_connection(&target)?;
        if !command.is_current() {
            return Err(StopCoordinationError::HomeAuthorityLost);
        }
        Ok(PreparedStop::Exact {
            target,
            connection,
            proof,
        })
    }

    fn stop_connection(
        &self,
        target: &syndic_storage::StopOperationTarget,
    ) -> Result<
        (
            Arc<ProjectionConnection>,
            crate::cas_projection::connection::StopTargetProof,
        ),
        StopCoordinationError,
    > {
        let mut connections = self
            .connections
            .lock()
            .map_err(|_| StopCoordinationError::TargetUnavailable)?;
        let mut found = None;
        let mut duplicate = false;
        connections.retain(|connection| {
            if connection.is_detached() {
                return false;
            }
            if connection.runtime_id() == target.runtime_id()
                && connection.process_generation() == target.loaded_generation().process()
                && let Ok(proof) = connection.stop_target(target)
            {
                if found.is_some() {
                    duplicate = true;
                    return true;
                }
                found = Some((Arc::clone(connection), proof));
            }
            true
        });
        if duplicate {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        found.ok_or(StopCoordinationError::TargetUnavailable)
    }

    fn ensure_current(&self) -> Result<(), ProjectionCoordinatorError> {
        let home = self
            .home
            .as_deref()
            .ok_or(ProjectionCoordinatorError::HomeOwnershipLeaked)?;
        if home.home_id() != self.home_id {
            return Err(ProjectionCoordinatorError::HomeIdentityMismatch {
                expected: self.home_id,
                actual: home.home_id(),
            });
        }
        let health = home.health();
        if health.state() != HomeHealthState::Healthy
            || health.generation() != Some(self.home_generation)
        {
            return Err(ProjectionCoordinatorError::HomeGenerationMismatch {
                expected: self.home_generation,
                actual: health.generation(),
                state: health.state(),
            });
        }
        self.storage
            .revision(home)
            .map_err(|source| ProjectionCoordinatorError::SyndicRevisionUnavailable { source })?;
        Ok(())
    }

    fn accepted_input_status(
        &self,
        home: &HomeStore,
        admission: &AcceptedInputAdmission,
    ) -> Result<InputAdmissionStatus, SyndicReadError> {
        #[cfg(test)]
        if self
            .admission_reconciliation_failures
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
        {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "accepted-input admission reconciliation test fault",
            });
        }
        self.storage
            .accepted_input_status(home, admission, super::input_replay::point_limit())
    }

    fn fail_closed_admission_boundary(&self) {
        self.command_gate.close_for_local_failure();
        if let Some(scheduler) = self.scheduler.as_ref() {
            scheduler.request_shutdown();
        } else {
            self.scheduler_signal.request_shutdown();
        }
    }

    fn register_connection(&self, connection: &Arc<ProjectionConnection>) {
        self.connections.reap_finished_ordinary_retirements();
        let mut active = self
            .connections
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if !active
            .iter()
            .any(|candidate| Arc::ptr_eq(candidate, connection))
        {
            active.push(Arc::clone(connection));
        }
    }

    /// Returns the registered Syndic handle paired with this owned home.
    #[must_use]
    pub const fn storage(&self) -> SyndicStorage {
        self.storage
    }

    #[must_use]
    pub const fn home_id(&self) -> BerylHomeId {
        self.home_id
    }

    #[must_use]
    pub const fn home_generation(&self) -> HomeGeneration {
        self.home_generation
    }

    /// Returns the process-local incarnation of this projection service.
    #[must_use]
    pub const fn service_generation(&self) -> ProjectionServiceGeneration {
        self.service_generation
    }

    /// Supplies the process shell with the same master gate used by projection workers.
    ///
    /// Draft, input-admission, catalog, and other store-dependent workers must retain one scoped
    /// permit through their complete preparation, execution, and publication boundary.
    #[must_use]
    pub(in crate::cas_projection) fn live_command_authorizer(&self) -> LiveCommandAuthorizer {
        self.command_authorizer.clone()
    }

    /// Admits one scoped store-dependent process-shell command.
    ///
    /// This is the only public path from the projection service to its owned
    /// `HomeStore`. The returned borrow cannot outlive the master-gate permit.
    pub fn live_home_command(
        &self,
    ) -> Result<LiveHomeCommand<'_>, super::persistent_failure::LiveCommandAdmissionError> {
        let permit = self.command_authorizer.authorize()?;
        let home = self
            .home
            .as_deref()
            .ok_or(super::persistent_failure::LiveCommandAdmissionError::Closed)?;
        Ok(LiveHomeCommand {
            home,
            _permit: permit,
        })
    }

    /// Returns bounded content-free state for the one-shot persistent-failure cut.
    #[must_use]
    pub fn persistent_failure_cut_snapshot(&self) -> PersistentFailureCutSnapshot {
        self.persistent_failure
            .as_ref()
            .expect("open service retains its persistent-failure coordinator")
            .snapshot()
    }

    /// Returns a cloneable nonblocking typed-health notification handle for process workers.
    #[must_use]
    pub fn persistent_failure_notification(&self) -> PersistentFailureNotification {
        self.persistent_failure
            .as_ref()
            .expect("open service retains its persistent-failure coordinator")
            .notification()
    }

    pub(in crate::cas_projection) fn attach_recovery_supervisor(
        &self,
        signal: std::sync::mpsc::SyncSender<()>,
    ) -> Result<(), ()> {
        self.persistent_failure_notification()
            .attach_recovery_supervisor(signal)
    }

    pub(in crate::cas_projection) fn wake_same_generation_verified(&self) {
        self.scheduler_signal
            .wake(AcceptedInputWakeReason::SameGenerationVerified);
    }

    pub(in crate::cas_projection) fn retained_home_for_recovery(&self) -> Arc<HomeStore> {
        Arc::clone(
            self.home
                .as_ref()
                .expect("an unsettled service retains its exact opened home"),
        )
    }

    pub(in crate::cas_projection) fn converge_dormant_startup(
        &mut self,
    ) -> Result<StartupRecoveryDiagnostics, ProjectionCoordinatorError> {
        assert!(
            matches!(self.startup, ProjectionServiceStartupState::Dormant),
            "only a dormant unpublished service may converge recovered startup state"
        );
        let home = self
            .home
            .as_deref()
            .ok_or(ProjectionCoordinatorError::HomeOwnershipLeaked)?;
        let recovery = super::accepted_delivery_recovery::recover_startup(
            home,
            self.home_id,
            self.home_generation,
            self.storage,
        )?;
        let storage_revision = self
            .storage
            .revision(home)
            .map_err(|source| ProjectionCoordinatorError::SyndicRevisionUnavailable { source })?;
        self.scheduler_signal.hand_off_recovery(recovery);
        self.startup = ProjectionServiceStartupState::Ready { storage_revision };
        Ok(recovery)
    }

    pub(in crate::cas_projection) fn stage_recovered_projection_lane(
        &self,
        entries: Vec<RecoveredProjectionLaneParts>,
    ) -> Result<(), RecoveredProjectionLaneStageError> {
        self.recovered_projection_lane.stage(entries)?;
        self.scheduler_signal
            .wake(AcceptedInputWakeReason::ExecutionReady);
        Ok(())
    }

    #[must_use]
    pub const fn initial_storage_revision(&self) -> DomainRevision {
        self.startup.storage_revision()
    }

    /// Returns the immutable local limits used for every admitted candidate.
    #[must_use]
    pub const fn config(&self) -> ProjectionServiceConfig {
        self.config
    }

    /// Returns content-free worker-pool count diagnostics.
    #[must_use]
    pub fn worker_pool_diagnostics(&self) -> ProjectionWorkerPoolDiagnostics {
        self.workers.diagnostics()
    }

    /// Returns bounded content-free accepted-input scheduler diagnostics.
    #[must_use]
    pub fn accepted_input_scheduler_diagnostics(&self) -> AcceptedInputSchedulerDiagnostics {
        self.scheduler.as_ref().map_or_else(
            || self.scheduler_signal.diagnostics(),
            AcceptedInputScheduler::diagnostics,
        )
    }

    /// Installs and wakes one exact test-only scheduler-main panic.
    #[cfg(feature = "test-faults")]
    #[doc(hidden)]
    pub fn install_accepted_input_scheduler_panic_for_test(
        &self,
    ) -> super::test_faults::AcceptedInputSchedulerPanicController {
        let controller = super::test_faults::install_accepted_input_scheduler_panic(
            self.home_id,
            self.home_generation,
            self.service_generation(),
        );
        self.scheduler_signal
            .wake(AcceptedInputWakeReason::WorkerCompleted);
        controller
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(in crate::cas_projection) fn install_blocked_scheduler_projection_worker_for_test(
        &self,
        projection: LoadedCasProjection,
        worker: super::service_config::ProjectionWorkerPermit,
    ) -> super::test_faults::AcceptedInputSchedulerWorkerController {
        let controller = super::test_faults::install_accepted_input_scheduler_worker(
            self.home_id,
            self.home_generation,
            self.service_generation(),
            projection,
            worker,
        );
        self.scheduler_signal
            .wake(AcceptedInputWakeReason::WorkerCompleted);
        controller
    }

    /// Returns bounded content-free context-compaction capacity diagnostics.
    #[must_use]
    pub fn context_compaction_diagnostics(&self) -> super::ContextCompactionDiagnostics {
        self.context_compaction
            .as_ref()
            .expect("open service retains its context-compaction coordinator")
            .diagnostics()
    }

    #[cfg(feature = "test-faults")]
    #[doc(hidden)]
    pub fn saturate_context_compaction_capacity_for_test(
        &self,
    ) -> Result<super::ContextCompactionCapacityTestGuard, super::ContextCompactionError> {
        self.context_compaction
            .as_ref()
            .ok_or(super::ContextCompactionError::Unavailable)?
            .saturate_capacity_for_test()
    }

    #[cfg(feature = "test-faults")]
    #[doc(hidden)]
    pub fn deny_context_compaction_capacity_probe_for_test(&self) -> bool {
        self.context_compaction
            .as_ref()
            .is_none_or(|coordinator| coordinator.deny_capacity_probe_for_test())
    }

    #[cfg(feature = "test-faults")]
    #[doc(hidden)]
    pub fn stage_context_compaction_continuation_for_test(
        &self,
    ) -> Result<syndic_storage::ContentReference, super::ContextCompactionError> {
        self.context_compaction
            .as_ref()
            .ok_or(super::ContextCompactionError::Unavailable)?
            .stage_lifecycle_content_for_test()
    }

    #[cfg(feature = "test-faults")]
    #[doc(hidden)]
    pub fn context_compaction_lifecycle_test_harness(
        &self,
    ) -> Result<super::ContextCompactionLifecycleTestHarness, super::ContextCompactionError> {
        Ok(self
            .context_compaction
            .as_ref()
            .ok_or(super::ContextCompactionError::Unavailable)?
            .lifecycle_test_harness())
    }

    /// Wakes the shared accepted-input scheduler after process-shell execution authority changes.
    pub fn notify_scheduled_ordinary_execution_ready(&self) {
        if self.command_authorizer.is_open() {
            self.scheduler_signal
                .wake(AcceptedInputWakeReason::ExecutionReady);
        }
    }

    pub(super) fn try_acquire_scheduled_ordinary_worker(
        &self,
    ) -> Result<super::service_config::ProjectionWorkerPermit, ProjectionWorkerPermitError> {
        self.workers.try_acquire_scheduled_ordinary_or_arm()
    }

    pub(super) fn begin_scheduled_ordinary_flight(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<ProjectionFlight, ProjectionCoordinatorError> {
        FlightRegistry::acquire_or_arm(
            self.home_id,
            self.home_generation,
            thread_id,
            &self.scheduler_signal,
        )
    }

    pub(super) fn issue_scheduled_ordinary_execution(
        &self,
        thread_id: SyndicThreadId,
        execution_binding: ExecutionBinding,
        worker: super::service_config::ProjectionWorkerPermit,
        flight: ProjectionFlight,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        let Ok(command) = self.command_authorizer.authorize() else {
            return Ok(ScheduledOrdinaryAdmissionResult::Unavailable(
                ScheduledOrdinaryExecutionUnavailable::ShuttingDown,
            ));
        };
        self.ensure_current()?;
        let expected_binding = execution_binding.clone();
        let admission = ScheduledOrdinaryAdmission::new(
            self.home_id,
            self.home_generation,
            thread_id,
            execution_binding,
            worker,
            self.persistent_failure
                .as_ref()
                .expect("open service retains persistent-failure coordination")
                .projection_retainer(self.home_id, self.home_generation),
            flight,
        );
        let mut result = self
            .scheduled_ordinary_provider
            .as_ref()
            .expect("open projection service retains its execution provider")
            .lock()
            .map_err(|_| ScheduledOrdinaryAdmissionError::ProviderPoisoned)?
            .try_issue(admission)?;
        if let ScheduledOrdinaryAdmissionResult::Issued(lease) = &result
            && (lease.home_id() != self.home_id
                || lease.home_generation() != self.home_generation
                || lease.thread_id() != thread_id
                || lease.execution_binding() != &expected_binding)
        {
            return Err(ScheduledOrdinaryAdmissionError::LeaseMismatch { thread_id });
        }
        if let ScheduledOrdinaryAdmissionResult::Issued(lease) = &mut result {
            self.validate_scheduled_ordinary_lease(lease)?;
        }
        if !command.is_current() {
            return Ok(ScheduledOrdinaryAdmissionResult::Unavailable(
                ScheduledOrdinaryExecutionUnavailable::ShuttingDown,
            ));
        }
        Ok(result)
    }

    fn validate_scheduled_ordinary_lease(
        &self,
        lease: &mut ScheduledOrdinaryExecutionLease,
    ) -> Result<(), ScheduledOrdinaryAdmissionError> {
        self.ensure_current()?;
        let home = self
            .home
            .as_deref()
            .ok_or(ProjectionCoordinatorError::HomeOwnershipLeaked)?;
        lease
            .assets()
            .revision(home)
            .map_err(|source| ScheduledOrdinaryAdmissionError::AssetAuthority { source })?;

        let retained_process_generation = lease.process_generation();
        let expected_connection = Arc::clone(lease.connection());
        let (runtime_id, process_generation, session_matches_connection) = {
            let session = lease.session();
            (
                session.runtime_id(),
                session.process_generation(),
                Arc::ptr_eq(session.connection(), &expected_connection),
            )
        };
        let is_owned = self
            .connections
            .lock()
            .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                registry: ProjectionRegistryKind::ProjectionConnection,
            })?
            .iter()
            .any(|candidate| Arc::ptr_eq(candidate, &expected_connection));
        if !is_owned
            || !session_matches_connection
            || expected_connection.is_retired()
            || expected_connection.is_detached()
            || runtime_id != lease.execution_binding().runtime_id()
            || process_generation != retained_process_generation
        {
            return Err(
                ScheduledOrdinaryAdmissionError::SessionAuthorityUnavailable {
                    runtime_id,
                    process_generation,
                },
            );
        }
        self.ensure_current()?;
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn cancel_active_steering_lifecycle_for_test(&self) {
        self.scheduler
            .as_ref()
            .expect("open service retains its scheduler")
            .cancel_current_lifecycle();
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn renew_active_steering_lifecycle_for_test(&self) {
        self.scheduler
            .as_ref()
            .expect("open service retains its scheduler")
            .renew_cancellation_lifecycle();
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn signal_accepted_ready_for_test(&self) {
        self.scheduler_signal
            .wake(AcceptedInputWakeReason::AcceptedReady);
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn signal_accepted_next_ready_for_test(&self) {
        self.scheduler_signal
            .wake(AcceptedInputWakeReason::AcceptedNextReady);
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn fail_admission_reconciliation_for_test(
        &self,
        failures: usize,
    ) {
        self.admission_reconciliation_failures
            .store(failures, Ordering::Release);
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn pause_admission_reconciliation_after_dispatch_for_test(
        &self,
        input_id: SyndicAcceptedInputId,
    ) -> AdmissionReconciliationPauseController {
        let token = NEXT_ADMISSION_RECONCILIATION_PAUSE.fetch_add(1, Ordering::Relaxed);
        let (arrived, observation) = sync_channel(1);
        let (release, continuation) = sync_channel(1);
        let pending = PendingAdmissionReconciliationPause {
            token,
            input_id,
            arrived,
            release: continuation,
        };
        let mut slot = self
            .admission_reconciliation_pause
            .lock()
            .expect("admission-reconciliation pause slot is usable");
        assert!(
            slot.replace(pending).is_none(),
            "one service may retain only one admission-reconciliation pause",
        );
        drop(slot);
        AdmissionReconciliationPauseController {
            slot: Arc::clone(&self.admission_reconciliation_pause),
            token,
            arrived: observation,
            release,
        }
    }

    #[cfg(test)]
    fn pause_admission_reconciliation_if_requested(&self, input_id: SyndicAcceptedInputId) {
        let pending = {
            let mut slot = self
                .admission_reconciliation_pause
                .lock()
                .expect("admission-reconciliation pause slot is usable");
            if slot
                .as_ref()
                .is_some_and(|pending| pending.input_id == input_id)
            {
                slot.take()
            } else {
                None
            }
        };
        let Some(pending) = pending else {
            return;
        };
        pending
            .arrived
            .send(())
            .expect("admission test still observes the reconciliation pause");
        pending
            .release
            .recv_timeout(Duration::from_secs(10))
            .expect("admission test releases reconciliation");
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn is_accepting_for_test(&self) -> bool {
        self.command_authorizer.is_open()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn retained_home_for_test(&self) -> &HomeStore {
        self.home
            .as_deref()
            .expect("unsettled test service retains its opened home")
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn registered_connection_count_for_test(&self) -> usize {
        self.connections
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .len()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn acquire_steering_worker_for_test(
        &self,
    ) -> Result<super::service_config::ProjectionWorkerPermit, ProjectionWorkerPermitError> {
        self.workers.try_acquire_steering_critical()
    }

    /// Elects ordinary shutdown against the exact typed persistent-failure cut.
    ///
    /// Ordinary shutdown retires all workers and explicitly closes the home. A
    /// winning persistent-failure cut instead returns its non-cloneable retained
    /// handoff without stopping providers, retiring connections, or closing the home.
    pub fn close(
        mut self,
    ) -> Result<ProjectionConnectionServiceCloseOutcome, ProjectionConnectionServiceCloseError>
    {
        self.close_inner()
    }

    fn close_inner(
        &mut self,
    ) -> Result<ProjectionConnectionServiceCloseOutcome, ProjectionConnectionServiceCloseError>
    {
        if self.settled {
            return Ok(ProjectionConnectionServiceCloseOutcome::Closed);
        }
        match self.command_gate.close_for_shutdown() {
            MasterCommandGateCloseOwner::OrdinaryShutdown => {
                self.ordinary_shutdown_inner()?;
                Ok(ProjectionConnectionServiceCloseOutcome::Closed)
            }
            MasterCommandGateCloseOwner::PersistentFailure(failure_generation) => {
                Ok(ProjectionConnectionServiceCloseOutcome::PersistentFailure(
                    self.retain_persistent_failure(failure_generation),
                ))
            }
        }
    }

    fn ordinary_shutdown_inner(&mut self) -> Result<(), ProjectionConnectionServiceCloseError> {
        self.settled = true;
        drop(
            self.persistent_failure_escrow
                .take()
                .expect("unsettled service retains its failure-escrow reservation"),
        );
        let persistent_failure = self
            .persistent_failure
            .take()
            .expect("unsettled service retains its persistent-failure coordinator");
        persistent_failure.request_shutdown();
        if let Some(scheduler) = self.scheduler.as_ref() {
            scheduler.request_shutdown();
        } else {
            self.scheduler_signal.request_shutdown();
        }
        if let Some(context_compaction) = self.context_compaction.as_ref() {
            context_compaction.request_shutdown();
        }
        self.connections.reap_finished_ordinary_retirements();
        let connections = match self.connections.lock() {
            Ok(mut connections) => std::mem::take(&mut *connections),
            Err(poison) => std::mem::take(&mut *poison.into_inner()),
        };
        let mut connection_failed = false;
        for connection in connections {
            if connection.shutdown().is_err() {
                connection_failed = true;
            }
        }
        let compaction_failed = self
            .context_compaction
            .take()
            .is_some_and(|coordinator| coordinator.shutdown().is_err());
        let scheduler_failed = match self.scheduler.take() {
            Some(scheduler) => scheduler
                .join()
                .map_or(true, AcceptedInputSchedulerExit::failed),
            None => false,
        };
        let provider_failed = self
            .scheduled_ordinary_provider
            .take()
            .is_some_and(|provider| match Arc::try_unwrap(provider) {
                Ok(provider) => {
                    provider
                        .into_inner()
                        .unwrap_or_else(|poison| poison.into_inner())
                        .shutdown();
                    false
                }
                Err(provider) => {
                    provider
                        .lock()
                        .unwrap_or_else(|poison| poison.into_inner())
                        .shutdown();
                    true
                }
            });
        let persistent_failure_failed = persistent_failure.join().is_err();
        let Some(home) = self.home.take() else {
            return if connection_failed {
                Err(ProjectionConnectionServiceCloseError::ConnectionShutdown)
            } else if scheduler_failed {
                Err(ProjectionConnectionServiceCloseError::SchedulerShutdown)
            } else if provider_failed {
                Err(ProjectionConnectionServiceCloseError::ExecutionProviderShutdown)
            } else if compaction_failed {
                Err(ProjectionConnectionServiceCloseError::ContextCompactionShutdown)
            } else if persistent_failure_failed {
                Err(ProjectionConnectionServiceCloseError::PersistentFailureWorkerShutdown)
            } else {
                Ok(())
            };
        };
        let home = Arc::try_unwrap(home)
            .map_err(|_| ProjectionConnectionServiceCloseError::HomeOwnershipLeaked)?;
        let close_result = home
            .close()
            .map_err(ProjectionConnectionServiceCloseError::HomeClose);
        if connection_failed {
            return Err(ProjectionConnectionServiceCloseError::ConnectionShutdown);
        }
        if scheduler_failed {
            return Err(ProjectionConnectionServiceCloseError::SchedulerShutdown);
        }
        if provider_failed {
            return Err(ProjectionConnectionServiceCloseError::ExecutionProviderShutdown);
        }
        if compaction_failed {
            return Err(ProjectionConnectionServiceCloseError::ContextCompactionShutdown);
        }
        if persistent_failure_failed {
            return Err(ProjectionConnectionServiceCloseError::PersistentFailureWorkerShutdown);
        }
        close_result
    }

    /// Explicitly joins a never-published service after its connection epochs became inert.
    ///
    /// The service may still be dormant or may already have converged its recovered startup state;
    /// in both cases its process-publication gate remains unpublished and the caller cancels that
    /// gate before disposal. Adoption retains the exact opened home outside this service, so
    /// terminal disposition settles all service-owned workers and the execution-provider fence
    /// without closing that home.
    pub(in crate::cas_projection) fn dispose_unpublished_inert(
        &mut self,
    ) -> Result<(), ProjectionConnectionServiceCloseError> {
        if self.settled {
            return Ok(());
        }
        self.settled = true;
        self.command_gate.close_for_local_failure();
        drop(self.persistent_failure_escrow.take());

        let persistent_failure = self.persistent_failure.take();
        if let Some(persistent_failure) = persistent_failure.as_ref() {
            persistent_failure.request_shutdown();
        }
        if let Some(scheduler) = self.scheduler.as_ref() {
            scheduler.request_shutdown();
        } else {
            self.scheduler_signal.request_shutdown();
        }
        if let Some(context_compaction) = self.context_compaction.as_ref() {
            context_compaction.request_shutdown();
        }

        self.connections
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clear();
        let compaction_failed = self
            .context_compaction
            .take()
            .is_some_and(|coordinator| coordinator.shutdown().is_err());
        let scheduler_failed = match self.scheduler.take() {
            Some(scheduler) => scheduler
                .join()
                .map_or(true, AcceptedInputSchedulerExit::failed),
            None => false,
        };
        let provider_failed = self
            .scheduled_ordinary_provider
            .take()
            .is_some_and(|provider| match Arc::try_unwrap(provider) {
                Ok(provider) => {
                    provider
                        .into_inner()
                        .unwrap_or_else(|poison| poison.into_inner())
                        .shutdown();
                    false
                }
                Err(provider) => {
                    provider
                        .lock()
                        .unwrap_or_else(|poison| poison.into_inner())
                        .shutdown();
                    true
                }
            });
        let persistent_failure_failed =
            persistent_failure.is_some_and(|coordinator| coordinator.join().is_err());

        if scheduler_failed {
            Err(ProjectionConnectionServiceCloseError::SchedulerShutdown)
        } else if provider_failed {
            Err(ProjectionConnectionServiceCloseError::ExecutionProviderShutdown)
        } else if compaction_failed {
            Err(ProjectionConnectionServiceCloseError::ContextCompactionShutdown)
        } else if persistent_failure_failed {
            Err(ProjectionConnectionServiceCloseError::PersistentFailureWorkerShutdown)
        } else {
            Ok(())
        }
    }

    fn request_implicit_ordinary_shutdown(&mut self) {
        self.settled = true;
        drop(self.persistent_failure_escrow.take());
        if let Some(persistent_failure) = self.persistent_failure.as_ref() {
            persistent_failure.request_shutdown();
        }
        if let Some(scheduler) = self.scheduler.as_ref() {
            scheduler.request_shutdown();
        } else {
            self.scheduler_signal.request_shutdown();
        }
        if let Some(context_compaction) = self.context_compaction.as_ref() {
            context_compaction.request_shutdown();
        }
        let connections = match self.connections.lock() {
            Ok(mut connections) => std::mem::take(&mut *connections),
            Err(poison) => std::mem::take(&mut *poison.into_inner()),
        };
        for connection in connections {
            connection.request_ordinary_retirement_after_service_shutdown();
        }
    }

    fn take_persistent_failure_retained_service(
        &mut self,
        failure_generation: super::PersistentFailureGeneration,
        persistent_failure: PersistentFailureCoordinator,
        mut completion: PersistentFailureCutCompletion,
    ) -> PersistentFailureRetainedService {
        let retained_connections = match self.connections.lock() {
            Ok(connections) => connections.clone(),
            Err(poison) => {
                completion = PersistentFailureCutCompletion::Incomplete;
                poison.into_inner().clone()
            }
        };
        PersistentFailureRetainedService::new(
            self.home
                .take()
                .expect("unsettled service retains its exact opened home"),
            self.home_id,
            self.home_generation,
            self.storage,
            self.startup.storage_revision(),
            self.config,
            self.workers.clone(),
            self.service_generation,
            failure_generation,
            self.command_gate.clone(),
            self.command_authorizer.clone(),
            persistent_failure,
            Arc::clone(&self.connections),
            retained_connections,
            Arc::clone(&self.stop_coordinator),
            self.context_compaction.take(),
            self.scheduler.take(),
            self.scheduler_signal.clone(),
            self.scheduled_ordinary_provider.take(),
            completion,
        )
    }

    fn retain_persistent_failure(
        &mut self,
        failure_generation: super::PersistentFailureGeneration,
    ) -> PersistentFailureCutHandoff {
        self.settled = true;
        let escrow = self
            .persistent_failure_escrow
            .take()
            .expect("unsettled service retains its failure-escrow reservation");
        let persistent_failure = self
            .persistent_failure
            .take()
            .expect("unsettled service retains its persistent-failure coordinator");
        let worker_join_failed = persistent_failure.join().is_err();
        let drain_failed = self.command_gate.wait_until_drained().is_err();
        let snapshot = persistent_failure.snapshot();
        let completion = if !worker_join_failed
            && !drain_failed
            && snapshot.state() == PersistentFailureCutState::Finished
        {
            PersistentFailureCutCompletion::Finished
        } else {
            PersistentFailureCutCompletion::Incomplete
        };
        self.take_persistent_failure_retained_service(
            failure_generation,
            persistent_failure,
            completion,
        )
        .escrow(escrow)
    }

    fn escrow_implicit_persistent_failure(
        &mut self,
        failure_generation: super::PersistentFailureGeneration,
    ) {
        self.settled = true;
        let escrow = self
            .persistent_failure_escrow
            .take()
            .expect("unsettled service retains its failure-escrow reservation");
        let persistent_failure = self
            .persistent_failure
            .take()
            .expect("unsettled service retains its persistent-failure coordinator");
        drop(
            self.take_persistent_failure_retained_service(
                failure_generation,
                persistent_failure,
                PersistentFailureCutCompletion::Incomplete,
            )
            .escrow(escrow),
        );
    }
}

#[cfg(test)]
impl AdmissionReconciliationPauseController {
    pub(in crate::cas_projection) fn wait_until_paused(&self, timeout: Duration) {
        self.arrived
            .recv_timeout(timeout)
            .expect("accepted-input command reached reconciliation");
    }

    pub(in crate::cas_projection) fn release(self) {
        self.release
            .send(())
            .expect("accepted-input command still awaits reconciliation");
    }
}

#[cfg(test)]
impl Drop for AdmissionReconciliationPauseController {
    fn drop(&mut self) {
        let mut slot = self
            .slot
            .lock()
            .expect("admission-reconciliation pause slot is usable");
        if slot
            .as_ref()
            .is_some_and(|pending| pending.token == self.token)
        {
            slot.take();
        }
    }
}

impl Drop for ProjectionConnectionService {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        match self.command_gate.close_for_shutdown() {
            MasterCommandGateCloseOwner::OrdinaryShutdown => {
                self.request_implicit_ordinary_shutdown();
            }
            MasterCommandGateCloseOwner::PersistentFailure(failure_generation) => {
                self.escrow_implicit_persistent_failure(failure_generation);
            }
        }
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

impl CasProjectionCoordinator {
    /// Binds a coordinator to the exact identity and current healthy generation of `home`.
    pub fn for_healthy_home(home: &HomeStore) -> Result<Self, ProjectionCoordinatorError> {
        let health = home.health();
        if health.state() != HomeHealthState::Healthy {
            return Err(ProjectionCoordinatorError::HomeNotHealthy {
                state: health.state(),
                generation: health.generation(),
            });
        }
        let Some(home_generation) = health.generation() else {
            return Err(ProjectionCoordinatorError::HealthyHomeGenerationMissing);
        };
        Ok(Self {
            home_id: home.home_id(),
            home_generation,
        })
    }

    /// Returns the exact durable home identity bound at construction.
    #[must_use]
    pub const fn home_id(&self) -> BerylHomeId {
        self.home_id
    }

    /// Returns the exact healthy home generation bound at construction.
    #[must_use]
    pub const fn home_generation(&self) -> HomeGeneration {
        self.home_generation
    }

    pub(crate) fn ensure_home(&self, home: &HomeStore) -> Result<(), ProjectionCoordinatorError> {
        if home.home_id() != self.home_id {
            return Err(ProjectionCoordinatorError::HomeIdentityMismatch {
                expected: self.home_id,
                actual: home.home_id(),
            });
        }
        let health = home.health();
        if health.state() != HomeHealthState::Healthy
            || health.generation() != Some(self.home_generation)
        {
            return Err(ProjectionCoordinatorError::HomeGenerationMismatch {
                expected: self.home_generation,
                actual: health.generation(),
                state: health.state(),
            });
        }
        Ok(())
    }

    /// Acquires the one process-wide projection flight for `thread_id` in this home generation.
    ///
    /// The returned non-cloneable guard releases the flight on drop. The
    /// registry mutex has already been released when this method returns.
    pub(super) fn begin_projection(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<ProjectionFlight, ProjectionCoordinatorError> {
        FlightRegistry::acquire(self.home_id, self.home_generation, thread_id)
    }

    pub(super) fn begin_scheduled_projection(
        &self,
        thread_id: SyndicThreadId,
        scheduler_signal: &AcceptedInputSchedulerSignal,
    ) -> Result<ProjectionFlight, ProjectionCoordinatorError> {
        FlightRegistry::acquire_or_arm(
            self.home_id,
            self.home_generation,
            thread_id,
            scheduler_signal,
        )
    }

    pub(super) fn ensure_projection_flight(
        &self,
        flight: &ProjectionFlight,
        thread_id: SyndicThreadId,
    ) -> Result<(), ProjectionCoordinatorError> {
        if flight.key
            == (ProjectionFlightKey {
                home_id: self.home_id,
                home_generation: self.home_generation,
                thread_id,
            })
        {
            Ok(())
        } else {
            Err(ProjectionCoordinatorError::ProjectionFlightMismatch { thread_id })
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ProjectionFlightKey {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    thread_id: SyndicThreadId,
}

static PROJECTION_FLIGHTS: OnceLock<
    Mutex<HashMap<ProjectionFlightKey, Option<AcceptedInputSchedulerSignal>>>,
> = OnceLock::new();

pub(super) struct FlightRegistry;

impl FlightRegistry {
    pub(super) fn acquire(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        thread_id: SyndicThreadId,
    ) -> Result<ProjectionFlight, ProjectionCoordinatorError> {
        let registry = PROJECTION_FLIGHTS.get_or_init(|| Mutex::new(HashMap::new()));
        let mut active =
            registry
                .lock()
                .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                    registry: ProjectionRegistryKind::ProjectionFlights,
                })?;
        let key = ProjectionFlightKey {
            home_id,
            home_generation,
            thread_id,
        };
        if active.contains_key(&key) {
            return Err(ProjectionCoordinatorError::ProjectionInFlight { thread_id });
        }
        active.insert(key, None);
        drop(active);
        Ok(ProjectionFlight { key })
    }

    pub(super) fn acquire_or_arm(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        thread_id: SyndicThreadId,
        scheduler_signal: &AcceptedInputSchedulerSignal,
    ) -> Result<ProjectionFlight, ProjectionCoordinatorError> {
        let registry = PROJECTION_FLIGHTS.get_or_init(|| Mutex::new(HashMap::new()));
        let mut active =
            registry
                .lock()
                .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                    registry: ProjectionRegistryKind::ProjectionFlights,
                })?;
        let key = ProjectionFlightKey {
            home_id,
            home_generation,
            thread_id,
        };
        if let Some(release_signal) = active.get_mut(&key) {
            if release_signal.is_none() {
                *release_signal = Some(scheduler_signal.clone());
            }
            return Err(ProjectionCoordinatorError::ProjectionInFlight { thread_id });
        }
        active.insert(key, None);
        drop(active);
        Ok(ProjectionFlight { key })
    }

    fn release(key: ProjectionFlightKey) {
        let registry = PROJECTION_FLIGHTS.get_or_init(|| Mutex::new(HashMap::new()));
        let mut active = match registry.lock() {
            Ok(active) => active,
            Err(poisoned) => poisoned.into_inner(),
        };
        let release_signal = active.remove(&key).flatten();
        drop(active);
        if let Some(signal) = release_signal {
            signal.wake(AcceptedInputWakeReason::ProjectionFlightReleased);
        }
    }
}

/// Non-cloneable RAII ownership of one Syndic thread's projection flight.
///
/// Holding this guard excludes another projection operation for the same
/// Syndic thread without holding a mutex during backend or storage work.
#[must_use = "dropping the guard releases the thread's projection flight"]
pub(super) struct ProjectionFlight {
    key: ProjectionFlightKey,
}

impl Drop for ProjectionFlight {
    fn drop(&mut self) {
        FlightRegistry::release(self.key);
    }
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
