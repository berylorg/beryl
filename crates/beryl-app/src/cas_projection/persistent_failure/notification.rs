use std::sync::{Arc, Condvar, Mutex, Weak, mpsc};

#[cfg(all(test, feature = "test-faults"))]
use std::{collections::HashMap, sync::LazyLock};

use beryl_home_store::{HomeGeneration, HomeHealthState, HomeStore};
use beryl_model::BerylHomeId;

use super::{
    ProjectionServiceGeneration,
    gate::{FailureObservationElection, GateInner, LiveCommandAdmissionError},
};

mod flight;
pub(in crate::cas_projection) use flight::persistent_failure_notification_channel;

/// Exact completion published by the sole running-session recovery supervisor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum RecoverySupervisorFlightCompletion {
    /// Verification kept this exact home and service generation current.
    VerifiedCurrent,
    /// Verification failed or the registered service epoch became stale.
    FailedOrStale,
    /// Shutdown or unavailable supervisor authority ended the flight.
    ShutdownOrUnavailable,
}

#[derive(Clone, Debug)]
pub(super) enum VerificationJoinDisposition {
    Waiting(Arc<VerificationCompletionCell>),
    NotVerification,
    AuthorityLost,
}

/// Closed result of one nonblocking persistent-failure health observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentFailureNotificationStatus {
    /// Exact verifying health was offered to the process recovery supervisor.
    VerificationSignaled,
    /// The exact verification signal joined an already pending or executing recovery flight.
    VerificationJoined,
    /// Exact failed health was offered to the dedicated one-shot worker.
    Signaled,
    /// The exact signal joined an already pending or executing cut.
    Joined,
    /// Typed health did not establish failure of this exact home generation.
    NotFailed,
    /// The retained home or one-shot worker is no longer available.
    Unavailable,
}

/// Cloneable, nonblocking notification handle for exact typed home failure.
#[derive(Clone, Debug)]
pub struct PersistentFailureNotification {
    home: Weak<HomeStore>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    signal: mpsc::SyncSender<()>,
    recovery_flight: Arc<RecoverySupervisorFlight>,
    gate: Arc<GateInner>,
}

#[derive(Debug)]
struct RecoverySupervisorFlight {
    state: Mutex<RecoverySupervisorFlightState>,
}

#[derive(Debug)]
struct RecoverySupervisorFlightState {
    signal: Option<mpsc::SyncSender<()>>,
    active: Option<Arc<VerificationCompletionCell>>,
    next: Option<Arc<VerificationCompletionCell>>,
    last_issued_epoch: u64,
    followup_requested: bool,
    terminal_completion: Option<RecoverySupervisorFlightCompletion>,
}

#[cfg(all(test, feature = "test-faults"))]
#[derive(Debug)]
struct VerificationJoinObservationHook {
    observed: mpsc::SyncSender<()>,
    resume: mpsc::Receiver<()>,
}

#[cfg(all(test, feature = "test-faults"))]
static VERIFICATION_JOIN_OBSERVATION_HOOKS: LazyLock<
    Mutex<HashMap<usize, VerificationJoinObservationHook>>,
> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug)]
pub(super) struct VerificationCompletionCell {
    epoch: u64,
    outcome: Mutex<Option<RecoverySupervisorFlightCompletion>>,
    completed: Condvar,
}

/// Exact immutable completion captured by the supervisor before it wakes scheduler lanes.
#[derive(Debug)]
pub(in crate::cas_projection) struct CompletedRecoverySupervisorFlight {
    cell: Arc<VerificationCompletionCell>,
}

