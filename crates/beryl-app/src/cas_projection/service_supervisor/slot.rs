use std::{
    ops::Deref,
    sync::{Arc, Condvar, Mutex, Weak},
};

use beryl_home_store::{HomeGeneration, HomeHealthState, HomeStore};
use beryl_state::BerylState;

use super::{RunningServiceAvailability, RunningSessionRecoveryDiagnostics};
use crate::cas_projection::{
    PersistentFailureNotification, ProjectionConnectionService, ProjectionServiceGeneration,
    persistent_failure::CompletedRecoverySupervisorFlight,
    service_startup::{ServiceStartupPublicationGuard, ServiceStartupWake},
};

pub(in crate::cas_projection) struct PublishedServiceEpoch {
    pub(super) service: Arc<ProjectionConnectionService>,
    pub(super) state: BerylState,
    home: Weak<HomeStore>,
}

pub(in crate::cas_projection) struct RunningServiceSlot {
    home: Weak<HomeStore>,
    state: Mutex<RunningServiceSlotState>,
    leases_changed: Condvar,
}

#[must_use = "the installed epoch must release the process slot before waking replacement workers"]
pub(in crate::cas_projection) struct InstalledRecoveredServiceEpoch<'slot, 'startup> {
    state: Option<std::sync::MutexGuard<'slot, RunningServiceSlotState>>,
    wake: Option<ServiceStartupWake<'startup>>,
}

struct RunningServiceSlotState {
    current: Option<PublishedServiceEpoch>,
    active_leases: usize,
    recovering: bool,
    shutting_down: bool,
    recovery_cycles: u64,
    verification_successes: u64,
    terminal_failures: u64,
}

/// Non-cloneable scoped access to the exact currently published projection service and Beryl
/// handles. Dropping the lease permits persistent-failure withdrawal to consume the service.
pub struct RunningProjectionServiceLease {
    service: Option<Arc<ProjectionConnectionService>>,
    state: BerylState,
    slot: Arc<RunningServiceSlot>,
}

impl RunningServiceSlot {
    pub(super) fn new(service: ProjectionConnectionService, state: BerylState) -> Arc<Self> {
        let home = service.retained_home_for_recovery();
        Arc::new(Self {
            home: Arc::downgrade(&home),
            state: Mutex::new(RunningServiceSlotState {
                current: Some(PublishedServiceEpoch {
                    service: Arc::new(service),
                    state,
                    home: Arc::downgrade(&home),
                }),
                active_leases: 0,
                recovering: false,
                shutting_down: false,
                recovery_cycles: 0,
                verification_successes: 0,
                terminal_failures: 0,
            }),
            leases_changed: Condvar::new(),
        })
    }

