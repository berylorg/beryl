use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use crate::cas_projection::{
    RunningSessionRecoverySupervisor, ScheduledOrdinaryExecutionProviderFactory,
    ScheduledOrdinaryProviderEpochContext,
};

struct Phase86ProviderFactory {
    epochs: Arc<Mutex<Vec<(BerylHomeId, HomeGeneration)>>>,
    provider_shutdowns: Arc<AtomicUsize>,
    factory_shutdowns: Arc<AtomicUsize>,
    stable_sessions: Arc<StableProviderSessionProbe>,
}

struct Phase86ProviderView {
    shutdowns: Arc<AtomicUsize>,
    stable_sessions: Arc<StableProviderSessionProbe>,
    active: bool,
}

struct StableProviderSessionProbe {
    factory_alive: AtomicBool,
    active_epoch_views: AtomicUsize,
    outstanding_checkouts: AtomicUsize,
    completed_checkouts: AtomicUsize,
    releases: AtomicUsize,
}

impl ScheduledOrdinaryExecutionProviderFactory for Phase86ProviderFactory {
    fn create_epoch(
        &mut self,
        context: ScheduledOrdinaryProviderEpochContext,
    ) -> Result<
        Box<dyn ScheduledOrdinaryExecutionProvider>,
        Box<dyn std::error::Error + Send + Sync + 'static>,
    > {
        self.epochs
            .lock()
            .unwrap()
            .push((context.home_id(), context.home_generation()));
        self.stable_sessions
            .active_epoch_views
            .fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(Phase86ProviderView {
            shutdowns: Arc::clone(&self.provider_shutdowns),
            stable_sessions: Arc::clone(&self.stable_sessions),
            active: true,
        }))
    }

    fn shutdown(&mut self) {
        self.factory_shutdowns.fetch_add(1, Ordering::SeqCst);
        assert_eq!(
            self.stable_sessions
                .active_epoch_views
                .load(Ordering::SeqCst),
            0
        );
        assert_eq!(
            self.stable_sessions
                .outstanding_checkouts
                .load(Ordering::SeqCst),
            0
        );
        if self
            .stable_sessions
            .factory_alive
            .swap(false, Ordering::SeqCst)
        {
            self.stable_sessions.releases.fetch_add(1, Ordering::SeqCst);
        }
    }
}

