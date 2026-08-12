use std::{
    num::NonZeroUsize,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use beryl_backend::ForegroundSessionConfig;
use beryl_home_store::{MinimumTurnCaptureReserve, TurnStartAdmissionRequirement};
use thiserror::Error;

pub(super) const CONNECTION_WORKER_PERMITS: usize = 2;
pub(super) const SCHEDULED_ORDINARY_WORKER_PERMITS: usize = 1;
pub(super) const STEERING_CRITICAL_WORKER_RESERVE: usize = 1;
const MINIMUM_WORKER_CAPACITY: usize = CONNECTION_WORKER_PERMITS
    + SCHEDULED_ORDINARY_WORKER_PERMITS
    + STEERING_CRITICAL_WORKER_RESERVE;

/// Immutable local limits for the non-GUI projection connection service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectionServiceConfig {
    foreground: ForegroundSessionConfig,
    worker_capacity: NonZeroUsize,
    turn_start_admission_requirement: TurnStartAdmissionRequirement,
}

impl ProjectionServiceConfig {
    fn compose_turn_start_admission_requirement(
        minimum_turn_capture_reserve: MinimumTurnCaptureReserve,
    ) -> Result<TurnStartAdmissionRequirement, ProjectionServiceConfigError> {
        let direct = beryl_home_store::DurableStartFootprint::compose(
            syndic_storage::idle_submission_max_footprint()
                .map_err(ProjectionServiceConfigError::DurableStartFootprint)?,
            Some(
                beryl_state::draft_to_submitted_item_owner_transfer_max_footprint()
                    .map_err(ProjectionServiceConfigError::DurableStartFootprint)?,
            ),
        )
        .map_err(ProjectionServiceConfigError::DurableStartFootprint)?;
        let queued = beryl_home_store::DurableStartFootprint::compose(
            syndic_storage::accepted_input_promotion_max_footprint()
                .map_err(ProjectionServiceConfigError::DurableStartFootprint)?,
            Some(
                beryl_state::accepted_input_to_submitted_item_owner_transfer_max_footprint()
                    .map_err(ProjectionServiceConfigError::DurableStartFootprint)?,
            ),
        )
        .map_err(ProjectionServiceConfigError::DurableStartFootprint)?;
        TurnStartAdmissionRequirement::try_new(direct, queued, minimum_turn_capture_reserve)
            .map_err(ProjectionServiceConfigError::TurnStartAdmissionRequirement)
    }

    /// Validates caller-supplied counts before a candidate connection is opened.
    pub fn try_new(
        pre_bind_control_capacity: u64,
        worker_capacity: u64,
        minimum_turn_capture_reserve: MinimumTurnCaptureReserve,
    ) -> Result<Self, ProjectionServiceConfigError> {
        let pre_bind_control_capacity =
            usize::try_from(pre_bind_control_capacity).map_err(|_| {
                ProjectionServiceConfigError::UnrepresentablePreBindControlCapacity {
                    capacity: pre_bind_control_capacity,
                }
            })?;
        let worker_capacity = usize::try_from(worker_capacity).map_err(|_| {
            ProjectionServiceConfigError::UnrepresentableWorkerCapacity {
                capacity: worker_capacity,
            }
        })?;
        let pre_bind_control_capacity = NonZeroUsize::new(pre_bind_control_capacity)
            .ok_or(ProjectionServiceConfigError::ZeroPreBindControlCapacity)?;
        let worker_capacity = NonZeroUsize::new(worker_capacity)
            .ok_or(ProjectionServiceConfigError::ZeroWorkerCapacity)?;
        if worker_capacity.get() < MINIMUM_WORKER_CAPACITY {
            return Err(ProjectionServiceConfigError::InsufficientWorkerCapacity {
                capacity: worker_capacity.get(),
                required: MINIMUM_WORKER_CAPACITY,
            });
        }
        let turn_start_admission_requirement =
            Self::compose_turn_start_admission_requirement(minimum_turn_capture_reserve)?;
        Ok(Self {
            foreground: ForegroundSessionConfig::new(pre_bind_control_capacity),
            worker_capacity,
            turn_start_admission_requirement,
        })
    }

