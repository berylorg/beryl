#![cfg(feature = "test-faults")]

mod catalog_source_support;
mod support;

use beryl_home_store::{CommandCancellation, CursorReadLimits};
use syndic_storage::{
    AcceptGeneratedThreadTitle, FrozenThreadCatalogSummaryAuthentication as Authentication,
    GeneratedThreadTitle, SyndicPointReadLimit, SyndicReadError, SyndicStorage,
    THREAD_DISCOVERY_PAGE_MAX_BYTES, THREAD_DISCOVERY_PAGE_MAX_ITEMS, ThreadAttributesRevision,
    ThreadCatalogSourceWitnesses, ThreadCatalogSummaryRebuildReason as Reason,
    ThreadCatalogSummaryRecord, ThreadCatalogTitle, ThreadCatalogTitleSource,
    test_faults::{FixtureBatch, FixtureDelete, FixtureRecord},
};

use catalog_source_support::Fixture;
use support::{draft_id, id, open, timestamp};

fn limits() -> CursorReadLimits {
    CursorReadLimits::new(usize::MAX, THREAD_DISCOVERY_PAGE_MAX_BYTES).unwrap()
}

fn summary_with(
    old: &ThreadCatalogSummaryRecord,
    title: Option<ThreadCatalogTitle>,
    sources: ThreadCatalogSourceWitnesses,
) -> ThreadCatalogSummaryRecord {
    ThreadCatalogSummaryRecord::new(
        old.thread_id(),
        old.revision(),
        title,
        old.execution().clone(),
        old.archive(),
        old.last_activity_at(),
        old.complete(),
        old.parent_thread_id(),
        old.lineage_depth(),
        old.lineage_digest(),
        sources,
    )
}

#[test]
fn frozen_discovery_is_bounded_complete_and_unchanged_across_live_writes() {
    let fixture = Fixture::new("frozen-discovery-writes");
    for seed in 0..34 {
        fixture.create(seed);
    }
    let frozen = fixture
        .store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let mut after = None;
    let mut found = Vec::new();
    for seed in 100..103 {
        let page = fixture
            .storage
            .frozen_threads_page(&fixture.store, &frozen, after, limits())
            .unwrap();
        assert!(page.records().len() <= THREAD_DISCOVERY_PAGE_MAX_ITEMS);
        assert!(page.stored_bytes() <= THREAD_DISCOVERY_PAGE_MAX_BYTES);
        assert!(page.decoded_bytes() <= THREAD_DISCOVERY_PAGE_MAX_BYTES);
        found.extend(page.records().iter().map(|thread| thread.id()));
        after = page.records().last().map(|thread| thread.id());
        fixture.create(seed);
        if !page.has_more() {
            break;
        }
    }
    assert_eq!(found, (0..34).map(id).collect::<Vec<_>>());
    fixture.store.release_frozen_read(&frozen).unwrap();
}

#[test]
fn frozen_sources_stay_current_while_new_capture_detects_unadvanced_compact_witness() {
    let fixture = Fixture::new("frozen-source-witnesses");
    fixture.create(3);
    let expected = fixture.summary(3);
    let frozen = fixture
        .store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    fixture.change_activity(3, 10);
    assert_eq!(fixture.summary(3).revision(), expected.revision());
    assert_eq!(
        fixture
            .storage
            .authenticate_frozen_thread_catalog_summary(&fixture.store, &frozen, id(3))
            .unwrap(),
        Authentication::Current(expected)
    );
    let changed = fixture
        .store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert_eq!(
        fixture
            .storage
            .authenticate_frozen_thread_catalog_summary(&fixture.store, &changed, id(3))
            .unwrap(),
        Authentication::RebuildRequired(Reason::Outdated)
    );
    fixture.store.release_frozen_read(&frozen).unwrap();
    fixture.store.release_frozen_read(&changed).unwrap();
}

#[test]
fn derived_summary_absence_is_rebuild_work_but_canonical_thread_absence_is_distinct() {
    let fixture = Fixture::new("frozen-summary-absence");
    fixture.create(3);
    fixture.remove_summary(3);
    let frozen = fixture
        .store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert_eq!(
        fixture
            .storage
            .authenticate_frozen_thread_catalog_summary(&fixture.store, &frozen, id(3))
            .unwrap(),
        Authentication::RebuildRequired(Reason::Missing)
    );
    assert_eq!(
        fixture
            .storage
            .authenticate_frozen_thread_catalog_summary(&fixture.store, &frozen, id(4))
            .unwrap(),
        Authentication::ThreadMissing
    );
    let prepared = fixture.replacement(3);
    fixture.execute(fixture.storage.rebuild_thread_catalog_summary(prepared));
    assert_eq!(
        fixture
            .storage
            .authenticate_frozen_thread_catalog_summary(&fixture.store, &frozen, id(3))
            .unwrap(),
        Authentication::RebuildRequired(Reason::Missing)
    );
    fixture.store.release_frozen_read(&frozen).unwrap();
}

#[test]
fn canonical_source_absence_is_an_invariant_failure() {
    let fixture = Fixture::new("frozen-canonical-source-absence");
    fixture.create(3);
    let mut batch = FixtureBatch::new();
    batch.delete(FixtureDelete::ThreadExecution(id(3))).unwrap();
    fixture.batch(batch);
    let frozen = fixture
        .store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert!(matches!(
        fixture
            .storage
            .authenticate_frozen_thread_catalog_summary(&fixture.store, &frozen, id(3)),
        Err(SyndicReadError::Invariant(_))
    ));
    fixture.store.release_frozen_read(&frozen).unwrap();
}