impl ScheduledOrdinaryExecutionProvider for Phase86ProviderView {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        assert!(self.stable_sessions.factory_alive.load(Ordering::SeqCst));
        self.stable_sessions
            .outstanding_checkouts
            .fetch_add(1, Ordering::SeqCst);
        self.stable_sessions
            .completed_checkouts
            .fetch_add(1, Ordering::SeqCst);
        assert_eq!(
            self.stable_sessions
                .outstanding_checkouts
                .fetch_sub(1, Ordering::SeqCst),
            1
        );
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        assert!(
            self.stable_sessions
                .active_epoch_views
                .fetch_sub(1, Ordering::SeqCst)
                > 0
        );
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn phase86_two_cycles_preserve_stable_connection_and_loaded_lease_identity() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut home = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut home).unwrap();
    let state = BerylState::register(&mut home).unwrap();
    let epochs = Arc::new(Mutex::new(Vec::new()));
    let provider_shutdowns = Arc::new(AtomicUsize::new(0));
    let factory_shutdowns = Arc::new(AtomicUsize::new(0));
    let stable_sessions = Arc::new(StableProviderSessionProbe {
        factory_alive: AtomicBool::new(true),
        active_epoch_views: AtomicUsize::new(0),
        outstanding_checkouts: AtomicUsize::new(0),
        completed_checkouts: AtomicUsize::new(0),
        releases: AtomicUsize::new(0),
    });
    let supervisor = RunningSessionRecoverySupervisor::start(
        home,
        ProjectionServiceConfig::try_new(8, 8).unwrap(),
        Box::new(Phase86ProviderFactory {
            epochs: Arc::clone(&epochs),
            provider_shutdowns: Arc::clone(&provider_shutdowns),
            factory_shutdowns: Arc::clone(&factory_shutdowns),
            stable_sessions: Arc::clone(&stable_sessions),
        }),
    )
    .unwrap();
    wait_until("the supervisor's initial scheduler pass", || {
        supervisor.acquire().is_ok_and(|service| {
            service
                .accepted_input_scheduler_diagnostics()
                .recovered_pending_pass_count()
                >= 1
        })
    });

    let service = supervisor.acquire().unwrap();
    let retained_home = service.retained_home_for_recovery();
    let home_pointer = Arc::as_ptr(&retained_home);
    let home_id = service.home_id();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let runtime_id = RuntimeId::from_bytes([204; 16]);
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            runtime_id,
            CasProcessGeneration::new(84_204).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();
    let connection = Arc::clone(session.connection());
    let connection_pointer = Arc::as_ptr(&connection);
    let stable_identity = connection.identity_observation();
    let initial_epoch = connection.epoch_identity_for_adoption_test().unwrap();
    let initial_epoch_pointer = connection.epoch_pointer_for_adoption_test().unwrap();
    let syndic_thread_id = SyndicThreadId::from_bytes([205; 16]);
    let cas_thread_id = CasThreadId::new("phase-86-two-cycle-candidate").unwrap();
    let execution = phase79_execution_binding(runtime_id, 206);
    let (binding_revision, lineage) = phase83_establish_pending_ordinary(
        &retained_home,
        storage,
        state,
        syndic_thread_id,
        204,
        execution.clone(),
        cas_thread_id.clone(),
    );
    let candidate_lease = phase79_register_candidate_lease(
        &service,
        &connection,
        cas_thread_id.clone(),
        syndic_thread_id,
    );
    let coordinator = CasProjectionCoordinator::for_healthy_home(&retained_home).unwrap();
    let projection = LoadedCasProjection::new(
        &coordinator,
        syndic_thread_id,
        binding_revision,
        execution.clone(),
        cas_thread_id,
        candidate_lease,
        lineage,
    );
    exercise_stable_provider_checkout(
        &service,
        SyndicThreadId::from_bytes([207; 16]),
        execution.clone(),
    );
    wait_until("the initial candidate worker hold", || {
        service.worker_pool_diagnostics().active() == 3
    });
    let registry_identity =
        crate::cas_projection::connection::registry::recovery_audit(&[stable_identity])
            .unwrap()
            .into_observations();

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(projection);
    drop(session);
    drop(service);
    drop(retained_home);

    wait_for_recovered_candidate(&supervisor, 1);
    let first = supervisor.acquire().unwrap();
    let first_home = first.retained_home_for_recovery();
    let first_epoch = connection.epoch_identity_for_adoption_test().unwrap();
    let first_epoch_pointer = connection.epoch_pointer_for_adoption_test().unwrap();
    assert_eq!(Arc::as_ptr(&first_home), home_pointer);
    assert_eq!(Arc::as_ptr(&connection), connection_pointer);
    assert_eq!(connection.identity_observation(), stable_identity);
    assert_eq!(first_epoch.home_id(), home_id);
    assert!(first_epoch.home_generation() > initial_epoch.home_generation());
    assert!(first_epoch.service_generation() > initial_epoch.service_generation());
    assert_ne!(first_epoch_pointer, initial_epoch_pointer);
    assert_eq!(registry_observations(stable_identity), registry_identity);
    exercise_stable_provider_checkout(
        &first,
        SyndicThreadId::from_bytes([208; 16]),
        execution.clone(),
    );

    fail_home_through_live_command(&first, first.state(), &faults);
    assert_eq!(
        first.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(first_home);
    drop(first);

    wait_for_recovered_candidate(&supervisor, 2);
    let second = supervisor.acquire().unwrap();
    let second_home = second.retained_home_for_recovery();
    let second_epoch = connection.epoch_identity_for_adoption_test().unwrap();
    let second_epoch_pointer = connection.epoch_pointer_for_adoption_test().unwrap();
    assert_eq!(Arc::as_ptr(&second_home), home_pointer);
    assert_eq!(Arc::as_ptr(&connection), connection_pointer);
    assert_eq!(connection.identity_observation(), stable_identity);
    assert_eq!(second_epoch.home_id(), home_id);
    assert!(second_epoch.home_generation() > first_epoch.home_generation());
    assert!(second_epoch.service_generation() > first_epoch.service_generation());
    assert_ne!(second_epoch_pointer, first_epoch_pointer);
    assert_eq!(registry_observations(stable_identity), registry_identity);
    exercise_stable_provider_checkout(&second, SyndicThreadId::from_bytes([209; 16]), execution);
    drop((second_home, second));

    let epoch_facts = epochs.lock().unwrap().clone();
    assert_eq!(epoch_facts.len(), 3);
    assert!(epoch_facts.iter().all(|(id, _)| *id == home_id));
    assert!(epoch_facts[0].1 < epoch_facts[1].1 && epoch_facts[1].1 < epoch_facts[2].1);
    assert_eq!(provider_shutdowns.load(Ordering::SeqCst), 2);
    assert_eq!(factory_shutdowns.load(Ordering::SeqCst), 0);
    assert!(stable_sessions.factory_alive.load(Ordering::SeqCst));
    assert_eq!(stable_sessions.active_epoch_views.load(Ordering::SeqCst), 1);
    assert_eq!(
        stable_sessions.outstanding_checkouts.load(Ordering::SeqCst),
        0
    );
    assert!(stable_sessions.completed_checkouts.load(Ordering::SeqCst) >= 3);
    assert_eq!(stable_sessions.releases.load(Ordering::SeqCst), 0);

    supervisor.shutdown().unwrap();
    assert!(registry_observations(stable_identity).is_empty());
    assert_eq!(provider_shutdowns.load(Ordering::SeqCst), 3);
    assert_eq!(factory_shutdowns.load(Ordering::SeqCst), 1);
    assert!(!stable_sessions.factory_alive.load(Ordering::SeqCst));
    assert_eq!(stable_sessions.active_epoch_views.load(Ordering::SeqCst), 0);
    assert_eq!(stable_sessions.releases.load(Ordering::SeqCst), 1);
    drop(connection);
    server.join();
    drop(directory);
}

