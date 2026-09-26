use std::sync::{MutexGuard, TryLockError};

use crate::{
    HomeGeneration, HomeHealthState, HomeMutationObservation, HomeMutationObservationError,
    HomeStore,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum HomeObservedCoherenceError {
    #[error("home mutation observation belongs to another store")]
    ForeignObservation,
    #[error(transparent)]
    Observation(#[from] HomeMutationObservationError),
    #[error(transparent)]
    Coherence(#[from] HomeCoherenceError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum HomeCoherenceError {
    #[error("home coherence is busy")]
    Busy,
    #[error("home coherence authority is unavailable")]
    Unavailable,
    #[error("home coherence authority is closed")]
    Closed,
    #[error("home reconciliation custody remains")]
    ReconciliationPending,
    #[error("home is not healthy: {0:?}")]
    Unhealthy(HomeHealthState),
    #[error("home coherence belongs to another generation")]
    StaleGeneration,
}

impl HomeStore {
    pub fn try_elect_observed_coherent<T>(
        &self,
        observation: &HomeMutationObservation,
        expected_generation: HomeGeneration,
        elect: impl FnOnce() -> T,
    ) -> Result<T, HomeObservedCoherenceError> {
        observation.try_elect_for(&self.mutation_boundary, || {
            self.reconciliation
                .try_elect_vacant(|| self.health.try_elect_healthy(expected_generation, elect))
                .map_err(Into::into)
        })
    }

    pub fn try_elect_coherent<T>(
        &self,
        expected_generation: HomeGeneration,
        elect: impl FnOnce() -> T,
    ) -> Result<T, HomeCoherenceError> {
        self.mutation_boundary.try_elect_quiescent(|| {
            self.reconciliation
                .try_elect_vacant(|| self.health.try_elect_healthy(expected_generation, elect))
        })
    }
}

impl<T> From<TryLockError<MutexGuard<'_, T>>> for HomeCoherenceError {
    fn from(error: TryLockError<MutexGuard<'_, T>>) -> Self {
        match error {
            TryLockError::WouldBlock => Self::Busy,
            TryLockError::Poisoned(_) => Self::Unavailable,
        }
    }
}
