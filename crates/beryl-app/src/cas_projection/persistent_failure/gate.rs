use std::sync::{Arc, Condvar, Mutex};

use beryl_home_store::{HomeGeneration, HomeHealthState, HomeStore};
use beryl_model::BerylHomeId;
use thiserror::Error;

use super::{
    PersistentFailureGeneration, PersistentFailureNotification, ProjectionServiceGeneration,
};

mod authorizer;
mod permit;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GateElection {
    Open,
    OrdinaryShutdown,
    FailureObserved,
    PersistentFailure(PersistentFailureGeneration),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum MasterCommandGateCloseOwner {
    OrdinaryShutdown,
    PersistentFailure(PersistentFailureGeneration),
}

#[derive(Debug)]
struct GateState {
    epoch: u64,
    active: usize,
    local_failure: bool,
    terminal_completion_unavailable: bool,
    election: GateElection,
}

#[derive(Debug)]
pub(super) struct GateInner {
    service_generation: ProjectionServiceGeneration,
    state: Mutex<GateState>,
    drained: Condvar,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FailureObservationElection {
    First,
    Joined,
    OrdinaryShutdown,
}

/// Cloneable process-shell authority for admitting one scoped store-dependent command.
#[derive(Clone, Debug)]
pub struct LiveCommandAuthorizer {
    inner: Arc<GateInner>,
    failure_notification: Option<PersistentFailureNotification>,
}

/// Non-cloneable scoped authorization for one exact live-command generation.
#[derive(Debug)]
pub struct LiveCommandPermit {
    inner: Arc<GateInner>,
    failure_notification: Option<PersistentFailureNotification>,
    service_generation: ProjectionServiceGeneration,
    epoch: u64,
    released: bool,
}

/// Short before/after health fence for one exact store operation.
#[must_use]
pub(in crate::cas_projection) struct LiveCommandHealthFence<'permit, 'home> {
    permit: &'permit LiveCommandPermit,
    home: &'home HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct LiveCommandHealthSettlement;

impl LiveCommandHealthSettlement {
    pub(in crate::cas_projection) const fn requires_retry(self) -> bool {
        false
    }
}

/// Why a store-dependent command could not enter the live service generation.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum LiveCommandAdmissionError {
    /// The service has invalidated all current and future live-command authority.
    #[error("the projection service no longer accepts live commands")]
    Closed,
    /// The command-gate synchronization boundary is unavailable.
    #[error("the projection live-command gate is unavailable")]
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum LiveCommandGateStatus {
    Open,
    OrdinaryShutdown,
    PersistentFailure,
    LocalFailure,
}

/// Exact command frontier sealed by one persistent-failure cut.
///
/// The value is content-free and non-authorizing. It lets the stable connection driver prove that
/// its terminal disposition names the same gate epoch that invalidated every queued old
/// command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct PersistentFailureCommandFrontier {
    service_generation: ProjectionServiceGeneration,
    failure_generation: PersistentFailureGeneration,
    gate_epoch: u64,
}

impl PersistentFailureCommandFrontier {
    pub(in crate::cas_projection) fn matches_cut(
        self,
        service_generation: ProjectionServiceGeneration,
        failure_generation: PersistentFailureGeneration,
    ) -> bool {
        self.service_generation == service_generation
            && self.failure_generation == failure_generation
            && self.gate_epoch != 0
    }
}

#[derive(Clone, Debug)]
pub(in crate::cas_projection) struct MasterCommandGate {
    inner: Arc<GateInner>,
    failure_notification: Option<PersistentFailureNotification>,
}

impl GateInner {
    pub(super) fn new(service_generation: ProjectionServiceGeneration) -> Arc<Self> {
        Arc::new(Self {
            service_generation,
            state: Mutex::new(GateState {
                epoch: 1,
                active: 0,
                local_failure: false,
                terminal_completion_unavailable: false,
                election: GateElection::Open,
            }),
            drained: Condvar::new(),
        })
    }

    pub(super) const fn service_generation(&self) -> ProjectionServiceGeneration {
        self.service_generation
    }

    fn status(
        &self,
        state: &GateState,
        permit: Option<(ProjectionServiceGeneration, u64)>,
    ) -> LiveCommandGateStatus {
        if state.local_failure {
            return LiveCommandGateStatus::LocalFailure;
        }
        match state.election {
            GateElection::Open
                if permit.is_none_or(|(service_generation, epoch)| {
                    service_generation == self.service_generation && epoch == state.epoch
                }) =>
            {
                LiveCommandGateStatus::Open
            }
            GateElection::Open => LiveCommandGateStatus::LocalFailure,
            GateElection::OrdinaryShutdown => LiveCommandGateStatus::OrdinaryShutdown,
            GateElection::FailureObserved | GateElection::PersistentFailure(_) => {
                LiveCommandGateStatus::PersistentFailure
            }
        }
    }

    fn status_exact(
        &self,
        permit: Option<(ProjectionServiceGeneration, u64)>,
    ) -> Result<LiveCommandGateStatus, LiveCommandAdmissionError> {
        self.state
            .lock()
            .map(|state| self.status(&state, permit))
            .map_err(|_| LiveCommandAdmissionError::Unavailable)
    }

    #[cfg(test)]
    pub(super) fn observe_failure(
        &self,
    ) -> Result<FailureObservationElection, LiveCommandAdmissionError> {
        self.observe_failure_with_completion(|| Ok(()))
    }

