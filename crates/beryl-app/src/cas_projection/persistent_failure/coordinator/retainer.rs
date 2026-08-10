use super::*;

impl PersistentFailureProjectionRetainer {
    pub(in crate::cas_projection) fn service_generation(&self) -> ProjectionServiceGeneration {
        self.notification.service_generation()
    }

    pub(in crate::cas_projection) fn failure_observed(&self) -> bool {
        self.notification.failure_observed()
    }

    pub(in crate::cas_projection) fn cut_identity(
        &self,
        failure_generation: PersistentFailureGeneration,
    ) -> PersistentFailureCutIdentity {
        PersistentFailureCutIdentity::new(
            self.home_id,
            self.home_generation,
            self.notification.service_generation(),
            failure_generation,
        )
    }

    pub(in crate::cas_projection) fn promotion_failure_transfer(&self) -> PromotionFailureTransfer {
        let state = Arc::clone(&self.state);
        PromotionFailureTransfer::new(
            self.cut_identity(PersistentFailureGeneration::FIRST),
            move |retained| {
                let mut state = state.0.lock().unwrap_or_else(|poison| poison.into_inner());
                state.retain_publication(RetainedPublication::Promotion(retained));
            },
        )
    }

    pub(in crate::cas_projection) fn cleanup_failure_transfer(&self) -> CleanupFailureTransfer {
        let state = Arc::clone(&self.state);
        CleanupFailureTransfer::new(
            self.cut_identity(PersistentFailureGeneration::FIRST),
            move |retained| {
                let mut state = state.0.lock().unwrap_or_else(|poison| poison.into_inner());
                state.retain_publication(RetainedPublication::Cleanup(retained));
            },
        )
    }

    pub(in crate::cas_projection) fn retain_promotion(
        &self,
        reservation: ConnectionPromotionReservation,
    ) {
        debug_assert!(self.failure_observed());
        let _ = reservation.retain_for_persistent_failure();
    }

    pub(in crate::cas_projection) fn retain(&self, projection: LoadedCasProjection) {
        debug_assert!(self.failure_observed());
        self.retain_from_exact_settlement(projection);
    }

    pub(in crate::cas_projection) fn retain_from_exact_settlement(
        &self,
        projection: LoadedCasProjection,
    ) {
        debug_assert_eq!(projection.home_id(), self.home_id);
        debug_assert_eq!(projection.home_generation(), self.home_generation);
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.retain_publication(RetainedPublication::Projection(projection));
    }

    pub(in crate::cas_projection) fn retain_target(&self, projection: LoadedCasProjection) {
        debug_assert!(self.failure_observed());
        debug_assert_eq!(projection.home_id(), self.home_id);
        debug_assert_eq!(projection.home_generation(), self.home_generation);
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.retain_publication(RetainedPublication::TargetProjection(projection));
    }

    pub(in crate::cas_projection) fn retain_reacquisition_anchor(
        &self,
        anchor: SameNativeReacquisitionAnchor,
    ) {
        debug_assert!(self.failure_observed());
        self.retain_reacquisition_anchor_from_exact_settlement(anchor);
    }

    pub(in crate::cas_projection) fn retain_reacquisition_anchor_from_exact_settlement(
        &self,
        anchor: SameNativeReacquisitionAnchor,
    ) {
        debug_assert_eq!(anchor.home_id(), self.home_id);
        debug_assert_eq!(anchor.home_generation(), self.home_generation);
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.retain_publication(RetainedPublication::ReacquisitionAnchor(anchor));
    }

    pub(in crate::cas_projection) fn retain_raw_loaded_lease(
        &self,
        lease: FailureRetainedRawLoadedLease,
    ) {
        let identity = lease.identity();
        debug_assert_eq!(identity.home_id, self.home_id);
        debug_assert_eq!(identity.home_generation, self.home_generation);
        debug_assert_eq!(
            identity.service_generation,
            self.notification.service_generation()
        );
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.retain_publication(RetainedPublication::RawLoadedLease(lease));
    }

    pub(in crate::cas_projection) fn retain_raw_quarantined_anchor(
        &self,
        anchor: FailureRetainedRawQuarantinedAnchor,
    ) {
        let identity = anchor.identity();
        debug_assert_eq!(identity.home_id, self.home_id);
        debug_assert_eq!(identity.home_generation, self.home_generation);
        debug_assert_eq!(
            identity.service_generation,
            self.notification.service_generation()
        );
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.retain_publication(RetainedPublication::RawQuarantinedAnchor(anchor));
    }

    pub(in crate::cas_projection) fn retain_raw_reacquisition_reservation(
        &self,
        reservation: FailureRetainedRawReacquisitionReservation,
    ) {
        let identity = reservation.identity();
        debug_assert_eq!(identity.home_id, self.home_id);
        debug_assert_eq!(identity.home_generation, self.home_generation);
        debug_assert_eq!(
            identity.service_generation,
            self.notification.service_generation()
        );
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.retain_publication(RetainedPublication::RawReacquisitionReservation(
            reservation,
        ));
    }
}
