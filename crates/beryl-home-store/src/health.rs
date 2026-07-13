use std::{
    num::NonZeroU64,
    sync::{Condvar, Mutex},
    time::Duration,
};

use thiserror::Error;

/// Coherent process-wide availability state for one opened Beryl home.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HomeHealthState {
    /// The configured home has not yet completed initial admission.
    Opening,
    /// State-dependent operations may be admitted.
    Healthy,
    /// A surfaced failure closed admission while the current generation is checked.
    Verifying,
    /// Verification or reopen failed and all state-dependent work remains gated.
    Failed,
    /// The same locked home is being force-recovered and validated.
    Reopening,
}

/// Monotonic identity of one validated in-process home generation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HomeGeneration(NonZeroU64);

impl HomeGeneration {
    pub(crate) const INITIAL: Self = Self(NonZeroU64::MIN);

    /// Returns the nonzero generation number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    pub(crate) fn checked_next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

/// Bounded observation of the current home-store gate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HomeHealthSnapshot {
    state: HomeHealthState,
    generation: Option<HomeGeneration>,
}

impl HomeHealthSnapshot {
    /// Constructs the state used by a caller before [`crate::HomeStore::open`] returns.
    #[must_use]
    pub const fn opening() -> Self {
        Self {
            state: HomeHealthState::Opening,
            generation: None,
        }
    }

    /// Returns the coherent availability state.
    #[must_use]
    pub const fn state(self) -> HomeHealthState {
        self.state
    }

    /// Returns the last or current validated generation, when one exists.
    #[must_use]
    pub const fn generation(self) -> Option<HomeGeneration> {
        self.generation
    }
}

/// A state-dependent operation was rejected by the process-wide home gate.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("Beryl-home state is unavailable while the store is {state:?}")]
pub struct HealthGateError {
    state: HomeHealthState,
    generation: HomeGeneration,
}

impl HealthGateError {
    /// Returns the state that rejected or invalidated the operation.
    #[must_use]
    pub const fn state(self) -> HomeHealthState {
        self.state
    }

    /// Returns the last validated home generation.
    #[must_use]
    pub const fn generation(self) -> HomeGeneration {
        self.generation
    }
}

/// Accepted delays between repeated same-home recovery attempts.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecoveryRetrySchedule {
    attempts: usize,
}

impl RecoveryRetrySchedule {
    /// Returns the next delay and advances the schedule.
    pub fn next_delay(&mut self) -> Duration {
        const DELAYS: [Duration; 5] = [
            Duration::from_secs(1),
            Duration::from_secs(2),
            Duration::from_secs(5),
            Duration::from_secs(10),
            Duration::from_secs(30),
        ];
        let delay = DELAYS[self.attempts.min(DELAYS.len() - 1)];
        self.attempts = self.attempts.saturating_add(1);
        delay
    }

    /// Restarts the schedule after one successful recovery.
    pub fn reset(&mut self) {
        self.attempts = 0;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FailureSeverity {
    Verify,
    Structural,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Maintenance {
    Verification,
    Recovery,
}

struct HealthInner {
    state: HomeHealthState,
    generation: HomeGeneration,
    active: usize,
    maintenance: Option<Maintenance>,
}

pub(crate) struct HealthGate {
    inner: Mutex<HealthInner>,
    drained: Condvar,
}

impl HealthGate {
    pub(crate) fn healthy() -> Self {
        Self {
            inner: Mutex::new(HealthInner {
                state: HomeHealthState::Healthy,
                generation: HomeGeneration::INITIAL,
                active: 0,
                maintenance: None,
            }),
            drained: Condvar::new(),
        }
    }

    pub(crate) fn snapshot(&self) -> HomeHealthSnapshot {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        HomeHealthSnapshot {
            state: inner.state,
            generation: Some(inner.generation),
        }
    }

    pub(crate) fn admit(&self) -> Result<HealthAdmission<'_>, HealthGateError> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.state != HomeHealthState::Healthy {
            return Err(gate_error(&inner));
        }
        inner.active = inner
            .active
            .checked_add(1)
            .expect("health admission count exhausted");
        Ok(HealthAdmission {
            gate: self,
            generation: inner.generation,
        })
    }

    pub(crate) fn signal_failure(&self, severity: FailureSeverity) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match (inner.state, severity) {
            (HomeHealthState::Healthy, FailureSeverity::Verify) => {
                inner.state = HomeHealthState::Verifying;
            }
            (
                HomeHealthState::Healthy | HomeHealthState::Verifying,
                FailureSeverity::Structural,
            ) => {
                inner.state = HomeHealthState::Failed;
            }
            _ => {}
        }
    }

