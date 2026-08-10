use crate::cas_projection::service::adoption::ReplacementResourceFailureForTest;

struct Phase94ResourceFailureFixture {
    _directory: tempfile::TempDir,
    error: PersistentFailureServiceAdoptionError,
    first: Arc<ProjectionConnection>,
    second: Arc<ProjectionConnection>,
    retained_home: Arc<beryl_home_store::HomeStore>,
    replacement_issues: Arc<AtomicUsize>,
    replacement_shutdowns: Arc<AtomicUsize>,
    first_server: admission_server::NormalTerminalServer,
    second_server: admission_server::NormalTerminalServer,
}

#[derive(Clone)]
struct Phase94ProviderProbe {
    issues: Arc<AtomicUsize>,
    shutdowns: Arc<AtomicUsize>,
}

impl ScheduledOrdinaryExecutionProvider for Phase94ProviderProbe {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        self.issues.fetch_add(1, Ordering::SeqCst);
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Clone, Copy)]
enum Phase94InjectedFailure {
    WorkerCapacity,
    BrokerSpawn,
}

impl Phase94InjectedFailure {
    fn select(self, connection_generation: u64) -> ReplacementResourceFailureForTest {
        match self {
            Self::WorkerCapacity => ReplacementResourceFailureForTest::WorkerCapacity {
                connection_generation,
            },
            Self::BrokerSpawn => ReplacementResourceFailureForTest::BrokerSpawn {
                connection_generation,
            },
        }
    }
}

