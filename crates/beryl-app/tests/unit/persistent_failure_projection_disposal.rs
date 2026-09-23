use super::*;
use crate::cas_projection::{PersistentFailureCutState, ProjectionConnectionServiceCloseOutcome};

#[test]
fn zero_worker_router_disposes_retained_target_projection_after_freeze_unlocks() {
    prove_projection_disposal(None);
}

#[test]
fn frozen_failure_batch_retains_projection_until_consumed() {
    prove_projection_disposal(Some(BatchDisposition::Consume));
}

#[test]
fn abandoned_failure_batch_disposes_retained_projection() {
    prove_projection_disposal(Some(BatchDisposition::Drop));
}

#[test]
fn unwinding_failure_batch_disposes_retained_projection() {
    prove_projection_disposal(Some(BatchDisposition::Unwind));
}

#[test]
fn later_failure_freeze_error_disposes_previously_collected_projection() {
    prove_projection_disposal(Some(BatchDisposition::CollectionFailure));
}

#[derive(Clone, Copy)]
enum BatchDisposition {
    Consume,
    Drop,
    Unwind,
    CollectionFailure,
}

fn prove_projection_disposal(disposition: Option<BatchDisposition>) {
    let fixture = Fixture::new();
    let server = server::NormalTerminalServer::spawn_projection_terminal_disposal();
    let mut session = admit(&fixture, &connector(server.endpoint())).unwrap();
    server.wait_for_admission();
    let connection = Arc::clone(session.connection());
    let router = connection.original_failure_router_for_test();
    let request = request(&fixture);
    let command = fixture.service.live_home_command().unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(command.home()).unwrap();
    let projection = coordinator
        .obtain_projection(
            command.home(),
            &fixture.service.storage,
            &mut session,
            &request,
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    server.wait_for_projection();
    let target = projection
        .into_active_live_event_target(
            beryl_model::SyndicTurnId::from_bytes([190; 16]),
            beryl_model::CasTurnId::new("captured-active-turn").unwrap(),
        )
        .unwrap();
    let home = command.home();
    let binding = fixture
        .service
        .storage
        .thread_execution(home, fixture.thread, point_limit())
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    let mutation = fixture.service.storage.create_thread(
        fixture.service.storage.revision(home).unwrap(),
        CreateThread::ordinary(
            SyndicThreadId::from_bytes([191; 16]),
            SyndicDraftId::from_bytes([192; 16]),
            binding,
            SyndicTimestamp::from_unix_millis(20),
            DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
        ),
    );
    fixture
        .faults
        .panic_next(beryl_home_store::test_faults::FaultPoint::BeforeCommit);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| execute(home, mutation))).is_err()
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while fixture.service.persistent_failure_cut_snapshot().state()
        != PersistentFailureCutState::Cutting
    {
        assert!(std::time::Instant::now() < deadline);
        thread::yield_now();
    }
    drop(target);
    assert_eq!(
        fixture
            .service
            .persistent_failure_cut_snapshot()
            .disposed_projection_count(),
        0
    );
    let retained_workers = matches!(disposition, Some(BatchDisposition::CollectionFailure))
        .then(|| connection.retain_persistent_failure_workers().unwrap());
    connection.shutdown().unwrap();
    if retained_workers.is_none() {
        assert!(connection.retain_persistent_failure_workers().is_none());
        assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
    } else {
        assert!(connection.retain_persistent_failure_workers().is_some());
    }
    assert_eq!(router.snapshot().unwrap().target_count(), 1);
    if matches!(disposition, Some(BatchDisposition::CollectionFailure)) {
        // The second visit rejects the already frozen router after collecting its first batch.
        fixture
            .service
            .connections
            .lock()
            .unwrap()
            .push(Arc::clone(&connection));
    } else if let Some(disposition) = disposition {
        let identity = crate::cas_projection::persistent_failure::PersistentFailureCutIdentity::new(
            fixture.service.home_id(),
            fixture.service.home_generation(),
            fixture.service.service_generation(),
            crate::cas_projection::persistent_failure::PersistentFailureGeneration::FIRST,
        );
        let batch = connection
            .freeze_original_failure_targets(identity, None)
            .unwrap()
            .unwrap();
        let mut witnesses = batch.witnesses();
        assert_eq!(witnesses.len(), 1);
        let witness = witnesses.next().unwrap();
        assert_eq!(
            witness.syndic_turn_id(),
            Some(beryl_model::SyndicTurnId::from_bytes([190; 16]))
        );
        assert_eq!(
            witness.cas_turn_id().unwrap().as_str(),
            "captured-active-turn"
        );
        assert_eq!(witness.cut_identity(), identity);
        drop(witnesses);
        assert_eq!(router.snapshot().unwrap().target_count(), 1);
        assert_eq!(
            fixture
                .service
                .persistent_failure_cut_snapshot()
                .disposed_projection_count(),
            0
        );
        match disposition {
            BatchDisposition::Consume => assert_eq!(batch.into_candidates().len(), 1),
            BatchDisposition::Drop => drop(batch),
            BatchDisposition::Unwind => assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                    let _batch = batch;
                    panic!("abandon frozen inventory consumer");
                }))
                .is_err()
            ),
            BatchDisposition::CollectionFailure => unreachable!(),
        }
        assert_eq!(
            fixture
                .service
                .persistent_failure_cut_snapshot()
                .disposed_projection_count(),
            1
        );
    }
    drop(command);
    let expected_state = if disposition.is_some() {
        PersistentFailureCutState::Incomplete
    } else {
        PersistentFailureCutState::Finished
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while fixture.service.persistent_failure_cut_snapshot().state() != expected_state {
        assert!(std::time::Instant::now() < deadline);
        thread::yield_now();
    }
    let snapshot = fixture.service.persistent_failure_cut_snapshot();
    assert_eq!(snapshot.target_count(), usize::from(disposition.is_none()));
    assert_eq!(
        snapshot.proven_nondispatch_count(),
        usize::from(disposition.is_none())
    );
    assert_eq!(snapshot.possible_dispatch_count(), 0);
    assert_eq!(snapshot.disposed_projection_count(), 1);
    drop(retained_workers);
    assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
    assert_eq!(
        router.failure_dispatch_guard_count_for_test(),
        usize::from(matches!(
            disposition,
            Some(BatchDisposition::CollectionFailure)
        ))
    );
    assert_eq!(router.snapshot().unwrap().target_count(), 1);
    assert!(matches!(
        fixture.service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(_)
    ));
    drop(session);
    drop(connection);
    drop(router);
    server.join();
}
