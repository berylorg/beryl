use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint, with_initial_publication_store},
};
use syndic_storage::SyndicStorage;

use crate::support::{TestHome, open, seed_populated};

#[test]
fn candidate_revision_preserves_identity_confirmation_and_publication_fences() {
    let home = TestHome::new("candidate-revision");
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let foreign_home = TestHome::new("candidate-revision-foreign");
    let mut foreign = open(foreign_home.path());
    let foreign_storage = SyndicStorage::register(&mut foreign).unwrap();
    let mut publication = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap();
    let access = publication.recovery_access().unwrap();
    let initial = storage.revision_candidate(&access).unwrap();
    assert!(foreign_storage.revision_candidate(&access).is_err());
    with_initial_publication_store(&publication, |store| {
        assert!(storage.revision(store).is_err());
    });
    let mut store = publication.publish().unwrap();
    assert_eq!(storage.revision(&store).unwrap(), initial);
    seed_populated(&store, storage.clone());
    let populated = storage.revision(&store).unwrap();
    assert_ne!(populated, initial);
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovered = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovered).unwrap();
    let access = recovered.recovery_access().unwrap();
    assert!(storage.revision_candidate(&access).is_err());
    assert!(foreign_storage.revision_candidate(&access).is_err());
    assert_eq!(fresh.revision_candidate(&access).unwrap(), populated);
    store = recovered.publish().unwrap();
    assert_eq!(fresh.revision(&store).unwrap(), populated);
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovered = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovered).unwrap();
    let access = recovered.recovery_access().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fresh.revision_candidate(&access).is_err());
    assert!(fresh.revision_candidate(&access).is_err());
    assert!(recovered.publish().is_err());
}
