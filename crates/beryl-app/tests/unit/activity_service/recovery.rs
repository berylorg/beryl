use super::*;
use beryl_home_store::{
    HomeHealthState, HomeOpenCandidate, HomeOpenOptions, HomeRecoveryCandidate, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use support::TestHome;

fn recovery_candidate() -> (
    TestHome,
    HomeRecoveryCandidate,
    SyndicStorage,
    HomeGeneration,
    FaultController,
) {
    let directory = TestHome::new("activity-recovery");
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let stale = SyndicStorage::register(&mut candidate).unwrap();
    let candidate = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap();
    let generation = candidate.generation();
    let home = candidate.publish().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    let candidate = home.recover_same_home().unwrap();
    (directory, candidate, stale, generation, faults)
}

#[test]
fn recovery_activity_stays_dormant_until_publication_and_disposes_before_abort() {
    for publish in [false, true] {
        let (_directory, mut candidate, _, _, _) = recovery_candidate();
        let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let mut runtime = owner(candidate.home_id());
        let source = runtime
            .activity_read_source(candidate.home_id(), candidate.generation())
            .unwrap();
        let prepared = PreparedActivityService::prepare_recovery(
            &mut candidate,
            storage,
            source,
            limits(1, 1, 1),
        )
        .unwrap();
        let shared = (!publish).then(|| Arc::downgrade(&prepared.service_for_test().shared));
        assert_eq!(
            prepared.service_for_test().shared.generation,
            candidate.generation()
        );
        for _ in 0..3 {
            assert!(matches!(
                prepared.service_for_test().prepare_collection(
                    id(30),
                    support::exact_cas::execution_binding().runtime_id()
                ),
                Err(ActivityReadError::Unavailable)
            ));
        }
        assert_eq!(
            prepared
                .service_for_test()
                .shared
                .collections
                .load(Ordering::Acquire),
            0
        );
        assert_eq!(
            prepared
                .service_for_test()
                .shared
                .pages
                .load(Ordering::Acquire),
            0
        );
        let home = if publish {
            let home = candidate.publish().unwrap();
            let service = prepared.into_service();
            assert!(matches!(
                service.prepare_collection(
                    id(30),
                    support::exact_cas::execution_binding().runtime_id()
                ),
                Err(ActivityReadError::Runtime(
                    RuntimeActivityReadError::RuntimeUnavailable
                ))
            ));
            assert_eq!(service.shared.collections.load(Ordering::Acquire), 0);
            service.retire();
            assert!(service.shared.state.lock().unwrap().resources.is_none());
            drop(service);
            home
        } else {
            drop(prepared);
            assert!(
                shared
                    .as_ref()
                    .is_none_or(|shared| shared.upgrade().is_none())
            );
            assert!(runtime.shutdown());
            let home = candidate.abort();
            assert_eq!(home.health().state(), HomeHealthState::Failed);
            home
        };
        assert!(
            shared
                .as_ref()
                .is_none_or(|shared| shared.upgrade().is_none())
        );
        if publish {
            assert!(runtime.shutdown());
        }
        home.close().unwrap();
    }
}

#[test]
fn recovery_activity_rejects_stale_foreign_and_failed_candidate_sources() {
    let (_directory, mut candidate, stale, old_generation, faults) = recovery_candidate();
    let (_foreign_directory, mut foreign, _, _, _) = recovery_candidate();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let foreign_storage = SyndicStorage::reacquire_candidate(&foreign).unwrap();
    let mut runtime = owner(candidate.home_id());
    let source = runtime
        .activity_read_source(candidate.home_id(), candidate.generation())
        .unwrap();
    let stale_source = runtime
        .activity_read_source(candidate.home_id(), old_generation)
        .unwrap();
    assert!(matches!(
        PreparedActivityService::prepare_recovery(
            &mut candidate,
            storage.clone(),
            stale_source,
            limits(1, 1, 1)
        ),
        Err(ActivityPreparationError::Identity)
    ));
    assert!(matches!(
        PreparedActivityService::prepare_recovery(
            &mut foreign,
            storage.clone(),
            source.clone(),
            limits(1, 1, 1)
        ),
        Err(ActivityPreparationError::Identity)
    ));
    for wrong_storage in [stale, foreign_storage] {
        assert!(matches!(
            PreparedActivityService::prepare_recovery(
                &mut candidate,
                wrong_storage,
                source.clone(),
                limits(1, 1, 1)
            ),
            Err(ActivityPreparationError::Storage(_))
        ));
    }
    assert_eq!(
        candidate.service_reference().health().state(),
        HomeHealthState::Reopening
    );
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(matches!(
        PreparedActivityService::prepare_recovery(
            &mut candidate,
            storage.clone(),
            source.clone(),
            limits(1, 1, 1)
        ),
        Err(ActivityPreparationError::Storage(_))
    ));
    assert!(matches!(
        PreparedActivityService::prepare_recovery(&mut candidate, storage, source, limits(1, 1, 1)),
        Err(ActivityPreparationError::Candidate(_))
    ));
    assert!(runtime.shutdown());
    candidate.abort().close().unwrap();
    foreign.abort().close().unwrap();
}
