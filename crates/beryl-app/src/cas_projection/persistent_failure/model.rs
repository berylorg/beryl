use std::{
    num::NonZeroU64,
    sync::atomic::{AtomicU64, Ordering},
};

use thiserror::Error;

use beryl_home_store::HomeGeneration;
use beryl_model::BerylHomeId;

static NEXT_SERVICE_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Nonzero identity of one process-local projection-service incarnation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectionServiceGeneration(NonZeroU64);

impl ProjectionServiceGeneration {
    pub(in crate::cas_projection) fn allocate() -> Result<Self, GenerationExhausted> {
        NEXT_SERVICE_GENERATION
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .ok()
            .and_then(NonZeroU64::new)
            .map(Self)
            .ok_or(GenerationExhausted)
    }

    /// Returns the nonzero process-local generation number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Nonzero identity of one persistent-failure cut inside a service generation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PersistentFailureGeneration(NonZeroU64);

impl PersistentFailureGeneration {
    pub(in crate::cas_projection) const FIRST: Self = Self(NonZeroU64::MIN);

    /// Returns the nonzero process-local generation number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::cas_projection) struct PersistentFailureCutIdentity {
    pub(in crate::cas_projection) home_id: BerylHomeId,
    pub(in crate::cas_projection) home_generation: HomeGeneration,
    pub(in crate::cas_projection) service_generation: ProjectionServiceGeneration,
    pub(in crate::cas_projection) failure_generation: PersistentFailureGeneration,
}

impl PersistentFailureCutIdentity {
    pub(in crate::cas_projection) const fn new(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        service_generation: ProjectionServiceGeneration,
        failure_generation: PersistentFailureGeneration,
    ) -> Self {
        Self {
            home_id,
            home_generation,
            service_generation,
            failure_generation,
        }
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("the process-local projection generation is exhausted")]
pub(in crate::cas_projection) struct GenerationExhausted;
