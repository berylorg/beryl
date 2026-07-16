use beryl_backend::ManagedBackendError;
use beryl_home_store::{HomeGeneration, HomeHealthState};
use beryl_model::{BerylHomeId, CasProcessGeneration, CasThreadId, RuntimeId, SyndicThreadId};
use thiserror::Error;

/// Failure to admit one exact managed backend session for projection work.
#[derive(Debug, Error)]
pub enum ProjectionSessionAdmissionError {
    #[error(
        "CAS projection session for runtime {runtime_id} process {process_generation:?} opted out of required live notifications"
    )]
    FullTurnStreamRequired {
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
    },
    #[error(
        "CAS compatibility admission failed for runtime {runtime_id} process {process_generation:?}"
    )]
    Compatibility {
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
    pub(super) fn new(
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        source: ManagedBackendError,
    ) -> Self {
        Self::Compatibility {
            runtime_id,
            process_generation,
            source: Box::new(source),
        }
    }

    pub(super) const fn full_turn_stream_required(
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
    ) -> Self {
        Self::FullTurnStreamRequired {
            runtime_id,
            process_generation,
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

    /// Returns the exact configured runtime whose owned session was probed.
    #[must_use]
    pub const fn runtime_id(&self) -> RuntimeId {
        match self {
            Self::FullTurnStreamRequired { runtime_id, .. }
            | Self::Compatibility { runtime_id, .. }
            | Self::ConnectionOwnership { runtime_id, .. } => *runtime_id,
        }
    }

    /// Returns the exact managed-process generation whose session was probed.
    #[must_use]
    pub const fn process_generation(&self) -> CasProcessGeneration {
        match self {
            Self::FullTurnStreamRequired {
                process_generation, ..
            }
            | Self::Compatibility {
                process_generation, ..
            }
            | Self::ConnectionOwnership {
                process_generation, ..
            } => *process_generation,
        }
    }

    /// Returns the typed backend failure produced by the synchronous probe.
    #[must_use]
    pub const fn backend_error(&self) -> Option<&ManagedBackendError> {
        match self {
            Self::Compatibility { source, .. } => Some(source),
            Self::FullTurnStreamRequired { .. } | Self::ConnectionOwnership { .. } => None,
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
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProjectionCoordinatorError {
    /// Coordinator construction observed a home that was not healthy.
    #[error("CAS projection coordination requires a healthy Beryl home, got {state:?}")]
    HomeNotHealthy {
        /// Coherent health state observed during construction.
        state: HomeHealthState,
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
    /// Another projection operation already owns the same Syndic-thread flight.
    #[error("CAS projection work is already in flight for {thread_id}")]
    ProjectionInFlight {
        /// Exact Syndic thread already owned by another operation.
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
    /// The OS could not start the sole connection worker.
    #[error("failed to start the CAS projection connection worker: {message}")]
    ProjectionWorkerSpawn { message: String },
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