    /// Returns the immutable backend foreground profile.
    #[must_use]
    pub const fn foreground(self) -> ForegroundSessionConfig {
        self.foreground
    }

    /// Returns the maximum number of concurrent projection worker threads.
    #[must_use]
    pub const fn worker_capacity(self) -> NonZeroUsize {
        self.worker_capacity
    }

    /// Returns the opaque requirement shared by direct and queued new-turn admission.
    #[must_use]
    pub const fn turn_start_admission_requirement(self) -> TurnStartAdmissionRequirement {
        self.turn_start_admission_requirement
    }
}

/// Invalid projection-service capacity configuration.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ProjectionServiceConfigError {
    #[error("projection pre-bind control capacity must be nonzero")]
    ZeroPreBindControlCapacity,
    #[error("projection worker capacity must be nonzero")]
    ZeroWorkerCapacity,
    #[error("durable-start footprint derivation failed: {0}")]
    DurableStartFootprint(beryl_home_store::DurableStartFootprintError),
    #[error("turn-start admission requirement is invalid: {0}")]
    TurnStartAdmissionRequirement(beryl_home_store::TurnStartAdmissionRequirementError),
    #[error(
        "projection worker capacity {capacity} is below the {required}-permit service minimum (two connection workers, one scheduled ordinary worker, and one protected steering-critical worker)"
    )]
    InsufficientWorkerCapacity { capacity: usize, required: usize },
    #[error("projection pre-bind control capacity {capacity} is not representable")]
    UnrepresentablePreBindControlCapacity { capacity: u64 },
    #[error("projection worker capacity {capacity} is not representable")]
    UnrepresentableWorkerCapacity { capacity: u64 },
}

/// Content-free count diagnostics for the app-owned projection worker pool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectionWorkerPoolDiagnostics {
    capacity: usize,
    available: usize,
    active: usize,
    high_water: usize,
    denied_pairs: u64,
    denied_singles: u64,
}

impl ProjectionWorkerPoolDiagnostics {
    #[must_use]
    pub const fn capacity(self) -> usize {
        self.capacity
    }

    #[must_use]
    pub const fn available(self) -> usize {
        self.available
    }

    #[must_use]
    pub const fn active(self) -> usize {
        self.active
    }

    #[must_use]
    pub const fn high_water(self) -> usize {
        self.high_water
    }

    #[must_use]
    pub const fn denied_pairs(self) -> u64 {
        self.denied_pairs
    }

    #[must_use]
    pub const fn denied_singles(self) -> u64 {
        self.denied_singles
    }
}

#[derive(Clone)]
pub(super) struct ProjectionWorkerPool {
    inner: Arc<Mutex<ProjectionWorkerPoolState>>,
    scheduler_signal: super::accepted_input_scheduler::AcceptedInputSchedulerSignal,
}

struct ProjectionWorkerPoolState {
    capacity: usize,
    available: usize,
    active_steering_critical: usize,
    high_water: usize,
    denied_pairs: u64,
    denied_singles: u64,
    release_waiter: ProjectionWorkerReleaseWaiter,
}

#[derive(Default)]
struct ProjectionWorkerReleaseWaiter {
    steering: bool,
    scheduled_ordinary: bool,
}

pub(super) struct ProjectionWorkerPermitPair {
    driver: Option<ProjectionWorkerPermit>,
    ingester: Option<ProjectionWorkerPermit>,
}

pub(super) struct ProjectionWorkerPermit {
    admission: Arc<ProjectionWorkerAdmission>,
}