    pub(super) fn acquire(
        self: &Arc<Self>,
    ) -> Result<RunningProjectionServiceLease, RunningServiceAvailability> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| RunningServiceAvailability::Unavailable)?;
        if state.shutting_down {
            return Err(RunningServiceAvailability::ShuttingDown);
        }
        let current = state
            .current
            .as_ref()
            .ok_or(RunningServiceAvailability::Recovering)?;
        let service = Arc::clone(&current.service);
        let beryl_state = current.state;
        state.active_leases = state
            .active_leases
            .checked_add(1)
            .ok_or(RunningServiceAvailability::Unavailable)?;
        Ok(RunningProjectionServiceLease {
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

    /// Recovers a poisoned slot only to clone the notification retained by its last published
    /// service. This does not certify that the epoch is current and must only be used to publish a
    /// conservative terminal disposition before the service is drained.
    pub(super) fn notification_for_disposition(&self) -> Option<PersistentFailureNotification> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state
            .current
            .as_ref()
            .map(|current| current.service.persistent_failure_notification())
    }

    pub(super) fn withdraw(
        &self,
        expected: ProjectionServiceGeneration,
    ) -> Result<PublishedServiceEpoch, ()> {
        let mut state = self.state.lock().map_err(|_| ())?;
        let current = state.current.as_ref().ok_or(())?;
        if current.service.service_generation() != expected {
            return Err(());
        }
        state.recovering = true;
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

    pub(in crate::cas_projection) fn prepare_epoch(
        service: ProjectionConnectionService,
        state: BerylState,
    ) -> PublishedServiceEpoch {
        let home = Arc::downgrade(&service.retained_home_for_recovery());
        PublishedServiceEpoch {
            service: Arc::new(service),
            state,
            home,
        }
    }

    pub(in crate::cas_projection) fn install_recovered<'slot, 'startup>(
        &'slot self,
        epoch: PublishedServiceEpoch,
        startup: ServiceStartupPublicationGuard<'startup>,
    ) -> Result<
        InstalledRecoveredServiceEpoch<'slot, 'startup>,
        (
            PublishedServiceEpoch,
            ServiceStartupPublicationGuard<'startup>,
        ),
    > {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return Err((epoch, startup)),
        };
        if state.shutting_down
            || !state.recovering
            || state.current.is_some()
            || state.active_leases != 0
            || !Weak::ptr_eq(&self.home, &epoch.home)
        {
            return Err((epoch, startup));
        }
        state.current = Some(epoch);
        state.recovering = false;
        state.recovery_cycles = state.recovery_cycles.saturating_add(1);
        let wake = startup.open_deferred();
        Ok(InstalledRecoveredServiceEpoch {
            state: Some(state),
            wake: Some(wake),
        })
    }

    /// Completes verification only for the pointer-current service and retained home generation.
    /// Provider waiters are completed before the exact scheduler wake while the process
    /// publication slot remains locked, so neither signal can authorize a replacement epoch.
    pub(super) fn complete_same_generation_verification(
        &self,
        home: &Arc<HomeStore>,
        expected_home_generation: HomeGeneration,
        expected_service_generation: ProjectionServiceGeneration,
        flight_notification: &PersistentFailureNotification,
    ) -> Option<CompletedRecoverySupervisorFlight> {
        let observed_home = Arc::downgrade(home);
        let Ok(mut state) = self.state.lock() else {
            let _ = flight_notification.publish_shutdown_completion();
            return None;
        };
        let Some(current) = state.current.as_ref() else {
            let _ = if state.shutting_down {
                flight_notification.publish_shutdown_completion()
            } else {
                flight_notification.elect_and_publish_stale_completion()
            };
            return None;
        };
        let health = home.health();
        if state.shutting_down
            || state.recovering
            || !Weak::ptr_eq(&self.home, &observed_home)
            || !Weak::ptr_eq(&current.home, &observed_home)
            || current.service.home_id() != home.home_id()
            || current.service.home_generation() != expected_home_generation
            || current.service.service_generation() != expected_service_generation
            || health.state() != HomeHealthState::Healthy
            || health.generation() != Some(expected_home_generation)
        {
            let _ = if state.shutting_down {
                flight_notification.publish_shutdown_completion()
            } else {
                flight_notification.elect_and_publish_stale_completion()
            };
            return None;
        }
        let notification = current.service.persistent_failure_notification();
        let completed = notification
            .publish_verified_current_completion()
            .ok()
            .flatten()?;
        state.verification_successes = state.verification_successes.saturating_add(1);
        state
            .current
            .as_ref()
            .expect("the validated current service remains installed under the slot lock")
            .service
            .wake_same_generation_verified();
        Some(completed)
    }

    pub(super) fn mark_terminal(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.terminal_failures = state.terminal_failures.saturating_add(1);
            state.recovering = false;
        }
    }

    pub(super) fn begin_shutdown(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(current) = state.current.as_ref() {
            let _ = current
                .service
                .persistent_failure_notification()
                .publish_shutdown_completion();
        }
        state.shutting_down = true;
    }

    pub(super) fn is_shutting_down(&self) -> bool {
        self.state.lock().map_or(true, |state| state.shutting_down)
    }

    pub(super) fn take_for_shutdown(&self) -> Option<PublishedServiceEpoch> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.current.take()
    }

    pub(super) fn diagnostics(&self) -> RunningSessionRecoveryDiagnostics {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let current = state.current.as_ref();
        RunningSessionRecoveryDiagnostics {
            current_home_generation: current.map(|epoch| epoch.service.home_generation()),
            current_service_generation: current.map(|epoch| epoch.service.service_generation()),
            active_service_leases: state.active_leases,
            recovering: state.recovering,
            shutting_down: state.shutting_down,
            recovery_cycles: state.recovery_cycles,
            verification_successes: state.verification_successes,
            terminal_failures: state.terminal_failures,
        }
    }
}

