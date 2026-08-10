use super::*;

/// Stable content-free identity facts for one exact projection connection.
///
/// This observation is deliberately non-authorizing. A caller that needs to keep the connection
/// alive retains its `Arc<ProjectionConnection>` separately and uses these immutable facts only
/// for exact recovery correlation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::cas_projection) struct ProjectionConnectionIdentityObservation {
    connection_generation: u64,
    runtime_id: RuntimeId,
    process_generation: CasProcessGeneration,
}

impl ProjectionConnectionIdentityObservation {
    pub(in crate::cas_projection::connection) const fn new(
        connection_generation: u64,
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
    ) -> Self {
        Self {
            connection_generation,
            runtime_id,
            process_generation,
        }
    }

    pub(in crate::cas_projection) const fn connection_generation(self) -> u64 {
        self.connection_generation
    }

    pub(in crate::cas_projection) const fn runtime_id(self) -> RuntimeId {
        self.runtime_id
    }

    pub(in crate::cas_projection) const fn process_generation(self) -> CasProcessGeneration {
        self.process_generation
    }
}

pub(in crate::cas_projection) struct ProjectionConnection {
    pub(super) authority: Arc<ConnectionRegistryAuthority>,
    runtime_id: RuntimeId,
    process_generation: CasProcessGeneration,
    process_fact: StableConnectionProcessFact,
    forwarding_hub: Arc<ForwardingHub>,
    ordinary_shutdown: Mutex<OrdinaryShutdownSettlement>,
    runtime: Mutex<Option<ConnectionRuntime>>,
    provider_pages: Mutex<beryl_stream::PagePoolDiagnostics>,
    recovery_diagnostics: Arc<recovery_source_broker::RecoveryReplayDiagnosticsSlot>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OrdinaryShutdownSettlement {
    Unsettled,
    Clean,
    Failed,
}

pub(super) struct ConnectionRuntime {
    pub(super) driver: ConnectionDriver,
}

impl std::fmt::Debug for ProjectionConnection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectionConnection")
            .field("runtime_id", &self.runtime_id)
            .field("process_generation", &self.process_generation)
            .field("retired", &self.authority.is_retired())
            .finish_non_exhaustive()
    }
}

impl ProjectionConnection {
    pub(in crate::cas_projection::connection) fn process_fact_observation(
        &self,
    ) -> super::router::ProcessEventObservation {
        self.process_fact.observe()
    }

    pub(in crate::cas_projection::connection) fn park_stable_driver_for_adoption(
        &self,
        cut: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    ) -> Result<super::driver::ParkedDriver, super::driver::DriverParkError> {
        let runtime = self.runtime.lock().map_err(|_| {
            super::driver::DriverParkError::new(
                super::driver::DriverParkErrorReason::CoordinationPoisoned,
            )
        })?;
        let runtime = runtime.as_ref().ok_or_else(|| {
            super::driver::DriverParkError::new(super::driver::DriverParkErrorReason::DriverStopped)
        })?;
        runtime.driver.park_for_adoption(cut)
    }

    pub(in crate::cas_projection::connection) fn disable_stable_driver_for_adoption(
        &self,
        cut: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    ) {
        let runtime = self
            .runtime
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(runtime) = runtime.as_ref() {
            runtime.driver.disable_for_adoption_failure(cut);
        }
    }

    pub(in crate::cas_projection) fn dispose_inert_driver_after_adoption_failure(
        &self,
    ) -> Result<(), ProjectionCoordinatorError> {
        let runtime = self
            .runtime
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take();
        if let Some(runtime) = runtime {
            runtime.driver.dispose_inert_after_adoption_failure()?;
        }
        Ok(())
    }

    pub(in crate::cas_projection::connection) fn lock_forwarding_epoch_for_adoption(
        &self,
    ) -> Result<super::forwarding_hub::ForwardingHubEpochGuard<'_>, ProjectionCoordinatorError>
    {
        self.forwarding_hub.lock_epoch()
    }

    pub(in crate::cas_projection::connection) fn detach_forwarding_epoch_for_inert_adoption(
        &self,
    ) -> Option<ForwardingEpochEndpoint> {
        self.forwarding_hub.detach_inert_recovering_poison()
    }

    pub(in crate::cas_projection::connection) fn mark_forwarding_epoch_inert_in_place_for_adoption_failure(
        &self,
    ) {
        if let Some(epoch) = self.forwarding_hub.mark_inert_in_place_recovering_poison() {
            epoch.request_ingester_cancel();
        }
    }

