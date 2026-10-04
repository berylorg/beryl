use super::*;

#[cfg(test)]
std::thread_local! {
    static FAIL_SCHEDULER_START: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(super) fn fail_scheduler_start_for_test(cleanup_fails: bool) {
    FAIL_SCHEDULER_START.set(Some(cleanup_fails));
}

impl ProjectionConnectionService {
    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection) fn outage_observer(
        &self,
    ) -> std::sync::Weak<super::super::outage_buffer::OutageInventory> {
        Arc::downgrade(&self.outage_inventory)
    }

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
        let health = home.health();
        if health.state() != HomeHealthState::Healthy {
            return Err(ProjectionCoordinatorError::HomeNotHealthy {
                state: health.state(),
                generation: health.generation(),
            });
        }
        let reference = Arc::new(home.service_reference());
        Self::construct(
            process,
            reference,
            home_generation,
            Some(home),
            storage,
            config,
            scheduled_ordinary_provider,
            InitialStartGate::ready(),
            storage_revision,
            recovery,
        )
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn new_borrowed_for_test(
        process: crate::process_admission::ProcessAdmissionGate,
        home: &HomeStore,
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
        let health = home.health();
        if health.state() != HomeHealthState::Healthy {
            return Err(ProjectionCoordinatorError::HomeNotHealthy {
                state: health.state(),
                generation: health.generation(),
            });
        }
        Self::construct(
            process,
            Arc::new(home.service_reference()),
            home_generation,
            None,
            storage,
            config,
            scheduled_ordinary_provider,
            InitialStartGate::ready(),
            storage_revision,
            recovery,
        )
    }

    pub(super) fn construct(
        process: crate::process_admission::ProcessAdmissionGate,
        home: Arc<HomeServiceReference>,
        home_generation: HomeGeneration,
        owned_home: Option<HomeStore>,
        storage: SyndicStorage,
        config: ProjectionServiceConfig,
        scheduled_ordinary_provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
        initial_start: Arc<InitialStartGate>,
        startup_storage_revision: DomainRevision,
        recovery: StartupRecoveryDiagnostics,
    ) -> Result<Self, ProjectionCoordinatorError> {
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
        let outage_inventory = Arc::new(super::super::outage_buffer::OutageInventory::new(
            super::super::persistent_failure::PersistentFailureCutIdentity::new(
                home.home_id(),
                home_generation,
                service_generation,
                super::super::PersistentFailureGeneration::FIRST,
            ),
            config.outage_buffer,
            config.outage_assembly,
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
            Arc::clone(&outage_inventory),
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
            outage_inventory,
            home_id: home.home_id(),
            home_generation,
            owned_home,
            initial_start: Arc::clone(&initial_start),
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
            resolution: Arc::new(super::super::process_tools::ResolutionAuthority::new(
                home.home_id(),
                home_generation,
                command_authorizer.clone(),
            )),
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
        let construction = (|| {
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
            #[cfg(test)]
            if let Some(cleanup_fails) = FAIL_SCHEDULER_START.replace(None) {
                if cleanup_fails {
                    service.connections.exhaust_revision_for_test();
                }
                return Err(ProjectionCoordinatorError::AcceptedInputSchedulerSpawn {
                    message: "injected scheduler start failure".to_owned(),
                });
            }
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
            Ok::<(), ProjectionCoordinatorError>(())
        })();
        if let Err(source) = construction {
            return Err(if service.close_inner().is_err() {
                ProjectionCoordinatorError::ServiceConstructionDisposal {
                    source: Box::new(source),
                }
            } else {
                source
            });
        }
        Ok(service)
    }
}
