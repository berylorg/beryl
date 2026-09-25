use std::sync::{Arc, Mutex, TryLockError, Weak};

use beryl_home_store::HomeGeneration;
use beryl_model::{BerylHomeId, RuntimeId};
use syndic_storage::{ActivityPeriodToken, ActivityWorkPeriod};
use thiserror::Error;

use super::{
    RuntimeInterestOwner, RuntimeInterestShared, RuntimeInterestState, RuntimeInterestStatus,
    RuntimeReadiness, activity::RuntimeActivityState,
};
use crate::cas_projection::ProjectionServiceGeneration;

#[derive(Clone)]
pub(crate) struct RuntimeActivityReadSource {
    shared: Weak<RuntimeInterestShared>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
}

pub(crate) struct RuntimeActivityObservation {
    source: RuntimeActivityReadSource,
    runtime_id: RuntimeId,
    attempt: u64,
    readiness: RuntimeReadiness,
    activity: Weak<Mutex<RuntimeActivityState>>,
    token: ActivityPeriodToken,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub(crate) enum RuntimeActivityReadError {
    #[error("runtime Activity publication is busy")]
    Busy,
    #[error("the original runtime attempt is not ready")]
    RuntimeUnavailable,
    #[error("the ready runtime has no proven Activity enrollment")]
    Unenrolled,
    #[error("runtime Activity read authority is unavailable")]
    Unavailable,
}

impl RuntimeInterestOwner {
    pub(in crate::cas_projection) fn activity_read_source(
        &self,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        service_generation: ProjectionServiceGeneration,
    ) -> Option<RuntimeActivityReadSource> {
        (self.shared.commands.service_generation() == service_generation
            && self
                .shared
                .enrollments
                .matches(home_id, self.shared.config.runtime_capacity()))
        .then(|| RuntimeActivityReadSource {
            shared: Arc::downgrade(&self.shared),
            home_id,
            home_generation,
            service_generation,
        })
    }
}

impl RuntimeActivityReadSource {
    pub(crate) fn home_id(&self) -> BerylHomeId {
        self.home_id
    }

    pub(crate) fn home_generation(&self) -> HomeGeneration {
        self.home_generation
    }

    pub(crate) fn service_generation(&self) -> ProjectionServiceGeneration {
        self.service_generation
    }

    pub(crate) fn observe(
        &self,
        runtime_id: RuntimeId,
    ) -> Result<RuntimeActivityObservation, RuntimeActivityReadError> {
        let shared = self
            .shared
            .upgrade()
            .ok_or(RuntimeActivityReadError::RuntimeUnavailable)?;
        let state = shared
            .state
            .lock()
            .map_err(|_| RuntimeActivityReadError::Unavailable)?;
        self.ensure_open(&shared, &state)?;
        let entry = state
            .runtimes
            .get(&runtime_id)
            .ok_or(RuntimeActivityReadError::RuntimeUnavailable)?;
        let RuntimeInterestStatus::Ready(readiness) = entry.status else {
            return Err(RuntimeActivityReadError::RuntimeUnavailable);
        };
        let activity = entry
            .activity
            .lock()
            .map_err(|_| RuntimeActivityReadError::Unavailable)?;
        let token = activity
            .token
            .as_ref()
            .ok_or(RuntimeActivityReadError::Unenrolled)?;
        if token.runtime_id() != runtime_id {
            return Err(RuntimeActivityReadError::Unavailable);
        }
        Ok(RuntimeActivityObservation {
            source: self.clone(),
            runtime_id,
            attempt: entry.attempt,
            readiness,
            activity: Arc::downgrade(&entry.activity),
            token: token.clone(),
        })
    }

    fn ensure_open(
        &self,
        shared: &RuntimeInterestShared,
        state: &RuntimeInterestState,
    ) -> Result<(), RuntimeActivityReadError> {
        if state.closed
            || shared.commands.service_generation() != self.service_generation
            || !shared.commands.is_open()
        {
            return Err(RuntimeActivityReadError::RuntimeUnavailable);
        }
        Ok(())
    }
}

impl RuntimeActivityObservation {
    pub(crate) fn runtime_id(&self) -> RuntimeId {
        self.runtime_id
    }

    pub(crate) fn work_period(&self) -> ActivityWorkPeriod {
        self.token.work_period()
    }

    pub(crate) fn with_current<T>(
        &self,
        publish: impl FnOnce() -> T,
    ) -> Result<T, RuntimeActivityReadError> {
        let shared = self
            .source
            .shared
            .upgrade()
            .ok_or(RuntimeActivityReadError::RuntimeUnavailable)?;
        let state = shared.state.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => RuntimeActivityReadError::Busy,
            TryLockError::Poisoned(_) => RuntimeActivityReadError::Unavailable,
        })?;
        self.source.ensure_open(&shared, &state)?;
        let entry = state
            .runtimes
            .get(&self.runtime_id)
            .filter(|entry| {
                entry.attempt == self.attempt
                    && entry.status == RuntimeInterestStatus::Ready(self.readiness)
                    && self.activity.ptr_eq(&Arc::downgrade(&entry.activity))
            })
            .ok_or(RuntimeActivityReadError::RuntimeUnavailable)?;
        let activity = entry.activity.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => RuntimeActivityReadError::Busy,
            TryLockError::Poisoned(_) => RuntimeActivityReadError::Unavailable,
        })?;
        if activity.token.as_ref() != Some(&self.token) {
            return Err(RuntimeActivityReadError::RuntimeUnavailable);
        }
        let command = shared
            .commands
            .authorize()
            .map_err(|_| RuntimeActivityReadError::RuntimeUnavailable)?;
        command
            .commit_if_current(publish)
            .map_err(|_| RuntimeActivityReadError::RuntimeUnavailable)
    }
}