    pub(crate) fn begin_verification(
        &self,
    ) -> Result<HealthMaintenance<'_>, HealthMaintenanceError> {
        self.begin(Maintenance::Verification)
    }

    pub(crate) fn begin_recovery(&self) -> Result<HealthMaintenance<'_>, HealthMaintenanceError> {
        self.begin(Maintenance::Recovery)
    }

    fn begin(
        &self,
        maintenance: Maintenance,
    ) -> Result<HealthMaintenance<'_>, HealthMaintenanceError> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.maintenance.is_some() {
            return Err(HealthMaintenanceError::InProgress { state: inner.state });
        }
        let accepted = match maintenance {
            Maintenance::Verification => inner.state == HomeHealthState::Verifying,
            Maintenance::Recovery => inner.state == HomeHealthState::Failed,
        };
        if !accepted {
            return Err(HealthMaintenanceError::InvalidState { state: inner.state });
        }
        if maintenance == Maintenance::Recovery {
            inner.state = HomeHealthState::Reopening;
        }
        inner.maintenance = Some(maintenance);
        while inner.active != 0 {
            inner = self
                .drained
                .wait(inner)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        Ok(HealthMaintenance {
            gate: self,
            maintenance,
            finished: false,
        })
    }
}

fn gate_error(inner: &HealthInner) -> HealthGateError {
    HealthGateError {
        state: inner.state,
        generation: inner.generation,
    }
}

pub(crate) struct HealthAdmission<'a> {
    gate: &'a HealthGate,
    generation: HomeGeneration,
}

impl HealthAdmission<'_> {
    pub(crate) const fn generation(&self) -> HomeGeneration {
        self.generation
    }

    pub(crate) fn confirm(&self) -> Result<(), HealthGateError> {
        let inner = self
            .gate
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.state == HomeHealthState::Healthy && inner.generation == self.generation {
            Ok(())
        } else {
            Err(gate_error(&inner))
        }
    }

    pub(crate) fn fail(&self, severity: FailureSeverity) {
        self.gate.signal_failure(severity);
    }
}

impl Drop for HealthAdmission<'_> {
    fn drop(&mut self) {
        let mut inner = self
            .gate
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.active = inner
            .active
            .checked_sub(1)
            .expect("health admission underflow");
        if inner.active == 0 {
            self.gate.drained.notify_all();
        }
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub(crate) enum HealthMaintenanceError {
    #[error("home maintenance is already active while the store is {state:?}")]
    InProgress { state: HomeHealthState },
    #[error("home maintenance cannot start while the store is {state:?}")]
    InvalidState { state: HomeHealthState },
}

pub(crate) struct HealthMaintenance<'a> {
    gate: &'a HealthGate,
    maintenance: Maintenance,
    finished: bool,
}

impl HealthMaintenance<'_> {
    pub(crate) fn finish_healthy(mut self, generation: HomeGeneration) {
        let mut inner = self
            .gate
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        debug_assert_eq!(inner.maintenance, Some(self.maintenance));
        inner.state = HomeHealthState::Healthy;
        inner.generation = generation;
        inner.maintenance = None;
        self.finished = true;
    }

    pub(crate) fn finish_failed(mut self) {
        let mut inner = self
            .gate
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        debug_assert_eq!(inner.maintenance, Some(self.maintenance));
        inner.state = HomeHealthState::Failed;
        inner.maintenance = None;
        self.finished = true;
    }
}

impl Drop for HealthMaintenance<'_> {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let mut inner = self
            .gate
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.maintenance == Some(self.maintenance) {
            inner.state = HomeHealthState::Failed;
            inner.maintenance = None;
        }
    }
}
