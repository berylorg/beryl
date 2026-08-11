use super::*;

/// Stable phase of the process-local persistent-failure safety cut.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentFailureCutState {
    Armed,
    Cutting,
    Finished,
    Incomplete,
    Stopped,
}

/// Bounded content-free observation of the one-shot persistent-failure coordinator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PersistentFailureCutSnapshot {
    pub(super) state: PersistentFailureCutState,
    pub(super) service_generation: ProjectionServiceGeneration,
    pub(super) failure_generation: Option<PersistentFailureGeneration>,
    pub(super) target_count: usize,
    pub(super) proven_nondispatch_count: usize,
    pub(super) possible_dispatch_count: usize,
    pub(super) disposed_projection_count: usize,
}

/// Stability of one terminally disposed persistent-failure cut.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentFailureCutCompletion {
    Finished,
    Incomplete,
}

/// Bounded authority-free evidence from terminally disposing one failed service generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PersistentFailureTerminalEvidence {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
    failure_generation: PersistentFailureGeneration,
    cut_snapshot: PersistentFailureCutSnapshot,
    completion: PersistentFailureCutCompletion,
}

impl PersistentFailureTerminalEvidence {
    pub(in crate::cas_projection) const fn new(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        service_generation: ProjectionServiceGeneration,
        failure_generation: PersistentFailureGeneration,
        cut_snapshot: PersistentFailureCutSnapshot,
        completion: PersistentFailureCutCompletion,
    ) -> Self {
        Self {
            home_id,
            home_generation,
            service_generation,
            failure_generation,
            cut_snapshot,
            completion,
        }
    }

    #[must_use]
    pub const fn home_id(self) -> BerylHomeId {
        self.home_id
    }

    #[must_use]
    pub const fn home_generation(self) -> HomeGeneration {
        self.home_generation
    }

    #[must_use]
    pub const fn service_generation(self) -> ProjectionServiceGeneration {
        self.service_generation
    }

    #[must_use]
    pub const fn failure_generation(self) -> PersistentFailureGeneration {
        self.failure_generation
    }

    #[must_use]
    pub const fn cut_snapshot(self) -> PersistentFailureCutSnapshot {
        self.cut_snapshot
    }

    #[must_use]
    pub const fn completion(self) -> PersistentFailureCutCompletion {
        self.completion
    }
}

impl PersistentFailureCutSnapshot {
    #[must_use]
    pub const fn state(self) -> PersistentFailureCutState {
        self.state
    }

    #[must_use]
    pub const fn service_generation(self) -> ProjectionServiceGeneration {
        self.service_generation
    }

    #[must_use]
    pub const fn failure_generation(self) -> Option<PersistentFailureGeneration> {
        self.failure_generation
    }

    #[must_use]
    pub const fn target_count(self) -> usize {
        self.target_count
    }

    /// Returns the bounded number of cut targets proven not to have dispatched.
    #[must_use]
    pub const fn proven_nondispatch_count(self) -> usize {
        self.proven_nondispatch_count
    }

    /// Returns the bounded number of cut targets whose dispatch may have occurred.
    #[must_use]
    pub const fn possible_dispatch_count(self) -> usize {
        self.possible_dispatch_count
    }

    /// Returns the bounded number of loaded projection authorities terminally disposed at the cut.
    #[must_use]
    pub const fn disposed_projection_count(self) -> usize {
        self.disposed_projection_count
    }
}

pub(super) struct CoordinatorState {
    pub(super) phase: PersistentFailureCutState,
    pub(super) failure_generation: Option<PersistentFailureGeneration>,
    pub(super) target_count: usize,
    pub(super) proven_nondispatch_count: usize,
    pub(super) possible_dispatch_count: usize,
    pub(super) disposed_projection_count: usize,
}

pub(super) struct PendingPersistentFailureResult {
    pub(super) completion: PersistentFailureCompletion,
}

pub(in crate::cas_projection) struct PersistentFailureCoordinator {
    pub(super) service_generation: ProjectionServiceGeneration,
    pub(super) notification: PersistentFailureNotification,
    pub(super) stop_requested: Arc<AtomicBool>,
    pub(super) state: Arc<(Mutex<CoordinatorState>, Condvar)>,
    pub(super) handle: Mutex<Option<JoinHandle<()>>>,
}

/// Cloneable terminal disposer for already-loaded authority overtaken by the one-shot cut.
#[derive(Clone)]
pub(in crate::cas_projection) struct PersistentFailureTerminalDisposer {
    pub(super) home_id: BerylHomeId,
    pub(super) home_generation: HomeGeneration,
    pub(super) notification: PersistentFailureNotification,
    pub(super) state: Arc<(Mutex<CoordinatorState>, Condvar)>,
}

impl std::fmt::Debug for PersistentFailureTerminalDisposer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PersistentFailureTerminalDisposer")
            .field("home_id", &self.home_id)
            .field("home_generation", &self.home_generation)
            .finish_non_exhaustive()
    }
}

pub(super) struct WorkerContext {
    pub(super) home: Arc<HomeStore>,
    pub(super) home_id: BerylHomeId,
    pub(super) home_generation: HomeGeneration,
    pub(super) service_generation: ProjectionServiceGeneration,
    pub(super) notification: PersistentFailureNotification,
    pub(super) gate: MasterCommandGate,
    pub(super) stop_coordinator: Arc<StopCoordinator>,
    pub(super) connections:
        Arc<crate::cas_projection::service_registry::ProjectionServiceConnectionRegistry>,
    pub(super) stop_requested: Arc<AtomicBool>,
    pub(super) state: Arc<(Mutex<CoordinatorState>, Condvar)>,
}
