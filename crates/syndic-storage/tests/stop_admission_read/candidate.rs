use super::*;
use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use syndic_storage::SyndicStorage;

fn open_candidate(
    path: &std::path::Path,
    faults: FaultController,
) -> (beryl_home_store::HomeOpenPublication, SyndicStorage) {
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    (
        candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap(),
        storage,
    )
}

#[test]
fn candidate_stop_evidence_preserves_exact_authority_across_publication_and_recovery() {
    for stopping in [false, true] {
        let fixture = active_stop_fixture("candidate-stop-authority");
        if stopping {
            fixture.admit_stop();
        }
        let expected = fixture
            .storage
            .stop_admission_read(&fixture.store, fixture.thread, point_limit())
            .unwrap();
        assert_eq!(matches!(expected, StopAdmissionRead::Stopping(_)), stopping);
        fixture.store.close().unwrap();
        let faults = FaultController::new();
        let (mut publication, storage) = open_candidate(fixture._home.path(), faults.clone());
        let access = publication.recovery_access().unwrap();
        assert_eq!(
            storage
                .stop_admission_read_candidate(&access, fixture.thread, point_limit())
                .unwrap(),
            expected
        );
        assert!(
            fixture
                .storage
                .stop_admission_read_candidate(&access, fixture.thread, point_limit())
                .is_err()
        );
        assert!(
            storage
                .stop_admission_read_candidate(
                    &access,
                    fixture.thread,
                    SyndicPointReadLimit::new(1).unwrap()
                )
                .is_err()
        );
        let access = publication.recovery_access().unwrap();
        let source = storage
            .delivery_recovery_startup_page_candidate(
                &access,
                None,
                beryl_home_store::CursorReadLimits::new(1, 65_536).unwrap(),
            )
            .unwrap()
            .records()[0]
            .clone();
        let candidate_case = storage
            .classify_delivery_recovery_candidate(&access, &source, point_limit())
            .unwrap();
        assert!(
            matches!(
                &candidate_case,
                syndic_storage::DeliveryRecoveryCase::Stopping(_) if stopping
            ) || matches!(
                &candidate_case,
                syndic_storage::DeliveryRecoveryCase::Active(_) if !stopping
            )
        );
        beryl_home_store::test_faults::with_initial_publication_store(&publication, |store| {
            assert!(
                storage
                    .stop_admission_read(store, fixture.thread, point_limit())
                    .is_err()
            );
        });
        let store = publication.publish().unwrap();
        assert_eq!(
            storage
                .classify_delivery_recovery(&store, &source, point_limit())
                .unwrap(),
            candidate_case
        );
        assert_eq!(
            storage
                .stop_admission_read(&store, fixture.thread, point_limit())
                .unwrap(),
            expected
        );
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        assert!(
            storage
                .stop_admission_read_candidate(&access, fixture.thread, point_limit())
                .is_err()
        );
        assert_eq!(
            fresh
                .stop_admission_read_candidate(&access, fixture.thread, point_limit())
                .unwrap(),
            expected
        );
        recovery.publish().unwrap().close().unwrap();
    }
}

#[test]
fn candidate_stop_evidence_rejects_missing_exact_stop_authority() {
    let fixture = active_stop_fixture("candidate-stop-missing");
    fixture.admit_stop();
    delete_fixture(
        &fixture.store,
        &fixture.storage,
        syndic_storage::test_faults::FixtureDelete::StopOperation(fixture.operation_id),
    );
    fixture.store.close().unwrap();
    let (mut publication, storage) = open_candidate(fixture._home.path(), FaultController::new());
    assert!(matches!(
        storage.stop_admission_read_candidate(
            &publication.recovery_access().unwrap(),
            fixture.thread,
            point_limit()
        ),
        Err(SyndicReadError::Invariant(_))
    ));
    publication.close().unwrap();
}

#[test]
fn candidate_stop_evidence_rejects_a_changed_stop_between_passes() {
    let fixture = active_stop_fixture("candidate-stop-drift");
    fixture.admit_stop();
    let stop = fixture.stop();
    let gate = fixture.gate();
    let join = JoinStopCause::new(
        stop.id(),
        stop.target().clone(),
        gate.revision(),
        stop.revision(),
        StopCause::DiagnosticControl,
    );
    fixture.store.close().unwrap();
    let faults = FaultController::new();
    let (mut publication, storage) = open_candidate(fixture._home.path(), faults.clone());
    let access = publication.recovery_access().unwrap();
    let blocks = (0..33)
        .map(|_| faults.block_next(FaultPoint::BeforeReadConfirmation))
        .collect::<Vec<_>>();
    for block in &blocks[..32] {
        block.release();
    }
    let outcome = thread::scope(|scope| {
        let reader = scope.spawn(|| {
            storage.stop_admission_read_candidate(&access, fixture.thread, point_limit())
        });
        assert!(blocks[32].wait_until_reached(Duration::from_secs(10)));
        assert!(matches!(
            access.execute_current(storage.current_join_stop_cause(join)),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        blocks[32].release();
        reader.join().unwrap()
    });
    assert!(
        matches!(
            outcome,
            Err(SyndicReadError::ConcurrentChange {
                operation: "stop-admission read"
            })
        ),
        "{outcome:?}"
    );
    publication.close().unwrap();
}

#[test]
fn candidate_stop_evidence_keeps_pending_and_finalizing_ineligible() {
    let pending = recovery_support::pending_home("candidate-stop-pending", 940);
    let finalizing =
        finalizing_history_support::terminal_home("candidate-stop-finalizing", 941, false);
    for fixture in [pending, finalizing] {
        let expected = fixture
            .storage
            .stop_admission_read(&fixture.store, fixture.thread, point_limit())
            .unwrap();
        assert!(matches!(expected, StopAdmissionRead::Ineligible(_)));
        fixture.store.close().unwrap();
        let (mut publication, storage) =
            open_candidate(fixture.home.path(), FaultController::new());
        assert_eq!(
            storage
                .stop_admission_read_candidate(
                    &publication.recovery_access().unwrap(),
                    fixture.thread,
                    point_limit()
                )
                .unwrap(),
            expected
        );
        publication.close().unwrap();
    }
}
