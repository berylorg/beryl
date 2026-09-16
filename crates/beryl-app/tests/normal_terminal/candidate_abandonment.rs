use beryl_app::cas_projection::{
    ProjectionPublicationFailure, StopCoordinationOutcome,
    test_faults::{abandon_active_candidate, abandon_stop_candidate},
};
use beryl_home_store::{
    CommandOutcome, CursorReadLimits, HomeCandidateError, HomeOpenCandidate, HomeOpenOptions,
    HomeOpenPublication, HomeSchemaVersion, ReconciliationResolution,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{BindingRevision, SyndicThreadId, SyndicTurnId};
use beryl_state::BerylState;
use syndic_storage::{
    AbandonActiveBinding, AbandonStopOperation, BindingState, DeliveryRecoveryCase, InputGateState,
    StopAdmissionRead, StopCause, StopCauseSet, StopOperationNonce, StopOperationState,
    SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, TurnIncompleteReason, TurnLifecycle,
};

use super::{TEST_LOCK, syndic::Fixture};

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(65_536).unwrap()
}

fn open_candidate(
    path: &std::path::Path,
    faults: FaultController,
) -> (HomeOpenPublication, SyndicStorage) {
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    BerylState::register(&mut candidate).unwrap();
    let publication = candidate
        .prepare_publication(
            SyndicStorage::required_domains()
                .unwrap()
                .merge(BerylState::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap();
    (publication, storage)
}

fn active_home() -> (
    tempfile::TempDir,
    SyndicThreadId,
    SyndicTurnId,
    AbandonActiveBinding,
    BindingRevision,
) {
    let mut fixture = Fixture::new(172);
    let submitted = fixture.submit_text("candidate active abandonment");
    fixture.activate_without_terminal(submitted);
    let startup = fixture
        .storage
        .delivery_recovery_startup_page(
            &fixture.home(),
            None,
            CursorReadLimits::new(1, 65_536).unwrap(),
        )
        .unwrap()
        .records()[0]
        .clone();
    let DeliveryRecoveryCase::Active(active) = fixture
        .storage
        .classify_delivery_recovery(&fixture.home(), &startup, limit())
        .unwrap()
    else {
        panic!("candidate active abandonment fixture must classify as active")
    };
    let request = active
        .generic_abandonment(
            "candidate active abandonment authority lost",
            SyndicTimestamp::from_unix_millis(1_000),
        )
        .unwrap();
    let revision = fixture
        .storage
        .current_binding(&fixture.home(), fixture.thread, limit())
        .unwrap()
        .unwrap()
        .binding()
        .revision();
    let thread = fixture.thread;
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    (directory, thread, submitted.turn, request, revision)
}

fn assert_active_abandoned(
    storage: &SyndicStorage,
    store: &beryl_home_store::HomeStore,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    returned_revision: BindingRevision,
) {
    let binding = storage
        .current_binding(store, thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(binding.binding().revision(), returned_revision);
    assert!(matches!(binding.binding().state(), BindingState::Stale(_)));
    assert!(matches!(
        storage.input_gate(store, thread, limit()).unwrap().unwrap().state(),
        InputGateState::PendingTurn(actual) if *actual == turn
    ));
}

fn stopping_home() -> (
    tempfile::TempDir,
    SyndicThreadId,
    SyndicTurnId,
    AbandonStopOperation,
) {
    let mut fixture = Fixture::new(173);
    let submitted = fixture.submit_text("candidate stop abandonment");
    fixture.activate_without_terminal(submitted);
    let command = fixture.store.live_home_command().unwrap();
    let StopAdmissionRead::Admissible(candidate) = fixture
        .storage
        .stop_admission_read(command.home(), fixture.thread, limit())
        .unwrap()
    else {
        panic!("candidate stop abandonment fixture must admit a stop")
    };
    let admission = candidate.admission(
        StopOperationNonce::from_bytes([173; 16]),
        StopCauseSet::from(StopCause::SelectedOperationControl),
    );
    assert!(matches!(
        command
            .home()
            .execute_current(fixture.storage.current_admit_stop_operation(admission)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let startup = fixture
        .storage
        .delivery_recovery_startup_page(
            &fixture.home(),
            None,
            CursorReadLimits::new(1, 65_536).unwrap(),
        )
        .unwrap()
        .records()[0]
        .clone();
    let DeliveryRecoveryCase::Stopping(stopping) = fixture
        .storage
        .classify_delivery_recovery(&fixture.home(), &startup, limit())
        .unwrap()
    else {
        panic!("candidate stop abandonment fixture must classify as stopping")
    };
    let observed_at = stopping.minimum_timestamp();
    let request = AbandonStopOperation::new(
        stopping.operation_id(),
        stopping.target().clone(),
        stopping.current_gate_revision(),
        stopping.stop_revision(),
        stopping.current_state_revision(),
        stopping.startup_abandonment_reason(),
        stopping
            .startup_stale_binding("candidate stop abandonment authority lost", observed_at)
            .unwrap(),
    );
    let thread = fixture.thread;
    drop(command);
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    (directory, thread, submitted.turn, request)
}

fn assert_stop_abandoned(
    storage: &SyndicStorage,
    store: &beryl_home_store::HomeStore,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    request: &AbandonStopOperation,
) {
    assert!(matches!(
        storage
            .stop_operation(store, request.operation_id(), limit())
            .unwrap()
            .unwrap()
            .state(),
        StopOperationState::Abandoned(_)
    ));
    assert!(matches!(
        storage.input_gate(store, thread, limit()).unwrap().unwrap().state(),
        InputGateState::FinalizingHistory(actual) if *actual == turn
    ));
    let state = storage.turn_state(store, turn, limit()).unwrap().unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Incomplete);
    assert_eq!(
        state.incomplete_reason(),
        Some(TurnIncompleteReason::AuthorityLost)
    );
    assert!(matches!(
        storage
            .current_binding(store, thread, limit())
            .unwrap()
            .unwrap()
            .binding()
            .state(),
        BindingState::Stale(stale) if stale == request.stale()
    ));
}

#[test]
fn candidate_active_abandonment_preserves_exact_revision_and_publication_parity() {
    let _guard = TEST_LOCK.lock().unwrap();
    for recovered in [false, true] {
        let (directory, thread, turn, request, prior_revision) = active_home();
        let faults = FaultController::new();
        let (mut publication, storage) = open_candidate(directory.path(), faults.clone());
        let (store, storage, returned_revision) = if recovered {
            let store = publication.publish().unwrap();
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
            let mut recovered = store.recover_same_home().unwrap();
            let fresh = SyndicStorage::reacquire_candidate(&recovered).unwrap();
            let access = recovered.recovery_access().unwrap();
            let revision = abandon_active_candidate(&access, &fresh, &request).unwrap();
            (recovered.publish().unwrap(), fresh, revision)
        } else {
            let access = publication.recovery_access().unwrap();
            let revision = abandon_active_candidate(&access, &storage, &request).unwrap();
            (publication.publish().unwrap(), storage, revision)
        };
        assert_eq!(returned_revision, prior_revision.checked_next().unwrap());
        assert_active_abandoned(&storage, &store, thread, turn, returned_revision);
        store.close().unwrap();
    }
}

#[test]
fn candidate_active_abandonment_refuses_stale_foreign_conflicting_and_failed_commands() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (directory, thread, turn, request, _) = active_home();
    let faults = FaultController::new();
    let (publication, stale_storage) = open_candidate(directory.path(), faults.clone());
    let store = publication.publish().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovered = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovered).unwrap();
    let access = recovered.recovery_access().unwrap();
    assert!(matches!(
        abandon_active_candidate(&access, &stale_storage, &request),
        Err(ProjectionPublicationFailure::Command(_))
    ));
    let (foreign_directory, _, _, _, _) = active_home();
    let (foreign_publication, foreign_storage) =
        open_candidate(foreign_directory.path(), FaultController::new());
    assert!(matches!(
        abandon_active_candidate(&access, &foreign_storage, &request),
        Err(ProjectionPublicationFailure::Command(_))
    ));
    foreign_publication.close().unwrap();
    let revision = abandon_active_candidate(&access, &fresh, &request).unwrap();
    assert!(matches!(
        abandon_active_candidate(&access, &fresh, &request),
        Err(ProjectionPublicationFailure::Command(_))
    ));
    let store = recovered.publish().unwrap();
    assert_active_abandoned(&fresh, &store, thread, turn, revision);
    store.close().unwrap();

    for fault in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
        FaultPoint::AfterPersist,
    ] {
        let (directory, _, _, request, _) = active_home();
        let faults = FaultController::new();
        let (mut publication, storage) = open_candidate(directory.path(), faults.clone());
        let access = publication.recovery_access().unwrap();
        faults.fail_next(fault);
        let result = abandon_active_candidate(&access, &storage, &request);
        match fault {
            FaultPoint::BeforeCommit => {
                assert!(matches!(
                    result,
                    Err(ProjectionPublicationFailure::Command(_))
                ));
                publication.close().unwrap();
            }
            FaultPoint::AfterCommitBeforePersist => {
                assert!(matches!(
                    result,
                    Err(ProjectionPublicationFailure::CommandIndeterminate { .. })
                ));
                assert_eq!(access.pending_reconciliations().len(), 1);
                let failure = publication.publish().unwrap_err();
                assert!(matches!(
                    failure.error(),
                    HomeCandidateError::PendingReconciliation { count: 1 }
                ));
                publication = failure.into_parts().1;
                let access = publication.recovery_access().unwrap();
                for handle in access.pending_reconciliations() {
                    assert!(matches!(
                        access.retry_reconciliation(&handle).unwrap(),
                        ReconciliationResolution::ExactNew { .. }
                    ));
                }
                publication.close().unwrap();
            }
            FaultPoint::AfterPersist => {
                assert!(matches!(
                    result,
                    Err(ProjectionPublicationFailure::CommandCommitted { .. })
                ));
                publication
                    .publish()
                    .unwrap_err()
                    .into_parts()
                    .1
                    .close()
                    .unwrap();
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn candidate_stop_abandonment_preserves_terminal_outcome_and_publication_parity() {
    let _guard = TEST_LOCK.lock().unwrap();
    for recovered in [false, true] {
        let (directory, thread, turn, request) = stopping_home();
        let faults = FaultController::new();
        let (mut publication, storage) = open_candidate(directory.path(), faults.clone());
        let (store, storage) = if recovered {
            let store = publication.publish().unwrap();
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
            let mut recovered = store.recover_same_home().unwrap();
            let fresh = SyndicStorage::reacquire_candidate(&recovered).unwrap();
            let access = recovered.recovery_access().unwrap();
            abandon_stop_candidate(&access, &fresh, &request).unwrap();
            (recovered.publish().unwrap(), fresh)
        } else {
            let access = publication.recovery_access().unwrap();
            abandon_stop_candidate(&access, &storage, &request).unwrap();
            (publication.publish().unwrap(), storage)
        };
        assert_stop_abandoned(&storage, &store, thread, turn, &request);
        store.close().unwrap();
    }
}

#[test]
fn candidate_stop_abandonment_refuses_stale_foreign_conflicting_and_failed_commands() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (directory, thread, turn, request) = stopping_home();
    let faults = FaultController::new();
    let (publication, stale_storage) = open_candidate(directory.path(), faults.clone());
    let store = publication.publish().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovered = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovered).unwrap();
    let access = recovered.recovery_access().unwrap();
    assert!(matches!(
        abandon_stop_candidate(&access, &stale_storage, &request),
        Err(ProjectionPublicationFailure::Command(_))
    ));
    let (foreign_directory, _, _, _) = stopping_home();
    let (foreign_publication, foreign_storage) =
        open_candidate(foreign_directory.path(), FaultController::new());
    assert!(matches!(
        abandon_stop_candidate(&access, &foreign_storage, &request),
        Err(ProjectionPublicationFailure::Command(_))
    ));
    foreign_publication.close().unwrap();
    abandon_stop_candidate(&access, &fresh, &request).unwrap();
    assert!(matches!(
        abandon_stop_candidate(&access, &fresh, &request),
        Err(ProjectionPublicationFailure::Command(_))
    ));
    let store = recovered.publish().unwrap();
    assert_stop_abandoned(&fresh, &store, thread, turn, &request);
    store.close().unwrap();

    for fault in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
        FaultPoint::AfterPersist,
    ] {
        let (directory, _, _, request) = stopping_home();
        let faults = FaultController::new();
        let (mut publication, storage) = open_candidate(directory.path(), faults.clone());
        let access = publication.recovery_access().unwrap();
        faults.fail_next(fault);
        let result = abandon_stop_candidate(&access, &storage, &request);
        match fault {
            FaultPoint::BeforeCommit => {
                assert!(matches!(
                    result,
                    Err(ProjectionPublicationFailure::Command(_))
                ));
                publication.close().unwrap();
            }
            FaultPoint::AfterCommitBeforePersist => {
                assert!(matches!(
                    result,
                    Err(ProjectionPublicationFailure::CommandIndeterminate { .. })
                ));
                assert_eq!(access.pending_reconciliations().len(), 1);
                let failure = publication.publish().unwrap_err();
                assert!(matches!(
                    failure.error(),
                    HomeCandidateError::PendingReconciliation { count: 1 }
                ));
                publication = failure.into_parts().1;
                let access = publication.recovery_access().unwrap();
                for handle in access.pending_reconciliations() {
                    assert!(matches!(
                        access.retry_reconciliation(&handle).unwrap(),
                        ReconciliationResolution::ExactNew { .. }
                    ));
                }
                publication.close().unwrap();
            }
            FaultPoint::AfterPersist => {
                assert!(matches!(
                    result,
                    Err(ProjectionPublicationFailure::CommandCommitted { .. })
                ));
                publication
                    .publish()
                    .unwrap_err()
                    .into_parts()
                    .1
                    .close()
                    .unwrap();
            }
            _ => unreachable!(),
        }
    }
}