#[cfg(test)]
impl RunningServiceSlot {
    pub(in crate::cas_projection) fn new_for_recovery_publication_test(
        service: ProjectionConnectionService,
        state: BerylState,
    ) -> Arc<Self> {
        Self::new(service, state)
    }

    pub(in crate::cas_projection) fn withdraw_for_recovery_publication_test(
        &self,
        expected: ProjectionServiceGeneration,
    ) -> Result<PublishedServiceEpoch, ()> {
        self.withdraw(expected)
    }

    pub(in crate::cas_projection) fn acquire_for_recovery_publication_test(
        self: &Arc<Self>,
    ) -> Result<RunningProjectionServiceLease, RunningServiceAvailability> {
        self.acquire()
    }

    pub(in crate::cas_projection) fn make_recovered_install_unavailable_for_test(&self) {
        let mut state = self.state.lock().unwrap();
        assert!(state.current.is_none());
        state.recovering = false;
    }

    pub(in crate::cas_projection) fn take_for_recovery_publication_test(
        &self,
    ) -> Option<PublishedServiceEpoch> {
        self.begin_shutdown();
        self.take_for_shutdown()
    }

    pub(in crate::cas_projection) fn diagnostics_for_recovery_publication_test(
        &self,
    ) -> RunningSessionRecoveryDiagnostics {
        self.diagnostics()
    }
}

impl PublishedServiceEpoch {
    pub(in crate::cas_projection) fn service(&self) -> &Arc<ProjectionConnectionService> {
        &self.service
    }

    pub(in crate::cas_projection) fn into_parts(
        self,
    ) -> Result<(ProjectionConnectionService, BerylState), PublishedServiceEpoch> {
        let Self {
            service,
            state,
            home,
        } = self;
        match Arc::try_unwrap(service) {
            Ok(service) => {
                drop(home);
                Ok((service, state))
            }
            Err(service) => Err(PublishedServiceEpoch {
                service,
                state,
                home,
            }),
        }
    }
}

impl InstalledRecoveredServiceEpoch<'_, '_> {
    /// Releases the process publication lock before issuing the deferred startup wake.
    pub(in crate::cas_projection) fn finish_after_unlock(mut self) {
        self.release_and_wake();
    }

    fn release_and_wake(&mut self) {
        drop(self.state.take());
        if let Some(wake) = self.wake.take() {
            wake.wake();
        }
    }
}

impl Drop for InstalledRecoveredServiceEpoch<'_, '_> {
    fn drop(&mut self) {
        self.release_and_wake();
    }
}

impl RunningProjectionServiceLease {
    /// Returns the exact complete Beryl handle set published with this service epoch.
    #[must_use]
    pub const fn state(&self) -> BerylState {
        self.state
    }
}

impl Deref for RunningProjectionServiceLease {
    type Target = ProjectionConnectionService;

    fn deref(&self) -> &Self::Target {
        self.service
            .as_deref()
            .expect("a live running-service lease retains its service owner")
    }
}

impl Drop for RunningProjectionServiceLease {
    fn drop(&mut self) {
        // The slot count authorizes exact-owner recovery, so the Arc must leave first. Struct
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
