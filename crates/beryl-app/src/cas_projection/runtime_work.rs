use std::sync::TryLockError;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum RuntimeWorkError {
    #[error("a runtime work source is busy")]
    Busy,
    #[error("a runtime work source is unavailable")]
    Unavailable,
    #[error("a runtime work source is closed")]
    Closed,
    #[error("runtime work evidence belongs to another source")]
    Foreign,
    #[error("runtime work changed")]
    Stale,
}

impl<T> From<TryLockError<T>> for RuntimeWorkError {
    fn from(error: TryLockError<T>) -> Self {
        match error {
            TryLockError::WouldBlock => Self::Busy,
            TryLockError::Poisoned(_) => Self::Unavailable,
        }
    }
}
