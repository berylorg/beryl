use std::{
    ops::Deref,
    sync::{Arc, Condvar, Mutex},
};

use beryl_home_store::HomeGeneration;
use beryl_state::BerylState;

use super::{ServiceAvailability, TerminalServiceDiagnostics};
use crate::cas_projection::{
    PersistentFailureNotification, ProjectionConnectionService, ProjectionServiceGeneration,
};

pub(in crate::cas_projection) struct RunningServiceOwner {
    pub(super) service: Arc<ProjectionConnectionService>,
    pub(super) state: BerylState,
}

pub(in crate::cas_projection) struct RunningServiceSlot {
    state: Mutex<RunningServiceSlotState>,
    leases_changed: Condvar,
}

struct RunningServiceSlotState {
    current: Option<RunningServiceOwner>,
    active_leases: usize,
    disposing: bool,
    shutting_down: bool,
    terminal_failures: u64,
    terminal_settled: bool,
}

/// Non-cloneable scoped access to the exact currently published projection service and Beryl
/// handles. Dropping the lease permits persistent-failure withdrawal to consume the service.
pub(super) struct RunningServiceLease {
    service: Option<Arc<ProjectionConnectionService>>,
    state: BerylState,
    slot: Arc<RunningServiceSlot>,
}

impl RunningServiceSlot {
    pub(super) fn new(service: ProjectionConnectionService, state: BerylState) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(RunningServiceSlotState {
                current: Some(RunningServiceOwner {
                    service: Arc::new(service),
                    state,
                }),
                active_leases: 0,
                disposing: false,
                shutting_down: false,
                terminal_failures: 0,
                terminal_settled: false,
            }),
            leases_changed: Condvar::new(),
        })
    }

    pub(super) fn acquire(self: &Arc<Self>) -> Result<RunningServiceLease, ServiceAvailability> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ServiceAvailability::Unavailable)?;
        if state.terminal_failures != 0 {
            return Err(ServiceAvailability::Unavailable);
        }
        if state.shutting_down {
            return Err(ServiceAvailability::ShuttingDown);
        }
        let current = state.current.as_ref().ok_or_else(|| {
            if state.terminal_failures != 0 {
                ServiceAvailability::Unavailable
            } else {
                ServiceAvailability::Disposing
            }
        })?;
        let service = Arc::clone(&current.service);
        let beryl_state = current.state.clone();
        state.active_leases = state
            .active_leases
            .checked_add(1)
            .ok_or(ServiceAvailability::Unavailable)?;
        Ok(RunningServiceLease {
            service: Some(service),
            state: beryl_state,
            slot: Arc::clone(self),
        })
    }

    pub(super) fn current_notification(
        &self,
    ) -> Result<
        (
            HomeGeneration,
            ProjectionServiceGeneration,
            PersistentFailureNotification,
        ),
        (),
    > {
        let state = self.state.lock().map_err(|_| ())?;
        let current = state.current.as_ref().ok_or(())?;
        Ok((
            current.service.home_generation(),
            current.service.service_generation(),
            current.service.persistent_failure_notification(),
        ))
    }

    pub(super) fn withdraw(
        &self,
        expected: ProjectionServiceGeneration,
    ) -> Result<RunningServiceOwner, ()> {
        let mut state = self.state.lock().map_err(|_| ())?;
        let current = state.current.as_ref().ok_or(())?;
        if current.service.service_generation() != expected {
            return Err(());
        }
        state.disposing = true;
        state.current.take().ok_or(())
    }

    pub(super) fn wait_until_unleased(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        while state.active_leases != 0 {
            state = self
                .leases_changed
                .wait(state)
                .unwrap_or_else(|poison| poison.into_inner());
        }
    }

    pub(super) fn mark_terminal(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.terminal_failures = state.terminal_failures.saturating_add(1);
            state.disposing = false;
        }
    }

    pub(super) fn mark_terminal_settled(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.terminal_settled = true;
        }
    }

    pub(super) fn begin_shutdown(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.shutting_down = true;
    }

    pub(super) fn is_shutting_down(&self) -> bool {
        self.state.lock().map_or(true, |state| state.shutting_down)
    }

    pub(super) fn take_for_shutdown(&self) -> Option<RunningServiceOwner> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.current.take()
    }

    pub(super) fn diagnostics(&self) -> TerminalServiceDiagnostics {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let current = state.current.as_ref();
        TerminalServiceDiagnostics {
            current_home_generation: current.map(|owner| owner.service.home_generation()),
            current_service_generation: current.map(|owner| owner.service.service_generation()),
            active_service_leases: state.active_leases,
            disposing: state.disposing,
            shutting_down: state.shutting_down,
            terminal_failures: state.terminal_failures,
            terminal_settled: state.terminal_settled,
        }
    }
}

impl RunningServiceOwner {
    pub(in crate::cas_projection) fn into_parts(
        self,
    ) -> Result<(ProjectionConnectionService, BerylState), RunningServiceOwner> {
        let Self { service, state } = self;
        match Arc::try_unwrap(service) {
            Ok(service) => Ok((service, state)),
            Err(service) => Err(RunningServiceOwner { service, state }),
        }
    }
}

impl RunningServiceLease {
    /// Returns the exact complete Beryl handle set owned with this service.
    #[must_use]
    pub fn state(&self) -> BerylState {
        self.state.clone()
    }
}

impl Deref for RunningServiceLease {
    type Target = ProjectionConnectionService;

    fn deref(&self) -> &Self::Target {
        self.service
            .as_deref()
            .expect("a live running-service lease retains its service owner")
    }
}

impl Drop for RunningServiceLease {
    fn drop(&mut self) {
        // The slot count authorizes exact-owner disposal, so the Arc must leave first. Struct
        // fields otherwise drop only after this method returns, allowing a zero-count wake to race
        // Arc::try_unwrap on the still-live service owner.
        drop(self.service.take());
        let mut state = self
            .slot
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.active_leases = state
            .active_leases
            .checked_sub(1)
            .expect("a running-service lease is released exactly once");
        if state.active_leases == 0 {
            self.slot.leases_changed.notify_all();
        }
    }
}
