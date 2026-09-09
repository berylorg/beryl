use std::{
    ops::{Deref, DerefMut},
    sync::{LockResult, Mutex, MutexGuard, PoisonError},
};

use super::StopCoordinatorState;

pub(super) struct StopState {
    inner: Mutex<VersionedState>,
}

struct VersionedState {
    revision: Option<u64>,
    value: StopCoordinatorState,
}

pub(super) struct StopStateGuard<'a> {
    inner: MutexGuard<'a, VersionedState>,
}

impl StopState {
    pub(super) fn new(value: StopCoordinatorState) -> Self {
        Self {
            inner: Mutex::new(VersionedState {
                revision: Some(0),
                value,
            }),
        }
    }

    pub(super) fn lock(&self) -> LockResult<StopStateGuard<'_>> {
        match self.inner.lock() {
            Ok(inner) => Ok(StopStateGuard { inner }),
            Err(poison) => Err(PoisonError::new(StopStateGuard {
                inner: poison.into_inner(),
            })),
        }
    }
}

impl StopStateGuard<'_> {
    pub(super) fn revision(&self) -> Option<u64> {
        self.inner.revision
    }
    pub(super) fn invalidate_revision(&mut self) {
        self.inner.revision = None;
    }
}

impl Deref for StopStateGuard<'_> {
    type Target = StopCoordinatorState;
    fn deref(&self) -> &Self::Target {
        &self.inner.value
    }
}

impl DerefMut for StopStateGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.revision = self.inner.revision.and_then(|value| value.checked_add(1));
        &mut self.inner.value
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/stop_work_revision.rs"
    ));
}
