use std::sync::{Arc, Mutex};

use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::BerylHomeId;
use syndic_storage::SyndicStorage;

use super::{AcceptedInputSchedulerSignal, RecoveredProjectionLane};
use crate::cas_projection::{
    ProjectionCancellationToken,
    persistent_failure::{MasterCommandGate, PersistentFailureProjectionRetainer},
    scheduled_ordinary::ScheduledOrdinaryExecutionProvider,
    service_config::ProjectionWorkerPool,
    service_registry::ProjectionServiceConnectionRegistry,
};

type ConnectionRegistry = Arc<ProjectionServiceConnectionRegistry>;
type ScheduledOrdinaryProvider = Arc<Mutex<Box<dyn ScheduledOrdinaryExecutionProvider>>>;

pub(super) const SCHEDULER_PASS_PAGE_BUDGET: usize = 256;

pub(super) struct ScanBudget {
    remaining_pages: usize,
}

impl ScanBudget {
    pub(super) const fn new(remaining_pages: usize) -> Self {
        Self { remaining_pages }
    }

    pub(super) fn take_page(&mut self) -> bool {
        let Some(remaining) = self.remaining_pages.checked_sub(1) else {
            return false;
        };
        self.remaining_pages = remaining;
        true
    }
}

#[derive(Clone)]
pub(in crate::cas_projection) struct ActiveSteeringCancellationLifecycle {
    current: Arc<Mutex<ProjectionCancellationToken>>,
}

impl ActiveSteeringCancellationLifecycle {
    pub(in crate::cas_projection) fn new() -> Self {
        Self {
            current: Arc::new(Mutex::new(ProjectionCancellationToken::new())),
        }
    }

    pub(super) fn snapshot(&self) -> ProjectionCancellationToken {
        self.current
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.snapshot().is_cancelled()
    }

    pub(super) fn cancel_current(&self) {
        self.snapshot().cancel();
    }

    pub(super) fn renew(&self) {
        *self
            .current
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = ProjectionCancellationToken::new();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WorkerDisposition {
    Settled,
    Parked,
    VerificationPending,
    RecoveredProjectionContinue,
    RecoveredProjectionParked,
    RecoveredPendingContinue,
    NextContinue,
    NextParked,
    PersistentHomeFailure,
    Fatal,
}

pub(in crate::cas_projection) struct AcceptedInputSchedulerContext {
    pub(super) home: Arc<HomeStore>,
    pub(super) home_id: BerylHomeId,
    pub(super) home_generation: HomeGeneration,
    pub(super) storage: SyndicStorage,
    pub(super) workers: ProjectionWorkerPool,
    pub(super) connections: ConnectionRegistry,
    pub(super) scheduled_ordinary_provider: ScheduledOrdinaryProvider,
    pub(super) command_gate: MasterCommandGate,
    pub(super) projection_retainer: PersistentFailureProjectionRetainer,
    pub(super) cancellation: ActiveSteeringCancellationLifecycle,
    pub(super) ordinary_cancellation: ProjectionCancellationToken,
    pub(super) signal: AcceptedInputSchedulerSignal,
    pub(super) recovered_projection_lane: RecoveredProjectionLane,
}

impl AcceptedInputSchedulerContext {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection) fn new(
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        storage: SyndicStorage,
        workers: ProjectionWorkerPool,
        connections: ConnectionRegistry,
        scheduled_ordinary_provider: ScheduledOrdinaryProvider,
        command_gate: MasterCommandGate,
        projection_retainer: PersistentFailureProjectionRetainer,
        cancellation: ActiveSteeringCancellationLifecycle,
        signal: AcceptedInputSchedulerSignal,
        recovered_projection_lane: RecoveredProjectionLane,
    ) -> Self {
        Self {
            home,
            home_id,
            home_generation,
            storage,
            workers,
            connections,
            scheduled_ordinary_provider,
            command_gate,
            projection_retainer,
            cancellation,
            ordinary_cancellation: ProjectionCancellationToken::new(),
            signal,
            recovered_projection_lane,
        }
    }
}
