use beryl_model::SyndicThreadId;

use super::{LocalDispatchState, StopCoordinationError, StopCoordinator};
use crate::cas_projection::persistent_failure::PersistentFailureCutIdentity;

/// Frozen local evidence about whether a prior durable primary interrupt may have dispatched.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum PersistentFailureStopEvidence {
    NoLocalStop,
    AdmittedNotClaimed,
    ClaimedNotDispatched,
    ClaimUnresolved,
    Dispatching,
    HardStopRunning,
    ProvenNondispatchSettling,
    PrimaryAccepted,
    PossiblyDispatched,
    DurablyAbandoned,
}

impl PersistentFailureStopEvidence {
    pub(in crate::cas_projection) const fn permits_volatile_interrupt(self) -> bool {
        matches!(
            self,
            Self::NoLocalStop | Self::AdmittedNotClaimed | Self::ClaimedNotDispatched
        )
    }
}

impl StopCoordinator {
    pub(in crate::cas_projection) fn freeze_for_persistent_failure(
        &self,
        identity: PersistentFailureCutIdentity,
    ) -> Result<(), StopCoordinationError> {
        if self.home_id != identity.home_id || self.home_generation != identity.home_generation {
            return Err(StopCoordinationError::HomeAuthorityLost);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        if let Some(existing) = state.persistent_failure {
            return if existing == identity {
                Ok(())
            } else {
                Err(StopCoordinationError::LocalAuthorityMismatch)
            };
        }
        for local in state.stops.values_mut() {
            if matches!(
                local.dispatch,
                LocalDispatchState::AdmittedNotClaimed | LocalDispatchState::ClaimedNotDispatched
            ) {
                local.dispatch = LocalDispatchState::FailureFrozenNondispatch;
            }
        }
        let commands = state.hard.freeze_for_persistent_failure();
        state.persistent_failure = Some(identity);
        drop(state);
        drop(commands);
        self.hard_wake.notify_all();
        Ok(())
    }

    pub(in crate::cas_projection) fn persistent_failure_evidence(
        &self,
        identity: PersistentFailureCutIdentity,
        thread_id: SyndicThreadId,
    ) -> Result<PersistentFailureStopEvidence, StopCoordinationError> {
        let state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        if state.persistent_failure != Some(identity) {
            return Err(StopCoordinationError::LocalAuthorityMismatch);
        }
        let Some(local) = state.stops.get(&thread_id) else {
            return Ok(PersistentFailureStopEvidence::NoLocalStop);
        };
        Ok(match local.dispatch {
            LocalDispatchState::AdmittedNotClaimed => {
                PersistentFailureStopEvidence::AdmittedNotClaimed
            }
            LocalDispatchState::ClaimUnresolved => PersistentFailureStopEvidence::ClaimUnresolved,
            LocalDispatchState::ClaimedNotDispatched
            | LocalDispatchState::FailureFrozenNondispatch => {
                if local.attempt.is_some() {
                    PersistentFailureStopEvidence::ClaimedNotDispatched
                } else {
                    PersistentFailureStopEvidence::AdmittedNotClaimed
                }
            }
            LocalDispatchState::Dispatching => PersistentFailureStopEvidence::Dispatching,
            LocalDispatchState::HardStopRunningProvenNondispatch => {
                PersistentFailureStopEvidence::HardStopRunning
            }
            LocalDispatchState::ProvenNondispatchSettling => {
                PersistentFailureStopEvidence::ProvenNondispatchSettling
            }
            LocalDispatchState::PrimaryAccepted => PersistentFailureStopEvidence::PrimaryAccepted,
            LocalDispatchState::PossiblyDispatched => {
                PersistentFailureStopEvidence::PossiblyDispatched
            }
            LocalDispatchState::DurablyAbandoned => PersistentFailureStopEvidence::DurablyAbandoned,
        })
    }

    pub(super) fn failure_cut_is_active(&self) -> bool {
        self.commands.failure_observed()
            || self
                .state
                .lock()
                .map(|state| state.persistent_failure.is_some())
                .unwrap_or(true)
    }
}
