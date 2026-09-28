#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{SyndicThreadId, WindowId};
use beryl_state::{BerylState, ThreadClaimCatalogSourceError};
use support::session_exit::{committed, seed};

#[test]
fn candidate_window_claims_preserve_pair_validation_provenance_and_read_failures() {
    for broken_pair in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let foreign_directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let mut candidate = candidate
            .prepare_publication(BerylState::required_domains().unwrap())
            .unwrap();
        let session = state.session();
        let window = WindowId::from_bytes([0; 16]);
        let thread = SyndicThreadId::from_bytes([0; 16]);
        let absent = WindowId::from_bytes([255; 16]);
        let reference = candidate.service_reference();
        let access = candidate.recovery_access().unwrap();
        assert_eq!(
            session
                .window_claim_catalog_source_candidate(&access, window)
                .unwrap()
                .claim(),
            None
        );
        assert!(
            session
                .window_claim_catalog_source(&reference, window)
                .is_err()
        );
        let store = candidate.publish().unwrap();
        seed(&store, &session, 2);
        let expected = session.window_claim_catalog_source(&store, window).unwrap();
        assert!(expected.claim().is_some());
        if broken_pair {
            committed(support::execute(
                &store,
                session.delete_thread_claim_copy_for_test(
                    session.revision(&store).unwrap(),
                    window,
                    thread,
                ),
            ));
        }
        let (_foreign, foreign_state) = support::open(foreign_directory.path());
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut recovered = store.recover_same_home().unwrap();
        let fresh = BerylState::reacquire_candidate(&recovered)
            .unwrap()
            .session();
        let access = recovered.recovery_access().unwrap();
        let revision = access.home_revision().unwrap();
        assert!(
            session
                .window_claim_catalog_source_candidate(&access, window)
                .is_err()
        );
        assert!(
            foreign_state
                .session()
                .window_claim_catalog_source_candidate(&access, window)
                .is_err()
        );
        let missing = fresh
            .window_claim_catalog_source_candidate(&access, absent)
            .unwrap();
        assert_eq!(missing.window_id(), absent);
        assert_eq!(missing.claim(), None);
        for _ in 0..2 {
            let result = fresh.window_claim_catalog_source_candidate(&access, window);
            if broken_pair {
                assert!(
                    matches!(result, Err(ThreadClaimCatalogSourceError::ReverseCopiesDisagree { thread_id }) if thread_id == thread)
                );
            } else {
                assert_eq!(result.unwrap(), expected);
            }
        }
        assert_eq!(access.home_revision().unwrap(), revision);
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(matches!(
            fresh.window_claim_catalog_source_candidate(&access, window),
            Err(ThreadClaimCatalogSourceError::Read(_))
        ));
        assert!(
            fresh
                .window_claim_catalog_source_candidate(&access, window)
                .is_err()
        );
        assert!(recovered.publish().is_err());
    }
}
