const ACCEPTED_READY: u32 = 1 << 0;
const TARGET_READY: u32 = 1 << 1;
pub(super) const WORKER_RELEASED: u32 = 1 << 2;
const ATTEMPT_RELEASED: u32 = 1 << 3;
const CANCELLATION_LIFECYCLE: u32 = 1 << 4;
const RECOVERY: u32 = 1 << 5;
const CANCELLATION_REQUESTED: u32 = 1 << 6;
pub(super) const SHUTDOWN: u32 = 1 << 7;
const ACCEPTED_NEXT_READY: u32 = 1 << 8;
const PROJECTION_FLIGHT_RELEASED: u32 = 1 << 9;
const EXECUTION_READY: u32 = 1 << 10;
const WORKER_COMPLETED: u32 = 1 << 11;
pub(super) const NEXT_WORKER_CAPACITY_RELEASED: u32 = 1 << 12;
const RECOVERED_PENDING_CONTINUE: u32 = 1 << 13;
const NATIVE_LINEAGE_READY: u32 = 1 << 14;
const NATIVE_LINEAGE_ROUTE_CAPACITY_RELEASED: u32 = 1 << 15;
const IDLE_RECHECK: u32 = 1 << 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum AcceptedInputWakeReason {
    AcceptedReady,
    TargetReady,
    WorkerReleased,
    AttemptReleased,
    CancellationLifecycle,
    Recovery,
    CancellationRequested,
    Shutdown,
    AcceptedNextReady,
    ProjectionFlightReleased,
    ExecutionReady,
    WorkerCompleted,
    NextWorkerCapacityReleased,
    RecoveredPendingContinue,
    NativeLineageReady,
    NativeLineageRouteCapacityReleased,
    IdleRecheck,
}

impl AcceptedInputWakeReason {
    pub(super) const fn bit(self) -> u32 {
        match self {
            Self::AcceptedReady => ACCEPTED_READY,
            Self::TargetReady => TARGET_READY,
            Self::WorkerReleased => WORKER_RELEASED,
            Self::AttemptReleased => ATTEMPT_RELEASED,
            Self::CancellationLifecycle => CANCELLATION_LIFECYCLE,
            Self::Recovery => RECOVERY,
            Self::CancellationRequested => CANCELLATION_REQUESTED,
            Self::Shutdown => SHUTDOWN,
            Self::AcceptedNextReady => ACCEPTED_NEXT_READY,
            Self::ProjectionFlightReleased => PROJECTION_FLIGHT_RELEASED,
            Self::ExecutionReady => EXECUTION_READY,
            Self::WorkerCompleted => WORKER_COMPLETED,
            Self::NextWorkerCapacityReleased => NEXT_WORKER_CAPACITY_RELEASED,
            Self::RecoveredPendingContinue => RECOVERED_PENDING_CONTINUE,
            Self::NativeLineageReady => NATIVE_LINEAGE_READY,
            Self::NativeLineageRouteCapacityReleased => NATIVE_LINEAGE_ROUTE_CAPACITY_RELEASED,
            Self::IdleRecheck => IDLE_RECHECK,
        }
    }
}

#[derive(Clone, Copy)]
pub(in super::super) struct WakeBatch {
    pub(super) bits: u32,
    pub(super) shutdown: bool,
}

impl WakeBatch {
    pub(in super::super) const fn rechecks_idle_sessions(self) -> bool {
        self.bits & IDLE_RECHECK != 0
    }

    pub(in super::super) const fn opens_steering_pass(self) -> bool {
        self.bits
            & (ACCEPTED_READY
                | TARGET_READY
                | WORKER_RELEASED
                | ATTEMPT_RELEASED
                | CANCELLATION_LIFECYCLE
                | RECOVERY
                | CANCELLATION_REQUESTED)
            != 0
    }

    pub(in super::super) const fn opens_retry_pass(self) -> bool {
        self.bits & (CANCELLATION_LIFECYCLE | RECOVERY) != 0
    }

    pub(in super::super) const fn shutdown(self) -> bool {
        self.shutdown || self.bits & SHUTDOWN != 0
    }

    pub(in super::super) const fn opens_next_pass(self) -> bool {
        self.bits
            & (ACCEPTED_NEXT_READY
                | EXECUTION_READY
                | CANCELLATION_LIFECYCLE
                | RECOVERY
                | CANCELLATION_REQUESTED)
            != 0
    }

    pub(in super::super) const fn restarts_recovered_pending_pass(self) -> bool {
        self.bits & (RECOVERY | EXECUTION_READY) != 0
    }

    pub(in super::super) const fn continues_recovered_pending_pass(self) -> bool {
        self.bits & RECOVERED_PENDING_CONTINUE != 0
    }

    pub(in super::super) const fn projection_flight_released(self) -> bool {
        self.bits & PROJECTION_FLIGHT_RELEASED != 0
    }

    pub(in super::super) const fn execution_ready(self) -> bool {
        self.bits & EXECUTION_READY != 0
    }

    pub(in super::super) const fn next_worker_capacity_released(self) -> bool {
        self.bits & NEXT_WORKER_CAPACITY_RELEASED != 0
    }

    pub(in super::super) const fn worker_completed(self) -> bool {
        self.bits & WORKER_COMPLETED != 0
    }

    pub(in super::super) const fn native_lineage_ready(self) -> bool {
        self.bits & NATIVE_LINEAGE_READY != 0
    }

    pub(in super::super) const fn native_lineage_route_capacity_released(self) -> bool {
        self.bits & NATIVE_LINEAGE_ROUTE_CAPACITY_RELEASED != 0
    }
}
