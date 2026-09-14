use std::sync::{MutexGuard, TryLockError};

use crate::{HomeGeneration, HomeHealthState, HomeStore};

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
