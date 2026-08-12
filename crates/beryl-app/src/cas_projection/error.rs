use beryl_backend::ManagedBackendError;
use beryl_home_store::{CommandError, CommitReceipt, HomeGeneration, HomeHealthState, ReadError};
use beryl_model::{BerylHomeId, CasProcessGeneration, CasThreadId, RuntimeId, SyndicThreadId};
use thiserror::Error;

/// Failure to create and admit one exact managed foreground candidate for projection work.
#[derive(Debug, Error)]
pub enum ProjectionSessionAdmissionError {
    #[error(
        "CAS projection service is closed for runtime {runtime_id} process {process_generation:?}"
    )]
    ServiceClosed {
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
    },
    #[error(
        "CAS foreground candidate connection failed for runtime {runtime_id} process {process_generation:?}"
    )]
    CandidateConnection {
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        #[source]
        source: Box<ManagedBackendError>,
    },
    #[error(
        "CAS foreground initialization failed for runtime {runtime_id} process {process_generation:?}"
    )]
    Initialization {
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        #[source]
        source: Box<ManagedBackendError>,
    },
    #[error("CAS release admission failed for runtime {runtime_id} process {process_generation:?}")]
    ReleaseAdmission {
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        #[source]
        source: Box<ManagedBackendError>,
    },
    #[error(
        "CAS connection ownership admission failed for runtime {runtime_id} process {process_generation:?}"
    )]
    ConnectionOwnership {
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        #[source]
        source: ProjectionCoordinatorError,
    },
}

impl ProjectionSessionAdmissionError {
    pub(super) const fn service_closed(
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
    ) -> Self {
        Self::ServiceClosed {
            runtime_id,
            process_generation,
        }
    }

    pub(super) fn candidate_connection(
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        source: ManagedBackendError,
    ) -> Self {
        Self::CandidateConnection {
            runtime_id,
            process_generation,
            source: Box::new(source),
        }
    }

    pub(super) fn initialization(
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        source: ManagedBackendError,
    ) -> Self {
        Self::Initialization {
            runtime_id,
            process_generation,
            source: Box::new(source),
        }
    }

    pub(super) fn release_admission(
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        source: ManagedBackendError,
    ) -> Self {
        Self::ReleaseAdmission {
            runtime_id,
            process_generation,
            source: Box::new(source),
        }
    }

    pub(super) fn connection_ownership(
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        source: ProjectionCoordinatorError,
    ) -> Self {
        Self::ConnectionOwnership {
            runtime_id,
            process_generation,
            source,
        }
    }

    /// Returns the exact configured runtime selected before candidate connection.
    #[must_use]
    pub const fn runtime_id(&self) -> RuntimeId {
        match self {
            Self::ServiceClosed { runtime_id, .. }
            | Self::CandidateConnection { runtime_id, .. }
            | Self::Initialization { runtime_id, .. }
            | Self::ReleaseAdmission { runtime_id, .. }
            | Self::ConnectionOwnership { runtime_id, .. } => *runtime_id,
        }
    }

    /// Returns the exact managed-process generation selected before candidate connection.
    #[must_use]
    pub const fn process_generation(&self) -> CasProcessGeneration {
        match self {
            Self::ServiceClosed {
                process_generation, ..
            }
            | Self::CandidateConnection {
                process_generation, ..
            }
            | Self::Initialization {
                process_generation, ..
            }
            | Self::ReleaseAdmission {
                process_generation, ..
            }
            | Self::ConnectionOwnership {
                process_generation, ..
            } => *process_generation,
        }
    }

    /// Returns the typed backend failure from connection, initialization, or release admission.
    #[must_use]
    pub const fn backend_error(&self) -> Option<&ManagedBackendError> {
        match self {
            Self::CandidateConnection { source, .. }
            | Self::Initialization { source, .. }
            | Self::ReleaseAdmission { source, .. } => Some(source),
            Self::ServiceClosed { .. } | Self::ConnectionOwnership { .. } => None,
        }
    }
}

/// In-memory registry whose poisoned state rejected projection coordination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionRegistryKind {
    /// Registry excluding competing projection work for one Syndic thread.
    ProjectionFlights,
    /// Registry correlating exact loaded CAS-thread observations and owners.
    LoadedThreads,
    /// Exact shared client connection owned by projection leases.
    ProjectionConnection,
    /// Exact connection-owned live-event targets.
    LiveEventRouter,
    /// Shared facts for one exact runtime process generation.
    LiveEventProcess,
}

