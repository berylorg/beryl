#![cfg(feature = "test-faults")]

mod catalog_source_support;
mod support;

use catalog_source_support::Fixture;
use support::{draft_id, id, timestamp};
use syndic_storage::{
    FrozenThreadCatalogSummaryAuthentication as Authentication, SyndicReadError,
    ThreadCatalogSummaryRebuildReason as Reason, ThreadCatalogTitleSource,
    test_faults::{FixtureBatch, FixtureDelete},
};

#[test]
fn live_compact_source_authenticates_current_and_refuses_unadvanced_witnesses() {
    let fixture = Fixture::new("live-compact-witnesses");
    fixture.create(3);
    let summary = fixture.summary(3);
    assert_eq!(
        fixture
            .storage
            .authenticate_thread_catalog_summary(&fixture.store, id(3))
            .unwrap(),
        Authentication::Current(summary)
    );
    fixture.change_activity(3, 10);
    assert_eq!(
        fixture
            .storage
            .authenticate_thread_catalog_summary(&fixture.store, id(3))
            .unwrap(),
        Authentication::RebuildRequired(Reason::Outdated)
    );
    fixture.execute(
        fixture
            .storage
            .rebuild_thread_catalog_summary(fixture.replacement(3)),
    );
    assert_eq!(
        fixture
            .storage
            .authenticate_thread_catalog_summary(&fixture.store, id(3))
            .unwrap(),
        Authentication::Current(fixture.summary(3))
    );
    fixture.remove_summary(3);
    assert_eq!(
        fixture
            .storage
            .authenticate_thread_catalog_summary(&fixture.store, id(3))
            .unwrap(),
        Authentication::RebuildRequired(Reason::Missing)
    );
    assert_eq!(
        fixture
            .storage
            .authenticate_thread_catalog_summary(&fixture.store, id(4))
            .unwrap(),
        Authentication::ThreadMissing
    );
}

#[test]
fn live_compact_source_requires_every_canonical_witness() {
    let fixture = Fixture::new("live-missing-execution-witness");
    fixture.create(3);
    let mut batch = FixtureBatch::new();
    batch.delete(FixtureDelete::ThreadExecution(id(3))).unwrap();
    fixture.batch(batch);
    assert!(matches!(
        fixture
            .storage
            .authenticate_thread_catalog_summary(&fixture.store, id(3)),
        Err(SyndicReadError::Invariant(_))
    ));
}

#[test]
fn live_earned_history_title_requires_no_history_traversal() {
    let fixture = Fixture::new("live-earned-history-title");
    fixture.create(3);
    let turn = support::exact_cas::submit_current_draft(
        &fixture.store,
        fixture.storage.clone(),
        id(3),
        draft_id(4),
        beryl_model::SyndicItemId::from_bytes([5; 16]),
        "Earned history title",
        timestamp(10),
    );
    fixture.execute(
        fixture
            .storage
            .rebuild_thread_catalog_summary(fixture.replacement(3)),
    );
    let summary = fixture.summary(3);
    assert_eq!(
        summary.title().unwrap().source(),
        ThreadCatalogTitleSource::HistoryDerived
    );
    let mut batch = FixtureBatch::new();
    batch.delete(FixtureDelete::Turn(turn)).unwrap();
    fixture.batch(batch);
    assert_eq!(
        fixture
            .storage
            .authenticate_thread_catalog_summary(&fixture.store, id(3))
            .unwrap(),
        Authentication::Current(summary)
    );
    assert!(matches!(
        fixture
            .storage
            .prepare_thread_catalog_summary(&fixture.store, id(3)),
        Err(SyndicReadError::Invariant(_))
    ));
}
