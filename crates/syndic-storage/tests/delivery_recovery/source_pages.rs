use beryl_home_store::{CursorReadLimits, WholeHomeScrubTrigger};
use beryl_model::InputGateRevision;
use syndic_storage::{
    NON_IDLE_GATE_PAGE_MAX_RECORDS, NonIdleGateSourceRecord, SyndicReadError, SyndicStorage,
    test_faults::{
        FixtureBatch, FixtureDelete, FixtureRecord, PhysicalCorruption, PhysicalFamily,
        inject_physical_corruption,
    },
};

use crate::{
    recovery_support::{ordered_id, pending_home, point_limit},
    support::{TestHome, batch, commit, open},
};

fn limits(items: usize) -> CursorReadLimits {
    CursorReadLimits::new(items, 65_536).unwrap()
}

#[test]
fn compact_pages_clamp_counts_and_honor_exact_stored_byte_limits() {
    let home = TestHome::new("compact-source-bounds");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    for start in (1..=300).step_by(20) {
        commit(
            &store,
            storage.clone(),
            batch(
                (start..start + 20).map(|value| FixtureRecord::NonIdleGateSource {
                    thread_id: ordered_id(value),
                    source: NonIdleGateSourceRecord::new(
                        ordered_id(value),
                        InputGateRevision::new(1).unwrap(),
                    ),
                }),
            ),
        );
    }
    let revision = storage.revision(&store).unwrap();
    let first = storage
        .non_idle_gate_source_page(
            &store,
            revision,
            None,
            CursorReadLimits::new(usize::MAX, usize::MAX).unwrap(),
        )
        .unwrap();
    assert_eq!(first.records().len(), NON_IDLE_GATE_PAGE_MAX_RECORDS);
    assert_eq!(first.records()[0].thread_id(), ordered_id(1));
    assert_eq!(first.records().last().unwrap().thread_id(), ordered_id(256));
    assert_eq!(first.stored_bytes(), 256 * (16 + 4 + 24));
    let second = storage
        .non_idle_gate_source_page(&store, revision, first.next_cursor(), limits(256))
        .unwrap();
    assert_eq!(second.records().len(), 44);
    assert_eq!(second.records()[0].thread_id(), ordered_id(257));
    assert!(second.next_cursor().is_none());
    let byte_limited = storage
        .non_idle_gate_source_page(
            &store,
            revision,
            None,
            CursorReadLimits::new(256, 88).unwrap(),
        )
        .unwrap();
    assert_eq!(byte_limited.records().len(), 2);
    assert_eq!(byte_limited.stored_bytes(), 88);
    assert!(byte_limited.next_cursor().is_some());
    assert!(
        storage
            .non_idle_gate_source_page(
                &store,
                revision,
                None,
                CursorReadLimits::new(256, 43).unwrap(),
            )
            .is_err()
    );
}

#[test]
fn exact_resolution_rejects_stable_missing_or_disagreeing_gate_authority() {
    let fixture = pending_home("source-anchor-corruption", 400);
    let source = fixture
        .storage
        .non_idle_gate_source(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let revision = fixture.storage.revision(&fixture.store).unwrap();
    let gate = fixture
        .storage
        .resolve_non_idle_gate_source(&fixture.store, revision, source, point_limit())
        .unwrap();
    assert_eq!(gate.revision(), source.gate_revision());

    let mut removal = FixtureBatch::new();
    removal
        .delete(FixtureDelete::InputGate(fixture.thread))
        .unwrap();
    commit(&fixture.store, fixture.storage.clone(), removal);
    assert!(matches!(
        fixture.storage.resolve_non_idle_gate_source(
            &fixture.store,
            revision,
            source,
            point_limit(),
        ),
        Err(SyndicReadError::StaleNonIdleGateSourceScan)
    ));
    commit(
        &fixture.store,
        fixture.storage.clone(),
        batch([FixtureRecord::NonIdleGateSource {
            thread_id: fixture.thread,
            source,
        }]),
    );
    let revision = fixture.storage.revision(&fixture.store).unwrap();
    assert!(matches!(
        fixture.storage.resolve_non_idle_gate_source(
            &fixture.store,
            revision,
            source,
            point_limit(),
        ),
        Err(SyndicReadError::Invariant(_))
    ));
    assert!(matches!(
        fixture
            .storage
            .delivery_recovery_startup_page(&fixture.store, None, limits(16),),
        Err(SyndicReadError::Invariant(_))
    ));
    assert!(matches!(
        fixture.storage.recovered_pending_page(
            &fixture.store,
            revision,
            None,
            limits(16),
            point_limit(),
        ),
        Err(SyndicReadError::Invariant(_))
    ));
}

#[test]
fn source_key_identity_disagreement_is_reported_before_publishing_a_page() {
    let fixture = pending_home("source-key-corruption", 401);
    commit(
        &fixture.store,
        fixture.storage.clone(),
        batch([FixtureRecord::NonIdleGateSource {
            thread_id: ordered_id(402),
            source: NonIdleGateSourceRecord::new(
                ordered_id(403),
                InputGateRevision::new(1).unwrap(),
            ),
        }]),
    );
    let revision = fixture.storage.revision(&fixture.store).unwrap();
    assert!(matches!(
        fixture
            .storage
            .non_idle_gate_source_page(&fixture.store, revision, None, limits(16),),
        Err(SyndicReadError::Invariant(_))
    ));
}

#[test]
fn recovery_discovery_does_not_visit_unselected_input_gate_rows() {
    let fixture = pending_home("source-only-discovery", 404);
    inject_physical_corruption(
        &fixture.store,
        fixture.storage.clone(),
        PhysicalFamily::InputGates,
        PhysicalCorruption::MalformedStoredKey,
    )
    .unwrap();
    let revision = fixture.storage.revision(&fixture.store).unwrap();
    let startup = fixture
        .storage
        .delivery_recovery_startup_page(&fixture.store, None, limits(16))
        .unwrap();
    assert_eq!(startup.records().len(), 1);
    assert_eq!(startup.records()[0].thread_id(), fixture.thread);
    let pending = fixture
        .storage
        .recovered_pending_page(&fixture.store, revision, None, limits(16), point_limit())
        .unwrap();
    assert_eq!(pending.records().len(), 1);
    assert_eq!(pending.records()[0].turn_id(), fixture.turn);
}

#[test]
fn old_generation_cursors_cannot_be_used_or_rebased_after_same_home_recovery() {
    let fixture = pending_home("source-generation-fence", 405);
    commit(
        &fixture.store,
        fixture.storage.clone(),
        batch([FixtureRecord::NonIdleGateSource {
            thread_id: ordered_id(406),
            source: NonIdleGateSourceRecord::new(
                ordered_id(406),
                InputGateRevision::new(1).unwrap(),
            ),
        }]),
    );
    let revision = fixture.storage.revision(&fixture.store).unwrap();
    let cursor = fixture
        .storage
        .non_idle_gate_source_page(&fixture.store, revision, None, limits(1))
        .unwrap()
        .next_cursor()
        .unwrap();
    assert!(
        fixture
            .store
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
    let candidate = fixture.store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let recovered = candidate.publish();
    let revision = storage.revision(&recovered).unwrap();
    assert!(matches!(
        storage.non_idle_gate_source_page(&recovered, revision, Some(cursor), limits(1),),
        Err(SyndicReadError::InvalidNonIdleGateSourceCursor)
    ));
    assert!(matches!(
        storage.rebase_non_idle_gate_source_cursor(&recovered, cursor),
        Err(SyndicReadError::InvalidNonIdleGateSourceCursor)
    ));
    recovered.close().unwrap();
}
