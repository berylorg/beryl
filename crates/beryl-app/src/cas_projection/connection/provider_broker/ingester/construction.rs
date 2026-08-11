use super::*;

impl ProviderBrokerBuildError {
    pub(super) fn new(
        failure: ProviderBrokerBuildFailure,
        resources: ProviderBrokerBuildResources,
    ) -> Self {
        Self { failure, resources }
    }

    #[cfg(test)]
    pub(super) fn resource_snapshot(&self) -> ProviderBrokerBuildResourceSnapshot {
        self.resources.snapshot()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn retains_complete_unstarted_resources_for_test(&self) -> bool {
        let resources = self.resource_snapshot();
        resources.worker
            && resources.pages
            && resources.channel
            && resources.sink
            && resources.control
            && resources.ingester
            && resources.start_gate
            && resources.initial_start
    }
}

#[cfg(test)]
impl ProviderBrokerBuildResources {
    fn snapshot(&self) -> ProviderBrokerBuildResourceSnapshot {
        match self {
            Self::Worker { worker } => ProviderBrokerBuildResourceSnapshot {
                worker: worker.retains_worker(),
                pages: false,
                channel: false,
                sink: false,
                control: false,
                ingester: false,
                start_gate: false,
                initial_start: false,
            },
            Self::PagePool { worker, pages } => {
                let diagnostics = pages.diagnostics();
                ProviderBrokerBuildResourceSnapshot {
                    worker: worker.retains_worker(),
                    pages: diagnostics.page_count == 1,
                    channel: false,
                    sink: false,
                    control: false,
                    ingester: false,
                    start_gate: false,
                    initial_start: false,
                }
            }
            Self::Unstarted(unstarted) => ProviderBrokerBuildResourceSnapshot {
                worker: unstarted.worker.retains_worker(),
                pages: unstarted.control.pages.diagnostics().page_count == 1,
                channel: unstarted.launch.retains_ingester(),
                sink: true,
                control: true,
                ingester: unstarted.launch.retains_ingester(),
                start_gate: true,
                initial_start: true,
            },
        }
    }
}

impl std::fmt::Display for ProviderBrokerBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.failure.fmt(formatter)
    }
}

impl std::fmt::Debug for ProviderBrokerBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderBrokerBuildError")
            .field("failure", &self.failure)
            .finish_non_exhaustive()
    }
}

impl std::error::Error for ProviderBrokerBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.failure.source()
    }
}

impl ProviderBrokerBuildFault {
    fn page_pool_failure(self) -> Option<PagePoolError> {
        #[cfg(test)]
        if self == Self::PagePool {
            return Some(PagePoolError::AllocationFailed);
        }
        None
    }

    fn channel_failure(self) -> Option<ChannelBuildError> {
        #[cfg(test)]
        if self == Self::Channel {
            return Some(ChannelBuildError::AllocationFailed);
        }
        None
    }

    pub(super) fn spawn_failure(self) -> Option<std::io::Error> {
        #[cfg(test)]
        if self == Self::Spawn {
            return Some(std::io::Error::other(
                "injected provider broker spawn failure",
            ));
        }
        None
    }
}

