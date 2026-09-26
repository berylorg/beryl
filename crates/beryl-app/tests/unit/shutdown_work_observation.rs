use super::*;
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/shutdown_support.rs"
));

fn observe(fixture: &Fixture) -> ShutdownWorkObservation {
    fixture
        .service
        .observe_shutdown_work(&fixture.sessions, &ProjectionCancellationToken::new())
        .unwrap()
}

#[test]
fn idle_and_hidden_pending_observations_preserve_state_and_execution_authority() {
    for pending in [false, true] {
        let fixture = Fixture::with_pending(pending);
        let permit = fixture.gate.execution_permit();
        let home = fixture.service.home.as_deref().unwrap();
        let before = home.home_revision().unwrap();
        let observation = observe(&fixture);
        assert_eq!(observation.has_work(), pending);
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
    let fixture = Fixture::idle();
    let held = fixture
        .service
        .stop_coordinator
        .compaction_custody
        .reserve_continuation(fixture.thread, fixture.turn)
        .unwrap();
    let observation = observe(&fixture);
    assert!(observation.has_work());
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
    assert!(observe(&fixture).has_work());
    drop(flight);
    assert!(!observe(&fixture).has_work());
}

#[test]
fn cancellation_and_foreign_sources_never_produce_an_idle_observation() {
    let fixture = Fixture::idle();
    let foreign = Fixture::idle();
    assert!(matches!(
        fixture
            .service
            .observe_shutdown_work(&foreign.sessions, &ProjectionCancellationToken::new(),),
        Err(ProcessWorkError::ForeignSources)
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
    let fixture = Fixture::idle();
    let result = fixture.service.collect_shutdown_observation(
        &fixture.sessions,
        &ProjectionCancellationToken::new(),
        || {
            drop(fixture.acquired_projection_flight(fixture.thread));
        },
    );
    assert!(matches!(result, Err(ProcessWorkError::StaleRevision)));
}

#[test]
fn durable_admission_during_observation_invalidates_the_result() {
    let fixture = Fixture::idle();
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
    assert!(matches!(result, Err(ProcessWorkError::StaleRevision)));
    assert!(observe(&fixture).has_work());
}

#[test]
fn hidden_accepted_input_is_preserved_by_confirmation_observation() {
    let fixture = Fixture::new();
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
    assert!(observe(&fixture).has_work());
    assert_eq!(home.home_revision().unwrap(), home_revision);
    let after = storage
        .accepted_next_source_page(home, revision, None, limits)
        .unwrap();
    assert_eq!(after.records(), before.records());
    permit.commit(|| ()).unwrap();
}

#[test]
fn failed_storage_read_cannot_claim_idle() {
    let fixture = Fixture::idle();
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
