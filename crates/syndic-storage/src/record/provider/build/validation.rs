use super::*;

pub(super) fn validate_frontier(
    kind: &'static str,
    minimum: u64,
    maximum: u64,
    actual: u64,
) -> Result<(), ProviderStorageRecordError> {
    if actual < minimum || actual > maximum {
        return Err(ProviderStorageRecordError::StagedFrontierOutOfRange {
            kind,
            minimum,
            maximum,
            actual,
        });
    }
    Ok(())
}

pub(super) fn reject_regression(
    kind: &'static str,
    previous: u64,
    actual: u64,
) -> Result<(), ProviderStorageRecordError> {
    if actual < previous {
        return Err(ProviderStorageRecordError::StagedFrontierRegression {
            kind,
            previous,
            actual,
        });
    }
    Ok(())
}
