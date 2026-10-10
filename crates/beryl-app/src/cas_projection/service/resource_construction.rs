use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, Ordering},
};

use beryl_home_store::{CommandCancellation, CommandOutcome};
use syndic_storage::indexed_resource as index;

use super::ProjectionConnectionService;

#[derive(Clone)]
pub(crate) struct ResourceConstructionAuthority {
    home: Weak<beryl_home_store::HomeServiceReference>,
    storage: syndic_storage::SyndicStorage,
    workers: super::ProjectionWorkerPool,
    authorizer: super::LiveCommandAuthorizer,
}

#[must_use]
pub(crate) enum ResourceConstructionStep {
    Command(CommandOutcome),
    PeerProgress(CommandOutcome),
    Terminal(index::BuildStatus),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ResourceConstructionError {
    #[error("resource construction was cancelled")]
    Cancelled,
    #[error("resource construction has no ordinary worker capacity")]
    Busy,
    #[error("resource construction admission failed: {0}")]
    Admission(#[from] crate::cas_projection::LiveCommandAdmissionError),
    #[error("resource construction is unavailable")]
    Unavailable,
    #[error("resource construction read or preparation failed: {0}")]
    Indexed(#[from] index::IndexedResourceError),
}

impl ProjectionConnectionService {
    pub(crate) fn resource_construction_authority(&self) -> ResourceConstructionAuthority {
        ResourceConstructionAuthority {
            home: self.home.as_ref().map_or_else(Weak::new, Arc::downgrade),
            storage: self.storage.clone(),
            workers: self.workers.clone(),
            authorizer: self.command_authorizer.clone(),
        }
    }

    pub(crate) fn step_resource_construction(
        &self,
        key: index::BuildKey,
        cancelled: &AtomicBool,
    ) -> Result<ResourceConstructionStep, ResourceConstructionError> {
        self.resource_construction_authority().step(key, cancelled)
    }
}

impl ResourceConstructionAuthority {
    pub(crate) fn step(
        &self,
        key: index::BuildKey,
        cancelled: &AtomicBool,
    ) -> Result<ResourceConstructionStep, ResourceConstructionError> {
        self.step_with_observation(key, cancelled, || ())
    }

    #[cfg(test)]
    pub(crate) fn step_after_observation(
        &self,
        key: index::BuildKey,
        cancelled: &AtomicBool,
        observed: impl FnOnce(),
    ) -> Result<ResourceConstructionStep, ResourceConstructionError> {
        self.step_with_observation(key, cancelled, observed)
    }

    fn step_with_observation(
        &self,
        key: index::BuildKey,
        cancelled: &AtomicBool,
        observed: impl FnOnce(),
    ) -> Result<ResourceConstructionStep, ResourceConstructionError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(ResourceConstructionError::Cancelled);
        }
        let _admitted = self.authorizer.authorize()?;
        let home = self
            .home
            .upgrade()
            .ok_or(ResourceConstructionError::Unavailable)?;
        let _worker = self
            .workers
            .try_acquire_ordinary()
            .map_err(|error| match error {
                super::ProjectionWorkerPermitError::CapacityFull { .. } => {
                    ResourceConstructionError::Busy
                }
                super::ProjectionWorkerPermitError::Poisoned => {
                    ResourceConstructionError::Unavailable
                }
            })?;
        let frozen = home
            .capture_frozen_read(&CommandCancellation::new())
            .map_err(|_| ResourceConstructionError::Unavailable)?;
        let state = self
            .storage
            .read_frozen_indexed_resource_build(&home, &frozen, key);
        home.release_frozen_read(&frozen)
            .map_err(|_| ResourceConstructionError::Unavailable)?;
        let state = state?;
        let revision = state.as_ref().map(index::BuildState::revision);
        observed();
        if cancelled.load(Ordering::Acquire) {
            return Err(ResourceConstructionError::Cancelled);
        }
        let command = match state {
            None => self.storage.current_start_indexed_resource_build(key),
            Some(state) if state.status() == index::BuildStatus::Building => self
                .storage
                .current_advance_indexed_resource_build(state, 65_536, 64)?,
            Some(state) => return Ok(ResourceConstructionStep::Terminal(state.status())),
        };
        if cancelled.load(Ordering::Acquire) {
            return Err(ResourceConstructionError::Cancelled);
        }
        let outcome = home.execute_current(command);
        if matches!(outcome, CommandOutcome::NotCommitted { .. })
            && !cancelled.load(Ordering::Acquire)
            && self.peer_progress(&home, key, revision).unwrap_or(false)
        {
            return Ok(ResourceConstructionStep::PeerProgress(outcome));
        }
        Ok(ResourceConstructionStep::Command(outcome))
    }

    fn peer_progress(
        &self,
        home: &beryl_home_store::HomeServiceReference,
        key: index::BuildKey,
        previous: Option<u64>,
    ) -> Result<bool, ResourceConstructionError> {
        let frozen = home
            .capture_frozen_read(&CommandCancellation::new())
            .map_err(|_| ResourceConstructionError::Unavailable)?;
        let state = self
            .storage
            .read_frozen_indexed_resource_build(home, &frozen, key);
        home.release_frozen_read(&frozen)
            .map_err(|_| ResourceConstructionError::Unavailable)?;
        Ok(state?.is_some_and(|state| {
            state.status() == index::BuildStatus::Building
                && previous.is_none_or(|revision| state.revision() > revision)
        }))
    }
}