impl ProviderBroker {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection::connection) fn prepare(
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        authority: Arc<ConnectionRegistryAuthority>,
        router: Arc<EventRouter>,
        stop_coordinator: Arc<crate::cas_projection::stop::StopCoordinator>,
        context_compaction: Arc<
            crate::cas_projection::context_compaction::ContextCompactionCoordinator,
        >,
        commands: LiveCommandAuthorizer,
        failure_notification: PersistentFailureNotification,
        worker: ProjectionWorkerPermit,
    ) -> Result<PreparedProviderBroker, ProviderBrokerBuildError> {
        Self::prepare_with_initial_start(
            home,
            home_id,
            home_generation,
            authority,
            router,
            stop_coordinator,
            context_compaction,
            commands,
            failure_notification,
            worker,
            InitialStartGate::ready(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection::connection) fn prepare_with_initial_start(
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        authority: Arc<ConnectionRegistryAuthority>,
        router: Arc<EventRouter>,
        stop_coordinator: Arc<crate::cas_projection::stop::StopCoordinator>,
        context_compaction: Arc<
            crate::cas_projection::context_compaction::ContextCompactionCoordinator,
        >,
        commands: LiveCommandAuthorizer,
        failure_notification: PersistentFailureNotification,
        worker: ProjectionWorkerPermit,
        initial_start: Arc<InitialStartGate>,
    ) -> Result<PreparedProviderBroker, ProviderBrokerBuildError> {
        Self::prepare_with_initial_start_inner(
            home,
            home_id,
            home_generation,
            authority,
            router,
            stop_coordinator,
            context_compaction,
            commands,
            failure_notification,
            worker,
            initial_start,
            ProviderBrokerBuildFault::None,
        )
    }

    /// Test-only preparation that exercises the ordinary broker spawn failure after
    /// every fixed broker resource has been constructed and retained by the build error.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection::connection) fn prepare_with_spawn_failure_for_test(
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        authority: Arc<ConnectionRegistryAuthority>,
        router: Arc<EventRouter>,
        stop_coordinator: Arc<crate::cas_projection::stop::StopCoordinator>,
        context_compaction: Arc<
            crate::cas_projection::context_compaction::ContextCompactionCoordinator,
        >,
        commands: LiveCommandAuthorizer,
        failure_notification: PersistentFailureNotification,
        worker: ProjectionWorkerPermit,
        initial_start: Arc<InitialStartGate>,
    ) -> Result<PreparedProviderBroker, ProviderBrokerBuildError> {
        Self::prepare_with_initial_start_inner(
            home,
            home_id,
            home_generation,
            authority,
            router,
            stop_coordinator,
            context_compaction,
            commands,
            failure_notification,
            worker,
            initial_start,
            ProviderBrokerBuildFault::Spawn,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_with_initial_start_inner(
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        authority: Arc<ConnectionRegistryAuthority>,
        router: Arc<EventRouter>,
        stop_coordinator: Arc<crate::cas_projection::stop::StopCoordinator>,
        context_compaction: Arc<
            crate::cas_projection::context_compaction::ContextCompactionCoordinator,
        >,
        commands: LiveCommandAuthorizer,
        failure_notification: PersistentFailureNotification,
        worker: ProjectionWorkerPermit,
        initial_start: Arc<InitialStartGate>,
        build_fault: ProviderBrokerBuildFault,
    ) -> Result<PreparedProviderBroker, ProviderBrokerBuildError> {
        let worker = Arc::new(ProviderBrokerWorkerOwner::new(worker));
        if let Some(error) = build_fault.page_pool_failure() {
            return Err(ProviderBrokerBuildError::new(
                ProviderBrokerBuildFailure::PagePool(error),
                ProviderBrokerBuildResources::Worker { worker },
            ));
        }
        let pages = match PagePool::new(
            NonZeroUsize::new(PROVIDER_PAGE_BYTES).expect("provider page size is nonzero"),
            NonZeroUsize::MIN,
        ) {
            Ok(pages) => pages,
            Err(error) => {
                return Err(ProviderBrokerBuildError::new(
                    ProviderBrokerBuildFailure::PagePool(error),
                    ProviderBrokerBuildResources::Worker { worker },
                ));
            }
        };
        if let Some(error) = build_fault.channel_failure() {
            return Err(ProviderBrokerBuildError::new(
                ProviderBrokerBuildFailure::Channel(error),
                ProviderBrokerBuildResources::PagePool { worker, pages },
            ));
        }
        let (sender, receiver) = match fixed_channel(NonZeroUsize::MIN) {
            Ok(channel) => channel,
            Err(error) => {
                return Err(ProviderBrokerBuildError::new(
                    ProviderBrokerBuildFailure::Channel(error),
                    ProviderBrokerBuildResources::PagePool { worker, pages },
                ));
            }
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let ack = Arc::new(AckSlot::new());
        let routing_failure = Arc::new(StickyRoutingFailure::default());
        let approval = Arc::new(ApprovalInterruptionSlot::new());
        let steering_results = Arc::new(SteeringResultSlot::default());
        #[cfg(feature = "test-faults")]
        let test_metrics =
            Arc::new(crate::cas_projection::test_faults::ProviderBrokerTestMetrics::default());
        let sink = BrokerSink::new(
            sender,
            Arc::clone(&ack),
            Arc::clone(&cancelled),
            #[cfg(feature = "test-faults")]
            home_id,
            #[cfg(feature = "test-faults")]
            Arc::clone(&test_metrics),
        );
        let control = Arc::new(ProviderBrokerControl {
            home: Arc::clone(&home),
            home_id,
            home_generation,
            authority: Arc::clone(&authority),
            router: Arc::clone(&router),
            stop_coordinator: Arc::clone(&stop_coordinator),
            context_compaction: Arc::clone(&context_compaction),
            commands: commands.clone(),
            failure_notification: failure_notification.clone(),
            cancelled: Arc::clone(&cancelled),
            ack: Arc::clone(&ack),
            routing_failure: Arc::clone(&routing_failure),
            approval: Arc::clone(&approval),
            steering_results: Arc::clone(&steering_results),
            pages: pages.clone(),
            loss: Mutex::new(()),
            #[cfg(feature = "test-faults")]
            test_metrics: Arc::clone(&test_metrics),
        });
        let launch = Arc::new(ProviderBrokerLaunchEscrow {
            ingester: Mutex::new(Some(Ingester {
                home: Arc::clone(&home),
                home_id,
                home_generation,
                authority: Arc::clone(&authority),
                router: Arc::clone(&router),
                stop_coordinator: Arc::clone(&stop_coordinator),
                context_compaction: Arc::clone(&context_compaction),
                commands: commands.clone(),
                failure_notification: failure_notification.clone(),
                receiver,
                ack: Arc::clone(&ack),
                cancelled: Arc::clone(&cancelled),
                routing_failure: Arc::clone(&routing_failure),
                approval: Arc::clone(&approval),
                steering_results: Arc::clone(&steering_results),
                pages,
                command: None,
                active: None,
                authority_lost: false,
                #[cfg(feature = "test-faults")]
                test_metrics: Arc::clone(&test_metrics),
            })),
        });
        let start_gate = Arc::new(ProviderBrokerStartGate::new());
        ProviderBrokerUnstarted {
            sink,
            control,
            launch,
            start_gate,
            initial_start,
            worker,
        }
        .spawn(authority.generation.get(), build_fault)
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection::connection) fn start(
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        authority: Arc<ConnectionRegistryAuthority>,
        router: Arc<EventRouter>,
        stop_coordinator: Arc<crate::cas_projection::stop::StopCoordinator>,
        context_compaction: Arc<
            crate::cas_projection::context_compaction::ContextCompactionCoordinator,
        >,
        commands: LiveCommandAuthorizer,
        failure_notification: PersistentFailureNotification,
        worker: ProjectionWorkerPermit,
    ) -> Result<
        (
            Box<dyn OrderedTurnStreamSink>,
            Arc<ProviderBrokerControl>,
            RunningProviderBrokerIngester,
        ),
        ProviderBrokerBuildError,
    > {
        let prepared = Self::prepare(
            home,
            home_id,
            home_generation,
            authority,
            router,
            stop_coordinator,
            context_compaction,
            commands,
            failure_notification,
            worker,
        )?;
        let PreparedProviderBroker {
            sink,
            control,
            ingester,
            start,
        } = prepared;
        Ok((sink, control, ingester.start(start)))
    }
}
