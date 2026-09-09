use std::sync::{Arc, Weak};

use super::{ProjectionWorkerAdmission, ProjectionWorkerPermitPair};

#[derive(Debug)]
pub(in crate::cas_projection) struct ConnectionWorkerRetentionSource {
    driver: Weak<ProjectionWorkerAdmission>,
    ingester: Weak<ProjectionWorkerAdmission>,
}

pub(in crate::cas_projection) struct ConnectionWorkerRetention {
    driver: Arc<ProjectionWorkerAdmission>,
    ingester: Arc<ProjectionWorkerAdmission>,
}

impl ProjectionWorkerPermitPair {
    pub(in crate::cas_projection) fn retention_source(&self) -> ConnectionWorkerRetentionSource {
        ConnectionWorkerRetentionSource {
            driver: Arc::downgrade(
                &self
                    .driver
                    .as_ref()
                    .expect("the unsplit pair retains its driver")
                    .admission,
            ),
            ingester: Arc::downgrade(
                &self
                    .ingester
                    .as_ref()
                    .expect("the unsplit pair retains its ingester")
                    .admission,
            ),
        }
    }
}

impl ConnectionWorkerRetentionSource {
    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection) fn retained_units_for_test(&self) -> (bool, bool) {
        (
            self.driver.strong_count() != 0,
            self.ingester.strong_count() != 0,
        )
    }

    pub(in crate::cas_projection) fn retain(&self) -> Option<ConnectionWorkerRetention> {
        Some(ConnectionWorkerRetention {
            driver: self.driver.upgrade()?,
            ingester: self.ingester.upgrade()?,
        })
    }
}

impl ConnectionWorkerRetention {
    pub(in crate::cas_projection) fn retain(&self) -> Self {
        Self {
            driver: Arc::clone(&self.driver),
            ingester: Arc::clone(&self.ingester),
        }
    }
}

#[cfg(test)]
impl std::fmt::Debug for ConnectionWorkerRetention {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConnectionWorkerRetention")
            .finish_non_exhaustive()
    }
}
