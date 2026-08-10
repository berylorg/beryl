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
        Self::construct_with_startup_gate(
            home,
            storage,
            config,
            scheduled_ordinary_provider,
            ServiceStartupGate::open_gate(),
            ProjectionServiceStartupPreparation::Ready {
                storage_revision,
                recovery,
            },
        )
    }

    pub(in crate::cas_projection) fn new_dormant_with_startup_gate(
        home: Arc<HomeStore>,
        storage: SyndicStorage,
        config: ProjectionServiceConfig,
        scheduled_ordinary_provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
        startup_gate: Arc<ServiceStartupGate>,
    ) -> Result<Self, ProjectionCoordinatorError> {
        Self::construct_with_startup_gate(
            home,
            storage,
            config,
            scheduled_ordinary_provider,
            startup_gate,
            ProjectionServiceStartupPreparation::Dormant,
        )
    }

    fn construct_with_startup_gate(
        home: Arc<HomeStore>,
        storage: SyndicStorage,
        config: ProjectionServiceConfig,
        scheduled_ordinary_provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
        startup_gate: Arc<ServiceStartupGate>,
        startup: ProjectionServiceStartupPreparation,
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
        let persistent_failure_escrow =
            super::super::persistent_failure::PersistentFailureServiceEscrowReservation::reserve(
                home.home_id(),
                home_generation,
                service_generation,
            )?;
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
        let persistent_failure = PersistentFailureCoordinator::start_with_startup_gate(
            Arc::clone(&home),
            home.home_id(),
            home_generation,
            service_generation,
            command_gate.clone(),
            failure_notification,
            failure_receiver,
            Arc::clone(&stop_coordinator),
            Arc::clone(&connections),
            Arc::clone(&startup_gate),
        )
        .map_err(
            |error| ProjectionCoordinatorError::PersistentFailureWorkerSpawn {
                message: error.to_string(),
            },
        )?;
        let scheduler_signal = AcceptedInputSchedulerSignal::new();
        let context_compaction =
            super::super::context_compaction::ContextCompactionCoordinator::new_with_startup_gate(
                Arc::clone(&home),
                home.home_id(),
                home_generation,
                storage,
                Arc::clone(&connections),
                Arc::clone(&stop_coordinator),
                command_authorizer.clone(),
                scheduler_signal.clone(),
                Arc::clone(&startup_gate),
            )
            .map_err(|_| ProjectionCoordinatorError::ContextCompactionCoordinatorUnavailable)?;
        let scheduled_ordinary_provider = Arc::new(Mutex::new(scheduled_ordinary_provider));
        let recovered_projection_lane = RecoveredProjectionLane::new(
            config.worker_capacity().get(),
            Arc::clone(&startup_gate),
            scheduler_signal.clone(),
        );
        let workers = ProjectionWorkerPool::new_with_scheduler(
            config.worker_capacity(),
            scheduler_signal.clone(),
        );
        let scheduler = AcceptedInputScheduler::start_with_startup_gate(
            AcceptedInputSchedulerContext::new(
                Arc::clone(&home),
                home.home_id(),
                home_generation,
                storage,
                workers.clone(),
                Arc::clone(&connections),
                Arc::clone(&scheduled_ordinary_provider),
                command_gate.clone(),
                persistent_failure.projection_retainer(home.home_id(), home_generation),
                ActiveSteeringCancellationLifecycle::new(),
                scheduler_signal.clone(),
                recovered_projection_lane.clone(),
            ),
            startup_gate,
        )?;
        let startup = match startup {
            ProjectionServiceStartupPreparation::Dormant => ProjectionServiceStartupState::Dormant,
            ProjectionServiceStartupPreparation::Ready {
                storage_revision,
                recovery,
            } => {
                scheduler_signal.hand_off_recovery(recovery);
                ProjectionServiceStartupState::Ready { storage_revision }
            }
        };
        Ok(Self {
            home_id: home.home_id(),
            home_generation,
            home: Some(home),
            storage,
            startup,
            config,
            workers,
            service_generation,
            command_gate,
            command_authorizer,
            persistent_failure: Some(persistent_failure),
            persistent_failure_escrow: Some(persistent_failure_escrow),
            #[cfg(test)]
            admission_reconciliation_failures: AtomicUsize::new(0),
            #[cfg(test)]
            admission_reconciliation_pause: Arc::new(Mutex::new(None)),
            connections,
            stop_coordinator,
            context_compaction: Some(context_compaction),
            scheduler: Some(scheduler),
            scheduler_signal,
            recovered_projection_lane,
            scheduled_ordinary_provider: Some(scheduled_ordinary_provider),
            settled: false,
        })
    }
}
