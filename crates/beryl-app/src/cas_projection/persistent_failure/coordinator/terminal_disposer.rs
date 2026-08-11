use super::*;

impl PersistentFailureTerminalDisposer {
    pub(in crate::cas_projection) fn service_generation(&self) -> ProjectionServiceGeneration {
        self.notification.service_generation()
    }

    pub(in crate::cas_projection) fn failure_observed(&self) -> bool {
        self.notification.failure_observed()
    }

    pub(in crate::cas_projection) fn dispose_loaded(&self, projection: LoadedCasProjection) {
        debug_assert_eq!(projection.home_id(), self.home_id);
        debug_assert_eq!(projection.home_generation(), self.home_generation);
        projection
            .into_terminal_loaded_lease_disposition_owner()
            .dispose_local();
        self.record_disposed_projection();
    }

    pub(in crate::cas_projection) fn dispose_target(&self, projection: LoadedCasProjection) {
        debug_assert_eq!(projection.home_id(), self.home_id);
        debug_assert_eq!(projection.home_generation(), self.home_generation);
        drop(projection.into_local_registry_disposition_owner());
        self.record_disposed_projection();
    }

    fn record_disposed_projection(&self) {
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.disposed_projection_count = state.disposed_projection_count.saturating_add(1);
    }
}
