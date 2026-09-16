use super::*;

impl ProjectionConnectionService {
    pub fn new(
        process: crate::process_admission::ProcessAdmissionGate,
        home: HomeStore,
        storage: SyndicStorage,
        config: ProjectionServiceConfig,
        scheduled_ordinary_provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
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
        let recovery = super::super::accepted_delivery_recovery::recover_startup(
            &home,
            home.home_id(),
            home_generation,
            &storage,
        )?;
        let storage_revision = storage
            .revision(&home)
            .map_err(|source| ProjectionCoordinatorError::SyndicRevisionUnavailable { source })?;
        Self::construct(
            process,
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
        process: crate::process_admission::ProcessAdmissionGate,
        owned_home: HomeStore,
        storage: SyndicStorage,
        config: ProjectionServiceConfig,
        scheduled_ordinary_provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
        initial_start: Arc<InitialStartGate>,
        startup_storage_revision: DomainRevision,
        recovery: StartupRecoveryDiagnostics,
    ) -> Result<Self, ProjectionCoordinatorError> {
        let home = Arc::new(owned_home.service_reference());
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
        let command_gate = MasterCommandGate::new(
            process,
            service_generation,
            Some(failure_notification.clone()),
        );
        let command_authorizer = command_gate.authorizer();
        let connections = ProjectionServiceConnectionRegistry::new(service_generation);
        let scheduler_signal = AcceptedInputSchedulerSignal::new();
        let mutation_observer = home.observe_mutations(scheduler_signal.idle_recheck_waker())?;
        let stop_coordinator = Arc::new(StopCoordinator::new(
            &home,
            home.home_id(),
            home_generation,
            storage.clone(),
            command_authorizer.clone(),
            scheduler_signal.clone(),
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
        let native_lineage_recovery = NativeLineageRecoveryControl::new(
            config.worker_capacity(),
            home.home_id(),
            home_generation,
            service_generation,
            scheduler_signal.clone(),
        );
        let workers = ProjectionWorkerPool::new_with_scheduler(
            config.worker_capacity(),
            scheduler_signal.clone(),
        );
        let mut service = Self {
            home_id: home.home_id(),
            home_generation,
            owned_home: Some(owned_home),
            home: Some(Arc::clone(&home)),
            storage: storage.clone(),
            startup_storage_revision,
            config,
            workers: workers.clone(),
            service_generation,
            command_gate: command_gate.clone(),
            command_authorizer: command_authorizer.clone(),
            persistent_failure: Some(persistent_failure),
            connections: Arc::clone(&connections),
            stop_coordinator: Arc::clone(&stop_coordinator),
            context_compaction: None,
            scheduler: None,
            scheduler_signal: scheduler_signal.clone(),
            mutation_observer,
            native_lineage_recovery: native_lineage_recovery.clone(),
            scheduled_ordinary_provider: Some(Arc::new(Mutex::new(scheduled_ordinary_provider))),
            runtime_interest: None,
            graceful_shutdown: Mutex::new(super::graceful_shutdown::ShutdownCoordinator::default()),
            settled: false,
        };
        service.context_compaction = Some(
            super::super::context_compaction::ContextCompactionCoordinator::new_with_initial_start(
                Arc::clone(&home),
                home.home_id(),
                home_generation,
                storage.clone(),
                Arc::clone(&connections),
                Arc::clone(&stop_coordinator),
                command_authorizer.clone(),
                scheduler_signal.clone(),
                Arc::clone(&initial_start),
            )
            .map_err(|_| ProjectionCoordinatorError::ContextCompactionCoordinatorUnavailable)?,
        );
        let scheduled_ordinary_provider = service
            .scheduled_ordinary_provider
            .as_ref()
            .expect("constructing service retains its execution provider");
        scheduled_ordinary_provider
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .attach(
                super::super::process_sessions::ScheduledExecutionProviderContext::new(
                    home.home_id(),
                    home_generation,
                    service_generation,
                    config,
                    command_authorizer.clone(),
                    Arc::downgrade(&connections),
                    scheduler_signal.clone(),
                ),
            );
        service.scheduler = Some(AcceptedInputScheduler::start_with_initial_start(
            AcceptedInputSchedulerContext::new(
                Arc::clone(&home),
                home.home_id(),
                home_generation,
                config.turn_start_admission_requirement(),
                storage.clone(),
                workers.clone(),
                Arc::clone(&connections),
                Arc::clone(scheduled_ordinary_provider),
                command_gate.clone(),
                service
                    .persistent_failure
                    .as_ref()
                    .expect("constructing service retains its failure coordinator")
                    .terminal_disposer(home.home_id(), home_generation),
                ActiveSteeringCancellationLifecycle::new(),
                scheduler_signal.clone(),
                native_lineage_recovery.clone(),
            ),
            initial_start,
        )?);
        scheduler_signal.hand_off_recovery(recovery);
        Ok(service)
    }
}
