use std::sync::{Arc, Mutex, Weak};

use crate::cas_projection::runtime_work::RuntimeWorkError;

#[derive(Clone, Debug)]
pub(in crate::cas_projection) struct ConnectionWorkBoundary {
    inner: Arc<Boundary>,
}

#[derive(Debug)]
struct Boundary {
    state: Mutex<State>,
    generated_wake: std::sync::OnceLock<std::task::Waker>,
    generated_capacity_waiter: std::sync::atomic::AtomicBool,
}

#[derive(Debug)]
struct State {
    revision: Option<u64>,
    active: usize,
    closed: bool,
}

#[derive(Clone, Debug)]
pub(in crate::cas_projection) struct ConnectionWorkObservation {
    owner: Weak<Boundary>,
    revision: u64,
}

#[derive(Debug)]
#[must_use]
pub(in crate::cas_projection) struct ConnectionWorkMutation {
    owner: Option<Arc<Boundary>>,
    notify_generated: bool,
}

impl beryl_backend::ResponseWorkMutation for ConnectionWorkMutation {}

impl beryl_backend::ResponseWorkMutationObserver for ConnectionWorkBoundary {
    fn begin_change(&self) -> Box<dyn beryl_backend::ResponseWorkMutation + '_> {
        Box::new(ConnectionWorkBoundary::begin_change(self))
    }
}

impl ConnectionWorkBoundary {
    pub(in crate::cas_projection) fn new() -> Self {
        Self {
            inner: Arc::new(Boundary {
                generated_wake: std::sync::OnceLock::new(),
                generated_capacity_waiter: std::sync::atomic::AtomicBool::new(false),
                state: Mutex::new(State {
                    revision: Some(0),
                    active: 0,
                    closed: false,
                }),
            }),
        }
    }

    pub(in crate::cas_projection) fn install_generated_waker(&self, wake: std::task::Waker) {
        assert!(self.inner.generated_wake.set(wake).is_ok(), "one generated observer per connection owner");
    }

    pub(in crate::cas_projection) fn await_generated_capacity(&self) {
        self.inner.generated_capacity_waiter.store(true, std::sync::atomic::Ordering::Release);
    }

    pub(in crate::cas_projection) fn wake_generated_capacity(&self) {
        if self.inner.generated_capacity_waiter.swap(false, std::sync::atomic::Ordering::AcqRel)
            && let Some(wake) = self.inner.generated_wake.get()
        { wake.wake_by_ref(); }
    }

    pub(in crate::cas_projection) fn begin_change(&self) -> ConnectionWorkMutation {
        self.begin_observed_change(true)
    }

    pub(in crate::cas_projection) fn begin_quiet_change(&self) -> ConnectionWorkMutation {
        self.begin_observed_change(false)
    }

    fn begin_observed_change(&self, notify_generated: bool) -> ConnectionWorkMutation {
        let mut state = match self.inner.state.lock() {
            Ok(state) => state,
            Err(poison) => {
                poison.into_inner().revision = None;
                return ConnectionWorkMutation { owner: None, notify_generated };
            }
        };
        if state.closed {
            return ConnectionWorkMutation { owner: None, notify_generated };
        }
        let next = state
            .revision
            .and_then(|revision| revision.checked_add(1))
            .zip(state.active.checked_add(1));
        let Some((revision, active)) = next else {
            state.revision = None;
            return ConnectionWorkMutation { owner: None, notify_generated };
        };
        state.revision = Some(revision);
        state.active = active;
        ConnectionWorkMutation {
            owner: Some(Arc::clone(&self.inner)),
            notify_generated,
        }
    }

    pub(in crate::cas_projection) fn try_observe(
        &self,
    ) -> Result<ConnectionWorkObservation, RuntimeWorkError> {
        let state = self.inner.state.try_lock()?;
        let revision = state.observable_revision()?;
        Ok(ConnectionWorkObservation {
            owner: Arc::downgrade(&self.inner),
            revision,
        })
    }

    pub(in crate::cas_projection) fn try_elect<T>(
        &self,
        observation: &ConnectionWorkObservation,
        publish: impl FnOnce() -> T,
    ) -> Result<T, RuntimeWorkError> {
        if !Weak::ptr_eq(&observation.owner, &Arc::downgrade(&self.inner)) {
            return Err(RuntimeWorkError::Foreign);
        }
        let state = self.inner.state.try_lock()?;
        if state.observable_revision()? != observation.revision {
            return Err(RuntimeWorkError::Stale);
        }
        let result = publish();
        drop(state);
        Ok(result)
    }

    pub(in crate::cas_projection) fn close(&self) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.closed = true;
    }

    pub(in crate::cas_projection) fn invalidate(&self) {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .revision = None;
    }
}

impl State {
    fn observable_revision(&self) -> Result<u64, RuntimeWorkError> {
        if self.closed {
            return Err(RuntimeWorkError::Closed);
        }
        let revision = self.revision.ok_or(RuntimeWorkError::Unavailable)?;
        if self.active != 0 {
            return Err(RuntimeWorkError::Busy);
        }
        Ok(revision)
    }
}

impl Drop for ConnectionWorkMutation {
    fn drop(&mut self) {
        let Some(owner) = self.owner.take() else {
            return;
        };
        let mut state = owner.state.lock().unwrap_or_else(|poison| {
            let mut state = poison.into_inner();
            state.revision = None;
            state
        });
        match state.active.checked_sub(1) {
            Some(active) => state.active = active,
            None => state.revision = None,
        }
        if std::thread::panicking() {
            state.revision = None;
        }
        drop(state);
        let capacity_waiting = owner.generated_capacity_waiter.swap(false, std::sync::atomic::Ordering::AcqRel);
        if (self.notify_generated || capacity_waiting) && let Some(wake) = owner.generated_wake.get() {
            wake.wake_by_ref();
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/connection_work_observation.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/generated_source_wakes.rs"]
mod generated_source_wake_tests;
