mod support;

use beryl_home_store::{
    CommandOutcome, HomeHealthState, HomeOpenCandidate, HomeOpenOptions, HomeOpenPublication,
    HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::RuntimeId;
use beryl_state::BerylState;

fn candidate(
    directory: &tempfile::TempDir,
    faults: FaultController,
) -> (HomeOpenPublication, BerylState) {
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    (
        candidate
            .prepare_publication(BerylState::required_domains().unwrap())
            .unwrap(),
        state,
    )
}

#[test]
fn candidate_runtime_reads_preserve_records_and_exact_generation() {
    let directory = tempfile::tempdir().unwrap();
    let foreign_directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (mut candidate, state) = candidate(&directory, faults.clone());
    let (_foreign, foreign_state) = self::candidate(&foreign_directory, FaultController::new());
    let runtimes = state.runtime_roots();
    let runtime_id = RuntimeId::from_bytes([1; 16]);
    let missing_id = RuntimeId::from_bytes([3; 16]);
    let reference = candidate.service_reference();
    let generation = candidate.generation();
    let access = candidate.recovery_access().unwrap();
    assert_eq!(
        runtimes.runtime_candidate(&access, runtime_id).unwrap(),
        None
    );
    assert!(
        foreign_state
            .runtime_roots()
            .runtime_candidate(&access, runtime_id)
            .is_err()
    );
    assert_eq!(reference.health().state(), HomeHealthState::Opening);
    assert!(runtimes.runtime(&reference, runtime_id).is_err());
    let store = candidate.publish().unwrap();
    assert!(matches!(
        support::execute(
            &store,
            runtimes.create_runtime_with_home_root(
                runtimes.revision(&store).unwrap(),
                support::host_runtime(1, 2, r"C:\Codex\codex.exe", r"C:\Users\operator"),
            ),
        ),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let persisted = runtimes.runtime(&store, runtime_id).unwrap();
    assert!(persisted.is_some());
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovered = store.recover_same_home().unwrap();
    let fresh = BerylState::reacquire_candidate(&recovered)
        .unwrap()
        .runtime_roots();
    let access = recovered.recovery_access().unwrap();
    assert_ne!(access.generation(), generation);
    assert!(runtimes.runtime_candidate(&access, runtime_id).is_err());
    assert!(
        foreign_state
            .runtime_roots()
            .runtime_candidate(&access, runtime_id)
            .is_err()
    );
    assert_eq!(
        fresh.runtime_candidate(&access, runtime_id).unwrap(),
        persisted
    );
    assert_eq!(fresh.runtime_candidate(&access, missing_id).unwrap(), None);
    assert_eq!(reference.health().state(), HomeHealthState::Reopening);
    assert!(fresh.runtime(&reference, runtime_id).is_err());
    let store = recovered.publish().unwrap();
    assert_eq!(fresh.runtime(&store, runtime_id).unwrap(), persisted);
    store.close().unwrap();

    let (mut reopened, state) = self::candidate(&directory, FaultController::new());
    let access = reopened.recovery_access().unwrap();
    assert_eq!(
        state
            .runtime_roots()
            .runtime_candidate(&access, runtime_id)
            .unwrap(),
        persisted
    );
    assert_eq!(
        state
            .runtime_roots()
            .runtime_candidate(&access, missing_id)
            .unwrap(),
        None
    );
}

#[test]
fn candidate_runtime_confirmation_failure_prevents_publication() {
    for recovering in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let (mut candidate, state) = candidate(&directory, faults.clone());
        let runtime_id = RuntimeId::from_bytes([1; 16]);
        if recovering {
            let store = candidate.publish().unwrap();
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
            let mut recovered = store.recover_same_home().unwrap();
            let runtimes = BerylState::reacquire_candidate(&recovered)
                .unwrap()
                .runtime_roots();
            let access = recovered.recovery_access().unwrap();
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(runtimes.runtime_candidate(&access, runtime_id).is_err());
            assert!(runtimes.runtime_candidate(&access, runtime_id).is_err());
            assert!(recovered.publish().is_err());
        } else {
            let access = candidate.recovery_access().unwrap();
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(
                state
                    .runtime_roots()
                    .runtime_candidate(&access, runtime_id)
                    .is_err()
            );
            assert!(
                state
                    .runtime_roots()
                    .runtime_candidate(&access, runtime_id)
                    .is_err()
            );
            assert!(candidate.publish().is_err());
        }
    }
}
