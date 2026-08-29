#![cfg(feature = "test-faults")]

#[path = "support/mod.rs"]
mod support;

use beryl_home_store::{
    HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use syndic_storage::{
    RecoveryProjectionError, RecoveryProjectionRequest, SelectedPathProof, SyndicPointReadLimit,
    SyndicStorage,
};

use support::{TestHome, id, seed_populated};

fn open_with_faults(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

#[test]
fn recovery_assembly_read_fault_preserves_state_for_same_home_recovery() {
    let home = TestHome::new("phase9-recovery-read-fault");
    let faults = FaultController::new();
    let mut store = open_with_faults(home.path(), faults.clone());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage.clone());
    let thread_id = id(30);
    let before = storage
        .thread(&store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let selected_path = SelectedPathProof::new(
        before.committed_tail(),
        before.revision(),
        before.selected_path_digest(),
    );

    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(
        storage
            .prepare_recovery_projection(
                &store,
                RecoveryProjectionRequest::for_current_selected_path(
                    thread_id,
                    selected_path,
                    Some(100_000),
                ),
            )
            .is_err()
    );
    assert_eq!(store.health().state(), HomeHealthState::Failed);

    let candidate = store.recover_same_home().unwrap();
    let _candidate_storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let recovered = candidate.publish();
    let storage = SyndicStorage::reacquire(&recovered).unwrap();
    assert_eq!(
        storage
            .thread(&recovered, thread_id, point_limit())
            .unwrap()
            .as_ref(),
        Some(&before)
    );
    assert!(matches!(
        storage
            .prepare_recovery_projection(
                &recovered,
                RecoveryProjectionRequest::for_current_selected_path(
                    thread_id,
                    selected_path,
                    Some(100_000),
                ),
            )
            .unwrap_err(),
        RecoveryProjectionError::IncompleteHistory {
            reason: "included turn has no canonical items"
        }
    ));
    recovered.close().unwrap();
}