impl std::fmt::Display for ProjectionRegistryKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProjectionFlights => formatter.write_str("projection-flight"),
            Self::LoadedThreads => formatter.write_str("loaded-thread"),
            Self::ProjectionConnection => formatter.write_str("projection-connection"),
            Self::LiveEventRouter => formatter.write_str("live-event-router"),
            Self::LiveEventProcess => formatter.write_str("live-event-process"),
        }
    }
}

/// Closed failures produced by the app-owned CAS projection coordinator.
#[derive(Debug, Error)]
pub enum ProjectionCoordinatorError {
    #[error("a coordinator mutation was proven not committed: {0}")]
    CommandNotCommitted(#[source] CommandError),
    #[error("a coordinator mutation committed before a later failure: {later_failure}")]
    CommandCommitted {
        receipt: CommitReceipt,
        #[source]
        later_failure: CommandError,
    },
    #[error("a coordinator mutation has an indeterminate durable outcome: {failure}")]
    CommandIndeterminate {
        #[source]
        failure: CommandError,
    },
    /// Coordinator construction observed a home that was not healthy.
    #[error(
        "CAS projection coordination requires a healthy Beryl home, got {state:?} at generation {generation:?}"
    )]
    HomeNotHealthy {
        /// Coherent health state observed during construction.
        state: HomeHealthState,
        /// Coherent last validated generation observed during construction.
        generation: Option<HomeGeneration>,
    },
    /// A nominally healthy home snapshot omitted its required generation.
    #[error("healthy Beryl-home state did not expose a generation")]
    HealthyHomeGenerationMissing,
    /// Work was offered a different durable home than its coordinator owns.
    #[error("CAS projection coordinator belongs to home {expected}, not {actual}")]
    HomeIdentityMismatch {
        expected: BerylHomeId,
        actual: BerylHomeId,
    },
    /// The home changed health state or generation after coordinator construction.
    #[error(
        "CAS projection coordinator generation {expected:?} is no longer current: state {state:?}, generation {actual:?}"
    )]
    HomeGenerationMismatch {
        expected: HomeGeneration,
        actual: Option<HomeGeneration>,
        state: HomeHealthState,
    },
    /// The exact admitted live-command permit was invalidated by ordinary service shutdown.
    #[error("the CAS projection live-command permit was closed by ordinary service shutdown")]
    LiveCommandOrdinaryShutdown,
    /// The exact admitted live-command permit was invalidated by persistent failure of its home.
    #[error(
        "the CAS projection live-command permit was closed by persistent failure of home generation {generation:?}"
    )]
    LiveCommandPersistentHomeFailure { generation: HomeGeneration },
    /// The exact admitted live-command permit was invalidated by an unrelated local failure.
    #[error("the CAS projection live-command permit was closed by a service-local failure")]
    LiveCommandLocalFailure,
    /// The exact live-command gate could not report a trustworthy permit status.
    #[error("the CAS projection live-command permit status is unavailable")]
    LiveCommandGateUnavailable,
    /// Another projection operation already owns the same Syndic-thread flight.
    #[error("CAS projection work is already in flight for {thread_id}")]
    ProjectionInFlight {
        /// Exact Syndic thread already owned by another operation.
        thread_id: SyndicThreadId,
    },
    /// A retained projection flight belongs to another home generation or Syndic thread.
    #[error("the retained projection flight does not authorize thread {thread_id}")]
    ProjectionFlightMismatch {
        /// Exact thread the attempted operation required.
        thread_id: SyndicThreadId,
    },
    /// A coordination mutex was poisoned by an unwind.
    #[error("{registry} registry is poisoned")]
    RegistryPoisoned {
        /// Exact registry that can no longer publish trusted results.
        registry: ProjectionRegistryKind,
    },
    /// The app-owned loaded-thread generation counter cannot advance safely.
    #[error("CAS loaded-thread generation is exhausted")]
    LoadedThreadGenerationExhausted,
    /// The process-wide admitted-connection generation cannot advance safely.
    #[error("CAS projection connection generation is exhausted")]
    ProjectionConnectionGenerationExhausted,
    /// The process-local projection-service generation cannot advance safely.
    #[error("CAS projection service generation is exhausted")]
    ProjectionServiceGenerationExhausted,
    /// The OS could not start the sole connection worker.
    #[error("failed to start the CAS projection connection worker: {message}")]
    ProjectionWorkerSpawn { message: String },
    /// The process-owned context-compaction coordinator could not start.
    #[error("failed to start the context-compaction coordinator")]
    ContextCompactionCoordinatorUnavailable,
    /// The OS could not start the process-owned accepted-input scheduler.
    #[error("failed to start the accepted-input scheduler: {message}")]
    AcceptedInputSchedulerSpawn { message: String },
    /// The OS could not start the dedicated persistent-failure cut worker.
    #[error("failed to start the persistent-failure cut worker: {message}")]
    PersistentFailureWorkerSpawn { message: String },
    /// The complete driver-and-ingester worker pair was unavailable.
    #[error("projection worker capacity is full: two permits are required and {available} remain")]
    ProjectionWorkerCapacityFull { available: usize },
    /// No still-admitted worker can own a pre-activation projection.
    #[error("pre-activation projection worker admission is unavailable")]
    PreactivationProjectionAdmissionUnavailable,
    /// The app-owned projection worker pool could not be trusted after an unwind.
    #[error("projection worker pool is poisoned")]
    ProjectionWorkerPoolPoisoned,
    /// Fixed provider broker backing could not be constructed.
    #[error("provider broker construction failed: {message}")]
    ProviderBrokerAdmission { message: String },
    /// The exact backend session rejected its ordered sink binding.
    #[error("ordered turn-stream binding failed: {source}")]
    OrderedTurnStreamBinding {
        source: beryl_backend::OrderedTurnStreamBindingError,
    },
    /// The registered Syndic domain cannot be read through the owned home.
    #[error("the owned projection service cannot read its registered Syndic revision: {source}")]
    SyndicRevisionUnavailable {
        #[source]
        source: ReadError,
    },
    /// Restart delivery discovery could not read its bounded durable source.
    #[error("accepted-delivery restart recovery could not read its durable source")]
    AcceptedDeliveryRecoveryRead,
    /// Restart delivery evidence was coherent but did not match a supported closed case.
    #[error("accepted-delivery restart recovery found inconsistent durable authority")]
    AcceptedDeliveryRecoveryInvariant,
    /// Restart delivery convergence could not prove its exact durable publication.
    #[error("accepted-delivery restart recovery could not converge durable authority")]
    AcceptedDeliveryRecoveryPublication,
    /// Restart delivery convergence could not construct a non-regressing durable timestamp.
    #[error("accepted-delivery restart recovery could not construct a durable timestamp")]
    AcceptedDeliveryRecoveryClock,
    /// The consuming service close found a store-bearing Arc after all workers joined.
    #[error("projection service shutdown left an unexpected Beryl-home owner")]
    HomeOwnershipLeaked,
    /// The sole connection worker stopped before completing a queued command.
    #[error("the CAS projection connection worker is stopped")]
    ProjectionWorkerStopped,
    /// The process-wide loaded-projection lease token cannot advance safely.
    #[error("CAS loaded-projection lease token is exhausted")]
    ProjectionLeaseTokenExhausted,
    /// The exact admitted connection has been retired or lost.
    #[error(
        "CAS projection connection for runtime {runtime_id} process {process_generation:?} is unavailable"
    )]
    ProjectionConnectionUnavailable {
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
    },
    /// One exact CAS thread was offered to a second Syndic-thread owner.
    #[error(
        "CAS thread {cas_thread_id} for runtime {runtime_id} process {process_generation:?} is owned by {existing_owner}, not {offered_owner}"
    )]
    CasThreadOwnerCollision {
        /// Exact configured runtime containing the collision.
        runtime_id: RuntimeId,
        /// Exact managed-process generation containing the collision.
        process_generation: CasProcessGeneration,
        /// Exact CAS thread whose owner cannot change.
        cas_thread_id: CasThreadId,
        /// Syndic thread permanently associated with this registry key.
        existing_owner: SyndicThreadId,
        /// Different Syndic thread offered by the rejected observation.
        offered_owner: SyndicThreadId,
    },
    /// One CAS thread was already registered through another exact connection.
    #[error(
        "CAS thread {cas_thread_id} for runtime {runtime_id} process {process_generation:?} already has a live connection subscription"
    )]
    CasThreadConnectionCollision {
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        cas_thread_id: CasThreadId,
    },
}
