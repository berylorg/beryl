use super::*;

impl ProjectionConnectionService {
    pub fn new(
        home: HomeStore,
        storage: SyndicStorage,
        config: ProjectionServiceConfig,
        scheduled_ordinary_provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
    ) -> Result<Self, ProjectionCoordinatorError> {
        let home = Arc::new(home);
        let health = home.health();
        if health.state() != HomeHealthState::Healthy {
            return Err(ProjectionCoordinatorError::HomeNotHealthy {
                state: health.state(),
                generation: health.generation(),
            });
        }
        let Some(home_generation) = health.generation() else {
            return Err(ProjectionCoordinatorError::HealthyHomeGenerationMissing);
        };
        let recovery = super::super::accepted_delivery_recovery::recover_startup(
            &home,
            home.home_id(),
            home_generation,
            storage,
        )?;
        let storage_revision = storage
            .revision(&home)
            .map_err(|source| ProjectionCoordinatorError::SyndicRevisionUnavailable { source })?;
        Self::construct(
            home,
            storage,
            config,
            scheduled_ordinary_provider,
            InitialStartGate::ready(),
            storage_revision,
            recovery,
        )
    }

    fn construct(
        home: Arc<HomeStore>,
        storage: SyndicStorage,
        config: ProjectionServiceConfig,
        scheduled_ordinary_provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
        initial_start: Arc<InitialStartGate>,
        startup_storage_revision: DomainRevision,
        recovery: StartupRecoveryDiagnostics,
    ) -> Result<Self, ProjectionCoordinatorError> {
        let health = home.health();
        if health.state() != HomeHealthState::Healthy {
            return Err(ProjectionCoordinatorError::HomeNotHealthy {
                state: health.state(),
                generation: health.generation(),
            });
        }
        let Some(home_generation) = health.generation() else {
            return Err(ProjectionCoordinatorError::HealthyHomeGenerationMissing);
        };
        let service_generation = ProjectionServiceGeneration::allocate()
            .map_err(|_| ProjectionCoordinatorError::ProjectionServiceGenerationExhausted)?;
        let (failure_notification, failure_receiver) = persistent_failure_notification_channel(
            &home,
            home.home_id(),
            home_generation,
            service_generation,
        );
        let command_gate =
            MasterCommandGate::new(service_generation, Some(failure_notification.clone()));
        let command_authorizer = command_gate.authorizer();
        let connections = ProjectionServiceConnectionRegistry::new(service_generation);
        let stop_coordinator = Arc::new(StopCoordinator::new(
            &home,
            home.home_id(),
            home_generation,
            storage,
            command_authorizer.clone(),
        ));
        let persistent_failure = PersistentFailureCoordinator::start_with_initial_start(
            Arc::clone(&home),
            home.home_id(),
            home_generation,
            service_generation,
            command_gate.clone(),
            failure_notification,
            failure_receiver,
            Arc::clone(&stop_coordinator),
            Arc::clone(&connections),
            Arc::clone(&initial_start),
        )
        .map_err(
            |error| ProjectionCoordinatorError::PersistentFailureWorkerSpawn {
                message: error.to_string(),
            },
        )?;
        let scheduler_signal = AcceptedInputSchedulerSignal::new();
        let context_compaction =
            super::super::context_compaction::ContextCompactionCoordinator::new_with_initial_start(
                Arc::clone(&home),
                home.home_id(),
                home_generation,
                storage,
                Arc::clone(&connections),
                Arc::clone(&stop_coordinator),
                command_authorizer.clone(),
                scheduler_signal.clone(),
                Arc::clone(&initial_start),
            )
            .map_err(|_| ProjectionCoordinatorError::ContextCompactionCoordinatorUnavailable)?;
        let scheduled_ordinary_provider = Arc::new(Mutex::new(scheduled_ordinary_provider));
        let workers = ProjectionWorkerPool::new_with_scheduler(
            config.worker_capacity(),
            scheduler_signal.clone(),
        );
        let scheduler = AcceptedInputScheduler::start_with_initial_start(
            AcceptedInputSchedulerContext::new(
                Arc::clone(&home),
                home.home_id(),
                home_generation,
                config.turn_start_admission_requirement(),
                storage,
                workers.clone(),
                Arc::clone(&connections),
                Arc::clone(&scheduled_ordinary_provider),
                command_gate.clone(),
                persistent_failure.terminal_disposer(home.home_id(), home_generation),
                ActiveSteeringCancellationLifecycle::new(),
                scheduler_signal.clone(),
            ),
            initial_start,
        )?;
        scheduler_signal.hand_off_recovery(recovery);
        Ok(Self {
            home_id: home.home_id(),
            home_generation,
            home: Some(home),
            storage,
            startup_storage_revision,
            config,
            workers,
            service_generation,
            command_gate,
            command_authorizer,
            persistent_failure: Some(persistent_failure),
            connections,
            stop_coordinator,
            context_compaction: Some(context_compaction),
            scheduler: Some(scheduler),
            scheduler_signal,
            scheduled_ordinary_provider: Some(scheduled_ordinary_provider),
            settled: false,
        })
    }
}
