use std::sync::{Arc, Condvar, Mutex};

#[derive(Clone, Copy, PartialEq, Eq)]
enum InitialStartState {
    Waiting,
    Released,
    Cancelled,
}

pub(super) struct InitialStartGate {
    state: Mutex<InitialStartState>,
    changed: Condvar,
}

pub(super) struct InitialStartOwner {
    gate: Arc<InitialStartGate>,
}

impl InitialStartOwner {
    pub(super) fn new() -> Self {
        Self {
            gate: InitialStartGate::with_state(InitialStartState::Waiting),
        }
    }

    pub(super) fn gate(&self) -> Arc<InitialStartGate> {
        Arc::clone(&self.gate)
    }

    pub(super) fn release(self) -> bool {
        self.gate.finish(InitialStartState::Released)
    }
}

impl Drop for InitialStartOwner {
    fn drop(&mut self) {
        self.gate.cancel();
    }
}

impl InitialStartGate {
    fn with_state(state: InitialStartState) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(state),
            changed: Condvar::new(),
        })
    }

    pub(super) fn ready() -> Arc<Self> {
        Self::with_state(InitialStartState::Released)
    }

    pub(super) fn wait(&self) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        while *state == InitialStartState::Waiting {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|poison| poison.into_inner());
        }
        *state == InitialStartState::Released
    }

    pub(super) fn cancel(&self) {
        self.finish(InitialStartState::Cancelled);
    }

    fn finish(&self, terminal: InitialStartState) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if *state != InitialStartState::Waiting {
            return false;
        }
        *state = terminal;
        drop(state);
        self.changed.notify_all();
        true
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/initial_start.rs"
    ));
}
