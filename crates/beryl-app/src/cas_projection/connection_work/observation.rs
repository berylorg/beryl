use std::sync::{Arc, Mutex, Weak};

use crate::cas_projection::runtime_work::RuntimeWorkError;

#[derive(Clone, Debug)]
pub(in crate::cas_projection) struct ConnectionWorkBoundary {
    inner: Arc<Boundary>,
}

#[derive(Debug)]
struct Boundary {
    state: Mutex<State>,
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
                state: Mutex::new(State {
                    revision: Some(0),
                    active: 0,
                    closed: false,
                }),
            }),
        }
    }

    pub(in crate::cas_projection) fn begin_change(&self) -> ConnectionWorkMutation {
        let mut state = match self.inner.state.lock() {
            Ok(state) => state,
            Err(poison) => {
                poison.into_inner().revision = None;
                return ConnectionWorkMutation { owner: None };
            }
        };
        if state.closed {
            return ConnectionWorkMutation { owner: None };
        }
        let next = state
            .revision
            .and_then(|revision| revision.checked_add(1))
            .zip(state.active.checked_add(1));
        let Some((revision, active)) = next else {
            state.revision = None;
            return ConnectionWorkMutation { owner: None };
        };
        state.revision = Some(revision);
        state.active = active;
        ConnectionWorkMutation {
            owner: Some(Arc::clone(&self.inner)),
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
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/connection_work_observation.rs"]
mod tests;
