use std::sync::{Arc, Mutex, Weak, mpsc};

use beryl_home_store::{HomeGeneration, HomeHealthState, HomeStore};
use beryl_model::BerylHomeId;

use super::{
    ProjectionServiceGeneration,
    gate::{FailureObservationElection, GateInner},
};

/// Closed result of one nonblocking persistent-failure health observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentFailureNotificationStatus {
    /// Exact failed health was offered to the dedicated terminal workers.
    Signaled,
    /// The exact signal joined an already pending or executing terminal cut.
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
    disposal_signal: Arc<Mutex<Option<mpsc::SyncSender<()>>>>,
    gate: Arc<GateInner>,
}

impl PersistentFailureNotification {
    /// Re-reads typed store health and coalesces only exact persistent failure.
    #[must_use]
    pub fn notify(&self) -> PersistentFailureNotificationStatus {
        let Some(home) = self.home.upgrade() else {
            return PersistentFailureNotificationStatus::Unavailable;
        };
        let health = home.health();
        if home.home_id() != self.home_id
            || health.generation() != Some(self.home_generation)
            || health.state() != HomeHealthState::Failed
        {
            return PersistentFailureNotificationStatus::NotFailed;
        }
        match self.gate.observe_failure_with_completion(|| Ok(())) {
            Ok(FailureObservationElection::First) => {}
            Ok(FailureObservationElection::Joined) => {
                return PersistentFailureNotificationStatus::Joined;
            }
            Ok(FailureObservationElection::OrdinaryShutdown) | Err(_) => {
                return PersistentFailureNotificationStatus::Unavailable;
            }
        }
        if let Ok(disposal) = self.disposal_signal.lock()
            && let Some(disposal) = disposal.as_ref()
        {
            let _ = disposal.try_send(());
        }
        match self.signal.try_send(()) {
            Ok(()) => PersistentFailureNotificationStatus::Signaled,
            Err(mpsc::TrySendError::Full(())) => PersistentFailureNotificationStatus::Joined,
            Err(mpsc::TrySendError::Disconnected(())) => {
                PersistentFailureNotificationStatus::Unavailable
            }
        }
    }

    pub(in crate::cas_projection) fn attach_terminal_disposer(
        &self,
        signal: mpsc::SyncSender<()>,
    ) -> Result<(), ()> {
        let mut disposal = self.disposal_signal.lock().map_err(|_| ())?;
        if disposal.is_some() {
            return Err(());
        }
        *disposal = Some(signal);
        drop(disposal);
        let _ = self.notify();
        Ok(())
    }

    #[must_use]
    pub fn service_generation(&self) -> ProjectionServiceGeneration {
        self.gate.service_generation()
    }

    pub(in crate::cas_projection::persistent_failure) fn unavailable_allows_command_drain(
        &self,
    ) -> bool {
        self.gate.ordinary_shutdown_elected()
    }

    pub(in crate::cas_projection::persistent_failure) fn wake_worker(&self) {
        let _ = self.signal.try_send(());
    }

    pub(in crate::cas_projection::persistent_failure) fn mark_cut_elected(&self) {
        self.gate.mark_cut_elected();
    }

    pub(in crate::cas_projection::persistent_failure) fn failure_observed(&self) -> bool {
        self.gate.failure_observed()
    }

    pub(in crate::cas_projection::persistent_failure) fn gate_inner(&self) -> Arc<GateInner> {
        Arc::clone(&self.gate)
    }
}

pub(in crate::cas_projection) fn persistent_failure_notification_channel(
    home: &Arc<HomeStore>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
) -> (PersistentFailureNotification, mpsc::Receiver<()>) {
    let (signal, receiver) = mpsc::sync_channel(1);
    (
        PersistentFailureNotification {
            home: Arc::downgrade(home),
            home_id,
            home_generation,
            signal,
            disposal_signal: Arc::new(Mutex::new(None)),
            gate: GateInner::new(service_generation),
        },
        receiver,
    )
}

#[cfg(all(test, feature = "test-faults"))]
pub(super) mod test_support;
