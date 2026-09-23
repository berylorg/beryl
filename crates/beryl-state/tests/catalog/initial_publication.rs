use super::*;
use catalog::{CatalogInitialStatus, PreparedInitialCatalogPublication};

fn prepare(
    store: &HomeStore,
    state: &CatalogState,
    thread: u8,
) -> PreparedInitialCatalogPublication {
    state
        .prepare_initial_publication(
            store,
            SyndicThreadId::from_bytes([thread; 16]),
            sources(1, false),
            facts(
                thread,
                100,
                CatalogResolvedTitle::history_derived("Straße").unwrap(),
                false,
            ),
        )
        .unwrap()
}

fn commit(store: &HomeStore, prepared: PreparedInitialCatalogPublication) -> CommandOutcome {
    execute_at(
        store,
        store.home_revision().unwrap(),
        prepared.contribution(),
    )
}

#[test]
fn initial_publication_is_exact_normalized_and_rejects_duplicate_identity() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = open(directory.path());
    let prepared = prepare(&store, &state, 1);
    let witness = prepared.publication().clone();
    assert_eq!(
        state.initial_publication_status(&store, &witness).unwrap(),
        CatalogInitialStatus::Absent
    );
    assert_eq!(witness.row().facts().search().title(), "strasse");
    assert_committed!(commit(&store, prepared));
    assert_eq!(
        state.initial_publication_status(&store, &witness).unwrap(),
        CatalogInitialStatus::Exact
    );
    assert_eq!(
        state
            .row(
                &store,
                witness.row().thread_id(),
                CatalogPointReadLimit::schema_maximum()
            )
            .unwrap()
            .as_ref(),
        Some(witness.row())
    );
    let duplicate = commit(&store, prepare(&store, &state, 1));
    let CommandOutcome::NotCommitted { evidence } = duplicate else {
        panic!("duplicate publication committed")
    };
    assert!(matches!(
        contributor_source::<CatalogMutationError>(&evidence),
        Some(CatalogMutationError::RowExists { .. })
    ));
    store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}

#[test]
fn each_partial_copy_is_collision_and_cannot_be_overwritten() {
    for primary in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let (store, state) = open(directory.path());
        let prepared = prepare(&store, &state, 1);
        let witness = prepared.publication().clone();
        assert_committed!(commit(&store, prepared));
        assert_committed!(execute(&store, &state, |revision| state
            .remove_copy_for_test(revision, witness.row().clone(), primary)));
        assert_eq!(
            state.initial_publication_status(&store, &witness).unwrap(),
            CatalogInitialStatus::Collision
        );
        let before = store.home_revision().unwrap();
        let outcome = commit(&store, prepare(&store, &state, 1));
        let CommandOutcome::NotCommitted { evidence } = outcome else {
            panic!("partial copy was overwritten")
        };
        if primary {
            assert!(matches!(
                contributor_source::<CatalogMutationError>(&evidence),
                Some(CatalogMutationError::IndexExists { .. })
            ));
        } else {
            assert!(matches!(
                contributor_source::<CatalogMutationError>(&evidence),
                Some(CatalogMutationError::RowExists { .. })
            ));
        }
        assert_eq!(store.home_revision().unwrap(), before);
        store.close().unwrap();
    }
}

#[test]
fn different_row_or_recency_copy_is_collision() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = open(directory.path());
    let prepared = prepare(&store, &state, 1);
    let witness = prepared.publication().clone();
    assert_committed!(commit(&store, prepared));
    let other = prepare(&store, &state, 2);
    let other_row = other.publication().row().clone();
    assert_committed!(commit(&store, other));
    assert_committed!(execute(&store, &state, |revision| state
        .corrupt_recency_copy_for_test(
            revision,
            witness.row().recency_cursor(),
            other_row
        )));
    assert_eq!(
        state.initial_publication_status(&store, &witness).unwrap(),
        CatalogInitialStatus::Collision
    );
    store.close().unwrap();

    let directory = tempfile::tempdir().unwrap();
    let (store, state) = open(directory.path());
    let prepared = prepare(&store, &state, 1);
    let witness = prepared.publication().clone();
    assert_committed!(publish(
        &store,
        &state,
        1,
        CatalogRowExpectation::Missing,
        1,
        101,
        CatalogResolvedTitle::absent()
    ));
    assert_eq!(
        state.initial_publication_status(&store, &witness).unwrap(),
        CatalogInitialStatus::Collision
    );
    store.close().unwrap();
}

