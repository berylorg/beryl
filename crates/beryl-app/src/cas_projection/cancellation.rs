use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Cloneable one-way cancellation signal for CAS projection work.
///
/// Cancellation is observed only at safe boundaries between synchronous
/// backend or storage operations. A synchronous backend call that has already
/// been dispatched is drained and classified; cancellation never retracts the
/// request or suppresses its outcome classification.
#[derive(Clone, Debug, Default)]
pub struct ProjectionCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl ProjectionCancellationToken {
    /// Creates an independent token whose cancellation has not been requested.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub(in crate::cas_projection) fn from_shared_flag(cancelled: Arc<AtomicBool>) -> Self {
        Self { cancelled }
    }

    /// Requests cancellation at the next safe operation boundary.
    ///
    /// This operation is idempotent and is observed by every clone.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Returns whether cancellation has been requested through any clone.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}
