use crate::{
    finalizing_history_support::terminal_home,
    recovery_support::{activate, pending_home, point_limit, startup_source},
    support::{batch, open},
};
use beryl_home_store::{
    CommandOutcome, CursorReadLimits, HomeCommand, HomeOpenCandidate, HomeOpenOptions,
    HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use syndic_storage::{
    DeliveryRecoveryClassificationError, InputGateRecord, SyndicPointReadLimit, SyndicStorage,
    test_faults::{FixtureBatch, FixtureDelete, FixtureRecord},
};

fn limits() -> CursorReadLimits {
    CursorReadLimits::new(1, 65_536).unwrap()
}

#[test]
fn candidate_classification_preserves_results_and_source_fences_across_publication() {
    let foreign = pending_home("candidate-classifier-foreign", 951);
    let foreign_source = startup_source(&foreign.store, foreign.storage.clone());
    for mode in 0..3 {
        let fixture = if mode == 2 {
            terminal_home("candidate-classifier-finalizing", 954, false)
        } else {
            pending_home("candidate-classifier", 952 + mode)
        };
        if mode == 1 {
            activate(
                &fixture.store,
                fixture.storage.clone(),
                fixture.thread,
                fixture.turn,
                true,
            );
        }
        let expected = fixture
            .storage
            .classify_delivery_recovery(
                &fixture.store,
                &startup_source(&fixture.store, fixture.storage.clone()),
                point_limit(),
            )
            .unwrap();
        fixture.store.close().unwrap();
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(fixture.home.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let mut publication = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap();
        let access = publication.recovery_access().unwrap();
        let source = storage
            .delivery_recovery_startup_page_candidate(&access, None, limits())
            .unwrap()
            .records()[0]
            .clone();
        assert_eq!(
            storage
                .classify_delivery_recovery_candidate(&access, &source, point_limit())
                .unwrap(),
            expected
        );
        assert!(matches!(
            storage.classify_delivery_recovery_candidate(&access, &foreign_source, point_limit()),
            Err(DeliveryRecoveryClassificationError::SourceDrift)
        ));
        assert!(
            fixture
                .storage
                .classify_delivery_recovery_candidate(&access, &source, point_limit())
                .is_err()
        );
        assert!(
            storage
                .classify_delivery_recovery_candidate(
                    &access,
                    &source,
                    SyndicPointReadLimit::new(1).unwrap()
                )
                .is_err()
        );
        beryl_home_store::test_faults::with_initial_publication_store(&publication, |store| {
            assert!(
                storage
                    .classify_delivery_recovery(store, &source, point_limit())
                    .is_err()
            );
        });
        let store = publication.publish().unwrap();
        assert_eq!(
            storage
                .classify_delivery_recovery(&store, &source, point_limit())
                .unwrap(),
            expected
        );
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut recovered = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovered).unwrap();
        let access = recovered.recovery_access().unwrap();
        assert!(matches!(
            fresh.classify_delivery_recovery_candidate(&access, &source, point_limit()),
            Err(DeliveryRecoveryClassificationError::SourceDrift)
        ));
        let fresh_source = fresh
            .delivery_recovery_startup_page_candidate(&access, None, limits())
            .unwrap()
            .records()[0]
            .clone();
        assert!(
            storage
                .classify_delivery_recovery_candidate(&access, &fresh_source, point_limit())
                .is_err()
        );
        assert_eq!(
            fresh
                .classify_delivery_recovery_candidate(&access, &fresh_source, point_limit())
                .unwrap(),
            expected
        );
        recovered.publish().unwrap().close().unwrap();
    }
    foreign.store.close().unwrap();
}

#[test]
fn candidate_classification_keeps_source_drift_distinct_from_stable_corruption() {
    for drift in [false, true] {
        let fixture = pending_home("candidate-classifier-drift", 960 + u64::from(drift));
        let gate = fixture
            .storage
            .input_gate(&fixture.store, fixture.thread, point_limit())
            .unwrap()
            .unwrap();
        let revision = fixture.storage.revision(&fixture.store).unwrap();
        fixture.store.close().unwrap();
        let mut candidate = open(fixture.home.path());
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let mut publication = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap();
        let access = publication.recovery_access().unwrap();
        let source = storage
            .delivery_recovery_startup_page_candidate(&access, None, limits())
            .unwrap()
            .records()[0]
            .clone();
        let changes = if drift {
            batch([FixtureRecord::InputGate(
                InputGateRecord::new(
                    fixture.thread,
                    gate.revision().checked_next().unwrap(),
                    gate.state().clone(),
                    gate.accepted_high_water(),
                    gate.route_generation_high_water(),
                    gate.selected_route(),
                    gate.live_steering_count(),
                    gate.live_next_turn_count(),
                    gate.live_logical_utf8_bytes(),
                )
                .unwrap(),
            )])
        } else {
            let mut changes = FixtureBatch::new();
            changes
                .delete(FixtureDelete::TurnState(fixture.turn))
                .unwrap();
            changes
        };
        let mut command = HomeCommand::new(access.home_revision().unwrap());
        command
            .add(storage.clone().fixture_contribution(revision, changes))
            .unwrap();
        assert!(matches!(
            access.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        let result = storage.classify_delivery_recovery_candidate(&access, &source, point_limit());
        if drift {
            assert!(
                matches!(
                    result,
                    Err(DeliveryRecoveryClassificationError::SourceDrift)
                ),
                "{result:?}"
            );
        } else {
            assert!(
                matches!(
                    result,
                    Err(DeliveryRecoveryClassificationError::Corruption(_))
                ),
                "{result:?}"
            );
        }
        publication.close().unwrap();
    }
}
