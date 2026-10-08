use super::*;

#[test]
fn cancellation_drains_an_admitted_coordinator_capture_before_returning() {
    use beryl_home_store::test_faults::{FaultController, FaultPoint};
    let faults = FaultController::new();
    let fixture = Fixture::new_with_faults(r"C:\Work\Beryl", faults.clone());
    let before = fixture.store.home_revision().unwrap();
    let (mut service, reader, start) = coordinator(&fixture);
    let blocked = faults.block_next(FaultPoint::BeforeReadConfirmation);
    start.release();
    assert!(blocked.wait_until_reached(Duration::from_secs(10)));
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 1);
    let (sender, receiver) = std::sync::mpsc::channel();
    let stopper = std::thread::spawn(move || sender.send(service.stop_and_join()).unwrap());
    assert!(receiver.recv_timeout(Duration::from_millis(50)).is_err());
    blocked.release();
    receiver
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    stopper.join().unwrap();
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    assert!(matches!(
        reader.certified_threads(),
        Err(CatalogSourceReadError::Retired)
    ));
}

fn finished(service: &CatalogSourceCoordinator) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !service.worker.as_ref().unwrap().is_finished() {
        assert!(Instant::now() < deadline, "repair worker did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn original_indeterminate_repair_handle_survives_drain_and_reconciles_in_fresh_candidate() {
    use beryl_home_store::test_faults::{FaultController, FaultPoint};
    let faults = FaultController::new();
    let fixture = Fixture::new_with_faults(r"C:\Work\Beryl", faults.clone());
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let (mut service, reader, start) = coordinator(&fixture);
    start.release();
    finished(&service);
    let CatalogSourceCoordinatorError::Repair(repair) = service.stop_and_join().unwrap_err() else {
        panic!("original repair outcome missing")
    };
    let RetainedCatalogRepair::Indeterminate { reconciliation, .. } = *repair else {
        panic!("original indeterminate handle missing")
    };
    assert_eq!(fixture.store.pending_reconciliations().len(), 1);
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
    assert!(matches!(
        reader.certified_threads(),
        Err(CatalogSourceReadError::Retired)
    ));
    assert_eq!(
        fixture.store.health().state(),
        beryl_home_store::HomeHealthState::Healthy
    );
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    drop(service);
    let mut candidate = fixture.store.recover_same_home().unwrap();
    assert!(matches!(
        candidate
            .recovery_access()
            .unwrap()
            .reconcile(&reconciliation)
            .unwrap(),
        beryl_home_store::ReconciliationResolution::ExactNew { .. }
    ));
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn repair_preserves_original_known_noncommit_and_postcommit_receipt() {
    use beryl_home_store::test_faults::{FaultController, FaultPoint};
    for (point, committed) in [
        (FaultPoint::BeforeCommit, false),
        (FaultPoint::AfterPersist, true),
    ] {
        let faults = FaultController::new();
        let fixture = Fixture::new_with_faults(r"C:\Work\Beryl", faults.clone());
        let before = fixture.store.home_revision().unwrap();
        faults.fail_next(point);
        let (mut service, _reader, start) = coordinator(&fixture);
        start.release();
        finished(&service);
        let CatalogSourceCoordinatorError::Repair(repair) = service.stop_and_join().unwrap_err()
        else {
            panic!("repair classification missing")
        };
        assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
        match (*repair, committed) {
            (RetainedCatalogRepair::NotCommitted(_), false) => {
                assert_eq!(
                    fixture.store.health().state(),
                    beryl_home_store::HomeHealthState::Failed
                );
                let fresh = fixture
                    .store
                    .recover_same_home()
                    .unwrap()
                    .publish()
                    .unwrap();
                assert_eq!(fresh.home_revision().unwrap(), before);
                fresh.close().unwrap();
            }
            (
                RetainedCatalogRepair::Committed(CommandOutcome::Committed {
                    receipt,
                    later_failure: Some(_),
                    ..
                }),
                true,
            ) => assert!(receipt.home_revision() > before),
            (other, _) => panic!("original classification changed: {other:?}"),
        }
    }
}
