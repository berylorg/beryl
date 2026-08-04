use beryl_home_store::{
    HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use syndic_storage::*;

use crate::{
    accepted_support::{AcceptedOperation, assert_operation_committed},
    support::{TestHome, batch, commit},
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
            commit(&store, storage, batch(operation.records()));

            faults.fail_next(point);
            assert!(
                store
                    .execute_current(operation.current_command(storage))
                    .is_err()
            );
            assert_eq!(store.health().state(), HomeHealthState::Verifying);
            store.verify_health().unwrap();
            let recovered = operation.status(&store, storage);
            assert_ne!(
                recovered,
                AcceptedInputDeliveryTransitionStatus::Collision,
                "an atomic commit cut must recover one whole recognized state",
            );
            if let Some(expected) = expected {
                assert_eq!(recovered, expected);
            }
            if recovered == AcceptedInputDeliveryTransitionStatus::Exact {
                assert_operation_committed(&store, storage, operation);
            }
            store.validate_registered_domains().unwrap();
            store.close().unwrap();

            let mut reopened = open_with_faults(home.path(), FaultController::new());
            let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
            assert_eq!(operation.status(&reopened, reopened_storage), recovered);
            if recovered == AcceptedInputDeliveryTransitionStatus::Exact {
                assert_operation_committed(&reopened, reopened_storage, operation);
            }
            reopened.validate_registered_domains().unwrap();
            reopened.close().unwrap();
        }
    }
}
