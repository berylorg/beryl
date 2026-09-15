use beryl_home_store::{
    CommandOutcome, CursorReadLimits, HomeCommand, HomeHealthState, HomeOpenCandidate,
    HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::SyndicItemId;
use syndic_storage::{
    InputGateRecord, InputGateState, SyndicReadError, SyndicStorage,
    test_faults::{FixtureBatch, FixtureDelete, FixtureRecord},
};

use crate::{
    recovery_support::{ordered_draft, ordered_id, pending_home, point_limit},
    support::{TestHome, batch, exact_cas, open, seed_canonical_empty_thread, timestamp},
};

fn limits(items: usize) -> CursorReadLimits {
    CursorReadLimits::new(items, 65_536).unwrap()
}

#[test]
fn unpublished_empty_startup_source_has_no_discovery_work() {
    let home = TestHome::new("candidate-empty-discovery");
    let mut candidate = open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let mut publication = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap();
    let page = storage
        .delivery_recovery_startup_page_candidate(
            &publication.recovery_access().unwrap(),
            None,
            limits(256),
        )
        .unwrap();
    assert!(page.records().is_empty());
    assert_eq!(page.stored_bytes(), 0);
    assert!(page.next_cursor().is_none());
    assert_eq!(publication.health().state(), HomeHealthState::Opening);
    beryl_home_store::test_faults::with_initial_publication_store(&publication, |store| {
        assert!(
            storage
                .delivery_recovery_startup_page(store, None, limits(256))
                .is_err()
        );
    });
    publication.close().unwrap();
}

#[test]
fn candidate_pages_preserve_bounds_identity_drift_and_explicit_rebase() {
    let fixture = pending_home("candidate-source-discovery", 1);
    for value in 2..=3 {
        let thread = ordered_id(value);
        seed_canonical_empty_thread(
            &fixture.store,
            fixture.storage.clone(),
            thread,
            ordered_draft(10_000 + value),
        );
        exact_cas::submit_current_draft(
            &fixture.store,
            fixture.storage.clone(),
            thread,
            ordered_draft(20_000 + value),
            SyndicItemId::from_bytes(*ordered_id(30_000 + value).as_bytes()),
            "pending",
            timestamp(3),
        );
    }
    let revision = fixture.storage.revision(&fixture.store).unwrap();
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    fixture.store.close().unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(fixture.home.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let mut publication = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap();
    let access = publication.recovery_access().unwrap();
    assert!(
        fixture
            .storage
            .delivery_recovery_startup_page_candidate(&access, None, limits(1))
            .is_err()
    );
    let first = storage
        .delivery_recovery_startup_page_candidate(&access, None, limits(1))
        .unwrap();
    assert_eq!(first.records().len(), 1);
    assert_eq!(first.records()[0].thread_id(), ordered_id(1));
    let cursor = first.next_cursor().unwrap();
    let bounded = storage
        .delivery_recovery_startup_page_candidate(
            &access,
            None,
            CursorReadLimits::new(256, 88).unwrap(),
        )
        .unwrap();
    assert_eq!(bounded.records().len(), 2);
    assert_eq!(bounded.stored_bytes(), 88);
    assert!(
        storage
            .delivery_recovery_startup_page_candidate(
                &access,
                None,
                CursorReadLimits::new(256, 43).unwrap()
            )
            .is_err()
    );

    let replacement = InputGateRecord::new(
        fixture.thread,
        gate.revision().checked_next().unwrap(),
        InputGateState::Idle,
        gate.accepted_high_water(),
        gate.route_generation_high_water(),
        gate.selected_route(),
        gate.live_steering_count(),
        gate.live_next_turn_count(),
        gate.live_logical_utf8_bytes(),
    )
    .unwrap();
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command
        .add(
            storage
                .clone()
                .fixture_contribution(revision, batch([FixtureRecord::InputGate(replacement)])),
        )
        .unwrap();
    assert!(matches!(
        access.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert!(matches!(
        storage.delivery_recovery_startup_page_candidate(&access, Some(cursor), limits(1)),
        Err(SyndicReadError::StaleNonIdleGateSourceScan)
    ));
    let rebased = storage
        .rebase_delivery_recovery_startup_cursor_candidate(&access, cursor)
        .unwrap();
    let second = storage
        .delivery_recovery_startup_page_candidate(&access, Some(rebased), limits(1))
        .unwrap();
    assert_eq!(second.records()[0].thread_id(), ordered_id(2));
    let remaining = storage
        .delivery_recovery_startup_page_candidate(&access, None, limits(256))
        .unwrap();
    assert_eq!(remaining.records().len(), 2);
    let store = publication.publish().unwrap();
    assert_eq!(
        storage
            .delivery_recovery_startup_page(&store, None, limits(256))
            .unwrap(),
        remaining
    );

    let foreign = pending_home("foreign-candidate-source", 10);
    let foreign_cursor = second.next_cursor().unwrap();
    foreign.store.close().unwrap();
    let mut foreign_candidate = open(foreign.home.path());
    let foreign_storage = SyndicStorage::register(&mut foreign_candidate).unwrap();
    let mut foreign_publication = foreign_candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap();
    assert!(
        foreign_storage
            .rebase_delivery_recovery_startup_cursor_candidate(
                &foreign_publication.recovery_access().unwrap(),
                foreign_cursor
            )
            .is_err()
    );
    foreign_publication.close().unwrap();

    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovered = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovered).unwrap();
    let access = recovered.recovery_access().unwrap();
    assert!(
        fresh
            .delivery_recovery_startup_page_candidate(&access, Some(foreign_cursor), limits(1))
            .is_err()
    );
    assert!(
        storage
            .delivery_recovery_startup_page_candidate(&access, None, limits(1))
            .is_err()
    );
    assert_eq!(
        fresh
            .delivery_recovery_startup_page_candidate(&access, None, limits(256))
            .unwrap()
            .records()
            .len(),
        2
    );
    recovered.publish().unwrap().close().unwrap();
}

#[test]
fn candidate_source_resolution_rejects_stable_missing_gate() {
    let fixture = pending_home("candidate-source-corruption", 11);
    let source = fixture
        .storage
        .non_idle_gate_source(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let mut removal = FixtureBatch::new();
    removal
        .delete(FixtureDelete::InputGate(fixture.thread))
        .unwrap();
    crate::support::commit(&fixture.store, fixture.storage.clone(), removal);
    crate::support::commit(
        &fixture.store,
        fixture.storage.clone(),
        batch([FixtureRecord::NonIdleGateSource {
            thread_id: fixture.thread,
            source,
        }]),
    );
    fixture.store.close().unwrap();
    let mut candidate = open(fixture.home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let mut publication = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap();
    assert!(matches!(
        storage.delivery_recovery_startup_page_candidate(
            &publication.recovery_access().unwrap(),
            None,
            limits(256)
        ),
        Err(SyndicReadError::Invariant(_))
    ));
    publication.close().unwrap();
}
