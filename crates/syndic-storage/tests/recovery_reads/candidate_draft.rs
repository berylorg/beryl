use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use syndic_storage::{SyndicPointReadLimit, SyndicStorage};

use crate::support::{TestHome, id, seed_populated};

#[test]
fn candidate_current_draft_read_failure_prevents_publication() {
    for thread in [id(30), id(250)] {
        let home = TestHome::new("candidate-draft-read-failure");
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        seed_populated(&store, storage.clone());
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut candidate = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let access = candidate.recovery_access().unwrap();
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        let limit = SyndicPointReadLimit::new(65_536).unwrap();
        assert!(
            fresh
                .current_draft_candidate(&access, thread, limit)
                .is_err()
        );
        assert!(
            fresh
                .current_draft_candidate(&access, thread, limit)
                .is_err()
        );
        assert!(candidate.publish().is_err());
    }
}
