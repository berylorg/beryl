use std::sync::{Arc, Condvar, Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ServiceStartupState {
    Closed,
    Open,
    Cancelled,
}

/// Shared one-way startup fence for a never-published replacement service.
pub(super) struct ServiceStartupGate {
    state: Mutex<ServiceStartupState>,
    changed: Condvar,
}

pub(in crate::cas_projection) struct ServiceStartupPublicationGuard<'a> {
    gate: &'a ServiceStartupGate,
    state: Option<std::sync::MutexGuard<'a, ServiceStartupState>>,
}

#[must_use = "opening a startup gate must wake its blocked replacement workers after publication locks are released"]
pub(in crate::cas_projection) struct ServiceStartupWake<'a> {
    gate: &'a ServiceStartupGate,
}

impl ServiceStartupGate {
    pub(super) fn open_gate() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(ServiceStartupState::Open),
            changed: Condvar::new(),
        })
    }

    pub(super) fn closed_gate() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(ServiceStartupState::Closed),
            changed: Condvar::new(),
        })
    }

    pub(super) fn wait(&self) -> bool {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return false,
        };
        while *state == ServiceStartupState::Closed {
            state = match self.changed.wait(state) {
                Ok(state) => state,
                Err(_) => return false,
            };
        }
        *state == ServiceStartupState::Open
    }

    pub(super) fn open(&self) -> bool {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return false,
        };
        if *state != ServiceStartupState::Closed {
            return false;
        }
        *state = ServiceStartupState::Open;
        drop(state);
        self.changed.notify_all();
        true
    }

    pub(super) fn cancel(&self) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poison) => poison.into_inner(),
        };
        if *state == ServiceStartupState::Closed {
            *state = ServiceStartupState::Cancelled;
        }
        drop(state);
        self.changed.notify_all();
    }

    pub(super) fn is_closed(&self) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| *state == ServiceStartupState::Closed)
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn is_open_for_test(&self) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| *state == ServiceStartupState::Open)
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn is_cancelled_for_test(&self) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| *state == ServiceStartupState::Cancelled)
    }

    pub(in crate::cas_projection) fn lock_for_publication(
        &self,
    ) -> Result<ServiceStartupPublicationGuard<'_>, ()> {
        let state = self.state.lock().map_err(|_| ())?;
        if *state != ServiceStartupState::Closed {
            return Err(());
        }
        Ok(ServiceStartupPublicationGuard {
            gate: self,
            state: Some(state),
        })
    }
}

impl<'a> ServiceStartupPublicationGuard<'a> {
    /// Publishes the one-way open state without waking a worker under the caller's final
    /// publication locks.
    pub(in crate::cas_projection) fn open_deferred(mut self) -> ServiceStartupWake<'a> {
        let mut state = self
            .state
            .take()
            .expect("a startup publication guard opens its exact gate once");
        debug_assert_eq!(*state, ServiceStartupState::Closed);
        *state = ServiceStartupState::Open;
        drop(state);
        ServiceStartupWake { gate: self.gate }
    }
}

impl ServiceStartupWake<'_> {
    pub(in crate::cas_projection) fn wake(self) {
        self.gate.changed.notify_all();
    }
}
