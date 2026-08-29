use beryl_home_store::{
    CommandError, CommandOutcome, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use syndic_storage::*;

use crate::{
    accepted_support::{AcceptedOperation, assert_operation_committed, seed_operation},
    support::TestHome,
};

fn open_with_faults(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

#[test]
fn every_delivery_transition_fault_cut_reconciles_to_exact_prior_or_successor() {
    for operation in AcceptedOperation::ALL {
        for (cut_name, point, expected) in [
            (
                "before-commit",
                FaultPoint::BeforeCommit,
                Some(AcceptedInputDeliveryTransitionStatus::Prior),
            ),
            (
                "after-commit-before-persist",
                FaultPoint::AfterCommitBeforePersist,
                None,
            ),
            (
                "after-persist",
                FaultPoint::AfterPersist,
                Some(AcceptedInputDeliveryTransitionStatus::Exact),
            ),
        ] {
            let name = format!("phase53-{}-fault-{cut_name}", operation.name());
            let home = TestHome::new(&name);
            let faults = FaultController::new();
            let mut store = open_with_faults(home.path(), faults.clone());
            let storage = SyndicStorage::register(&mut store).unwrap();
            seed_operation(&store, &storage, operation);

            faults.fail_next(point);
            let reconciliation = match (
                point,
                store.execute_current(operation.current_command(&storage)),
            ) {
                (FaultPoint::BeforeCommit, CommandOutcome::NotCommitted { evidence }) => {
                    assert!(matches!(evidence, CommandError::Commit { .. }));
                    None
                }
                (
                    FaultPoint::AfterCommitBeforePersist,
                    CommandOutcome::Indeterminate {
                        failure,
                        reconciliation,
                    },
                ) => {
                    assert!(matches!(failure, CommandError::Persistence { .. }));
                    Some(reconciliation)
                }
                (
                    FaultPoint::AfterPersist,
                    CommandOutcome::Committed {
                        later_failure: Some(CommandError::Persistence { .. }),
                        ..
                    },
                ) => None,
                (_, outcome) => panic!("unexpected delivery fault outcome: {outcome:?}"),
            };
            let retains_reconciliation = reconciliation.is_some();
            let (store, storage) = if let Some(reconciliation) = reconciliation {
                reconciliation.install();
                assert_eq!(store.health().state(), HomeHealthState::Healthy);
                (store, storage)
            } else {
                assert_eq!(store.health().state(), HomeHealthState::Failed);
                let candidate = store.recover_same_home().unwrap();
                let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
                let store = candidate.publish();
                assert_eq!(store.health().state(), HomeHealthState::Healthy);
                (store, storage)
            };
            let recovered = operation.status(&store, &storage);
            assert_ne!(
                recovered,
                AcceptedInputDeliveryTransitionStatus::Collision,
                "an atomic commit cut must recover one whole recognized state",
            );
            if let Some(expected) = expected {
                assert_eq!(recovered, expected);
            }
            if recovered == AcceptedInputDeliveryTransitionStatus::Exact {
                assert_operation_committed(&store, &storage, operation);
            }
            store
                .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
                .unwrap();
            if retains_reconciliation {
                let close_error = store
                    .close()
                    .expect_err("installed indeterminate custody must block orderly close");
                assert_eq!(close_error.pending_reconciliation_scopes(), Some(1));
                drop(close_error);
                continue;
            }
            store.close().unwrap();

            let mut reopened = open_with_faults(home.path(), FaultController::new());
            let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
            assert_eq!(operation.status(&reopened, &reopened_storage), recovered);
            if recovered == AcceptedInputDeliveryTransitionStatus::Exact {
                assert_operation_committed(&reopened, &reopened_storage, operation);
            }
            reopened
                .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
                .unwrap();
            reopened.close().unwrap();
        }
    }
}
