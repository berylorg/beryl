use super::*;

pub(super) fn add(left: u64, right: u64) -> Result<u64, SyndicValidationError> {
    left.checked_add(right)
        .ok_or(SyndicValidationError::Invariant(
            "accepted-route validation aggregate overflowed",
        ))
}

pub(super) fn invariant<T>(message: &'static str) -> Result<T, SyndicValidationError> {
    Err(SyndicValidationError::Invariant(message))
}
