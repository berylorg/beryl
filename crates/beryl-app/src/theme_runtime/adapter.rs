use std::{num::NonZeroU64, sync::Arc};

use super::{AdapterFailureClass, AppearanceGeneration, StalePublicationReason, WindowSetEpoch};

/// Stable process-local identity of one eligible window adapter.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WindowAdapterId(NonZeroU64);

impl WindowAdapterId {
    #[must_use]
    pub const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> NonZeroU64 {
        self.0
    }
}

#[derive(Clone)]
pub struct AppearanceWindowSetSnapshot {
    pub epoch: WindowSetEpoch,
    pub count: usize,
    pub capacity: usize,
    pub current: Arc<AppearanceGeneration>,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppearancePublicationFailure {
    Unavailable,
    Reentrant,
    CapacityReached,
    Stale(StalePublicationReason),
    Adapter {
        adapter: WindowAdapterId,
        class: AdapterFailureClass,
    },
}

pub trait AppearancePublicationTarget: Send + Sync {
    fn snapshot(&self) -> AppearanceWindowSetSnapshot;

    fn publish(
        &self,
        epoch: WindowSetEpoch,
        previous: Arc<AppearanceGeneration>,
        generation: Arc<AppearanceGeneration>,
    ) -> Result<(), AppearancePublicationFailure>;

    fn is_publication_thread(&self) -> bool;

    fn retire(&self);
}
