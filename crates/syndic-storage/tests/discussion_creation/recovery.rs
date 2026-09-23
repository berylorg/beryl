use super::*;
use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, ReconciliationResolution,
    test_faults::{FaultController, FaultPoint},
};
use syndic_storage::test_faults::{FixtureBatch, FixtureDelete};

fn fault_seeded(home: &TestHome, faults: FaultController) -> (HomeStore, SyndicStorage) {
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    seed_populated(&store, storage.clone());
    (store, storage)
}

#[test]
fn failed_creation_has_exact_old_or_new_candidate_outcome() {
    for (point, expected) in [
        (FaultPoint::BeforeCommit, ThreadCreationStatus::Absent),
        (FaultPoint::AfterPersist, ThreadCreationStatus::Exact),
        (
            FaultPoint::AfterCommitBeforePersist,
            ThreadCreationStatus::Exact,
        ),
    ] {
        let home = TestHome::new("discussion-creation-fault");
        let faults = FaultController::new();
        let (store, storage) = fault_seeded(&home, faults.clone());
        let prepared = prepare(&store, &storage);
        let intent = prepared.intent().clone();
        let command = command(&store, prepared);
        faults.fail_next(point);
        match (point, store.execute(command)) {
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
            (_, outcome) => panic!("unexpected fault result: {outcome:?}"),
        }
        if store.health().state() == beryl_home_store::HomeHealthState::Healthy {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
        }
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        assert!(
            storage
                .discussion_creation_status_candidate(&access, &intent)
                .is_err()
        );
        assert_eq!(
            fresh
                .discussion_creation_status_candidate(&access, &intent)
                .unwrap(),
            expected
        );
        assert_eq!(
            fresh
                .discussion_handoff_gate_candidate(&access, intent.thread_id(), limit())
                .unwrap()
                .is_some(),
            expected == ThreadCreationStatus::Exact
        );
        for pending in access.pending_reconciliations() {
            assert!(matches!(
                access.reconcile(&pending).unwrap(),
                ReconciliationResolution::ExactNew { .. }
            ));
        }
        let store = recovery.publish().unwrap();
        assert_eq!(
            fresh.discussion_creation_status(&store, &intent).unwrap(),
            expected
        );
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        store.close().unwrap();
    }
}

#[test]
fn missing_gate_prevents_exact_new_reconciliation_of_indeterminate_creation() {
    let home = TestHome::new("discussion-missing-gate-reconciliation");
    let faults = FaultController::new();
    let (store, storage) = fault_seeded(&home, faults.clone());
    let prepared = prepare(&store, &storage);
    let intent = prepared.intent().clone();
    let command = command(&store, prepared);
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let CommandOutcome::Indeterminate { reconciliation, .. } = store.execute(command) else {
        panic!("expected ambiguous creation")
    };
    assert_eq!(
        store.health().state(),
        beryl_home_store::HomeHealthState::Healthy
    );
    let mut deletion = FixtureBatch::new();
    deletion
        .delete(FixtureDelete::DiscussionHandoffGate(intent.thread_id()))
        .unwrap();
    support::commit(&store, storage.clone(), deletion);
    assert_eq!(
        storage.discussion_creation_status(&store, &intent).unwrap(),
        ThreadCreationStatus::Collision
    );
    reconciliation.install();
    let pending = store.pending_reconciliations().pop().unwrap();
    assert_eq!(
        store.reconcile(&pending).unwrap(),
        ReconciliationResolution::Collision
    );
    assert_eq!(store.pending_reconciliations().len(), 1);
    store.close().unwrap();
}

#[test]
fn creation_outcome_cannot_be_read_against_a_foreign_home() {
    let home = TestHome::new("discussion-intent-home");
    let (store, storage) = seeded(&home);
    let prepared = prepare(&store, &storage);
    let foreign_home = TestHome::new("discussion-intent-foreign");
    let (foreign, foreign_storage) = seeded(&foreign_home);
    assert!(
        foreign_storage
            .discussion_creation_status(&foreign, prepared.intent())
            .is_err()
    );
    foreign.close().unwrap();
    store.close().unwrap();
}
