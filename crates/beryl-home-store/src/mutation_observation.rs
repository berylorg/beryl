use std::{
    sync::{Arc, Mutex, MutexGuard, Weak},
    task::Waker,
};

use crate::HomeStore;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum HomeMutationObservationError {
    #[error("a home mutation is in progress")]
    Busy,
    #[error("the home mutation interval changed")]
    Stale,
    #[error("the home mutation observer was revoked")]
    Revoked,
    #[error("the home mutation observation boundary is closed")]
    Closed,
    #[error("home mutation observation is unavailable")]
    Unavailable,
}

#[derive(Clone, Debug)]
pub struct HomeMutationObserver {
    owner: Arc<ObserverOwner>,
}

#[derive(Clone, Debug)]
pub struct HomeMutationObservation {
    registration: Weak<ObserverRegistration>,
    revision: u64,
}

#[derive(Debug)]
struct ObserverRegistration {
    boundary: Weak<MutationBoundary>,
    wake: Waker,
}

#[derive(Debug)]
struct ObserverOwner {
    registration: Arc<ObserverRegistration>,
}

#[derive(Debug)]
pub(crate) struct MutationBoundary {
    state: Mutex<MutationState>,
}

#[derive(Debug)]
struct MutationState {
    revision: Option<u64>,
    active: bool,
    closed: bool,
    observer: Weak<ObserverRegistration>,
}

pub(crate) struct MutationActivity<'a> {
    boundary: &'a MutationBoundary,
    settled: bool,
}

pub(crate) struct ObservedWriter<'a> {
    writer: Option<MutexGuard<'a, ()>>,
    activity: Option<MutationActivity<'a>>,
}

impl Default for MutationBoundary {
    fn default() -> Self {
        Self {
            state: Mutex::new(MutationState {
                revision: Some(0),
                active: false,
                closed: false,
                observer: Weak::new(),
            }),
        }
    }
}

impl HomeStore {
    pub fn observe_mutations(
        &self,
        wake: Waker,
    ) -> Result<HomeMutationObserver, HomeMutationObservationError> {
        let mut state = self
            .mutation_boundary
            .state
            .lock()
            .map_err(|_| HomeMutationObservationError::Unavailable)?;
        if state.closed {
            return Err(HomeMutationObservationError::Closed);
        }
        let registration = Arc::new(ObserverRegistration {
            boundary: Arc::downgrade(&self.mutation_boundary),
            wake,
        });
        state.observer = Arc::downgrade(&registration);
        Ok(HomeMutationObserver {
            owner: Arc::new(ObserverOwner { registration }),
        })
    }
}

impl HomeMutationObserver {
    pub fn observe(&self) -> Result<HomeMutationObservation, HomeMutationObservationError> {
        let boundary = self
            .owner
            .registration
            .boundary
            .upgrade()
            .ok_or(HomeMutationObservationError::Closed)?;
        let state = boundary
            .state
            .lock()
            .map_err(|_| HomeMutationObservationError::Unavailable)?;
        state.validate(&self.owner.registration)?;
        if state.active {
            return Err(HomeMutationObservationError::Busy);
        }
        Ok(HomeMutationObservation {
            registration: Arc::downgrade(&self.owner.registration),
            revision: state
                .revision
                .ok_or(HomeMutationObservationError::Unavailable)?,
        })
    }
}

impl Drop for ObserverOwner {
    fn drop(&mut self) {
        if let Some(boundary) = self.registration.boundary.upgrade() {
            let mut state = boundary
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if state.observer.ptr_eq(&Arc::downgrade(&self.registration)) {
                state.observer = Weak::new();
            }
        }
    }
}

impl HomeMutationObservation {
    pub fn try_elect<T>(
        &self,
        elect: impl FnOnce() -> T,
    ) -> Result<T, HomeMutationObservationError> {
        let registration = self
            .registration
            .upgrade()
            .ok_or(HomeMutationObservationError::Revoked)?;
        let boundary = registration
            .boundary
            .upgrade()
            .ok_or(HomeMutationObservationError::Closed)?;
        let state = boundary
            .state
            .lock()
            .map_err(|_| HomeMutationObservationError::Unavailable)?;
        state.validate(&registration)?;
        if state.active {
            return Err(HomeMutationObservationError::Busy);
        }
        if state
            .revision
            .ok_or(HomeMutationObservationError::Unavailable)?
            != self.revision
        {
            return Err(HomeMutationObservationError::Stale);
        }
        Ok(elect())
    }
}

impl MutationState {
    fn validate(
        &self,
        registration: &Arc<ObserverRegistration>,
    ) -> Result<(), HomeMutationObservationError> {
        if self.closed {
            return Err(HomeMutationObservationError::Closed);
        }
        if !self.observer.ptr_eq(&Arc::downgrade(registration)) {
            return Err(HomeMutationObservationError::Revoked);
        }
        Ok(())
    }
}

impl MutationBoundary {
    pub(crate) fn begin(&self) -> MutationActivity<'_> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.revision = state.revision.and_then(|revision| revision.checked_add(1));
        state.active = true;
        MutationActivity {
            boundary: self,
            settled: false,
        }
    }

    pub(crate) fn close(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.closed = true;
        state.observer = Weak::new();
    }
}

impl MutationActivity<'_> {
    fn finish(&mut self, writer: Option<MutexGuard<'_, ()>>) {
        if self.settled {
            return;
        }
        let observer = {
            let mut state = self
                .boundary
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            state.active = false;
            self.settled = true;
            drop(writer);
            state.observer.upgrade()
        };
        if let Some(observer) = observer {
            observer.wake.wake_by_ref();
        }
    }
}

impl Drop for MutationActivity<'_> {
    fn drop(&mut self) {
        self.finish(None);
    }
}

impl<'a> ObservedWriter<'a> {
    pub(crate) fn new(writer: MutexGuard<'a, ()>, boundary: &'a MutationBoundary) -> Self {
        Self {
            writer: Some(writer),
            activity: Some(boundary.begin()),
        }
    }
}

impl Drop for ObservedWriter<'_> {
    fn drop(&mut self) {
        if let Some(mut activity) = self.activity.take() {
            activity.finish(self.writer.take());
        }
    }
}