impl PersistentFailureNotification {
    #[cfg(all(test, feature = "test-faults"))]
    fn pause_verification_join_after_prelock_observation(
        &self,
        ticket: &Arc<VerificationCompletionCell>,
    ) -> Result<(), LiveCommandAdmissionError> {
        let hook = VERIFICATION_JOIN_OBSERVATION_HOOKS
            .lock()
            .map_err(|_| LiveCommandAdmissionError::Unavailable)?
            .remove(&(Arc::as_ptr(ticket) as usize));
        let Some(hook) = hook else {
            return Ok(());
        };
        // This snapshot models the former pre-lock observation. The actual classification below
        // must re-observe after acquiring recovery-flight state.
        let _ = ticket.outcome()?;
        hook.observed
            .send(())
            .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
        hook.resume
            .recv()
            .map_err(|_| LiveCommandAdmissionError::Unavailable)
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(super) fn install_verification_join_observation_hook(
        &self,
        ticket: &Arc<VerificationCompletionCell>,
        observed: mpsc::SyncSender<()>,
        resume: mpsc::Receiver<()>,
    ) {
        VERIFICATION_JOIN_OBSERVATION_HOOKS
            .lock()
            .expect("verification join observation hook lock")
            .insert(
                Arc::as_ptr(ticket) as usize,
                VerificationJoinObservationHook { observed, resume },
            );
    }

    /// Re-reads typed store health and coalesces only exact persistent failure.
    #[must_use]
    pub fn notify(&self) -> PersistentFailureNotificationStatus {
        let Some(home) = self.home.upgrade() else {
            return PersistentFailureNotificationStatus::Unavailable;
        };
        let health = home.health();
        if home.home_id() != self.home_id || health.generation() != Some(self.home_generation) {
            return PersistentFailureNotificationStatus::NotFailed;
        }
        if health.state() == HomeHealthState::Verifying {
            return match self.signal_recovery_supervisor() {
                Some(true) => PersistentFailureNotificationStatus::VerificationSignaled,
                Some(false) => PersistentFailureNotificationStatus::VerificationJoined,
                None => PersistentFailureNotificationStatus::NotFailed,
            };
        }
        if health.state() != HomeHealthState::Failed {
            return PersistentFailureNotificationStatus::NotFailed;
        }
        let mut recovery_signal = None;
        match self.gate.observe_failure_with_completion(|| {
            recovery_signal = self.publish_terminal_recovery_supervisor_completion(
                RecoverySupervisorFlightCompletion::FailedOrStale,
            )?;
            Ok(())
        }) {
            Ok(FailureObservationElection::First) => {}
            Ok(FailureObservationElection::Joined) => {
                return PersistentFailureNotificationStatus::Joined;
            }
            Ok(FailureObservationElection::OrdinaryShutdown) | Err(_) => {
                return PersistentFailureNotificationStatus::Unavailable;
            }
        }
        if let Some(recovery_signal) = recovery_signal {
            let _ = recovery_signal.try_send(());
        }
        match self.signal.try_send(()) {
            Ok(()) => PersistentFailureNotificationStatus::Signaled,
            Err(mpsc::TrySendError::Full(())) => PersistentFailureNotificationStatus::Joined,
            Err(mpsc::TrySendError::Disconnected(())) => {
                PersistentFailureNotificationStatus::Unavailable
            }
        }
    }

    fn signal_recovery_supervisor(&self) -> Option<bool> {
        let mut flight = self.recovery_flight.state.lock().ok()?;
        Self::request_recovery_supervisor_locked(&mut flight)
            .ok()
            .map(|(signaled, _cell)| signaled)
    }

    pub(super) fn unavailable_allows_command_drain(&self) -> bool {
        self.gate.ordinary_shutdown_elected()
            && self.recovery_flight.state.lock().is_ok_and(|flight| {
                flight.terminal_completion
                    == Some(RecoverySupervisorFlightCompletion::ShutdownOrUnavailable)
            })
    }

    fn request_recovery_supervisor_locked(
        flight: &mut RecoverySupervisorFlightState,
    ) -> Result<(bool, Arc<VerificationCompletionCell>), LiveCommandAdmissionError> {
        if let Some(completion) = flight.terminal_completion {
            return Err(Self::completion_authority_error(completion));
        }
        if let Some(active) = flight.active.as_ref().cloned() {
            if active.outcome()?.is_none() {
                return Ok((false, active));
            }
            let next = Self::next_completion_cell_locked(flight)?;
            debug_assert!(next.epoch > active.epoch);
            flight.followup_requested = true;
            return Ok((false, next));
        }
        let next = Self::take_next_completion_cell_locked(flight)?;
        let signal = flight
            .signal
            .as_ref()
            .ok_or(LiveCommandAdmissionError::Unavailable)?
            .clone();
        match signal.try_send(()) {
            Ok(()) => {
                flight.active = Some(Arc::clone(&next));
                Ok((true, next))
            }
            Err(mpsc::TrySendError::Full(())) => {
                flight.active = Some(Arc::clone(&next));
                Ok((false, next))
            }
            Err(mpsc::TrySendError::Disconnected(())) => {
                flight.signal = None;
                let _ = next.publish(RecoverySupervisorFlightCompletion::ShutdownOrUnavailable);
                Err(LiveCommandAdmissionError::Unavailable)
            }
        }
    }

    fn next_completion_cell_locked(
        flight: &mut RecoverySupervisorFlightState,
    ) -> Result<Arc<VerificationCompletionCell>, LiveCommandAdmissionError> {
        if let Some(next) = flight.next.as_ref() {
            debug_assert_eq!(next.epoch, flight.last_issued_epoch);
            return Ok(Arc::clone(next));
        }
        flight.last_issued_epoch = flight
            .last_issued_epoch
            .checked_add(1)
            .ok_or(LiveCommandAdmissionError::Unavailable)?;
        let next = VerificationCompletionCell::new(flight.last_issued_epoch);
        flight.next = Some(Arc::clone(&next));
        Ok(next)
    }

    fn take_next_completion_cell_locked(
        flight: &mut RecoverySupervisorFlightState,
    ) -> Result<Arc<VerificationCompletionCell>, LiveCommandAdmissionError> {
        let next = Self::next_completion_cell_locked(flight)?;
        flight.next = None;
        Ok(next)
    }

    pub(super) fn verification_completion_ticket(
        &self,
        home: &HomeStore,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        service_generation: ProjectionServiceGeneration,
    ) -> Result<(Arc<VerificationCompletionCell>, bool), LiveCommandAdmissionError> {
        if !self.matches_exact_epoch(home, home_id, home_generation, service_generation) {
            return Err(LiveCommandAdmissionError::Closed);
        }
        let mut flight = self
            .recovery_flight
            .state
            .lock()
            .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
        if let Some(completion) = flight.terminal_completion {
            return Err(Self::completion_authority_error(completion));
        }
        let health = home.health();
        if health.generation() != Some(home_generation)
            || !matches!(
                health.state(),
                HomeHealthState::Healthy | HomeHealthState::Verifying
            )
        {
            return Err(LiveCommandAdmissionError::Closed);
        }
        if health.state() == HomeHealthState::Verifying {
            if let Some(active) = flight.active.as_ref() {
                if active.outcome()?.is_none() {
                    return Ok((Arc::clone(active), true));
                }
            }
        }
        Self::next_completion_cell_locked(&mut flight)
            .map(|ticket| (ticket, health.state() == HomeHealthState::Verifying))
    }

    pub(super) fn register_verification_join(
        &self,
        home: &HomeStore,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        service_generation: ProjectionServiceGeneration,
        ticket: &Arc<VerificationCompletionCell>,
    ) -> Result<VerificationJoinDisposition, LiveCommandAdmissionError> {
        if !self.matches_exact_epoch(home, home_id, home_generation, service_generation) {
            return Ok(VerificationJoinDisposition::AuthorityLost);
        }
        #[cfg(all(test, feature = "test-faults"))]
        self.pause_verification_join_after_prelock_observation(ticket)?;
        let mut flight = self
            .recovery_flight
            .state
            .lock()
            .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
        // Completion publishes while holding this same flight-state lock. Checking the immutable
        // ticket outcome here makes that completion authoritative even after active retirement.
        if ticket.outcome()?.is_some() {
            return Ok(VerificationJoinDisposition::Waiting(Arc::clone(ticket)));
        }
        if flight.terminal_completion.is_some() {
            return Ok(VerificationJoinDisposition::AuthorityLost);
        }
        let health = home.health();
        if health.generation() != Some(home_generation) {
            return Ok(VerificationJoinDisposition::AuthorityLost);
        }
        let is_active = flight
            .active
            .as_ref()
            .is_some_and(|active| Arc::ptr_eq(active, ticket));
        let is_next = flight
            .next
            .as_ref()
            .is_some_and(|next| Arc::ptr_eq(next, ticket));
        if health.state() == HomeHealthState::Healthy {
            return Ok(if is_active {
                VerificationJoinDisposition::Waiting(Arc::clone(ticket))
            } else {
                VerificationJoinDisposition::NotVerification
            });
        }
        if !matches!(
            health.state(),
            HomeHealthState::Verifying | HomeHealthState::Failed
        ) {
            return Ok(VerificationJoinDisposition::AuthorityLost);
        }

        let failed = health.state() == HomeHealthState::Failed;
        let disposition = if is_active {
            Ok(VerificationJoinDisposition::Waiting(Arc::clone(ticket)))
        } else if is_next {
            if flight.active.is_some() {
                flight.followup_requested = true;
                Ok(VerificationJoinDisposition::Waiting(Arc::clone(ticket)))
            } else {
                Self::request_recovery_supervisor_locked(&mut flight).map(
                    |(_signaled, activated)| {
                        debug_assert!(Arc::ptr_eq(&activated, ticket));
                        VerificationJoinDisposition::Waiting(activated)
                    },
                )
            }
        } else {
            Ok(VerificationJoinDisposition::AuthorityLost)
        };
        drop(flight);
        if failed {
            let _ = self.notify();
        }
        disposition
    }

    pub(super) fn wait_for_verification_completion(
        &self,
        target: &Arc<VerificationCompletionCell>,
    ) -> Result<RecoverySupervisorFlightCompletion, LiveCommandAdmissionError> {
        target.wait()
    }

    fn publish_terminal_recovery_supervisor_completion(
        &self,
        completion: RecoverySupervisorFlightCompletion,
    ) -> Result<Option<mpsc::SyncSender<()>>, LiveCommandAdmissionError> {
        let mut flight = self
            .recovery_flight
            .state
            .lock()
            .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
        if flight.terminal_completion.is_some() {
            return Ok(None);
        }
        debug_assert_ne!(
            completion,
            RecoverySupervisorFlightCompletion::VerifiedCurrent
        );
        flight.terminal_completion = Some(completion);
        let recovery_signal = flight.signal.take();
        flight.followup_requested = false;
        let active = flight.active.as_ref().cloned();
        let next = flight.next.take();
        if let Some(active) = active {
            let _ = active.publish(completion)?;
        }
        if let Some(next) = next {
            let _ = next.publish(completion)?;
        }
        Ok(recovery_signal)
    }

    pub(in crate::cas_projection) fn publish_verified_current_completion(
        &self,
    ) -> Result<Option<CompletedRecoverySupervisorFlight>, LiveCommandAdmissionError> {
        let flight = self
            .recovery_flight
            .state
            .lock()
            .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
        if flight.terminal_completion.is_some() {
            return Ok(None);
        }
        let Some(active) = flight.active.as_ref().cloned() else {
            return Ok(None);
        };
        if !active.publish(RecoverySupervisorFlightCompletion::VerifiedCurrent)? {
            return Ok(None);
        }
        Ok(Some(CompletedRecoverySupervisorFlight { cell: active }))
    }

    pub(in crate::cas_projection) fn elect_and_publish_stale_completion(
        &self,
    ) -> Result<(), LiveCommandAdmissionError> {
        match self.gate.observe_failure_with_completion(|| {
            self.publish_terminal_recovery_supervisor_completion(
                RecoverySupervisorFlightCompletion::FailedOrStale,
            )
            .map(|_| ())
        })? {
            FailureObservationElection::First => match self.signal.try_send(()) {
                Ok(()) | Err(mpsc::TrySendError::Full(())) => {}
                Err(mpsc::TrySendError::Disconnected(())) => {
                    return Err(LiveCommandAdmissionError::Unavailable);
                }
            },
            FailureObservationElection::Joined => {}
            FailureObservationElection::OrdinaryShutdown => {
                return Err(LiveCommandAdmissionError::Closed);
            }
        }
        Ok(())
    }

    pub(in crate::cas_projection) fn publish_shutdown_completion(
        &self,
    ) -> Result<(), LiveCommandAdmissionError> {
        self.publish_terminal_recovery_supervisor_completion(
            RecoverySupervisorFlightCompletion::ShutdownOrUnavailable,
        )
        .map(|_| ())
    }

    /// Completes one supervisor flight and preserves a wake if the same home generation became
    /// unhealthy again before the executing flight reached this boundary.
    pub(in crate::cas_projection) fn finish_completed_recovery_supervisor_flight(
        &self,
        completed: Option<CompletedRecoverySupervisorFlight>,
        requeue_unhealthy: bool,
    ) {
        let Ok(mut flight) = self.recovery_flight.state.lock() else {
            return;
        };
        if let Some(completed) = completed.as_ref()
            && !flight
                .active
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, &completed.cell))
        {
            return;
        }
        let Some(active) = flight.active.take() else {
            if !requeue_unhealthy {
                flight.terminal_completion =
                    Some(RecoverySupervisorFlightCompletion::ShutdownOrUnavailable);
                flight.signal = None;
                if let Some(next) = flight.next.take() {
                    let _ = next.publish(RecoverySupervisorFlightCompletion::ShutdownOrUnavailable);
                }
            }
            return;
        };
        if active.outcome().ok().flatten().is_none() {
            let _ = active.publish(RecoverySupervisorFlightCompletion::ShutdownOrUnavailable);
            flight.terminal_completion =
                Some(RecoverySupervisorFlightCompletion::ShutdownOrUnavailable);
            flight.signal = None;
        }
        if let Some(terminal) = flight.terminal_completion {
            if let Some(next) = flight.next.take() {
                let _ = next.publish(terminal);
            }
            return;
        }
        if !requeue_unhealthy {
            flight.terminal_completion =
                Some(RecoverySupervisorFlightCompletion::ShutdownOrUnavailable);
            flight.signal = None;
            if let Some(next) = flight.next.take() {
                let _ = next.publish(RecoverySupervisorFlightCompletion::ShutdownOrUnavailable);
            }
            return;
        }
        let remains_unhealthy = requeue_unhealthy
            && self.home.upgrade().is_some_and(|home| {
                let health = home.health();
                home.home_id() == self.home_id
                    && health.generation() == Some(self.home_generation)
                    && matches!(
                        health.state(),
                        HomeHealthState::Verifying | HomeHealthState::Failed
                    )
            });
        let followup_requested = std::mem::take(&mut flight.followup_requested);
        if !remains_unhealthy {
            if followup_requested {
                flight.terminal_completion =
                    Some(RecoverySupervisorFlightCompletion::ShutdownOrUnavailable);
                flight.signal = None;
                if let Some(next) = flight.next.take() {
                    let _ = next.publish(RecoverySupervisorFlightCompletion::ShutdownOrUnavailable);
                }
            }
            return;
        }
        let _ = Self::request_recovery_supervisor_locked(&mut flight);
    }

    #[cfg(test)]
    fn finish_recovery_supervisor_flight(&self, requeue_unhealthy: bool) {
        self.finish_completed_recovery_supervisor_flight(None, requeue_unhealthy);
    }
}

#[cfg(all(test, feature = "test-faults"))]
pub(super) mod test_support;

#[cfg(all(test, feature = "test-faults"))]
mod tests;