    pub(super) fn observe_failure_with_completion(
        &self,
        publish_completion: impl FnOnce() -> Result<(), LiveCommandAdmissionError>,
    ) -> Result<FailureObservationElection, LiveCommandAdmissionError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
        if state.terminal_completion_unavailable {
            return Err(LiveCommandAdmissionError::Unavailable);
        }
        let outcome = match state.election {
            GateElection::Open => {
                state.epoch = state
                    .epoch
                    .checked_add(1)
                    .ok_or(LiveCommandAdmissionError::Unavailable)?;
                state.election = GateElection::FailureObserved;
                if let Err(error) = publish_completion() {
                    state.local_failure = true;
                    state.terminal_completion_unavailable = true;
                    self.drained.notify_all();
                    return Err(error);
                }
                FailureObservationElection::First
            }
            GateElection::FailureObserved | GateElection::PersistentFailure(_) => {
                FailureObservationElection::Joined
            }
            GateElection::OrdinaryShutdown => FailureObservationElection::OrdinaryShutdown,
        };
        self.drained.notify_all();
        Ok(outcome)
    }

    pub(super) fn mark_cut_elected(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if matches!(state.election, GateElection::FailureObserved) {
            state.election = GateElection::PersistentFailure(PersistentFailureGeneration::FIRST);
        }
        self.drained.notify_all();
    }

    pub(super) fn failure_observed(&self) -> bool {
        self.state
            .lock()
            .map(|state| {
                matches!(
                    state.election,
                    GateElection::FailureObserved | GateElection::PersistentFailure(_)
                )
            })
            .unwrap_or(true)
    }

    pub(super) fn ordinary_shutdown_elected(&self) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| matches!(state.election, GateElection::OrdinaryShutdown))
    }
}

impl MasterCommandGate {
    pub(in crate::cas_projection) fn new(
        service_generation: ProjectionServiceGeneration,
        failure_notification: Option<PersistentFailureNotification>,
    ) -> Self {
        if let Some(notification) = failure_notification {
            debug_assert_eq!(notification.service_generation(), service_generation);
            return Self {
                inner: notification.gate_inner(),
                failure_notification: Some(notification),
            };
        }
        Self {
            inner: GateInner::new(service_generation),
            failure_notification: None,
        }
    }

    pub(in crate::cas_projection) fn authorizer(&self) -> LiveCommandAuthorizer {
        LiveCommandAuthorizer {
            inner: Arc::clone(&self.inner),
            failure_notification: self.failure_notification.clone(),
        }
    }

    pub(in crate::cas_projection) fn service_generation(&self) -> ProjectionServiceGeneration {
        self.inner.service_generation
    }

    pub(in crate::cas_projection) fn close_for_persistent_failure(
        &self,
        failure_generation: PersistentFailureGeneration,
    ) -> Result<bool, LiveCommandAdmissionError> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
        match state.election {
            GateElection::FailureObserved => {
                state.election = GateElection::PersistentFailure(failure_generation);
                self.inner.drained.notify_all();
                Ok(true)
            }
            GateElection::PersistentFailure(existing) => Ok(existing == failure_generation),
            GateElection::Open | GateElection::OrdinaryShutdown => Ok(false),
        }
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn elect_persistent_failure_for_test(
        &self,
        failure_generation: PersistentFailureGeneration,
    ) -> Result<bool, LiveCommandAdmissionError> {
        match self.inner.observe_failure()? {
            FailureObservationElection::First | FailureObservationElection::Joined => {
                self.close_for_persistent_failure(failure_generation)
            }
            FailureObservationElection::OrdinaryShutdown => Ok(false),
        }
    }

    pub(in crate::cas_projection) fn close_for_shutdown(&self) -> MasterCommandGateCloseOwner {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let owner = match state.election {
            GateElection::PersistentFailure(generation) => {
                MasterCommandGateCloseOwner::PersistentFailure(generation)
            }
            GateElection::FailureObserved => {
                state.election =
                    GateElection::PersistentFailure(PersistentFailureGeneration::FIRST);
                MasterCommandGateCloseOwner::PersistentFailure(PersistentFailureGeneration::FIRST)
            }
            GateElection::OrdinaryShutdown => MasterCommandGateCloseOwner::OrdinaryShutdown,
            GateElection::Open => {
                state.epoch = state.epoch.saturating_add(1);
                state.election = GateElection::OrdinaryShutdown;
                MasterCommandGateCloseOwner::OrdinaryShutdown
            }
        };
        self.inner.drained.notify_all();
        owner
    }

    pub(in crate::cas_projection) fn close_for_local_failure(&self) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if !state.local_failure {
            if matches!(state.election, GateElection::Open) {
                state.epoch = state.epoch.saturating_add(1);
            }
            state.local_failure = true;
        }
        self.inner.drained.notify_all();
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn poison_for_test(&self) {
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _state = self
                .inner
                .state
                .lock()
                .expect("command gate starts unpoisoned");
            panic!("poison live-command gate for recovery inventory test");
        }));
        assert!(panicked.is_err());
    }

    pub(in crate::cas_projection) fn wait_until_drained(
        &self,
    ) -> Result<(), LiveCommandAdmissionError> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
        while state.active != 0 {
            state = self
                .inner
                .drained
                .wait(state)
                .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
        }
        Ok(())
    }

    pub(in crate::cas_projection) fn matches_failure(
        &self,
        service_generation: ProjectionServiceGeneration,
        failure_generation: PersistentFailureGeneration,
    ) -> bool {
        if service_generation != self.inner.service_generation {
            return false;
        }
        self.inner
            .state
            .lock()
            .map(|state| state.election == GateElection::PersistentFailure(failure_generation))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests;