#[test]
fn earned_history_title_authentication_does_not_load_history_again() {
    let fixture = Fixture::new("frozen-earned-history-title");
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
    let expected = fixture.summary(3);
    assert_eq!(
        expected.title().unwrap().source(),
        ThreadCatalogTitleSource::HistoryDerived
    );
    assert_eq!(expected.title().unwrap().text(), "Earned history title");
    let mut batch = FixtureBatch::new();
    batch.delete(FixtureDelete::Turn(turn)).unwrap();
    fixture.batch(batch);
    let frozen = fixture
        .store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert_eq!(
        fixture
            .storage
            .authenticate_frozen_thread_catalog_summary(&fixture.store, &frozen, id(3))
            .unwrap(),
        Authentication::Current(expected)
    );
    assert!(matches!(
        fixture
            .storage
            .prepare_thread_catalog_summary(&fixture.store, id(3)),
        Err(SyndicReadError::Invariant(_))
    ));
    fixture.store.release_frozen_read(&frozen).unwrap();
}

#[test]
fn generated_title_precedence_rejects_matching_witness_with_wrong_title_source() {
    let fixture = Fixture::new("frozen-generated-title-precedence");
    fixture.create(3);
    let item_id = beryl_model::SyndicItemId::from_bytes([5; 16]);
    let turn = support::exact_cas::submit_current_draft(
        &fixture.store,
        fixture.storage.clone(),
        id(3),
        draft_id(4),
        item_id,
        "History title",
        timestamp(10),
    );
    let limit = SyndicPointReadLimit::new(65_536).unwrap();
    let thread = fixture
        .storage
        .thread(&fixture.store, id(3), limit)
        .unwrap()
        .unwrap();
    let item = fixture
        .storage
        .canonical_item(&fixture.store, item_id, limit)
        .unwrap()
        .unwrap();
    let generated = GeneratedThreadTitle::new(
        "Generated title",
        turn,
        item.presentation_content().unwrap(),
        thread.selected_path_digest(),
        thread.revision(),
        timestamp(11),
    )
    .unwrap();
    fixture.execute(fixture.storage.accept_generated_thread_title(
        fixture.storage.revision(&fixture.store).unwrap(),
        AcceptGeneratedThreadTitle::new(id(3), ThreadAttributesRevision::FIRST, generated),
    ));
    fixture.execute(
        fixture
            .storage
            .rebuild_thread_catalog_summary(fixture.replacement(3)),
    );
    let good = fixture.summary(3);
    let frozen = fixture
        .store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert_eq!(
        fixture
            .storage
            .authenticate_frozen_thread_catalog_summary(&fixture.store, &frozen, id(3))
            .unwrap(),
        Authentication::Current(good.clone())
    );
    fixture.store.release_frozen_read(&frozen).unwrap();
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::ThreadCatalogSummary(summary_with(
            &good,
            Some(
                ThreadCatalogTitle::new(
                    "Generated title",
                    ThreadCatalogTitleSource::HistoryDerived,
                )
                .unwrap(),
            ),
            good.sources(),
        )))
        .unwrap();
    fixture.batch(batch);
    let frozen = fixture
        .store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert_eq!(
        fixture
            .storage
            .authenticate_frozen_thread_catalog_summary(&fixture.store, &frozen, id(3))
            .unwrap(),
        Authentication::RebuildRequired(Reason::Outdated)
    );
    fixture.store.release_frozen_read(&frozen).unwrap();
}

#[test]
fn future_source_witness_is_rejected_as_an_invariant() {
    let fixture = Fixture::new("frozen-future-witness");
    fixture.create(3);
    let old = fixture.summary(3);
    let source = old.sources();
    let future = ThreadCatalogSourceWitnesses::new(
        source.attributes_revision(),
        source.history_summary_revision().checked_next().unwrap(),
        source.history_thread_revision(),
        source.history_selected_path_digest(),
        source.thread_revision(),
    );
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::ThreadCatalogSummary(summary_with(
            &old,
            old.title().cloned(),
            future,
        )))
        .unwrap();
    fixture.batch(batch);
    let frozen = fixture
        .store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert!(matches!(
        fixture
            .storage
            .authenticate_frozen_thread_catalog_summary(&fixture.store, &frozen, id(3)),
        Err(SyndicReadError::Invariant(_))
    ));
    fixture.store.release_frozen_read(&frozen).unwrap();
}

#[test]
fn foreign_and_released_reads_cannot_authenticate_or_discover() {
    let first = Fixture::new("frozen-source-home-one");
    let second = Fixture::new("frozen-source-home-two");
    first.create(3);
    second.create(3);
    let frozen = first
        .store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert!(
        second
            .storage
            .authenticate_frozen_thread_catalog_summary(&second.store, &frozen, id(3))
            .is_err()
    );
    assert!(
        second
            .storage
            .frozen_threads_page(&second.store, &frozen, None, limits())
            .is_err()
    );
    first.store.release_frozen_read(&frozen).unwrap();
    assert!(
        first
            .storage
            .authenticate_frozen_thread_catalog_summary(&first.store, &frozen, id(3))
            .is_err()
    );
    assert!(
        first
            .storage
            .frozen_threads_page(&first.store, &frozen, None, limits())
            .is_err()
    );
}

#[test]
fn surviving_nonowning_read_does_not_delay_close_and_cannot_enter_reopened_generation() {
    let fixture = Fixture::new("frozen-source-generation");
    fixture.create(3);
    let frozen = fixture
        .store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let Fixture {
        store,
        storage,
        home,
    } = fixture;
    drop(storage);
    store.close().unwrap();
    let mut candidate = open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    assert!(
        storage
            .authenticate_frozen_thread_catalog_summary(&store, &frozen, id(3))
            .is_err()
    );
    assert!(
        storage
            .frozen_threads_page(&store, &frozen, None, limits())
            .is_err()
    );
}