    #[cfg(test)]
    pub(in crate::cas_projection::connection) fn forwarding_epoch_is_inert_and_attached_after_adoption_failure_for_test(
        &self,
    ) -> bool {
        self.forwarding_hub
            .is_inert_and_attached_recovering_poison_for_test()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn poison_forwarding_epoch_barrier_for_test(&self) {
        self.forwarding_hub.poison_epoch_barrier_for_test();
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn forwarding_epoch_is_inert_and_detached_for_test(
        &self,
    ) -> bool {
        self.forwarding_hub
            .is_inert_and_detached_recovering_poison_for_test()
    }

    pub(super) fn current_epoch(
        &self,
    ) -> Result<Arc<ConnectionServiceEpoch>, ProjectionCoordinatorError> {
        self.forwarding_hub.current_epoch()
    }

    pub(super) fn current_router(&self) -> Result<Arc<EventRouter>, ProjectionCoordinatorError> {
        self.current_epoch().map(|epoch| Arc::clone(&epoch.router))
    }

    pub(in crate::cas_projection) fn identity_observation(
        &self,
    ) -> ProjectionConnectionIdentityObservation {
        ProjectionConnectionIdentityObservation::new(
            self.authority.generation.get(),
            self.runtime_id,
            self.process_generation,
        )
    }

    pub(in crate::cas_projection) fn record_thread_closed(
        &self,
        thread_id: &CasThreadId,
    ) -> Result<super::ConnectionThreadClosedOutcome, ProjectionCoordinatorError> {
        self.forwarding_hub.record_thread_closed(thread_id)
    }

    pub(in crate::cas_projection) fn validate_failure_retained_barrier_topology(
        self: &Arc<Self>,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
        expected_promotion_count: usize,
        expected_cleanup_count: usize,
    ) -> Result<(), super::authority::FailureRetainedBarrierTopologyError> {
        self.authority.validate_failure_retained_barrier_topology(
            identity,
            expected_promotion_count,
            expected_cleanup_count,
        )
    }

    pub(in crate::cas_projection) fn install_pending_projection_quarantine_owner(
        self: &Arc<Self>,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
        promotions: Vec<super::authority::FailureRetainedPromotionReservation>,
        cleanup: Vec<super::authority::FailureRetainedCleanupOwner>,
    ) -> Result<
        super::authority::PendingProjectionConnectionOwner,
        super::authority::PendingProjectionConnectionOwnerInstallError,
    > {
        self.authority
            .install_pending_projection_quarantine_owner(self, identity, promotions, cleanup)
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn retire_authority_for_recovery_test(
        &self,
    ) -> Result<super::authority::ConnectionRetirementOutcome, ProjectionCoordinatorError> {
        self.authority.retire()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn observe_next_retirement_gate_attempt_for_test(
        &self,
    ) -> super::authority::RetirementGateAttemptObservation {
        self.authority
            .observe_next_retirement_gate_attempt_for_test()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn poison_authority_for_recovery_test(&self) {
        self.authority.poison_for_recovery_test();
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn lock_authority_for_test(
        &self,
    ) -> std::sync::MutexGuard<'_, super::authority::ConnectionAuthorityState> {
        self.authority.lock_for_test()
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection) fn new(
        mut backend: ManagedBackendSession,
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: beryl_home_store::HomeGeneration,
        storage: syndic_storage::SyndicStorage,
        mut worker_permits: ProjectionWorkerPermitPair,
        scheduler_signal: crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal,
        stop_coordinator: Arc<StopCoordinator>,
        context_compaction: Arc<
            crate::cas_projection::context_compaction::ContextCompactionCoordinator,
        >,
        commands: crate::cas_projection::persistent_failure::LiveCommandAuthorizer,
        failure_notification: crate::cas_projection::PersistentFailureNotification,
        projection_retainer:
            crate::cas_projection::persistent_failure::PersistentFailureProjectionRetainer,
    ) -> Result<Arc<Self>, ProjectionCoordinatorError> {
        let authority = Arc::new(ConnectionRegistryAuthority::new(
            runtime_id,
            process_generation,
        )?);
        let process_fact = StableConnectionProcessFact::register(
            runtime_id,
            process_generation,
            authority.generation.get(),
        )?;
        let router = Arc::new(EventRouter::new_with_process(
            runtime_id,
            process_generation,
            authority.generation.get(),
            scheduler_signal.clone(),
            commands.clone(),
            Some(projection_retainer.clone()),
            process_fact.observe(),
        )?);
        let forwarding_hub = ForwardingHub::new(Arc::clone(&authority));
        let persistent_failure = Arc::new(persistent_failure::PersistentFailureDriverSlot::new());
        let ingester_permit = worker_permits.take_ingester();
        let (sink, broker, ingester) = match ProviderBroker::start(
            Arc::clone(&home),
            home_id,
            home_generation,
            Arc::clone(&authority),
            Arc::clone(&router),
            Arc::clone(&stop_coordinator),
            Arc::clone(&context_compaction),
            commands.clone(),
            failure_notification.clone(),
            ingester_permit,
        ) {
            Ok(started) => started,
            Err(error) => {
                let _ = backend.shutdown();
                let _ = authority.retire();
                router.retire(LiveEventTargetCloseReason::StreamFailure);
                return Err(ProjectionCoordinatorError::ProviderBrokerAdmission {
                    message: error.to_string(),
                });
            }
        };
        let provider_pages = broker.page_diagnostics();
        let epoch = Arc::new(ConnectionServiceEpoch {
            identity: ConnectionEpochIdentity::new(
                home_id,
                home_generation,
                commands.service_generation(),
            ),
            home,
            storage,
            router: Arc::clone(&router),
            broker: Arc::clone(&broker),
            ingester: Mutex::new(Some(ingester)),
            commands: commands.clone(),
            persistent_failure: Arc::clone(&persistent_failure),
            stop_coordinator,
            context_compaction,
            scheduler_signal,
            failure_notification,
            projection_retainer,
        });
        forwarding_hub.install_initial(ForwardingEpochEndpoint::new(Arc::clone(&epoch), sink))?;
        if let Err(source) = backend.bind_ordered_turn_stream_sink(forwarding_hub.bind_sink()) {
            epoch.request_ingester_cancel();
            drop(epoch.stop_and_join_ingester());
            let _ = backend.shutdown();
            let _ = authority.retire();
            router.retire(LiveEventTargetCloseReason::StreamFailure);
            return Err(ProjectionCoordinatorError::OrderedTurnStreamBinding { source });
        }
        let driver_permit = worker_permits.take_driver();
        let driver = match ConnectionDriver::start(
            backend,
            Arc::clone(&authority),
            Arc::clone(&forwarding_hub),
            driver_permit,
        ) {
            Ok(driver) => driver,
            Err(error) => {
                epoch.request_ingester_cancel();
                drop(epoch.stop_and_join_ingester());
                let _ = authority.retire();
                router.retire(LiveEventTargetCloseReason::StreamFailure);
                return Err(error);
            }
        };
        Ok(Arc::new(Self {
            authority,
            runtime_id,
            process_generation,
            process_fact,
            forwarding_hub,
            ordinary_shutdown: Mutex::new(OrdinaryShutdownSettlement::Unsettled),
            runtime: Mutex::new(Some(ConnectionRuntime { driver })),
            provider_pages: Mutex::new(provider_pages),
            recovery_diagnostics: Arc::new(
                recovery_source_broker::RecoveryReplayDiagnosticsSlot::new(),
            ),
        }))
    }

    pub(in crate::cas_projection) fn stop_target(
        &self,
        target: &syndic_storage::StopOperationTarget,
    ) -> Result<StopTargetProof, StopElectionAcquireError> {
        if self.runtime_id != target.runtime_id()
            || self.process_generation != target.loaded_generation().process()
        {
            return Err(StopElectionAcquireError::TargetMismatch);
        }
        let router = self
            .current_router()
            .map_err(|_| StopElectionAcquireError::Router)?;
        let proof = router.stop_target(
            target.thread_id(),
            target.cas_thread_id(),
            target.cas_turn_id(),
        )?;
        if !proof.matches(target) {
            return Err(StopElectionAcquireError::TargetMismatch);
        }
        Ok(proof)
    }

    pub(in crate::cas_projection) fn coordinate_stop(
        &self,
        coordinator: &Arc<StopCoordinator>,
        proof: StopTargetProof,
        cause: syndic_storage::StopCause,
    ) -> Result<StopOwnership, StopCoordinationError> {
        let router = self
            .current_router()
            .map_err(|_| StopCoordinationError::ConnectionUnavailable)?;
        coordinator.coordinate(&router, proof, cause)
    }

    pub(in crate::cas_projection) fn dispatch_exact_stop(
        &self,
        owner: StopDispatchOwner,
    ) -> Result<StopCoordinationOutcome, StopCoordinationError> {
        let settlement = self
            .with_runtime(|runtime| runtime.driver.dispatch_exact_stop(owner))
            .map_err(|_| StopCoordinationError::ConnectionUnavailable)??;
        Ok(match settlement {
            StopDispatchSettlement::Stopping(operation_id) => StopCoordinationOutcome::Stopping {
                operation_id,
                primary_owner: true,
            },
            StopDispatchSettlement::SafelyReopened(operation_id) => {
                StopCoordinationOutcome::SafelyReopened { operation_id }
            }
            StopDispatchSettlement::Abandoned(operation_id) => {
                StopCoordinationOutcome::Abandoned { operation_id }
            }
            StopDispatchSettlement::HardStop(_) => {
                return Err(StopCoordinationError::LocalAuthorityMismatch);
            }
        })
    }

    pub(in crate::cas_projection) fn dispatch_exact_hard_stop(
        &self,
        owner: HardStopRunOwner,
        proof: StopTargetProof,
    ) -> Result<(), StopCoordinationError> {
        let settlement = self
            .with_runtime(|runtime| runtime.driver.dispatch_exact_hard_stop(owner, proof))
            .map_err(|_| StopCoordinationError::ConnectionUnavailable)??;
        match settlement {
            StopDispatchSettlement::Stopping(_)
            | StopDispatchSettlement::SafelyReopened(_)
            | StopDispatchSettlement::Abandoned(_) => Ok(()),
            StopDispatchSettlement::HardStop(_) => {
                Err(StopCoordinationError::LocalAuthorityMismatch)
            }
        }
    }

    pub(in crate::cas_projection) fn active_steering_target_registration(
        &self,
        owner: SyndicThreadId,
        target: &syndic_storage::SteeringTargetProof,
        loaded_generation: CasLoadedSessionGeneration,
    ) -> Result<(TargetRegistrationProof, Duration), ActiveSteeringTargetLookupError> {
        self.current_router()
            .map_err(|_| ActiveSteeringTargetLookupError::Router)?
            .active_steering_target_registration(owner, target, loaded_generation)
    }

    pub(in crate::cas_projection) fn acquire_active_steering_attempt(
        &self,
        registration: &TargetRegistrationProof,
        target: &syndic_storage::SteeringTargetProof,
        loaded_generation: CasLoadedSessionGeneration,
        arm_waiter: bool,
    ) -> Result<ActiveSteeringAttemptPermit, ActiveSteeringAttemptAcquireError> {
        let router = self
            .current_router()
            .map_err(|_| ActiveSteeringAttemptAcquireError::Router)?;
        if arm_waiter {
            router.acquire_active_steering_attempt_or_arm(registration, target, loaded_generation)
        } else {
            router.acquire_active_steering_attempt(registration, target, loaded_generation)
        }
    }

    pub(in crate::cas_projection) fn arm_active_steering_lifecycle(
        &self,
        attempt: &ActiveSteeringAttemptPermit,
        route: &syndic_storage::SyndicDeliveringSteeringInput,
        home_generation: beryl_home_store::HomeGeneration,
        correlation: &beryl_backend::ClientUserMessageId,
    ) -> Result<CheckedSteeringLifecycleOwner, CheckedSteeringLifecycleArmError> {
        self.attached_broker()
            .map_err(|_| CheckedSteeringLifecycleArmError::Closed)?
            .arm_checked_steering_lifecycle(attempt, route, home_generation, correlation)
    }

    pub(in crate::cas_projection) fn converge_owned_active_steering_loss(
        &self,
        attempt: ActiveSteeringAttemptPermit,
        owner: CheckedSteeringLifecycleOwner,
        disposition: ActiveBindingLossDisposition,
        cause: syndic_storage::TurnIncompleteReason,
    ) -> Result<ProviderBrokerLossOutcome, ProviderBrokerLossError> {
        self.attached_broker()
            .map_err(|_| ProviderBrokerLossError::TargetUnavailable)?
            .converge_active_steering_loss(attempt, owner, disposition, cause)
    }

    pub(in crate::cas_projection) fn converge_unarmed_active_steering_loss(
        &self,
        attempt: ActiveSteeringAttemptPermit,
        disposition: ActiveBindingLossDisposition,
        cause: syndic_storage::TurnIncompleteReason,
    ) -> Result<ProviderBrokerLossOutcome, ProviderBrokerLossError> {
        self.attached_broker()
            .map_err(|_| ProviderBrokerLossError::TargetUnavailable)?
            .converge_unarmed_active_steering_loss(attempt, disposition, cause)
    }

    pub(in crate::cas_projection) fn converge_settled_active_steering_loss(
        &self,
        registration: &TargetRegistrationProof,
        cause: syndic_storage::TurnIncompleteReason,
    ) -> Result<ProviderBrokerLossOutcome, ProviderBrokerLossError> {
        self.attached_broker()
            .map_err(|_| ProviderBrokerLossError::TargetUnavailable)?
            .converge_target_loss(registration, cause)
    }

    pub(in crate::cas_projection) const fn runtime_id(&self) -> RuntimeId {
        self.runtime_id
    }

    pub(in crate::cas_projection) const fn process_generation(&self) -> CasProcessGeneration {
        self.process_generation
    }

    pub(in crate::cas_projection) fn settle_persistent_failure_target_guards(
        self: &Arc<Self>,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
        observations: &[router::PersistentFailureTargetGuardObservation],
    ) -> Result<(), router::PersistentFailureTargetGuardSettlementError> {
        self.current_router()
            .map_err(|_| router::PersistentFailureTargetGuardSettlementError::RouterPoisoned)?
            .settle_persistent_failure_target_guards(
                self.identity_observation(),
                identity,
                observations,
            )
    }

    pub(in crate::cas_projection) fn validate_persistent_failure_target_guard_topology(
        self: &Arc<Self>,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
        observations: &[router::PersistentFailureTargetGuardObservation],
    ) -> Result<(), router::PersistentFailureTargetGuardSettlementError> {
        self.current_router()
            .map_err(|_| router::PersistentFailureTargetGuardSettlementError::RouterPoisoned)?
            .validate_persistent_failure_target_guard_topology(
                self.identity_observation(),
                identity,
                observations,
            )
    }

    pub(in crate::cas_projection) fn persistent_failure_target_threads(
        &self,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    ) -> Result<Vec<SyndicThreadId>, router::PersistentFailureTargetIneligibility> {
        self.current_router()
            .map_err(|_| router::PersistentFailureTargetIneligibility::RouterUnavailable)?
            .persistent_failure_target_threads(identity)
    }

    pub(in crate::cas_projection) fn freeze_persistent_failure_targets(
        &self,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
        stop_evidence: &std::collections::HashMap<
            SyndicThreadId,
            crate::cas_projection::stop::PersistentFailureStopEvidence,
        >,
    ) -> Result<router::PersistentFailureTargetBatch, router::PersistentFailureTargetIneligibility>
    {
        self.current_router()
            .map_err(|_| router::PersistentFailureTargetIneligibility::RouterUnavailable)?
            .freeze_persistent_failure_targets(identity, stop_evidence)
    }

    pub(in crate::cas_projection) fn install_persistent_failure_obligations(
        &self,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
        proofs: Vec<router::PersistentFailureTargetProof>,
    ) -> Result<Vec<persistent_failure::PersistentFailureCompletion>, ()> {
        self.current_epoch()
            .map_err(|_| ())?
            .persistent_failure
            .install(identity, proofs)
    }

    pub(in crate::cas_projection) fn reserve_scheduled_promotion(
        self: &Arc<Self>,
    ) -> Result<Option<ConnectionPromotionReservation>, ProjectionCoordinatorError> {
        let epoch = self.current_epoch()?;
        let command = match epoch.commands.authorize() {
            Ok(command) => command,
            Err(_) => return Ok(None),
        };
        let failure_transfer = epoch
            .router
            .projection_retainer()?
            .promotion_failure_transfer();
        self.authority
            .reserve_scheduled_promotion(self, command, failure_transfer)
    }

    pub(in crate::cas_projection) fn acquire_cleanup_owner(
        self: &Arc<Self>,
    ) -> Result<Option<ConnectionCleanupOwner>, ProjectionCoordinatorError> {
        let epoch = self.current_epoch()?;
        let command = match epoch.commands.authorize() {
            Ok(command) => command,
            Err(_) => return Ok(None),
        };
        let failure_transfer = epoch
            .router
            .projection_retainer()?
            .cleanup_failure_transfer();
        self.authority
            .acquire_cleanup_owner(self, command, failure_transfer)
    }

    pub(in crate::cas_projection) fn release_session_owner(self: &Arc<Self>) {
        let Ok(epoch) = self.current_epoch() else {
            self.authority.mark_session_owner_released();
            return;
        };
        let command = match epoch.commands.authorize() {
            Ok(command) => command,
            Err(_) => {
                self.authority.mark_session_owner_released();
                return;
            }
        };
        let should_detach = self
            .authority
            .release_session_owner(|| self.elect_ordinary_retirement(&command));
        match should_detach {
            Ok(true) => {
                self.signal_ordinary_retirement();
            }
            Err(_) => self.request_ordinary_retirement(),
            Ok(false) => {}
        }
        drop(command);
    }

    pub(in crate::cas_projection) fn call<T>(
        &self,
        operation: impl FnOnce(&mut ConnectionRequestSession<'_>) -> Result<T, ManagedBackendError>
        + Send
        + 'static,
    ) -> Result<T, ProjectionExecutionError>
    where
        T: Send + 'static,
    {
        let command = self.call_ordered(operation)?;
        self.publish_ordered_result(command)
    }

    pub(super) fn publish_ordered_result<T>(
        &self,
        command: ConnectionCommandOutcome<Result<T, ManagedBackendError>>,
    ) -> Result<T, ProjectionExecutionError> {
        let (operation_result, routing_failure) = command.into_parts();
        if operation_result
            .as_ref()
            .is_err_and(ManagedBackendError::invalidates_connection_authority)
        {
            let Err(error) = operation_result else {
                unreachable!("invalidating operation result must be an error")
            };
            return Err(ProjectionExecutionError::from(error));
        }
        match routing_failure {
            Some(ConnectionRoutingFailure::Backend | ConnectionRoutingFailure::Router) => {
                Err(self.unavailable().into())
            }
            Some(ConnectionRoutingFailure::Target { thread_id, reason }) => {
                Err(ProjectionExecutionError::LiveEventRouting { thread_id, reason })
            }
            None => operation_result.map_err(ProjectionExecutionError::from),
        }
    }

    pub(in crate::cas_projection) fn call_ordered<T>(
        &self,
        operation: impl FnOnce(&mut ConnectionRequestSession<'_>) -> Result<T, ManagedBackendError>
        + Send
        + 'static,
    ) -> Result<ConnectionCommandOutcome<Result<T, ManagedBackendError>>, ProjectionCoordinatorError>
    where
        T: Send + 'static,
    {
        if self.authority.is_retired() {
            return Err(self.unavailable());
        }
        self.with_runtime(|runtime| runtime.driver.call(operation))
    }

    pub(in crate::cas_projection) fn inject_thread_items_with_source(
        &self,
        target: FreshIdleThread,
        preflight: ThreadInjectionPreflight,
        prepared: recovery_source_broker::PreparedRecoverySource,
        timeout: Duration,
        next_page: impl FnMut(
            usize,
            PageLease,
        )
            -> Result<Option<ThreadInjectionSourcePage>, ThreadInjectionSourceError>,
    ) -> Result<ConnectionCommandOutcome<ThreadInjectionOutcome>, ProjectionCoordinatorError> {
        if self.authority.is_retired() {
            return Err(self.unavailable());
        }
        self.with_runtime(|runtime| {
            runtime.driver.call_classified_with_recovery_source(
                move |session, source| {
                    session.inject_thread_items(target, &preflight, source, timeout)
                },
                prepared,
                next_page,
                |outcome| match outcome {
                    ThreadInjectionOutcome::TransportLost { error, .. }
                    | ThreadInjectionOutcome::CompletionUnknown { error, .. } => {
                        error.invalidates_connection_authority()
                    }
                    ThreadInjectionOutcome::Succeeded { .. }
                    | ThreadInjectionOutcome::Rejected { .. }
                    | ThreadInjectionOutcome::ProvenNotDispatched { .. } => false,
                },
            )
        })
    }

    pub(in crate::cas_projection) fn prepare_recovery_source(
        &self,
    ) -> Result<recovery_source_broker::PreparedRecoverySource, ProjectionCoordinatorError> {
        let prepared = recovery_source_broker::prepare()?;
        self.recovery_diagnostics.publish(prepared.diagnostics());
        Ok(prepared)
    }

    pub(in crate::cas_projection) fn retire(&self) {
        let _ = self.shutdown();
    }

    pub(super) fn request_ordinary_retirement(&self) {
        let Ok(epoch) = self.current_epoch() else {
            return;
        };
        let command = match epoch.commands.authorize() {
            Ok(command) => command,
            Err(_) => return,
        };
        let mut authority = match self.authority.lock() {
            Ok(authority) => authority,
            Err(_) => {
                self.signal_ordinary_retirement();
                return;
            }
        };
        let elected = command
            .commit_if_current(|| {
                let elected = epoch.begin_ordinary_retirement();
                if elected {
                    self.authority.retire_locked(&mut authority);
                }
                elected
            })
            .unwrap_or(false);
        drop(authority);
        drop(command);
        if elected || self.authority.is_retired() {
            self.signal_ordinary_retirement();
        }
    }

    pub(in crate::cas_projection) fn request_ordinary_retirement_after_service_shutdown(&self) {
        let Ok(epoch) = self.current_epoch() else {
            return;
        };
        let mut authority = match self.authority.lock() {
            Ok(authority) => authority,
            Err(_) => {
                if epoch.begin_ordinary_retirement() {
                    self.signal_ordinary_retirement();
                }
                return;
            }
        };
        let elected = epoch.begin_ordinary_retirement();
        if elected {
            self.authority.retire_locked(&mut authority);
        }
        drop(authority);
        if elected {
            self.signal_ordinary_retirement();
        }
    }

    pub(super) fn signal_ordinary_retirement(&self) {
        self.process_fact
            .retire(LiveEventTargetCloseReason::ConnectionRetired);
        if let Ok(epoch) = self.current_epoch() {
            epoch
                .router
                .retire(LiveEventTargetCloseReason::ConnectionRetired);
            epoch.request_ingester_cancel();
        }
        if let Some(runtime) = self
            .runtime
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .as_ref()
        {
            runtime.driver.request_stop();
        }
    }

    pub(in crate::cas_projection) fn try_reap_ordinary_retirement(&self) -> bool {
        if !self.authority.is_retired() || !self.authority.retirement_complete() {
            return false;
        }
        let Ok(mut settlement) = self.ordinary_shutdown.lock() else {
            return false;
        };
        match *settlement {
            OrdinaryShutdownSettlement::Clean => return self.is_detached(),
            OrdinaryShutdownSettlement::Failed => return false,
            OrdinaryShutdownSettlement::Unsettled => {}
        }
        let broker_finished = self
            .current_epoch()
            .map_or(true, |epoch| epoch.ingester_is_finished());
        let driver_finished = self
            .runtime
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .as_ref()
            .is_none_or(|runtime| runtime.driver.is_finished());
        if !broker_finished || !driver_finished {
            return false;
        }
        let _ = self.settle_ordinary_shutdown_locked(&mut settlement);
        *settlement == OrdinaryShutdownSettlement::Clean && self.is_detached()
    }

    pub(in crate::cas_projection) fn shutdown(&self) -> Result<(), ProjectionCoordinatorError> {
        let mut settlement = self.ordinary_shutdown.lock().map_err(|_| {
            ProjectionCoordinatorError::RegistryPoisoned {
                registry: crate::cas_projection::ProjectionRegistryKind::ProjectionConnection,
            }
        })?;
        match *settlement {
            OrdinaryShutdownSettlement::Clean => return Ok(()),
            OrdinaryShutdownSettlement::Failed => {
                return Err(ProjectionCoordinatorError::ProjectionWorkerStopped);
            }
            OrdinaryShutdownSettlement::Unsettled => {}
        }
        let epoch = self.current_epoch()?;
        let command = match epoch.commands.authorize() {
            Ok(command) => Some(command),
            Err(_) if epoch.commands.is_persistent_failure_cut() => return Ok(()),
            Err(_) => None,
        };
        let elected = command.as_ref().map_or_else(
            || epoch.begin_ordinary_retirement(),
            |command| {
                command
                    .commit_if_current(|| epoch.begin_ordinary_retirement())
                    .unwrap_or(false)
            },
        );
        if !elected {
            return Ok(());
        }
        self.settle_ordinary_shutdown_locked(&mut settlement)
    }

    pub(super) fn elect_ordinary_retirement(
        &self,
        command: &crate::cas_projection::persistent_failure::LiveCommandPermit,
    ) -> bool {
        let Ok(epoch) = self.current_epoch() else {
            return false;
        };
        command.service_generation() == epoch.identity.service_generation()
            && command
                .commit_if_current(|| epoch.begin_ordinary_retirement())
                .unwrap_or(false)
    }

    /// Elects connection-local ordinary retirement while the caller already owns the master gate.
    pub(super) fn begin_ordinary_retirement_under_gate(&self) -> bool {
        self.current_epoch()
            .is_ok_and(|epoch| epoch.begin_ordinary_retirement())
    }

    pub(super) fn shutdown_after_ordinary_retirement(
        &self,
    ) -> Result<(), ProjectionCoordinatorError> {
        let mut settlement = self.ordinary_shutdown.lock().map_err(|_| {
            ProjectionCoordinatorError::RegistryPoisoned {
                registry: crate::cas_projection::ProjectionRegistryKind::ProjectionConnection,
            }
        })?;
        self.settle_ordinary_shutdown_locked(&mut settlement)
    }

    fn settle_ordinary_shutdown_locked(
        &self,
        settlement: &mut OrdinaryShutdownSettlement,
    ) -> Result<(), ProjectionCoordinatorError> {
        match *settlement {
            OrdinaryShutdownSettlement::Clean => return Ok(()),
            OrdinaryShutdownSettlement::Failed => {
                return Err(ProjectionCoordinatorError::ProjectionWorkerStopped);
            }
            OrdinaryShutdownSettlement::Unsettled => {}
        }
        let result = self.execute_ordinary_shutdown();
        *settlement = if result.is_ok() {
            OrdinaryShutdownSettlement::Clean
        } else {
            OrdinaryShutdownSettlement::Failed
        };
        result
    }

    fn execute_ordinary_shutdown(&self) -> Result<(), ProjectionCoordinatorError> {
        match self.authority.retire()? {
            ConnectionRetirementOutcome::Complete => {}
            ConnectionRetirementOutcome::FailureRetained(_) => return Ok(()),
        }
        let epoch = self.current_epoch()?;
        epoch
            .router
            .retire(LiveEventTargetCloseReason::ConnectionRetired);
        // A foreground call holds `runtime` while its driver can be blocked on this broker's
        // acknowledgement, so cancellation must remain independently reachable.
        epoch.request_ingester_cancel();
        let (runtime, poisoned) = match self.runtime.lock() {
            Ok(mut runtime) => (runtime.take(), false),
            Err(poison) => (poison.into_inner().take(), true),
        };
        let mut first_error = poisoned.then_some(ProjectionCoordinatorError::RegistryPoisoned {
            registry: crate::cas_projection::ProjectionRegistryKind::ProjectionConnection,
        });
        if let Some(runtime) = runtime {
            runtime.driver.request_stop();
            if let Err(error) = runtime.driver.join()
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        match epoch.stop_and_join_ingester_after_ordinary_retirement() {
            Ok(receipt)
                if !receipt.is_exact(
                    epoch.identity.service_generation(),
                    epoch.identity.home_generation(),
                ) && first_error.is_none() =>
            {
                first_error = Some(ProjectionCoordinatorError::ProjectionWorkerStopped);
            }
            Err(error) if first_error.is_none() => first_error = Some(error),
            Ok(_) | Err(_) => {}
        }
        let diagnostics = epoch.broker.page_diagnostics();
        *self
            .provider_pages
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = diagnostics;
        match self.forwarding_hub.lock_epoch() {
            Ok(mut hub) => {
                drop(hub.mark_inert());
            }
            Err(error) if first_error.is_none() => first_error = Some(error),
            Err(_) => {}
        }
        first_error.map_or(Ok(()), Err)
    }

    pub(in crate::cas_projection) fn is_retired(&self) -> bool {
        self.authority.is_retired()
    }

    pub(in crate::cas_projection) fn is_detached(&self) -> bool {
        self.forwarding_hub.is_detached()
            && self
                .runtime
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .is_none()
    }

    pub(in crate::cas_projection) fn provider_page_diagnostics(
        &self,
    ) -> beryl_stream::PagePoolDiagnostics {
        if let Ok(broker) = self.attached_broker() {
            return broker.page_diagnostics();
        }
        *self
            .provider_pages
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    pub(in crate::cas_projection) fn recovery_replay_diagnostics(
        &self,
    ) -> Option<recovery_source_broker::RecoveryReplayDiagnosticsSnapshot> {
        self.recovery_diagnostics.snapshot()
    }

    pub(in crate::cas_projection) fn recovery_replay_diagnostics_observer(
        &self,
    ) -> recovery_source_broker::RecoveryReplayDiagnosticsObserver {
        self.recovery_diagnostics.observer()
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection) fn provider_test_key(
        &self,
    ) -> crate::cas_projection::test_faults::ProviderTestKey {
        self.attached_broker()
            .expect("attached test connection retains its provider broker")
            .test_key()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn poison_router_for_test(self: &Arc<Self>) {
        self.current_router()
            .expect("attached test connection retains its router")
            .poison_state_for_test();
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn pause_stable_driver_before_next_cycle_for_test(
        &self,
    ) -> super::driver::DriverPreCyclePauseController {
        let connection_generation = self.identity_observation().connection_generation();
        self.with_runtime(|runtime| {
            Ok(runtime
                .driver
                .pause_before_next_cycle_for_test(connection_generation))
        })
        .expect("an attached test connection retains its stable driver")
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection) fn provider_broker_test_snapshot(
        &self,
    ) -> crate::cas_projection::test_faults::ProviderBrokerSnapshot {
        self.attached_broker()
            .expect("attached test connection retains its provider broker")
            .test_snapshot()
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection) fn fail_next_write_before_dispatch_for_test(
        &self,
    ) -> Result<(), ProjectionCoordinatorError> {
        self.with_runtime(|runtime| {
            runtime
                .driver
                .call_classified(
                    |session| session.fail_next_write_before_dispatch_for_test(),
                    |_| false,
                )
                .map(|_| ())
        })
    }

    pub(super) fn with_runtime<T>(
        &self,
        operation: impl FnOnce(&ConnectionRuntime) -> Result<T, ProjectionCoordinatorError>,
    ) -> Result<T, ProjectionCoordinatorError> {
        let runtime =
            self.runtime
                .lock()
                .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                    registry: crate::cas_projection::ProjectionRegistryKind::ProjectionConnection,
                })?;
        let Some(runtime) = runtime.as_ref() else {
            return Err(self.unavailable());
        };
        operation(runtime)
    }

    fn attached_broker(&self) -> Result<Arc<ProviderBrokerControl>, ProjectionCoordinatorError> {
        self.current_epoch().map(|epoch| Arc::clone(&epoch.broker))
    }

    pub(in crate::cas_projection) fn stop_coordinator(
        &self,
    ) -> Result<Arc<StopCoordinator>, ProjectionCoordinatorError> {
        self.current_epoch()
            .map(|epoch| Arc::clone(&epoch.stop_coordinator))
    }

    pub(in crate::cas_projection) fn context_compaction_coordinator(
        &self,
    ) -> Result<
        Arc<crate::cas_projection::context_compaction::ContextCompactionCoordinator>,
        ProjectionCoordinatorError,
    > {
        self.current_epoch()
            .map(|epoch| Arc::clone(&epoch.context_compaction))
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn close_checked_steering_lifecycles_for_test(
        &self,
    ) -> Result<(), ProjectionCoordinatorError> {
        self.attached_broker()?
            .close_checked_steering_lifecycles_for_test();
        Ok(())
    }

    pub(super) fn unavailable(&self) -> ProjectionCoordinatorError {
        ProjectionCoordinatorError::ProjectionConnectionUnavailable {
            runtime_id: self.runtime_id,
            process_generation: self.process_generation,
        }
    }

    pub(super) fn authorize_command(
        &self,
    ) -> Result<
        crate::cas_projection::persistent_failure::LiveCommandPermit,
        crate::cas_projection::persistent_failure::LiveCommandAdmissionError,
    > {
        self.current_epoch()
            .map_err(|_| {
                crate::cas_projection::persistent_failure::LiveCommandAdmissionError::Closed
            })?
            .commands
            .authorize()
    }

    pub(super) fn settle_authority<T>(
        &self,
        current: impl FnOnce() -> T,
        persistent_failure: impl FnOnce(crate::cas_projection::PersistentFailureGeneration) -> T,
        closed: impl FnOnce() -> T,
    ) -> Result<T, crate::cas_projection::persistent_failure::LiveCommandAdmissionError> {
        self.current_epoch()
            .map_err(|_| {
                crate::cas_projection::persistent_failure::LiveCommandAdmissionError::Closed
            })?
            .commands
            .settle_authority(current, persistent_failure, closed)
    }

    fn key(&self, cas_thread_id: CasThreadId) -> LoadedThreadKey {
        LoadedThreadKey {
            runtime_id: self.runtime_id,
            process_generation: self.process_generation,
            cas_thread_id,
        }
    }

    pub(in crate::cas_projection) fn register_new(
        self: &Arc<Self>,
        cas_thread_id: CasThreadId,
        owner: SyndicThreadId,
        unsubscribe_timeout: Duration,
        preactivation_issuer: Option<
            &crate::cas_projection::service_config::ProjectionPreactivationSurrenderIssuer,
        >,
    ) -> Result<LoadedProjectionLease, ProjectionCoordinatorError> {
        let command = self.authorize_command().map_err(|_| self.unavailable())?;
        let key = self.key(cas_thread_id);
        let preactivation_surrender = preactivation_issuer
            .map(crate::cas_projection::service_config::ProjectionPreactivationSurrenderIssuer::try_mint)
            .transpose()?;
        let mut seed = RawLoadedLeaseSeed::pending(
            Arc::clone(self),
            key.clone(),
            owner,
            unsubscribe_timeout,
            preactivation_surrender,
        );
        let Some(()) = self
            .authority
            .register_new(key, owner, &command, &mut seed)?
        else {
            return Err(self.unavailable());
        };
        seed.into_lease().ok_or_else(|| self.unavailable())
    }

    pub(in crate::cas_projection) fn acquire_existing(
        self: &Arc<Self>,
        cas_thread_id: &CasThreadId,
        owner: SyndicThreadId,
        unsubscribe_timeout: Duration,
        preactivation_issuer: Option<
            &crate::cas_projection::service_config::ProjectionPreactivationSurrenderIssuer,
        >,
    ) -> Result<ExistingLease, ProjectionCoordinatorError> {
        let command = self.authorize_command().map_err(|_| self.unavailable())?;
        let key = self.key(cas_thread_id.clone());
        let preactivation_surrender = preactivation_issuer
            .map(crate::cas_projection::service_config::ProjectionPreactivationSurrenderIssuer::try_mint)
            .transpose()?;
        let mut seed = RawLoadedLeaseSeed::pending(
            Arc::clone(self),
            key.clone(),
            owner,
            unsubscribe_timeout,
            preactivation_surrender,
        );
        let Some(subscription) = self
            .authority
            .acquire_existing(&key, owner, &command, &mut seed)?
        else {
            return Err(self.unavailable());
        };
        Ok(match subscription {
            ExistingSubscription::Absent => ExistingLease::Absent,
            ExistingSubscription::AnotherConnection => ExistingLease::AnotherConnection,
            ExistingSubscription::Quarantined => ExistingLease::Quarantined,
            ExistingSubscription::AnotherOwner { existing_owner } => {
                ExistingLease::AnotherOwner { existing_owner }
            }
            ExistingSubscription::Exact { .. } => ExistingLease::Exact(
                seed.into_lease()
                    .expect("exact subscription arms its raw loaded-lease seed"),
            ),
        })
    }

    pub(in crate::cas_projection) fn live_event_snapshot(
        &self,
    ) -> Result<LiveEventRouterSnapshot, ProjectionCoordinatorError> {
        self.current_router()?.snapshot()
    }

    pub(in crate::cas_projection) fn live_event_process_snapshot(
        &self,
    ) -> Result<LiveEventProcessSnapshot, ProjectionCoordinatorError> {
        self.current_router()?.process_snapshot()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn register_event_target(
        self: &Arc<Self>,
        key: &LoadedThreadKey,
        owner: SyndicThreadId,
        generation: CasLoadedSessionGeneration,
        home_generation: u64,
        request_timeout: Duration,
        token: LeaseToken,
        turn: TargetTurnRegistration,
    ) -> Result<TargetRegistration, LiveEventTargetRegistrationError> {
        let command = self
            .authorize_command()
            .map_err(|_| LiveEventTargetRegistrationError::ConnectionRetired)?;
        let detached = match &turn {
            TargetTurnRegistration::Pending(activation) => {
                Some(provider_broker::DetachedActivationAuthority::new(
                    key.cas_thread_id.clone(),
                    generation,
                    home_generation,
                    activation.clone(),
                ))
            }
            TargetTurnRegistration::Active(_) | TargetTurnRegistration::ContextCompaction(_) => {
                None
            }
        };
        let result = (|| {
            let gate = self.authority.lock()?;
            if self.authority.is_retired() {
                return Err(LiveEventTargetRegistrationError::ConnectionRetired);
            }
            if !registry::contains_exact(key, self.authority.generation, owner, generation, token)?
            {
                return Err(LiveEventTargetRegistrationError::ProjectionNotLive);
            }
            let result = self
                .current_router()
                .map_err(|_| LiveEventTargetRegistrationError::ConnectionRetired)?
                .register(
                    &command,
                    key.clone(),
                    owner,
                    generation,
                    home_generation,
                    request_timeout,
                    turn,
                );
            drop(gate);
            result
        })();
        if result.is_err()
            && detached.is_some_and(|authority| {
                self.attached_broker().map_or(true, |broker| {
                    broker.abandon_detached_activation(authority).is_err()
                })
            })
        {
            self.retire();
            return Err(LiveEventTargetRegistrationError::ActivationCleanupFailed);
        }
        if matches!(
            result,
            Err(LiveEventTargetRegistrationError::ConnectionRetired
                | LiveEventTargetRegistrationError::RouterPoisoned)
        ) {
            self.retire();
        }
        result
    }

    pub(super) fn converge_target_loss(
        &self,
        registration: &TargetRegistration,
        cause: syndic_storage::TurnIncompleteReason,
    ) -> Result<provider_broker::ProviderBrokerLossOutcome, provider_broker::ProviderBrokerLossError>
    {
        self.attached_broker()
            .map_err(|_| provider_broker::ProviderBrokerLossError::TargetUnavailable)?
            .converge_target_loss(&registration.proof(), cause)
    }

    pub(in crate::cas_projection) fn settle_abandoned_target_projection(
        &self,
        registration: &TargetRegistration,
        projection: crate::cas_projection::LoadedCasProjection,
    ) {
        let Ok(router) = self.current_router() else {
            drop(projection);
            self.request_ordinary_retirement();
            return;
        };
        match router.settle_abandoned_target_projection(registration, projection) {
            router::TargetProjectionDropSettlement::Ordinary {
                projection,
                connection_retired,
            } => {
                drop(projection);
                if connection_retired {
                    self.request_ordinary_retirement();
                }
            }
            router::TargetProjectionDropSettlement::PersistentFailure => {}
            router::TargetProjectionDropSettlement::Unavailable(projection) => {
                drop(projection);
                self.request_ordinary_retirement();
            }
        }
    }

    pub(super) fn invalidate_target_generation(&self, registration: &TargetRegistration) {
        if registry::invalidate_exact_generation(
            registration.key(),
            self.authority.generation,
            registration.owner(),
            registration.loaded_generation(),
        )
        .is_err()
        {
            self.retire();
        }
    }

    pub(in crate::cas_projection) fn retire_thread(
        &self,
        cas_thread_id: &CasThreadId,
        owner: SyndicThreadId,
        timeout: Duration,
    ) -> Result<ThreadRetirement, LoadedProjectionReleaseError> {
        let key = self.key(cas_thread_id.clone());
        let observed = match registry::invalidate_thread(&key, self.authority.generation, owner) {
            Ok(observed) => observed,
            Err(error) => {
                self.retire();
                return Err(LoadedProjectionReleaseError::Registry(error));
            }
        };
        match observed {
            ObservedSubscription::Absent => Ok(ThreadRetirement::Absent),
            ObservedSubscription::AnotherConnection => Ok(ThreadRetirement::AnotherConnection),
            ObservedSubscription::AnotherOwner { existing_owner } => {
                Ok(ThreadRetirement::AnotherOwner { existing_owner })
            }
            ObservedSubscription::Exact(generation) => {
                let release_error = self.try_unsubscribe(cas_thread_id, timeout).err();
                Ok(ThreadRetirement::Retired {
                    generation,
                    release_error,
                })
            }
        }
    }

    pub(super) fn try_unsubscribe(
        &self,
        thread_id: &CasThreadId,
        timeout: Duration,
    ) -> Result<LoadedProjectionReleaseOutcome, LoadedProjectionReleaseError> {
        if self.authority.is_retired() {
            return Ok(LoadedProjectionReleaseOutcome::ConnectionRetired);
        }
        let thread_id = thread_id.clone();
        let (operation_result, routing_failure) = self
            .with_runtime(|runtime| {
                runtime
                    .driver
                    .call(move |session| session.unsubscribe_thread(&thread_id, timeout))
            })
            .map_err(LoadedProjectionReleaseError::Registry)?
            .into_parts();
        if !operation_result
            .as_ref()
            .is_err_and(ManagedBackendError::invalidates_connection_authority)
        {
            match routing_failure {
                Some(ConnectionRoutingFailure::Target { thread_id, reason }) => {
                    return Err(LoadedProjectionReleaseError::LiveEventRouting {
                        thread_id,
                        reason,
                    });
                }
                Some(ConnectionRoutingFailure::Backend | ConnectionRoutingFailure::Router) => {
                    return Ok(LoadedProjectionReleaseOutcome::ConnectionRetired);
                }
                None => {}
            }
        }
        match operation_result {
            Ok(response) => Ok(LoadedProjectionReleaseOutcome::Unsubscribe(response.status)),
            Err(error) => Err(LoadedProjectionReleaseError::Backend(Box::new(error))),
        }
    }
}

impl Drop for ProjectionConnection {
    fn drop(&mut self) {
        self.request_ordinary_retirement();
        if let Ok(epoch) = self.forwarding_hub.current_epoch() {
            epoch.request_ingester_cancel();
        }
        if let Some(runtime) = self
            .runtime
            .get_mut()
            .unwrap_or_else(|poison| poison.into_inner())
            .as_ref()
        {
            runtime.driver.request_stop();
        }
    }
}

#[cfg(test)]
mod phase82_adoption_tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/connection_epoch_adoption_barrier.rs"
    ));
}
