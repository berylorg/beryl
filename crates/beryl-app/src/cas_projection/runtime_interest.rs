use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread::JoinHandle,
    time::Duration,
};

use beryl_backend::ManagedBackendLaunchSpec;
use beryl_model::{CasProcessGeneration, ExecutionBinding, RuntimeId};
use thiserror::Error;

use super::persistent_failure::LiveCommandAuthorizer;

mod managed;
mod owner;
mod worker;

#[cfg(feature = "test-faults")]
mod test_support;
#[cfg(feature = "test-faults")]
pub use test_support::{RuntimeInterestTestHarness, RuntimeInterestTestProbe};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeInterestConfig {
    runtime_capacity: NonZeroUsize,
    interest_capacity: NonZeroUsize,
    admission_timeout: Duration,
}

impl RuntimeInterestConfig {
    pub fn new(
        runtime_capacity: NonZeroUsize,
        interest_capacity: NonZeroUsize,
        admission_timeout: Duration,
    ) -> Result<Self, RuntimeInterestError> {
        if admission_timeout.is_zero() {
            return Err(RuntimeInterestError::InvalidTimeout);
        }
        Ok(Self {
            runtime_capacity,
            interest_capacity,
            admission_timeout,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeInterestKind {
    View,
    RequiredWork,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeActivityPeriod(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeReadiness {
    process_generation: CasProcessGeneration,
    activity_period: RuntimeActivityPeriod,
}

impl RuntimeReadiness {
    pub fn process_generation(self) -> CasProcessGeneration {
        self.process_generation
    }

    pub fn activity_period(self) -> RuntimeActivityPeriod {
        self.activity_period
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeFailure {
    Launch,
    Admission,
    ProcessExited,
    ConnectionLost,
    AppRetirement,
    BackendDisposal,
    WorkerPanicked,
    IdentityExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeInterestStatus {
    Starting,
    Ready(RuntimeReadiness),
    Retiring,
    Unavailable(RuntimeFailure),
    Retired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum RuntimeInterestError {
    #[error("runtime interest is not configured")]
    NotConfigured,
    #[error("runtime interest is already configured")]
    AlreadyConfigured,
    #[error("runtime admission timeout must be positive")]
    InvalidTimeout,
    #[error("the runtime owner generation is closed")]
    Closed,
    #[error("runtime ownership capacity is full")]
    RuntimeCapacity,
    #[error("runtime interest capacity is full")]
    InterestCapacity,
    #[error("this runtime is still retiring")]
    Retiring,
    #[error("runtime launch configuration does not match the exact demand")]
    ConfigurationMismatch,
    #[error("runtime interest identity is exhausted")]
    IdentityExhausted,
    #[error("runtime admission preparation is unavailable")]
    AdmissionUnavailable,
    #[error("the runtime ownership worker could not start")]
    WorkerStart,
}

pub struct RuntimeInterest {
    shared: Arc<RuntimeInterestShared>,
    runtime_id: RuntimeId,
    attempt: u64,
    interest: u64,
    binding: ExecutionBinding,
    kind: RuntimeInterestKind,
}

impl RuntimeInterest {
    pub fn binding(&self) -> &ExecutionBinding {
        &self.binding
    }

    pub fn kind(&self) -> RuntimeInterestKind {
        self.kind
    }

    pub fn status(&self) -> RuntimeInterestStatus {
        let state = self.shared.lock();
        self.status_locked(&state)
    }

    pub fn is_current(&self, period: RuntimeActivityPeriod) -> bool {
        matches!(self.status(), RuntimeInterestStatus::Ready(ready) if ready.activity_period == period)
    }

    pub fn wait_for_change(
        &self,
        observed: RuntimeInterestStatus,
        timeout: Duration,
    ) -> RuntimeInterestStatus {
        let state = self.shared.lock();
        let (state, _) = self
            .shared
            .changed
            .wait_timeout_while(state, timeout, |state| {
                self.status_locked(state) == observed
            })
            .unwrap_or_else(|poison| poison.into_inner());
        self.status_locked(&state)
    }

    fn status_locked(&self, state: &RuntimeInterestState) -> RuntimeInterestStatus {
        if state.closed || !self.shared.commands.is_open() {
            return RuntimeInterestStatus::Retired;
        }
        state
            .runtimes
            .get(&self.runtime_id)
            .filter(|entry| {
                entry.attempt == self.attempt && entry.interests.contains_key(&self.interest)
            })
            .map_or(RuntimeInterestStatus::Retired, |entry| entry.status)
    }
}

impl Drop for RuntimeInterest {
    fn drop(&mut self) {
        let mut state = self.shared.lock();
        if let Some(entry) = state.runtimes.get_mut(&self.runtime_id)
            && entry.attempt == self.attempt
            && entry.interests.remove(&self.interest).is_some()
        {
            if entry.interests.is_empty()
                && matches!(
                    entry.status,
                    RuntimeInterestStatus::Starting | RuntimeInterestStatus::Ready(_)
                )
            {
                entry.status = RuntimeInterestStatus::Retiring;
            }
            state.interest_count -= 1;
            self.shared.changed.notify_all();
        }
    }
}

pub(super) struct RuntimeInterestOwner {
    shared: Arc<RuntimeInterestShared>,
}

struct RuntimeInterestShared {
    state: Mutex<RuntimeInterestState>,
    changed: Condvar,
    commands: LiveCommandAuthorizer,
    config: RuntimeInterestConfig,
}

impl RuntimeInterestShared {
    fn lock(&self) -> MutexGuard<'_, RuntimeInterestState> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poison) => {
                let mut state = poison.into_inner();
                state.closed = true;
                self.changed.notify_all();
                state
            }
        }
    }
}

struct RuntimeInterestState {
    closed: bool,
    next_identity: u64,
    interest_count: usize,
    runtimes: HashMap<RuntimeId, RuntimeEntry>,
}

struct RuntimeEntry {
    spec: ManagedBackendLaunchSpec,
    attempt: u64,
    interests: HashMap<u64, RuntimeInterestKind>,
    status: RuntimeInterestStatus,
    worker: Option<JoinHandle<bool>>,
    cleanup_complete: bool,
}

trait RunningRuntime: Send {
    fn process_generation(&self) -> CasProcessGeneration;
    fn poll_health(&mut self) -> Result<(), RuntimeFailure>;
    fn retire(&mut self) -> Result<(), RuntimeFailure>;
}
