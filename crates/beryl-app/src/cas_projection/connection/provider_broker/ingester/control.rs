use super::*;

impl ProviderBrokerControl {
    pub(in crate::cas_projection) fn stop_coordinator(
        &self,
    ) -> Arc<crate::cas_projection::stop::StopCoordinator> {
        Arc::clone(&self.stop_coordinator)
    }

    pub(in crate::cas_projection) fn context_compaction_coordinator(
        &self,
    ) -> Arc<crate::cas_projection::context_compaction::ContextCompactionCoordinator> {
        Arc::clone(&self.context_compaction)
    }

    pub(in crate::cas_projection::connection) fn request_cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        #[cfg(feature = "test-faults")]
        {
            let broker_id = Arc::as_ptr(&self.cancelled) as usize;
            crate::cas_projection::test_faults::observe_approval_submit_cancellation(broker_id);
            crate::cas_projection::test_faults::observe_provider_stage_cancellation(
                crate::cas_projection::test_faults::ProviderTestKey::new(self.home_id, broker_id),
            );
        }
        self.approval.close();
        self.steering_results.close();
        self.ack.wake();
    }

    pub(in crate::cas_projection::connection) fn take_approval_interruption(
        &self,
    ) -> Option<PendingApprovalInterruption> {
        self.approval.take()
    }

    pub(in crate::cas_projection::connection) fn close_approval_interruptions(&self) {
        self.approval.close();
    }

    pub(in crate::cas_projection::connection::provider_broker) fn close_checked_steering_lifecycles_for_loss(
        &self,
    ) {
        self.steering_results.close();
    }

    pub(in crate::cas_projection::connection) fn seal_checked_steering_proven_nondispatch(&self) {
        self.steering_results.seal_armed_after_proven_nondispatch();
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn close_checked_steering_lifecycles_for_test(&self) {
        self.close_checked_steering_lifecycles_for_loss();
    }

    pub(in crate::cas_projection::connection) fn clear_approval_interruption(&self) {
        self.approval.clear_pending();
    }

    pub(in crate::cas_projection) fn arm_checked_steering_lifecycle(
        &self,
        attempt: &ActiveSteeringAttemptPermit,
        route: &SyndicDeliveringSteeringInput,
        home_generation: HomeGeneration,
        correlation: &ClientUserMessageId,
    ) -> Result<CheckedSteeringLifecycleOwner, CheckedSteeringLifecycleArmError> {
        if !attempt.matches_target(
            route.input().thread_id(),
            route.target(),
            route.loaded_generation(),
        ) || attempt.home_generation_number() != home_generation.get()
        {
            return Err(CheckedSteeringLifecycleArmError::TargetMismatch);
        }
        self.steering_results
            .arm(route.clone(), home_generation, correlation.clone())
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn arm_checked_steering_lifecycle_for_test(
        &self,
        route: &SyndicDeliveringSteeringInput,
        home_generation: HomeGeneration,
        correlation: &ClientUserMessageId,
    ) -> Result<CheckedSteeringLifecycleOwner, CheckedSteeringLifecycleArmError> {
        self.steering_results
            .arm(route.clone(), home_generation, correlation.clone())
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn take_checked_steering_lifecycle(
        &self,
    ) -> Option<CheckedSteeringLifecycle> {
        self.steering_results.take()
    }

    pub(in crate::cas_projection::connection) fn has_checked_steering_lifecycle(&self) -> bool {
        self.steering_results.has_ready()
    }

    pub(in crate::cas_projection::connection) fn wait_for_checked_steering_consumption(
        &self,
        timeout: std::time::Duration,
    ) {
        self.steering_results.wait_while_ready(timeout);
    }

    pub(in crate::cas_projection::connection) fn record_backend_failure(&self) {
        self.routing_failure
            .record(WholeConnectionRoutingFailure::Backend);
    }

    pub(in crate::cas_projection::connection) fn record_router_failure(&self) {
        self.routing_failure
            .record(WholeConnectionRoutingFailure::Router);
    }

    pub(in crate::cas_projection::connection) fn routing_failure(
        &self,
    ) -> Option<ConnectionRoutingFailure> {
        self.routing_failure.get().map(Into::into)
    }

    pub(in crate::cas_projection::connection) fn page_diagnostics(&self) -> PagePoolDiagnostics {
        self.pages.diagnostics()
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection::connection) fn test_key(
        &self,
    ) -> crate::cas_projection::test_faults::ProviderTestKey {
        crate::cas_projection::test_faults::ProviderTestKey::new(
            self.home_id,
            Arc::as_ptr(&self.cancelled) as usize,
        )
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection::connection) fn test_snapshot(
        &self,
    ) -> crate::cas_projection::test_faults::ProviderBrokerSnapshot {
        self.test_metrics.snapshot()
    }
}

impl std::fmt::Debug for ProviderBrokerControl {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderBrokerControl")
            .field("cancelled", &self.cancelled.load(Ordering::Acquire))
            .field("pages", &self.pages.diagnostics())
            .finish_non_exhaustive()
    }
}
