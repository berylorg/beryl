use beryl_home_store::{CurrentHomeCommand, DomainHandle};

use super::{source::*, *};

fn joined(
    state: &CatalogState,
    source: &DomainHandle<SourceDomain>,
    expected: Option<u8>,
    next: u8,
) -> CurrentHomeCommand {
    let mut command = CurrentHomeCommand::new(source.current_command(ChangeSource {
        key: 1,
        expected,
        next,
    }));
    command
        .add(state.invalidate_current(SyndicThreadId::from_bytes([1; 16])))
        .unwrap();
    command
}

#[test]
fn source_change_and_writer_time_invalidation_publish_together_after_unrelated_writes() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state, source) = open_source(directory.path(), FaultController::new());
    let original = seed_rows(&store, &state, 2);
    let command = joined(&state, &source, None, 7);
    assert_committed!(store.execute_current(source.current_command(ChangeSource {
        key: 2,
        expected: None,
        next: 9
    })));
    assert_committed!(publish(
        &store,
        &state,
        1,
        CatalogRowExpectation::Revision(original[0].revision()),
        2,
        900,
        CatalogResolvedTitle::generated("Latest before source change").unwrap()
    ));
    let latest = row(&store, &state, 1);
    let home_before = store.home_revision().unwrap();
    let catalog_before = state.revision(&store).unwrap();
    let source_before = store.domain_revision(&source).unwrap();
    assert_committed!(store.execute_current_home(command));
    assert_eq!(value(&store, &source, 1), Some(7));
    assert_eq!(value(&store, &source, 2), Some(9));
    assert_stale_pair(&store, &state, &latest);
    assert_eq!(row(&store, &state, 2), original[1]);
    assert_eq!(
        store.home_revision().unwrap(),
        home_before.checked_next().unwrap()
    );
    assert_eq!(
        state.revision(&store).unwrap(),
        catalog_before.checked_next().unwrap()
    );
    assert_eq!(
        store.domain_revision(&source).unwrap(),
        source_before.checked_next().unwrap()
    );
    store.close().unwrap();
}

#[test]
fn failed_source_or_catalog_preparation_never_publishes_the_other_participant() {
    for source_fails in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (store, state, source) = open_source(directory.path(), FaultController::new());
        let original = seed_rows(&store, &state, 1).remove(0);
        if !source_fails {
            assert_committed!(execute(&store, &state, |revision| state
                .remove_copy_for_test(revision, original.clone(), false)));
        }
        let home_before = store.home_revision().unwrap();
        let catalog_before = state.revision(&store).unwrap();
        let source_before = store.domain_revision(&source).unwrap();
        let CommandOutcome::NotCommitted { evidence } =
            store.execute_current_home(joined(&state, &source, source_fails.then_some(99), 7))
        else {
            panic!("a failed participant published part of the joined command")
        };
        if source_fails {
            assert!(matches!(
                contributor_source::<SourceError>(&evidence),
                Some(SourceError::Changed {
                    expected: Some(99),
                    actual: None
                })
            ));
        } else {
            assert!(matches!(
                contributor_source::<CatalogMutationError>(&evidence),
                Some(CatalogMutationError::IndexMissing { .. })
            ));
        }
        assert_eq!(value(&store, &source, 1), None);
        assert_eq!(row(&store, &state, 1), original);
        assert_eq!(store.home_revision().unwrap(), home_before);
        assert_eq!(state.revision(&store).unwrap(), catalog_before);
        assert_eq!(store.domain_revision(&source).unwrap(), source_before);
        store.close().unwrap();
    }
}

#[test]
fn joined_invalidation_indeterminate_outcome_reconciles_original_source_and_both_copies() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (store, state, source) = open_source(directory.path(), faults.clone());
    let original = seed_rows(&store, &state, 1).remove(0);
    let home_before = store.home_revision().unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = indeterminate_handle(store.execute_current_home(joined(&state, &source, None, 7)));
    let ReconciliationResolution::ExactNew { receipt } = store.reconcile(&handle).unwrap() else {
        panic!("joined original outcome did not reconcile exact-new")
    };
    assert_eq!(receipt.home_revision(), home_before.checked_next().unwrap());
    assert_eq!(value(&store, &source, 1), Some(7));
    assert_stale_pair(&store, &state, &original);
    assert!(store.pending_reconciliations().is_empty());
    assert_eq!(
        store.home_revision().unwrap(),
        home_before.checked_next().unwrap()
    );
    store.close().unwrap();
}

#[test]
fn joined_precommit_and_known_committed_failures_preserve_their_original_outcomes() {
    for fault in [FaultPoint::BeforeCommit, FaultPoint::AfterPersist] {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let (store, state, source) = open_source(directory.path(), faults.clone());
        let original = seed_rows(&store, &state, 1).remove(0);
        let home_before = store.home_revision().unwrap();
        faults.fail_next(fault);
        let committed = match (
            fault,
            store.execute_current_home(joined(&state, &source, None, 7)),
        ) {
            (FaultPoint::BeforeCommit, CommandOutcome::NotCommitted { .. }) => false,
            (
                FaultPoint::AfterPersist,
                CommandOutcome::Committed {
                    receipt,
                    later_failure: Some(_),
                    ..
                },
            ) => {
                assert_eq!(receipt.home_revision(), home_before.checked_next().unwrap());
                true
            }
            (_, outcome) => panic!("unexpected joined fault outcome: {outcome:?}"),
        };
        let (store, state, source) = recover_source(store);
        if committed {
            assert_eq!(value(&store, &source, 1), Some(7));
            assert_stale_pair(&store, &state, &original);
            assert_eq!(
                store.home_revision().unwrap(),
                home_before.checked_next().unwrap()
            );
        } else {
            assert_eq!(value(&store, &source, 1), None);
            assert_eq!(row(&store, &state, 1), original);
            assert_eq!(store.home_revision().unwrap(), home_before);
        }
        store.close().unwrap();
    }
}
