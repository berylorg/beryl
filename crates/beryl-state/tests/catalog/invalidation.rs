use super::*;

#[path = "invalidation/joined.rs"]
mod joined;
#[path = "invalidation/source.rs"]
mod source;

fn row(store: &HomeStore, state: &CatalogState, thread: u8) -> CatalogRow {
    state
        .row(
            store,
            SyndicThreadId::from_bytes([thread; 16]),
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap()
}

fn assert_stale_pair(store: &HomeStore, state: &CatalogState, original: &CatalogRow) -> CatalogRow {
    let stale = state
        .row(
            store,
            original.thread_id(),
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(stale.freshness(), CatalogFreshness::Stale);
    assert_eq!(stale.revision().get(), original.revision().get() + 1);
    assert_eq!(stale.sources(), original.sources());
    assert_eq!(stale.facts(), original.facts());
    assert_eq!(stale.recency_cursor(), original.recency_cursor());
    let page = state
        .recency_page(
            store,
            None,
            CursorReadLimits::new(10, 10 * CATALOG_MAX_STORED_RECENCY_BYTES).unwrap(),
        )
        .unwrap();
    assert_eq!(
        page.rows()
            .iter()
            .find(|item| item.thread_id() == stale.thread_id()),
        Some(&stale)
    );
    stale
}

#[test]
fn writer_time_invalidation_uses_latest_named_pair_after_unrelated_and_named_updates() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = open(directory.path());
    let original = seed_rows(&store, &state, 2);
    let command = state.invalidate_current(original[0].thread_id());
    assert_committed!(publish(
        &store,
        &state,
        2,
        CatalogRowExpectation::Revision(original[1].revision()),
        2,
        400,
        CatalogResolvedTitle::generated("Unrelated replacement").unwrap(),
    ));
    assert_committed!(publish(
        &store,
        &state,
        1,
        CatalogRowExpectation::Revision(original[0].revision()),
        3,
        500,
        CatalogResolvedTitle::generated("Latest named source facts").unwrap(),
    ));
    let latest = row(&store, &state, 1);
    let unrelated = row(&store, &state, 2);
    let home_before = store.home_revision().unwrap();
    let domain_before = state.revision(&store).unwrap();
    assert_committed!(store.execute_current(command));
    let stale = assert_stale_pair(&store, &state, &latest);
    assert_eq!(row(&store, &state, 2), unrelated);
    assert_eq!(
        store.home_revision().unwrap(),
        home_before.checked_next().unwrap()
    );
    assert_eq!(
        state.revision(&store).unwrap(),
        domain_before.checked_next().unwrap()
    );
    store.close().unwrap();
    let (reopened, fresh) = open(directory.path());
    assert_eq!(row(&reopened, &fresh, 1), stale);
    assert_eq!(row(&reopened, &fresh, 2), unrelated);
    reopened
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

#[test]
fn already_stale_writer_time_invalidation_validates_and_advances_without_changing_sources() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = open(directory.path());
    let original = seed_rows(&store, &state, 1).remove(0);
    assert_committed!(store.execute_current(state.invalidate_current(original.thread_id())));
    let stale = assert_stale_pair(&store, &state, &original);
    assert_committed!(store.execute_current(state.invalidate_current(original.thread_id())));
    let advanced = assert_stale_pair(&store, &state, &stale);
    let before = store.home_revision().unwrap();
    let old_api = execute(&store, &state, |revision| {
        state.mark_stale(
            revision,
            MarkCatalogRowStale::new(advanced.thread_id(), advanced.revision()),
        )
    });
    let CommandOutcome::NotCommitted { evidence } = old_api else {
        panic!("caller-fenced stale marker changed its already-stale contract")
    };
    assert!(matches!(
        contributor_source::<CatalogMutationError>(&evidence),
        Some(CatalogMutationError::AlreadyStale { .. })
    ));
    assert_eq!(store.home_revision().unwrap(), before);
    assert_eq!(row(&store, &state, 1), advanced);
    store.close().unwrap();
}

#[test]
fn writer_time_invalidation_rejects_missing_and_disagreeing_copies_including_stale_rows() {
    for stale_first in [false, true] {
        for corruption in 0..4 {
            let directory = tempfile::tempdir().unwrap();
            let (store, state) = open(directory.path());
            seed_rows(&store, &state, 2);
            let thread_id = SyndicThreadId::from_bytes([1; 16]);
            if stale_first {
                assert_committed!(store.execute_current(state.invalidate_current(thread_id)));
            }
            let target = row(&store, &state, 1);
            assert_committed!(execute(&store, &state, |revision| match corruption {
                0 => state.remove_copy_for_test(revision, target.clone(), true),
                1 => state.remove_copy_for_test(revision, target.clone(), false),
                2 => state.corrupt_recency_copy_for_test(
                    revision,
                    target.recency_cursor(),
                    row(&store, &state, 2)
                ),
                _ =>
                    state.corrupt_primary_copy_for_test(revision, thread_id, row(&store, &state, 2)),
            }));
            let home_before = store.home_revision().unwrap();
            let domain_before = state.revision(&store).unwrap();
            let unrelated = row(&store, &state, 2);
            if corruption == 3 {
                assert!(matches!(
                    state.row(&store, thread_id, CatalogPointReadLimit::schema_maximum()),
                    Err(CatalogReadError::Invariant(
                        "catalog point key does not match its row identity"
                    ))
                ));
            }
            let CommandOutcome::NotCommitted { evidence } =
                store.execute_current(state.invalidate_current(thread_id))
            else {
                panic!("invalidation accepted an incomplete or inconsistent pair")
            };
            let error = contributor_source::<CatalogMutationError>(&evidence).unwrap();
            assert!(matches!((corruption, error),
                (0, CatalogMutationError::RowMissing { thread_id: actual })
                | (1, CatalogMutationError::IndexMissing { thread_id: actual })
                | (2 | 3, CatalogMutationError::IndexMismatch { thread_id: actual }) if *actual == thread_id));
            assert_eq!(store.home_revision().unwrap(), home_before);
            assert_eq!(state.revision(&store).unwrap(), domain_before);
            assert_eq!(row(&store, &state, 2), unrelated);
            if corruption == 1 || corruption == 2 {
                assert_eq!(row(&store, &state, 1), target);
            } else if corruption == 3 {
                assert!(matches!(
                    state.row(&store, thread_id, CatalogPointReadLimit::schema_maximum()),
                    Err(CatalogReadError::Invariant(
                        "catalog point key does not match its row identity"
                    ))
                ));
            }
            store.close().unwrap();
        }
    }
}

#[test]
fn writer_time_invalidation_cancellation_and_precommit_failure_preserve_both_copies() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (store, state) = open_with_faults(directory.path(), faults.clone());
    let original = seed_rows(&store, &state, 1).remove(0);
    let home_before = store.home_revision().unwrap();
    let domain_before = state.revision(&store).unwrap();
    let cancellation = beryl_home_store::CommandCancellation::new();
    cancellation.cancel();
    assert!(matches!(
        store.execute_current(
            state
                .invalidate_current(original.thread_id())
                .with_cancellation(cancellation)
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(store.home_revision().unwrap(), home_before);
    assert_eq!(state.revision(&store).unwrap(), domain_before);
    assert_eq!(row(&store, &state, 1), original);
    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(matches!(
        store.execute_current(state.invalidate_current(original.thread_id())),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        store.health().state(),
        beryl_home_store::HomeHealthState::Failed
    );
    let recovery = store.recover_same_home().unwrap();
    let state = CatalogState::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish().unwrap();
    assert_eq!(store.home_revision().unwrap(), home_before);
    assert_eq!(state.revision(&store).unwrap(), domain_before);
    assert_eq!(row(&store, &state, 1), original);
    store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}

#[test]
fn writer_time_invalidation_reconciles_original_indeterminate_pair_without_resubmission() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (store, state) = open_with_faults(directory.path(), faults.clone());
    let original = seed_rows(&store, &state, 1).remove(0);
    let home_before = store.home_revision().unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle =
        indeterminate_handle(store.execute_current(state.invalidate_current(original.thread_id())));
    let ReconciliationResolution::ExactNew { receipt } = store.reconcile(&handle).unwrap() else {
        panic!("original invalidation outcome did not reconcile exact-new")
    };
    assert_eq!(receipt.home_revision(), home_before.checked_next().unwrap());
    assert_stale_pair(&store, &state, &original);
    assert!(store.pending_reconciliations().is_empty());
    assert_eq!(
        store.home_revision().unwrap(),
        home_before.checked_next().unwrap()
    );
    store.close().unwrap();
}

#[test]
fn exhausted_row_revision_refuses_invalidation_without_changing_either_exact_copy() {
    for stale_first in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (store, state) = open(directory.path());
        let original = seed_rows(&store, &state, 1).remove(0);
        if stale_first {
            assert_committed!(
                store.execute_current(state.invalidate_current(original.thread_id()))
            );
        }
        assert_committed!(execute(&store, &state, |revision| state
            .set_row_revision_for_test(
                revision,
                original.thread_id(),
                catalog::CatalogRevision::new(u64::MAX).unwrap()
            )));
        let exhausted = row(&store, &state, 1);
        let home_before = store.home_revision().unwrap();
        let domain_before = state.revision(&store).unwrap();
        let CommandOutcome::NotCommitted { evidence } =
            store.execute_current(state.invalidate_current(original.thread_id()))
        else {
            panic!("exhausted catalog revision admitted invalidation")
        };
        assert!(matches!(
            contributor_source::<CatalogMutationError>(&evidence),
            Some(CatalogMutationError::Value(
                catalog::CatalogValueError::CatalogRevisionExhausted
            ))
        ));
        assert_eq!(row(&store, &state, 1), exhausted);
        assert_eq!(store.home_revision().unwrap(), home_before);
        assert_eq!(state.revision(&store).unwrap(), domain_before);
        store
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .unwrap();
        store.close().unwrap();
    }
}
