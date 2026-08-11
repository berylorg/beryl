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
        Self::start_with_initial_start(
            home,
            home_id,
            home_generation,
            service_generation,
            gate,
            notification,
            receiver,
            stop_coordinator,
            connections,
            crate::cas_projection::initial_start::InitialStartGate::ready(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection) fn start_with_initial_start(
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
        initial_start: Arc<crate::cas_projection::initial_start::InitialStartGate>,
    ) -> Result<Self, std::io::Error> {
        let stop_requested = Arc::new(AtomicBool::new(false));
        let state = Arc::new((
            Mutex::new(CoordinatorState {
                phase: PersistentFailureCutState::Armed,
                failure_generation: None,
                target_count: 0,
                proven_nondispatch_count: 0,
                possible_dispatch_count: 0,
                disposed_projection_count: 0,
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
                if initial_start.wait() {
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
            proven_nondispatch_count: state.proven_nondispatch_count,
            possible_dispatch_count: state.possible_dispatch_count,
            disposed_projection_count: state.disposed_projection_count,
        }
    }

    pub(in crate::cas_projection) fn terminal_disposer(
        &self,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
    ) -> PersistentFailureTerminalDisposer {
        PersistentFailureTerminalDisposer {
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

    pub(in crate::cas_projection) fn dispose_terminal_authority(
        &self,
        identity: PersistentFailureCutIdentity,
    ) -> Result<(), ()> {
        let state = self.state.0.lock().map_err(|_| ())?;
        if state.failure_generation != Some(identity.failure_generation)
            || self.service_generation != identity.service_generation
        {
            return Err(());
        }
        Ok(())
    }
}