fn phase94_resource_failure_fixture(
    failure: Phase94InjectedFailure,
) -> Phase94ResourceFailureFixture {
    let (directory, faults, state, _shutdowns, service) = service_with_worker_capacity(8);
    let first_server = admission_server::NormalTerminalServer::spawn_admission_only_controlled_close();
    let second_server = admission_server::NormalTerminalServer::spawn_admission_only_controlled_close();
    let first_connector = ManagedBackendClientConnector::for_lifecycle_test(
        first_server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let second_connector = ManagedBackendClientConnector::for_lifecycle_test(
        second_server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let first_session = service
        .admit_lifecycle_test_candidate(
            &first_connector,
            RuntimeId::from_bytes([194; 16]),
            CasProcessGeneration::new(94_194).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    first_server.wait_for_admission();
    let second_session = service
        .admit_lifecycle_test_candidate(
            &second_connector,
            RuntimeId::from_bytes([195; 16]),
            CasProcessGeneration::new(94_195).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    second_server.wait_for_admission();
    let first = Arc::clone(first_session.connection());
    let second = Arc::clone(second_session.connection());
    assert!(
        first.identity_observation().connection_generation()
            < second.identity_observation().connection_generation()
    );

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(first_session);
    drop(second_session);
    wait_until("the Phase 94 two-connection cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the complete old stable set remains recovery-owned")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let retained_home = Arc::clone(inventory.retained_home());
    let config = inventory.retained_service_config();
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    retained_home.recover_same_home().unwrap();
    let replacement_issues = Arc::new(AtomicUsize::new(0));
    let replacement_shutdowns = Arc::new(AtomicUsize::new(0));
    let replacement = UnpublishedProjectionConnectionService::from_recovered_home_with_replacement_resource_failure_for_test(
        Arc::clone(&retained_home),
        config,
        Box::new(Phase94ProviderProbe {
            issues: Arc::clone(&replacement_issues),
            shutdowns: Arc::clone(&replacement_shutdowns),
        }),
        failure.select(second.identity_observation().connection_generation()),
    )
    .unwrap();
    assert!(replacement.startup_gate().is_closed());
    let error = quarantine.adopt_unpublished_service(replacement).unwrap_err();

    assert!(error.inventory_reescrow_is_disarmed_for_test());
    assert!(first.forwarding_epoch_is_inert_and_detached_for_test());
    assert!(second.forwarding_epoch_is_inert_and_detached_for_test());
    assert_eq!(replacement_issues.load(Ordering::SeqCst), 0);
    assert_eq!(replacement_shutdowns.load(Ordering::SeqCst), 0);
    assert!(retained_home.home_revision().is_ok());

    Phase94ResourceFailureFixture {
        _directory: directory,
        error,
        first,
        second,
        retained_home,
        replacement_issues,
        replacement_shutdowns,
        first_server,
        second_server,
    }
}

#[test]
fn phase94_later_stable_connection_capacity_failure_retains_one_inert_owner() {
    let fixture = phase94_resource_failure_fixture(Phase94InjectedFailure::WorkerCapacity);

    assert_eq!(
        fixture.error.reason(),
        PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable
    );
    let diagnostics = fixture.error.replacement_resource_failure_diagnostics_for_test();
    assert!(diagnostics.selector_consumed());
    assert_eq!(diagnostics.prepared_connection_count(), 1);
    assert_eq!(diagnostics.preparation_failure_count(), 0);
    assert_eq!(diagnostics.inert_attachment_count(), 2);
    assert_eq!(diagnostics.replacement_worker_count(), 2);
    assert!(diagnostics.startup_fence_never_opened());
    assert!(diagnostics.startup_fence_cancelled());
    assert!(!diagnostics.broker_spawn_resources_retained());

    fixture.first_server.assert_quiet_and_close();
    fixture.second_server.assert_quiet_and_close();
    fixture.first_server.join();
    fixture.second_server.join();
    fixture.error.dispose().unwrap();
    assert_eq!(fixture.replacement_issues.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.replacement_shutdowns.load(Ordering::SeqCst), 1);
    assert!(fixture.retained_home.home_revision().is_ok());
    drop(fixture.first);
    drop(fixture.second);
}

#[test]
fn phase94_later_stable_connection_broker_spawn_failure_retains_fixed_resources() {
    let fixture = phase94_resource_failure_fixture(Phase94InjectedFailure::BrokerSpawn);

    assert_eq!(
        fixture.error.reason(),
        PersistentFailureServiceAdoptionReason::ConnectionPreparation
    );
    let diagnostics = fixture.error.replacement_resource_failure_diagnostics_for_test();
    assert!(diagnostics.selector_consumed());
    assert_eq!(diagnostics.prepared_connection_count(), 1);
    assert_eq!(diagnostics.preparation_failure_count(), 1);
    assert_eq!(diagnostics.inert_attachment_count(), 2);
    assert_eq!(diagnostics.replacement_worker_count(), 4);
    assert!(diagnostics.startup_fence_never_opened());
    assert!(diagnostics.startup_fence_cancelled());
    assert!(diagnostics.broker_spawn_resources_retained());

    fixture.first_server.assert_quiet_and_close();
    fixture.second_server.assert_quiet_and_close();
    fixture.first_server.join();
    fixture.second_server.join();
    fixture.error.dispose().unwrap();
    assert_eq!(fixture.replacement_issues.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.replacement_shutdowns.load(Ordering::SeqCst), 1);
    assert!(fixture.retained_home.home_revision().is_ok());
    drop(fixture.first);
    drop(fixture.second);
}

#[test]
fn phase94_worker_capacity_failure_implicit_drop_is_bounded_and_nonexecuting() {
    let fixture = phase94_resource_failure_fixture(Phase94InjectedFailure::WorkerCapacity);
    let (dropped_tx, dropped_rx) = std::sync::mpsc::sync_channel(1);

    let disposer = std::thread::spawn(move || {
        drop(fixture.error);
        dropped_tx.send((fixture.first, fixture.second)).unwrap();
    });
    let (first, second) = dropped_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("Phase 94 implicit inert-owner drop must never join replacement workers");
    disposer.join().unwrap();
    assert!(first.forwarding_epoch_is_inert_and_detached_for_test());
    assert!(second.forwarding_epoch_is_inert_and_detached_for_test());
    assert_eq!(fixture.replacement_issues.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.replacement_shutdowns.load(Ordering::SeqCst), 0);
    assert!(fixture.retained_home.home_revision().is_ok());

    drop(first);
    drop(second);
    fixture.first_server.assert_quiet_and_close();
    fixture.second_server.assert_quiet_and_close();
    fixture.first_server.join();
    fixture.second_server.join();
}
