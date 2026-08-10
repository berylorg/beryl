use super::*;

impl PersistentFailureCoordinator {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection) fn start(
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        service_generation: ProjectionServiceGeneration,
        gate: MasterCommandGate,
        notification: PersistentFailureNotification,
        receiver: mpsc::Receiver<()>,
        stop_coordinator: Arc<StopCoordinator>,
        connections: Arc<
            crate::cas_projection::service_registry::ProjectionServiceConnectionRegistry,
        >,
    ) -> Result<Self, std::io::Error> {
        Self::start_with_startup_gate(
            home,
            home_id,
            home_generation,
            service_generation,
            gate,
            notification,
            receiver,
            stop_coordinator,
            connections,
            crate::cas_projection::service_startup::ServiceStartupGate::open_gate(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection) fn start_with_startup_gate(
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        service_generation: ProjectionServiceGeneration,
        gate: MasterCommandGate,
        notification: PersistentFailureNotification,
        receiver: mpsc::Receiver<()>,
        stop_coordinator: Arc<StopCoordinator>,
        connections: Arc<
            crate::cas_projection::service_registry::ProjectionServiceConnectionRegistry,
        >,
        startup: Arc<crate::cas_projection::service_startup::ServiceStartupGate>,
    ) -> Result<Self, std::io::Error> {
        let stop_requested = Arc::new(AtomicBool::new(false));
        let state = Arc::new((
            Mutex::new(CoordinatorState {
                phase: PersistentFailureCutState::Armed,
                failure_generation: None,
                target_count: 0,
                retained_connections: Vec::new(),
                retained_results: Vec::new(),
                retained_projections: Vec::new(),
                retained_target_projections: Vec::new(),
                retained_reacquisition_anchors: Vec::new(),
                retained_raw_loaded_leases: Vec::new(),
                retained_raw_quarantined_anchors: Vec::new(),
                retained_raw_reacquisition_reservations: Vec::new(),
                retained_promotion_reservations: Vec::new(),
                retained_cleanup_owners: Vec::new(),
                sealed_counts: None,
                late_publication_count: 0,
                pending_quarantine: PendingQuarantineStage::Available,
            }),
            Condvar::new(),
        ));
        let context = WorkerContext {
            home,
            home_id,
            home_generation,
            service_generation,
            notification: notification.clone(),
            gate,
            stop_coordinator,
            connections,
            stop_requested: Arc::clone(&stop_requested),
            state: Arc::clone(&state),
        };
        let handle = std::thread::Builder::new()
            .name("beryl-persistent-failure-cut".to_owned())
            .spawn(move || {
                if startup.wait() {
                    super::worker::run_worker(receiver, context);
                }
            })?;
        Ok(Self {
            service_generation,
            notification,
            stop_requested,
            state,
            handle: Mutex::new(Some(handle)),
        })
    }

    pub(in crate::cas_projection) fn notification(&self) -> PersistentFailureNotification {
        self.notification.clone()
    }

    pub(in crate::cas_projection) fn snapshot(&self) -> PersistentFailureCutSnapshot {
        let state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        PersistentFailureCutSnapshot {
            state: state.phase,
            service_generation: self.service_generation,
            failure_generation: state.failure_generation,
            target_count: state.target_count,
            retained_projection_count: state
                .retained_projections
                .len()
                .checked_add(state.retained_target_projections.len())
                .and_then(|count| count.checked_add(state.retained_reacquisition_anchors.len()))
                .and_then(|count| count.checked_add(state.retained_raw_loaded_leases.len()))
                .and_then(|count| count.checked_add(state.retained_raw_quarantined_anchors.len()))
                .and_then(|count| {
                    count.checked_add(state.retained_raw_reacquisition_reservations.len())
                })
                .expect("retained projection allocations fit the process address space"),
            retained_promotion_count: state.retained_promotion_reservations.len(),
            retained_cleanup_count: state.retained_cleanup_owners.len(),
        }
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn retained_loaded_projection_counts_for_test(
        &self,
    ) -> (usize, usize) {
        let state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        (
            state.retained_projections.len(),
            state.retained_raw_loaded_leases.len(),
        )
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn retained_reacquisition_anchor_counts_for_test(
        &self,
    ) -> (usize, usize) {
        let state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        (
            state.retained_reacquisition_anchors.len(),
            state.retained_raw_quarantined_anchors.len(),
        )
    }

    #[cfg(test)]
    pub(in crate::cas_projection::persistent_failure) fn orphan_one_retained_promotion_for_test(
        &self,
    ) -> bool {
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        assert_eq!(state.phase, PersistentFailureCutState::Finished);
        assert!(state.sealed_counts.is_none());
        state.retained_promotion_reservations.pop().is_some()
    }

    #[cfg(test)]
    pub(in crate::cas_projection::persistent_failure) fn orphan_one_retained_connection_for_test(
        &self,
    ) -> bool {
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        assert_eq!(state.phase, PersistentFailureCutState::Finished);
        assert!(state.sealed_counts.is_none());
        state.retained_connections.pop().is_some()
    }

    #[cfg(test)]
    pub(in crate::cas_projection::persistent_failure) fn orphan_one_retained_target_result_for_test(
        &self,
    ) -> bool {
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        assert_eq!(state.phase, PersistentFailureCutState::Finished);
        assert!(state.sealed_counts.is_none());
        state.retained_results.pop().is_some()
    }

    #[cfg(test)]
    pub(in crate::cas_projection::persistent_failure) fn corrupt_one_target_disposition_for_test(
        &self,
    ) -> bool {
        let (witness, connections) = {
            let state = self
                .state
                .0
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            assert_eq!(state.phase, PersistentFailureCutState::Finished);
            assert!(state.sealed_counts.is_none());
            let Some(target) = state.retained_results.first() else {
                return false;
            };
            (target.witness.clone(), state.retained_connections.clone())
        };
        let Some(connection) = connections
            .iter()
            .find(|connection| connection.identity_observation() == witness.connection())
        else {
            return false;
        };
        let Ok(observation) = witness.observe_guard(connection) else {
            return false;
        };
        let mismatched = match observation.disposition() {
            PersistentFailureTargetGuardDisposition::Frozen => {
                PersistentFailureDriverResult::NoDispatch(
                    PersistentFailureNoDispatchReason::RandomUnavailable,
                )
            }
            PersistentFailureTargetGuardDisposition::Spent => {
                PersistentFailureDriverResult::NoDispatch(
                    PersistentFailureNoDispatchReason::Router(
                        PersistentFailureTargetIneligibility::RouterUnavailable,
                    ),
                )
            }
        };
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let Some(target) = state
            .retained_results
            .iter_mut()
            .find(|target| target.witness == witness)
        else {
            return false;
        };
        target.result = mismatched;
        true
    }

    pub(in crate::cas_projection) fn projection_retainer(
        &self,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
    ) -> PersistentFailureProjectionRetainer {
        PersistentFailureProjectionRetainer {
            home_id,
            home_generation,
            notification: self.notification.clone(),
            state: Arc::clone(&self.state),
        }
    }

    pub(in crate::cas_projection) fn request_shutdown(&self) {
        self.stop_requested.store(true, Ordering::Release);
        self.notification.wake_worker();
    }

    pub(in crate::cas_projection) fn join(&self) -> Result<(), ()> {
        let (handle, owner_poisoned) = match self.handle.lock() {
            Ok(mut handle) => (handle.take(), false),
            Err(poison) => (poison.into_inner().take(), true),
        };
        if handle.is_some_and(|handle| handle.join().is_err()) || owner_poisoned {
            return Err(());
        }
        Ok(())
    }

    pub(in crate::cas_projection::persistent_failure) fn seal_retention(
        &self,
    ) -> Result<PersistentFailureRecoveryInventoryCounts, ()> {
        let mut state = self.state.0.lock().map_err(|_| ())?;
        if state.phase != PersistentFailureCutState::Finished || state.sealed_counts.is_some() {
            return Err(());
        }
        let counts = state.recovery_inventory_counts();
        state.sealed_counts = Some(counts);
        Ok(counts)
    }

    pub(in crate::cas_projection::persistent_failure) fn recovery_inventory_observation(
        &self,
    ) -> PersistentFailureRecoveryInventoryObservation {
        match self.state.0.lock() {
            Ok(state) => PersistentFailureRecoveryInventoryObservation {
                retained_counts: state.recovery_inventory_counts(),
                late_publication_count: state.late_publication_count,
                retention_poisoned: false,
                pending_quarantine_available: state.pending_quarantine.is_available(),
            },
            Err(poison) => {
                let state = poison.into_inner();
                PersistentFailureRecoveryInventoryObservation {
                    retained_counts: state.recovery_inventory_counts(),
                    late_publication_count: state.late_publication_count,
                    retention_poisoned: true,
                    pending_quarantine_available: state.pending_quarantine.is_available(),
                }
            }
        }
    }

    #[cfg(test)]
    pub(in crate::cas_projection::persistent_failure) fn poison_recovery_inventory_retention_for_test(
        &self,
    ) {
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _state = self
                .state
                .0
                .lock()
                .expect("retention state starts unpoisoned");
            panic!("poison retained capability state for inventory test");
        }));
        assert!(panicked.is_err());
    }
}