fn exercise_stable_provider_checkout(
    service: &ProjectionConnectionService,
    probe_thread_id: SyndicThreadId,
    execution: ExecutionBinding,
) {
    // The probe must not compete for the retained candidate's exact scheduler flight.
    let worker = service.try_acquire_scheduled_ordinary_worker().unwrap();
    let flight = service
        .begin_scheduled_ordinary_flight(probe_thread_id)
        .unwrap();
    let outcome = service
        .issue_scheduled_ordinary_execution(probe_thread_id, execution, worker, flight)
        .unwrap();
    assert!(matches!(
        outcome,
        ScheduledOrdinaryAdmissionResult::Unavailable(
            ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady
        )
    ));
}

fn wait_for_recovered_candidate(
    supervisor: &RunningSessionRecoverySupervisor,
    expected_cycle: u64,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let supervisor_diagnostics = supervisor.diagnostics();
        let scheduler_diagnostics = supervisor
            .acquire()
            .ok()
            .map(|service| service.accepted_input_scheduler_diagnostics());
        if supervisor_diagnostics.recovery_cycles() == expected_cycle
            && scheduler_diagnostics
                .is_some_and(|diagnostics| diagnostics.recovered_projection_retained() == 1)
        {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for recovered candidate after cycle {expected_cycle}; supervisor={supervisor_diagnostics:?}; scheduler={scheduler_diagnostics:?}"
        );
        std::thread::yield_now();
    }
}

fn registry_observations(
    stable_identity: crate::cas_projection::connection::ProjectionConnectionIdentityObservation,
) -> Vec<crate::cas_projection::connection::registry::LoadedRegistryRecoveryObservation> {
    crate::cas_projection::connection::registry::recovery_audit(&[stable_identity])
        .unwrap()
        .into_observations()
}
