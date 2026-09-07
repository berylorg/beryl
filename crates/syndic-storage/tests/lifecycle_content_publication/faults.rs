use beryl_home_store::{
    CommandCancellation, CommandError, CommandOutcome, HomeHealthState, HomeOpenOptions,
    HomeSchemaVersion, HomeStore, ReconciliationResolution,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::ContentRevision;
use syndic_storage::SyndicStorage;

use super::{assert_exact, content_support::*};

fn fault_fixture(
    name: &str,
) -> (
    crate::support::TestHome,
    HomeStore,
    SyndicStorage,
    FaultController,
) {
    let home = crate::support::TestHome::new(&format!("lifecycle-content-{name}"));
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    (home, store, storage, faults)
}

#[test]
fn cancellation_before_admission_preserves_absence() {
    let (_home, store, storage) = fixture("cancelled");
    let before = store.home_revision().unwrap();
    let cancellation = CommandCancellation::new();
    let command = storage
        .current_publish_lifecycle_continuation_content()
        .with_cancellation(cancellation.clone());
    cancellation.cancel();
    assert!(matches!(
        store.execute_current(command),
        CommandOutcome::NotCommitted {
            evidence: CommandError::CancelledBeforeAdmission
        }
    ));
    assert!(snapshot(&store, &storage).is_empty());
    assert_eq!(store.home_revision().unwrap(), before);
    assert!(store.pending_reconciliations().is_empty());
    store.close().unwrap();
}

#[test]
fn foreign_and_retired_commands_cannot_publish() {
    let (_home, store, storage) = fixture("foreign-source");
    let (_other_home, other, other_storage) = fixture("foreign-target");
    assert!(matches!(
        other.execute_current(storage.current_publish_lifecycle_continuation_content()),
        CommandOutcome::NotCommitted {
            evidence: CommandError::ForeignDomain { .. }
        }
    ));
    assert!(snapshot(&other, &other_storage).is_empty());
    assert!(snapshot(&store, &storage).is_empty());
    other.close().unwrap();
    store.close().unwrap();

    let (_home, store, storage, faults) = fault_fixture("retired");
    let stale_command = storage.current_publish_lifecycle_continuation_content();
    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(matches!(
        store.execute_current(storage.current_publish_lifecycle_continuation_content()),
        CommandOutcome::NotCommitted {
            evidence: CommandError::Commit { .. }
        }
    ));
    let candidate = store.recover_same_home().unwrap();
    let current = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let store = candidate.publish();
    let before = store.home_revision().unwrap();
    assert!(matches!(
        store.execute_current(stale_command),
        CommandOutcome::NotCommitted {
            evidence: CommandError::ForeignDomain { .. }
        }
    ));
    assert_eq!(store.home_revision().unwrap(), before);
    assert!(snapshot(&store, &current).is_empty());
    assert_committed(
        store.execute_current(current.current_publish_lifecycle_continuation_content()),
    );
    assert_exact(&store, &current, ContentRevision::new(1).unwrap());
    store.close().unwrap();
}

#[test]
fn writer_faults_leave_exact_absence_or_the_complete_sealed_closure() {
    for (index, point) in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
        FaultPoint::AfterPersist,
    ]
    .into_iter()
    .enumerate()
    {
        let (_home, store, storage, faults) = fault_fixture(&format!("cut-{index}"));
        let before = store.home_revision().unwrap();
        faults.fail_next(point);
        let outcome =
            store.execute_current(storage.current_publish_lifecycle_continuation_content());
        if point == FaultPoint::AfterCommitBeforePersist {
            let CommandOutcome::Indeterminate {
                failure: CommandError::Persistence { .. },
                reconciliation,
            } = outcome
            else {
                panic!("expected retained commit ambiguity, got {outcome:?}");
            };
            let handle = reconciliation.install_and_handle();
            assert_eq!(store.pending_reconciliations().len(), 1);
            assert_eq!(store.health().state(), HomeHealthState::Healthy);
            match store.reconcile(&handle).unwrap() {
                ReconciliationResolution::ExactNew { receipt } => {
                    assert_eq!(receipt.home_revision(), before.checked_next().unwrap())
                }
                other => panic!("expected exact sealed publication, got {other:?}"),
            }
            assert!(store.pending_reconciliations().is_empty());
            assert_exact(&store, &storage, ContentRevision::new(1).unwrap());
            assert_already_published(
                store.execute_current(storage.current_publish_lifecycle_continuation_content()),
                ContentRevision::new(1).unwrap(),
            );
            store.close().unwrap();
            continue;
        }
        match (point, outcome) {
            (
                FaultPoint::BeforeCommit,
                CommandOutcome::NotCommitted {
                    evidence: CommandError::Commit { .. },
                },
            ) => {}
            (
                FaultPoint::AfterPersist,
                CommandOutcome::Committed {
                    later_failure: Some(CommandError::Persistence { .. }),
                    ..
                },
            ) => {}
            (_, outcome) => panic!("unexpected fault outcome: {outcome:?}"),
        }
        assert_eq!(store.health().state(), HomeHealthState::Failed);
        let candidate = store.recover_same_home().unwrap();
        let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let store = candidate.publish();
        assert!(store.pending_reconciliations().is_empty());
        if point == FaultPoint::BeforeCommit {
            assert!(snapshot(&store, &storage).is_empty());
            assert_eq!(store.home_revision().unwrap(), before);
        } else {
            assert_exact(&store, &storage, ContentRevision::new(1).unwrap());
        }
        store.close().unwrap();
    }
}

#[test]
fn reuse_never_enters_the_commit_or_acknowledgement_loss_boundaries() {
    for (index, point) in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
        FaultPoint::AfterPersist,
    ]
    .into_iter()
    .enumerate()
    {
        let (_home, store, storage, faults) = fault_fixture(&format!("reuse-cut-{index}"));
        assert_committed(
            store.execute_current(storage.current_publish_lifecycle_continuation_content()),
        );
        let before = snapshot(&store, &storage);
        let revision = store.home_revision().unwrap();
        faults.fail_next(point);
        assert_already_published(
            store.execute_current(storage.current_publish_lifecycle_continuation_content()),
            ContentRevision::new(1).unwrap(),
        );
        assert_eq!(snapshot(&store, &storage), before);
        assert_eq!(store.home_revision().unwrap(), revision);
        assert_eq!(store.health().state(), HomeHealthState::Healthy);
        assert!(store.pending_reconciliations().is_empty());
        store.close().unwrap();
    }
}
