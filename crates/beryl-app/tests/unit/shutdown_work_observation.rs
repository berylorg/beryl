use super::*;
use beryl_home_store::CursorReadLimits;
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/shutdown_support.rs"
));

fn fixture(pending: bool) -> Fixture {
    let mut fixture = Fixture::with_pending(pending);
    // Synthetic observations hold work stable; mutation tests change it explicitly.
    let scheduler = fixture.service.scheduler.take().unwrap();
    scheduler.request_shutdown();
    assert!(matches!(
        scheduler.join().unwrap(),
        crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerExit::Clean
    ));
    fixture
}

fn observe(fixture: &Fixture) -> ShutdownWorkObservation {
    let job = fixture
        .service
        .prepare_shutdown_observation(&fixture.sessions);
    std::thread::spawn(move || job.collect(&ProjectionCancellationToken::new()))
        .join()
        .unwrap()
        .unwrap()
}

#[test]
fn idle_and_hidden_pending_observations_preserve_state_and_execution_authority() {
    for pending in [false, true] {
        let fixture = fixture(pending);
        let permit = fixture.gate.execution_permit();
        let home = fixture.service.home.as_deref().unwrap();
        let before = home.home_revision().unwrap();
        let observation = observe(&fixture);
        assert_eq!(observation.has_work(), pending);
        assert_eq!(observation.running_threads(), u64::from(pending));
        fixture
            .service
            .validate_shutdown_work_revision(&fixture.sessions, observation.revision())
            .unwrap();
        assert_eq!(home.home_revision().unwrap(), before);
        permit.commit(|| ()).unwrap();
    }
}

#[test]
fn continuation_and_projection_flight_are_work_without_a_non_idle_gate() {
    let fixture = fixture(false);
    let held = fixture
        .service
        .stop_coordinator
        .compaction_custody
        .reserve_continuation(fixture.thread, fixture.turn)
        .unwrap();
    let observation = observe(&fixture);
    assert!(observation.has_work());
    assert_eq!(observation.running_threads(), 1);
    fixture
        .service
        .validate_shutdown_work_revision(&fixture.sessions, observation.revision())
        .unwrap();
    drop(held);
    assert!(
        fixture
            .service
            .validate_shutdown_work_revision(&fixture.sessions, observation.revision(),)
            .is_err()
    );
    assert!(!observe(&fixture).has_work());
    let flight = fixture.acquired_projection_flight(fixture.thread);
    assert_eq!(observe(&fixture).running_threads(), 1);
    drop(flight);
    assert!(!observe(&fixture).has_work());
}

#[test]
fn cancellation_and_foreign_sources_never_produce_an_idle_observation() {
    let foreign = fixture(false);
    let fixture = fixture(false);
    assert!(matches!(
        fixture
            .service
            .observe_shutdown_work(&foreign.sessions, &ProjectionCancellationToken::new(),),
        Err(ShutdownWorkError::Work(ProcessWorkError::ForeignSources))
    ));
    let cancellation = ProjectionCancellationToken::new();
    cancellation.cancel();
    assert!(
        fixture
            .service
            .observe_shutdown_work(&fixture.sessions, &cancellation)
            .is_err()
    );
    let cancellation = ProjectionCancellationToken::new();
    assert!(
        fixture
            .service
            .collect_shutdown_observation(&fixture.sessions, &cancellation, || cancellation
                .cancel(),)
            .is_err()
    );
}

#[test]
fn work_acquire_release_during_observation_invalidates_the_result() {
    let fixture = fixture(false);
    let result = fixture.service.collect_shutdown_observation(
        &fixture.sessions,
        &ProjectionCancellationToken::new(),
        || {
            drop(fixture.acquired_projection_flight(fixture.thread));
        },
    );
    assert!(matches!(
        result,
        Err(ShutdownWorkError::Work(ProcessWorkError::StaleRevision))
    ));
}

#[test]
fn durable_admission_during_observation_invalidates_the_result() {
    let fixture = fixture(false);
    let result = fixture.service.collect_shutdown_observation(
        &fixture.sessions,
        &ProjectionCancellationToken::new(),
        || {
            submission_fixture::submit_atoms(
                fixture.service.home.as_deref().unwrap(),
                fixture.service.storage.clone(),
                fixture.assets.clone(),
                fixture.thread,
                SyndicDraftId::from_bytes([81; 16]),
                SyndicItemId::from_bytes([82; 16]),
                &[submission_fixture::Atom::Text("new pending work")],
                83,
                SyndicTimestamp::from_unix_millis(4),
            );
        },
    );
    assert!(matches!(
        result,
        Err(ShutdownWorkError::Work(ProcessWorkError::StaleRevision))
    ));
    assert!(observe(&fixture).has_work());
}

#[test]
fn hidden_accepted_input_is_preserved_by_confirmation_observation() {
    let fixture = fixture(true);
    let home = fixture.service.home.as_deref().unwrap();
    let storage = &fixture.service.storage;
    let (kind, _) = submission_fixture::submit_atoms(
        home,
        storage.clone(),
        fixture.assets.clone(),
        fixture.thread,
        SyndicDraftId::from_bytes([91; 16]),
        SyndicItemId::from_bytes([92; 16]),
        &[submission_fixture::Atom::Text("preserve queued input")],
        93,
        SyndicTimestamp::from_unix_millis(4),
    );
    assert_eq!(kind, syndic_storage::FirstAcceptanceKind::Accepted);
    let revision = storage.revision(home).unwrap();
    let limits = CursorReadLimits::new(256, 65_536).unwrap();
    let before = storage
        .accepted_next_source_page(home, revision, None, limits)
        .unwrap();
    assert!(!before.records().is_empty());
    let home_revision = home.home_revision().unwrap();
    let permit = fixture.gate.execution_permit();
    assert_eq!(observe(&fixture).running_threads(), 1);
    assert_eq!(home.home_revision().unwrap(), home_revision);
    let after = storage
        .accepted_next_source_page(home, revision, None, limits)
        .unwrap();
    assert_eq!(after.records(), before.records());
    permit.commit(|| ()).unwrap();
}

#[test]
fn overlapping_durable_live_and_flight_work_counts_each_thread_once() {
    let fixture = fixture(true);
    let continuation = fixture
        .service
        .stop_coordinator
        .compaction_custody
        .reserve_continuation(fixture.thread, fixture.turn)
        .unwrap();
    let flight = fixture.acquired_projection_flight(fixture.thread);
    assert_eq!(observe(&fixture).running_threads(), 1);
    let other = SyndicThreadId::from_bytes([99; 16]);
    let other_flight = fixture.acquired_projection_flight(other);
    assert_eq!(observe(&fixture).running_threads(), 2);
    drop((continuation, flight, other_flight));
    assert_eq!(observe(&fixture).running_threads(), 1);
}

#[test]
fn flight_only_count_crosses_source_page_boundary_without_catalog_metadata() {
    let fixture = fixture(false);
    let flights: Vec<_> = (0_u128..260)
        .map(|id| fixture.acquired_projection_flight(SyndicThreadId::from_bytes(id.to_be_bytes())))
        .collect();
    assert_eq!(observe(&fixture).running_threads(), 260);
    drop(flights);
    assert_eq!(observe(&fixture).running_threads(), 0);
}

#[test]
fn failed_storage_read_cannot_claim_idle() {
    let fixture = fixture(false);
    fixture
        .faults
        .fail_next(beryl_home_store::test_faults::FaultPoint::BeforeReadConfirmation);
    assert!(
        fixture
            .service
            .observe_shutdown_work(&fixture.sessions, &ProjectionCancellationToken::new(),)
            .is_err()
    );
}
