use std::{num::NonZeroU64, sync::Arc};

use super::{AdapterFailureClass, AppearanceGeneration};

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

/// An adapter-owned publication whose commit cannot report failure.
///
/// Implementations perform every fallible operation during `prepare`. The
/// coordinator calls `commit` only after every adapter in the captured epoch
/// has accepted the exact same immutable generation.
pub trait PreparedWindowAppearance: Send {
    fn commit(self: Box<Self>);
}

/// Pure pre-GUI boundary implemented later by window-local presentation code.
pub trait AppearanceWindowAdapter: Send + Sync {
    fn id(&self) -> WindowAdapterId;

    fn prepare(
        &self,
        generation: Arc<AppearanceGeneration>,
    ) -> Result<Box<dyn PreparedWindowAppearance>, AdapterFailureClass>;
}