struct ProjectionWorkerAdmission {
    pool: ProjectionWorkerPool,
    role: ProjectionWorkerRole,
    committed_steering_worker: AtomicBool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectionWorkerRole {
    Connection,
    ScheduledOrdinary,
    SteeringCritical,
}

impl ProjectionWorkerReleaseWaiter {
    fn arm(&mut self, role: ProjectionWorkerRole) {
        match role {
            ProjectionWorkerRole::Connection => {}
            ProjectionWorkerRole::ScheduledOrdinary => self.scheduled_ordinary = true,
            ProjectionWorkerRole::SteeringCritical => self.steering = true,
        }
    }

    fn clear(&mut self, role: ProjectionWorkerRole) {
        match role {
            ProjectionWorkerRole::Connection => {}
            ProjectionWorkerRole::ScheduledOrdinary => self.scheduled_ordinary = false,
            ProjectionWorkerRole::SteeringCritical => self.steering = false,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum ProjectionWorkerPermitError {
    CapacityFull { available: usize },
    Poisoned,
}

impl ProjectionWorkerPoolState {
    fn noncritical_role_fits(&self, permits: usize) -> bool {
        let Some(remaining) = self.available.checked_sub(permits) else {
            return false;
        };
        self.active_steering_critical > 0 || remaining >= STEERING_CRITICAL_WORKER_RESERVE
    }

    fn record_acquisition(&mut self, permits: usize) {
        self.available = self
            .available
            .checked_sub(permits)
            .expect("admission checks worker capacity before acquisition");
        self.high_water = self
            .high_water
            .max(self.capacity.saturating_sub(self.available));
    }
}

impl ProjectionWorkerPool {
    pub(super) fn new(capacity: NonZeroUsize) -> Self {
        Self::new_with_scheduler(
            capacity,
            super::accepted_input_scheduler::AcceptedInputSchedulerSignal::new(),
        )
    }

    pub(super) fn new_with_scheduler(
        capacity: NonZeroUsize,
        scheduler_signal: super::accepted_input_scheduler::AcceptedInputSchedulerSignal,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ProjectionWorkerPoolState {
                capacity: capacity.get(),
                available: capacity.get(),
                active_steering_critical: 0,
                high_water: 0,
                denied_pairs: 0,
                denied_singles: 0,
                release_waiter: ProjectionWorkerReleaseWaiter::default(),
            })),
            scheduler_signal,
        }
    }

    pub(super) fn try_acquire_pair(
        &self,
    ) -> Result<ProjectionWorkerPermitPair, ProjectionWorkerPermitError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ProjectionWorkerPermitError::Poisoned)?;
        if !state.noncritical_role_fits(CONNECTION_WORKER_PERMITS) {
            state.denied_pairs = state.denied_pairs.saturating_add(1);
            return Err(ProjectionWorkerPermitError::CapacityFull {
                available: state.available,
            });
        }
        state.record_acquisition(CONNECTION_WORKER_PERMITS);
        drop(state);
        Ok(ProjectionWorkerPermitPair {
            driver: Some(ProjectionWorkerPermit::new(
                self.clone(),
                ProjectionWorkerRole::Connection,
            )),
            ingester: Some(ProjectionWorkerPermit::new(
                self.clone(),
                ProjectionWorkerRole::Connection,
            )),
        })
    }

    /// Reserves one scheduled ordinary worker without retaining candidate work.
    pub(super) fn try_acquire_scheduled_ordinary_or_arm(
        &self,
    ) -> Result<ProjectionWorkerPermit, ProjectionWorkerPermitError> {
        self.try_acquire_role(ProjectionWorkerRole::ScheduledOrdinary, true)
    }

    /// Acquires steering-critical progress capacity for a direct delivery.
    pub(super) fn try_acquire_steering_critical(
        &self,
    ) -> Result<ProjectionWorkerPermit, ProjectionWorkerPermitError> {
        self.try_acquire_role(ProjectionWorkerRole::SteeringCritical, false)
    }

    /// Acquires steering-critical progress capacity or arms the sole release waiter.
    pub(super) fn try_acquire_steering_critical_quiet_or_arm(
        &self,
    ) -> Result<ProjectionWorkerPermit, ProjectionWorkerPermitError> {
        self.try_acquire_role(ProjectionWorkerRole::SteeringCritical, true)
    }

    fn try_acquire_role(
        &self,
        role: ProjectionWorkerRole,
        arm_release_waiter: bool,
    ) -> Result<ProjectionWorkerPermit, ProjectionWorkerPermitError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ProjectionWorkerPermitError::Poisoned)?;
        let fits = match role {
            ProjectionWorkerRole::Connection => false,
            ProjectionWorkerRole::ScheduledOrdinary => {
                state.noncritical_role_fits(SCHEDULED_ORDINARY_WORKER_PERMITS)
            }
            ProjectionWorkerRole::SteeringCritical => state.available > 0,
        };
        if !fits {
            state.denied_singles = state.denied_singles.saturating_add(1);
            if arm_release_waiter {
                state.release_waiter.arm(role);
            }
            return Err(ProjectionWorkerPermitError::CapacityFull {
                available: state.available,
            });
        }
        state.release_waiter.clear(role);
        state.record_acquisition(1);
        if role == ProjectionWorkerRole::SteeringCritical {
            state.active_steering_critical = state
                .active_steering_critical
                .checked_add(1)
                .expect("steering-critical acquisitions retain exact permit accounting");
        }
        drop(state);
        Ok(ProjectionWorkerPermit::new(self.clone(), role))
    }

    pub(super) fn diagnostics(&self) -> ProjectionWorkerPoolDiagnostics {
        let state = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        ProjectionWorkerPoolDiagnostics {
            capacity: state.capacity,
            available: state.available,
            active: state.capacity.saturating_sub(state.available),
            high_water: state.high_water,
            denied_pairs: state.denied_pairs,
            denied_singles: state.denied_singles,
        }
    }
}

