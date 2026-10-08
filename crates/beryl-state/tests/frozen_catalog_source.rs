#![cfg(feature = "test-faults")]

#[path = "frozen_catalog_source/coverage.rs"]
mod coverage;
#[path = "frozen_catalog_source/support.rs"]
mod frozen_support;
#[path = "frozen_catalog_source/sources.rs"]
mod sources;
mod support;

use beryl_home_store::{CommandCancellation, CommandOutcome, CursorReadLimits, HomeStore};
use beryl_model::{RootId, RuntimeId, SyndicThreadId, WindowId};
use beryl_state::*;
use frozen_support::*;

#[test]
fn frozen_primary_and_recency_pages_keep_exact_rows_across_a_concurrent_writer() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let catalog = state.catalog();
    let original = seed_rows(&store, &state, 18);
    let frozen = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let revision = catalog.frozen_revision(&store, &frozen).unwrap();
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                publish(
                    &store,
                    &state,
                    1,
                    CatalogRowExpectation::Revision(original[0].revision()),
                    2,
                    9000,
                );
                publish(&store, &state, 19, CatalogRowExpectation::Missing, 1, 10000);
            })
            .join()
            .unwrap();
    });
    assert!(store.home_revision().unwrap() > frozen.home_revision());
    assert!(catalog.revision(&store).unwrap() > revision);
    assert_eq!(catalog.frozen_revision(&store, &frozen).unwrap(), revision);
    let mut after = None;
    let mut primary = Vec::new();
    loop {
        let page = catalog
            .frozen_primary_page(&store, &frozen, after, limits(4))
            .unwrap();
        assert!(page.records().len() <= 4);
        assert!(page.stored_bytes() <= limits(4).max_bytes());
        primary.extend_from_slice(page.records());
        if !page.has_more() {
            break;
        }
        after = Some(page.records().last().unwrap().thread_id());
    }
    assert_eq!(primary, original);
    let mut after = None;
    let mut recency = Vec::new();
    loop {
        let page = catalog
            .frozen_recency_page(&store, &frozen, after, limits(4))
            .unwrap();
        recency.extend_from_slice(page.rows());
        if !page.has_more() {
            break;
        }
        after = page.next_after();
    }
    let mut expected = original;
    expected.reverse();
    assert_eq!(recency, expected);
    assert!(
        catalog
            .frozen_row(
                &store,
                &frozen,
                thread(19),
                CatalogPointReadLimit::schema_maximum()
            )
            .unwrap()
            .is_none()
    );
    store.release_frozen_read(&frozen).unwrap();
}

#[test]
fn empty_and_stale_frozen_coverage_do_not_certify_source_readiness() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let empty = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert!(
        state
            .catalog()
            .frozen_primary_page(&store, &empty, None, limits(1))
            .unwrap()
            .records()
            .is_empty()
    );
    assert!(
        state
            .catalog()
            .frozen_recency_page(&store, &empty, None, limits(1))
            .unwrap()
            .rows()
            .is_empty()
    );
    store.release_frozen_read(&empty).unwrap();
    let original = seed_rows(&store, &state, 1);
    committed(support::execute(
        &store,
        state.catalog().mark_stale(
            state.catalog().revision(&store).unwrap(),
            MarkCatalogRowStale::new(thread(1), original[0].revision()),
        ),
    ));
    let stale = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert_eq!(
        state
            .catalog()
            .frozen_row(
                &store,
                &stale,
                thread(1),
                CatalogPointReadLimit::schema_maximum()
            )
            .unwrap()
            .unwrap()
            .freshness(),
        CatalogFreshness::Stale
    );
    assert_eq!(
        state
            .catalog()
            .frozen_primary_page(&store, &stale, None, limits(1))
            .unwrap()
            .records()
            .len(),
        1
    );
    assert_eq!(
        state
            .catalog()
            .frozen_recency_page(&store, &stale, None, limits(1))
            .unwrap()
            .rows()
            .len(),
        1
    );
    store.release_frozen_read(&stale).unwrap();
}

#[test]
fn foreign_released_and_retired_frozen_capabilities_are_refused_by_every_state_source() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    seed_rows(&store, &state, 1);
    support::create_host_runtime(
        &store,
        &state,
        10,
        11,
        r"C:\Codex\codex.exe",
        r"C:\Work\beryl",
    );
    let frozen = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let foreign_directory = tempfile::tempdir().unwrap();
    let (foreign, foreign_state) = support::open(foreign_directory.path());
    assert!(
        foreign_state
            .catalog()
            .frozen_primary_page(&foreign, &frozen, None, limits(1))
            .is_err()
    );
    assert!(
        foreign_state
            .runtime_roots()
            .frozen_catalog_source(&foreign, &frozen, runtime(), root())
            .is_err()
    );
    assert!(
        foreign_state
            .session()
            .frozen_thread_claim_catalog_source(&foreign, &frozen, thread(1))
            .is_err()
    );
    store.release_frozen_read(&frozen).unwrap();
    assert!(
        state
            .catalog()
            .frozen_row(
                &store,
                &frozen,
                thread(1),
                CatalogPointReadLimit::schema_maximum()
            )
            .is_err()
    );
    assert!(
        state
            .runtime_roots()
            .frozen_runtime(&store, &frozen, runtime())
            .is_err()
    );
    assert!(
        state
            .session()
            .frozen_window_claim_catalog_source(&store, &frozen, WindowId::from_bytes([1; 16]))
            .is_err()
    );
    let escaped = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    store.close().unwrap();
    let (reopened, fresh) = support::open(directory.path());
    assert!(
        fresh
            .catalog()
            .frozen_revision(&reopened, &escaped)
            .is_err()
    );
    assert!(
        fresh
            .runtime_roots()
            .frozen_root(&reopened, &escaped, root())
            .is_err()
    );
    assert!(
        fresh
            .session()
            .frozen_thread_claim_catalog_source(&reopened, &escaped, thread(1))
            .is_err()
    );
}
