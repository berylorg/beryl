use beryl_app::cas_projection::test_faults::{recover_startup, recover_startup_candidate};
use beryl_home_store::{
    HomeCandidateError, HomeCandidateRecoveryAccess, HomeCommand, HomeOpenCandidate,
    HomeOpenOptions, HomeSchemaVersion, ReconciliationResolution,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::BerylState;
use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};
use syndic_storage::{
    ConsumedRepairRequest, InputGateRecord, InputGateState, RepairCaptureGap,
    RepairCaptureGapReason, RepairRequestAttemptNonce, RepairRequestDisposition,
    RepairRequiredTarget, RequireTerminalRepair, SyndicPointReadLimit, SyndicStorage,
    TurnEndStatus, TurnIncompleteReason,
};

use super::{TEST_LOCK, syndic::Fixture};

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(65_536).unwrap()
}

fn enter_repair(fixture: &mut Fixture) -> RepairRequiredTarget {
    let submitted = fixture.submit_text("startup repair recovery");
    let source = fixture.activate_without_terminal(submitted);
    let status = TurnEndStatus::new(
        syndic_storage::TurnTerminalOutcome::Interrupted,
        Some(TurnIncompleteReason::StreamLost),
    )
    .unwrap();
    fixture.publish_terminal_without_convergence(submitted, &source, status);
    let home = fixture.home();
    let state = fixture
        .storage
        .turn_state(&home, submitted.turn, limit())
        .unwrap()
        .unwrap();
    let terminal = fixture
        .storage
        .source_event(
            &home,
            submitted.turn,
            syndic_storage::SourceEventSequence::new(state.source_event_count()).unwrap(),
            limit(),
        )
        .unwrap()
        .unwrap();
    let target = RepairRequiredTarget::new(
        submitted.turn,
        source,
        RepairCaptureGap::new(
            terminal.repair_witness(),
            status,
            RepairCaptureGapReason::ForcedAbortOrderingUnproven,
            None,
        )
        .unwrap(),
        RepairRequestDisposition::Available,
    );
    let gate = fixture
        .storage
        .input_gate(&home, fixture.thread, limit())
        .unwrap()
        .unwrap();
    assert!(matches!(
        home.execute_current(fixture.storage.current_require_terminal_repair(
            RequireTerminalRepair::new(fixture.thread, gate.revision(), target.clone()),
        )),
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    target
}

fn consume_repair(fixture: &Fixture, target: RepairRequiredTarget) -> RepairRequiredTarget {
    let home = fixture.home();
    let current = fixture
        .storage
        .input_gate(&home, fixture.thread, limit())
        .unwrap()
        .unwrap();
    let next = current.revision().checked_next().unwrap();
    let target = RepairRequiredTarget::new(
        target.turn_id(),
        target.source().clone(),
        target.gap(),
        RepairRequestDisposition::Consumed(
            ConsumedRepairRequest::new(
                RepairRequestAttemptNonce::from_bytes([232; 16]),
                current.revision(),
                next,
            )
            .unwrap(),
        ),
    );
    let gate = InputGateRecord::new(
        current.thread_id(),
        next,
        InputGateState::RepairRequired(target.clone()),
        current.accepted_high_water(),
        current.route_generation_high_water(),
        current.selected_route(),
        current.live_steering_count(),
        current.live_next_turn_count(),
        current.live_logical_utf8_bytes(),
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::InputGate(gate)).unwrap();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(
            fixture
                .storage
                .fixture_contribution(fixture.storage.revision(&home).unwrap(), batch),
        )
        .unwrap();
    assert!(matches!(
        home.execute(command),
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    target
}

fn assert_recovered(
    storage: &SyndicStorage,
    home: &beryl_home_store::HomeStore,
    thread: beryl_model::SyndicThreadId,
    turn: beryl_model::SyndicTurnId,
    target: &RepairRequiredTarget,
) {
    assert_eq!(
        storage
            .input_gate(home, thread, limit())
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::Idle
    );
    let state = storage.turn_state(home, turn, limit()).unwrap().unwrap();
    assert_eq!(
        state.incomplete_reason(),
        Some(TurnIncompleteReason::AuthorityLost)
    );
    assert_eq!(state.resolved_repair().unwrap().target(), target);
    let terminal = storage
        .source_event(home, turn, target.gap().terminal().sequence(), limit())
        .unwrap()
        .unwrap();
    assert_eq!(terminal.repair_witness(), target.gap().terminal());
    assert!(matches!(
        terminal.payload(),
        syndic_storage::SourceEventPayload::TurnEnded(status) if *status == target.gap().status()
    ));
    let head = storage
        .transcript_view_head(home, thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.thread_id(), thread);
    assert_eq!(head.committed_tail(), Some(turn));
    assert_eq!(
        head.lifecycle(),
        syndic_storage::ProjectionLifecycle::Current
    );
    let summary = storage
        .history_summary(home, thread, limit())
        .unwrap()
        .unwrap();
    assert!(!summary.complete());
    assert_eq!(state.finalized_item_count(), state.item_count());
    assert_eq!(summary.committed_tail(), Some(turn));
}

#[test]
fn ordinary_startup_preserves_repair_disposition_without_backend_history() {
    let _guard = TEST_LOCK.lock().unwrap();
    for consumed in [false, true] {
        let mut fixture = Fixture::new(210);
        let target = enter_repair(&mut fixture);
        let target = consumed
            .then(|| consume_repair(&fixture, target.clone()))
            .unwrap_or(target);
        let turn = target.turn_id();
        let home = fixture.home();
        recover_startup(&home, &fixture.storage).unwrap();
        assert_recovered(&fixture.storage, &home, fixture.thread, turn, &target);
        recover_startup(&home, &fixture.storage).unwrap();
        assert_recovered(&fixture.storage, &home, fixture.thread, turn, &target);
        drop(home);
        let (directory, service) = fixture.into_service();
        let _ = service.close().unwrap();
        drop(directory);
    }
}

#[test]
fn candidate_startup_converges_consumed_repair_before_publication() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::new(211);
    let admitted = enter_repair(&mut fixture);
    let target = consume_repair(&fixture, admitted);
    let thread = fixture.thread;
    let turn = target.turn_id();
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    BerylState::register(&mut candidate).unwrap();
    let mut publication = candidate
        .prepare_publication(
            SyndicStorage::required_domains()
                .unwrap()
                .merge(BerylState::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap();
    let access: HomeCandidateRecoveryAccess<'_> = publication.recovery_access().unwrap();
    recover_startup_candidate(&access, &storage).unwrap();
    drop(access);
    let home = publication.publish().unwrap();
    assert_recovered(&storage, &home, thread, turn, &target);
    home.close().unwrap();
}

#[test]
fn candidate_startup_reconciles_repair_convergence_before_reopen_and_retry() {
    let _guard = TEST_LOCK.lock().unwrap();
    for fault in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
        FaultPoint::AfterPersist,
    ] {
        let mut fixture = Fixture::new(212);
        let target = enter_repair(&mut fixture);
        let thread = fixture.thread;
        let turn = target.turn_id();
        let (directory, service) = fixture.into_service();
        let _ = service.close().unwrap();
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        BerylState::register(&mut candidate).unwrap();
        let mut publication = candidate
            .prepare_publication(
                SyndicStorage::required_domains()
                    .unwrap()
                    .merge(BerylState::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap();
        let access = publication.recovery_access().unwrap();
        faults.fail_next(fault);
        let result = recover_startup_candidate(&access, &storage);
        use beryl_app::cas_projection::ProjectionCoordinatorError;
        assert!(matches!(
            (fault, result),
            (
                FaultPoint::BeforeCommit,
                Err(ProjectionCoordinatorError::CommandNotCommitted(_))
            ) | (
                FaultPoint::AfterCommitBeforePersist,
                Err(ProjectionCoordinatorError::CommandIndeterminate { .. })
            ) | (
                FaultPoint::AfterPersist,
                Err(ProjectionCoordinatorError::CommandCommitted { .. })
            )
        ));
        if fault == FaultPoint::AfterCommitBeforePersist {
            assert_eq!(access.pending_reconciliations().len(), 1);
            drop(access);
            let error = publication.publish().unwrap_err();
            assert!(matches!(
                error.error(),
                HomeCandidateError::PendingReconciliation { count: 1 }
            ));
            publication = error.into_parts().1;
            let access = publication.recovery_access().unwrap();
            for reconciliation in access.pending_reconciliations() {
                assert!(matches!(
                    access.retry_reconciliation(&reconciliation).unwrap(),
                    ReconciliationResolution::ExactNew { .. }
                ));
            }
            drop(access);
        } else {
            assert!(access.pending_reconciliations().is_empty());
            drop(access);
        }
        publication.close().unwrap();

        let mut retry = HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let storage = SyndicStorage::register(&mut retry).unwrap();
        BerylState::register(&mut retry).unwrap();
        let mut retry = retry
            .prepare_publication(
                SyndicStorage::required_domains()
                    .unwrap()
                    .merge(BerylState::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap();
        let access = retry.recovery_access().unwrap();
        recover_startup_candidate(&access, &storage).unwrap();
        drop(access);
        let home = retry.publish().unwrap();
        assert_recovered(&storage, &home, thread, turn, &target);
        home.close().unwrap();
    }
}