impl ProjectionWorkerPermitPair {
    pub(super) fn into_parts(mut self) -> (ProjectionWorkerPermit, ProjectionWorkerPermit) {
        (self.take_driver(), self.take_ingester())
    }

    pub(super) fn take_driver(&mut self) -> ProjectionWorkerPermit {
        self.driver
            .take()
            .expect("an acquired worker pair has one driver permit")
    }

    pub(super) fn take_ingester(&mut self) -> ProjectionWorkerPermit {
        self.ingester
            .take()
            .expect("an acquired worker pair has one ingester permit")
    }
}

impl Drop for ProjectionWorkerAdmission {
    fn drop(&mut self) {
        let mut state = self
            .pool
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.available = state
            .available
            .checked_add(1)
            .expect("worker releases cannot exceed configured capacity");
        if self.role == ProjectionWorkerRole::SteeringCritical {
            state.active_steering_critical = state
                .active_steering_critical
                .checked_sub(1)
                .expect("steering-critical releases retain exact permit accounting");
        }
        debug_assert!(state.available <= state.capacity);
        let releases_scheduled_capacity = self.role == ProjectionWorkerRole::Connection
            || (self.role == ProjectionWorkerRole::SteeringCritical
                && self.committed_steering_worker.load(Ordering::Acquire));
        let next_capacity_released = releases_scheduled_capacity
            && std::mem::take(&mut state.release_waiter.scheduled_ordinary);
        let steering_capacity_released = std::mem::take(&mut state.release_waiter.steering);
        let steering_released =
            self.committed_steering_worker.load(Ordering::Acquire) || steering_capacity_released;
        drop(state);
        self.pool
            .scheduler_signal
            .wake_worker_release(steering_released, next_capacity_released);
    }
}

impl ProjectionWorkerPermit {
    fn new(pool: ProjectionWorkerPool, role: ProjectionWorkerRole) -> Self {
        Self {
            admission: Arc::new(ProjectionWorkerAdmission {
                pool,
                role,
                committed_steering_worker: AtomicBool::new(false),
            }),
        }
    }

    pub(super) fn commit_steering_worker(&mut self) {
        debug_assert_eq!(self.admission.role, ProjectionWorkerRole::SteeringCritical);
        self.admission
            .committed_steering_worker
            .store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/projection_service_config.rs"
    ));
}