#[test]
fn preparation_rejects_stale_revision_and_foreign_home() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = open(directory.path());
    let stale = prepare(&store, &state, 1);
    let witness = stale.publication().clone();
    assert_committed!(commit(&store, prepare(&store, &state, 2)));
    assert!(matches!(
        commit(&store, stale),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        state.initial_publication_status(&store, &witness).unwrap(),
        CatalogInitialStatus::Absent
    );
    let foreign_directory = tempfile::tempdir().unwrap();
    let (foreign, foreign_state) = open(foreign_directory.path());
    assert!(
        foreign_state
            .initial_publication_status(&foreign, &witness)
            .is_err()
    );
    assert!(
        state
            .prepare_initial_publication(
                &foreign,
                witness.row().thread_id(),
                sources(1, false),
                witness.row().facts().clone()
            )
            .is_err()
    );
    assert!(matches!(
        commit(&foreign, prepare(&store, &state, 3)),
        CommandOutcome::NotCommitted { .. }
    ));
    foreign.close().unwrap();
    store.close().unwrap();
}

#[test]
fn candidate_recovery_preserves_exact_outcome_and_rejects_old_authority() {
    for (fault, expected) in [
        (FaultPoint::BeforeCommit, CatalogInitialStatus::Absent),
        (FaultPoint::AfterPersist, CatalogInitialStatus::Exact),
        (
            FaultPoint::AfterCommitBeforePersist,
            CatalogInitialStatus::Exact,
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let (store, state) = open_with_faults(directory.path(), faults.clone());
        let prepared = prepare(&store, &state, 1);
        let witness = prepared.publication().clone();
        let stale = prepare(&store, &state, 2);
        faults.fail_next(fault);
        match (fault, commit(&store, prepared)) {
            (FaultPoint::BeforeCommit, CommandOutcome::NotCommitted { .. }) => {}
            (
                FaultPoint::AfterPersist,
                CommandOutcome::Committed {
                    later_failure: Some(_),
                    ..
                },
            ) => {}
            (
                FaultPoint::AfterCommitBeforePersist,
                CommandOutcome::Indeterminate { reconciliation, .. },
            ) => {
                reconciliation.install();
            }
            (_, result) => panic!("unexpected result: {result:?}"),
        }
        if store.health().state() == beryl_home_store::HomeHealthState::Healthy {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
        }
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = CatalogState::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        assert!(
            state
                .initial_publication_status_candidate(&access, &witness)
                .is_err()
        );
        assert_eq!(
            fresh
                .initial_publication_status_candidate(&access, &witness)
                .unwrap(),
            expected
        );
        let mut command = HomeCommand::new(access.home_revision().unwrap());
        command.add(stale.contribution()).unwrap();
        assert!(matches!(
            access.execute(command),
            CommandOutcome::NotCommitted { .. }
        ));
        for pending in access.pending_reconciliations() {
            assert!(matches!(
                access.reconcile(&pending).unwrap(),
                ReconciliationResolution::ExactNew { .. }
            ));
        }
        let store = recovery.publish().unwrap();
        assert_eq!(
            fresh.initial_publication_status(&store, &witness).unwrap(),
            expected
        );
        store
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .unwrap();
        store.close().unwrap();
    }
}

#[test]
fn missing_index_keeps_ambiguous_publication_in_collision() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (store, state) = open_with_faults(directory.path(), faults.clone());
    let prepared = prepare(&store, &state, 1);
    let witness = prepared.publication().clone();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let CommandOutcome::Indeterminate { reconciliation, .. } = commit(&store, prepared) else {
        panic!("expected ambiguous commit")
    };
    assert_committed!(execute(&store, &state, |revision| state
        .remove_copy_for_test(revision, witness.row().clone(), false)));
    assert_eq!(
        state.initial_publication_status(&store, &witness).unwrap(),
        CatalogInitialStatus::Collision
    );
    reconciliation.install();
    assert_eq!(
        store
            .reconcile(&store.pending_reconciliations()[0])
            .unwrap(),
        ReconciliationResolution::Collision
    );
    assert_eq!(store.pending_reconciliations().len(), 1);
    store.close().unwrap();
}
