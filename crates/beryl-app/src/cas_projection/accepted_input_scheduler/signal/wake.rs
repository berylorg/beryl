const ACCEPTED_READY: u16 = 1 << 0;
const TARGET_READY: u16 = 1 << 1;
pub(super) const WORKER_RELEASED: u16 = 1 << 2;
const ATTEMPT_RELEASED: u16 = 1 << 3;
const CANCELLATION_LIFECYCLE: u16 = 1 << 4;
const RECOVERY: u16 = 1 << 5;
const CANCELLATION_REQUESTED: u16 = 1 << 6;
pub(super) const SHUTDOWN: u16 = 1 << 7;
const ACCEPTED_NEXT_READY: u16 = 1 << 8;
const PROJECTION_FLIGHT_RELEASED: u16 = 1 << 9;
const EXECUTION_READY: u16 = 1 << 10;
const WORKER_COMPLETED: u16 = 1 << 11;
pub(super) const NEXT_WORKER_CAPACITY_RELEASED: u16 = 1 << 12;
const RECOVERED_PENDING_CONTINUE: u16 = 1 << 13;
const SAME_GENERATION_VERIFIED: u16 = 1 << 14;

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
    SameGenerationVerified,
}

impl AcceptedInputWakeReason {
    pub(super) const fn bit(self) -> u16 {
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
            Self::SameGenerationVerified => SAME_GENERATION_VERIFIED,
        }
    }
}

#[derive(Clone, Copy)]
pub(in super::super) struct WakeBatch {
    pub(super) bits: u16,
    pub(super) shutdown: bool,
}

impl WakeBatch {
    pub(in super::super) const fn opens_steering_pass(self) -> bool {
        self.bits
            & (ACCEPTED_READY
                | TARGET_READY
                | WORKER_RELEASED
                | ATTEMPT_RELEASED
                | CANCELLATION_LIFECYCLE
                | RECOVERY
                | CANCELLATION_REQUESTED
                | SAME_GENERATION_VERIFIED)
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
                | CANCELLATION_REQUESTED
                | SAME_GENERATION_VERIFIED)
            != 0
    }

    pub(in super::super) const fn restarts_recovered_pending_pass(self) -> bool {
        self.bits & (RECOVERY | EXECUTION_READY | SAME_GENERATION_VERIFIED) != 0
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

    pub(in super::super) const fn same_generation_verified(self) -> bool {
        self.bits & SAME_GENERATION_VERIFIED != 0
    }
}
