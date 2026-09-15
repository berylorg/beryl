use beryl_app::cas_projection::{
    OrdinaryTurnExecutionError,
    test_faults::{converge_terminal_history, converge_terminal_history_candidate},
};
use beryl_home_store::{
    HomeCandidateError, HomeOpenCandidate, HomeOpenOptions, HomeOpenPublication, HomeSchemaVersion,
    ReconciliationResolution,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{SyndicThreadId, SyndicTurnId};
use beryl_state::BerylState;
use syndic_storage::{
    InputGateState, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, TurnEndStatus,
    TurnIncompleteReason, TurnLifecycle,
};

use super::{TEST_LOCK, syndic::Fixture};

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(65_536).unwrap()
}

fn finalizing_home(complete: bool) -> (tempfile::TempDir, SyndicThreadId, SyndicTurnId) {
    let mut fixture = Fixture::new(170);
    let submitted = fixture.submit_text("candidate terminal history");
    let source = fixture.activate_without_terminal(submitted);
    fixture.publish_terminal_without_convergence(
        submitted,
        &source,
        if complete {
            TurnEndStatus::complete()
        } else {
            TurnEndStatus::incomplete(TurnIncompleteReason::AuthorityLost)
        },
    );
    let thread = fixture.thread;
    assert_eq!(
        fixture
            .storage
            .input_gate(&fixture.home(), thread, limit())
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::FinalizingHistory(submitted.turn)
    );
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    (directory, thread, submitted.turn)
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

#[test]
fn candidate_history_convergence_preserves_complete_and_incomplete_fixed_points() {
    let _guard = TEST_LOCK.lock().unwrap();
    for complete in [false, true] {
        for reopen in [false, true] {
            let (directory, thread, turn) = finalizing_home(complete);
            let faults = FaultController::new();
            let (mut publication, storage) = open_candidate(directory.path(), faults.clone());
            beryl_home_store::test_faults::with_initial_publication_store(&publication, |store| {
                assert!(
                    converge_terminal_history(
                        store,
                        &storage,
                        thread,
                        turn,
                        SyndicTimestamp::from_unix_millis(1_000),
                        limit()
                    )
                    .is_err()
                );
            });
            let (store, storage, evidence) = if reopen {
                let store = publication.publish().unwrap();
                faults.fail_next(FaultPoint::BeforeReadConfirmation);
                assert!(store.home_revision().is_err());
                let mut recovered = store.recover_same_home().unwrap();
                let fresh = SyndicStorage::reacquire_candidate(&recovered).unwrap();
                let access = recovered.recovery_access().unwrap();
                assert!(
                    converge_terminal_history_candidate(
                        &access,
                        &storage,
                        thread,
                        turn,
                        SyndicTimestamp::from_unix_millis(1_000),
                        limit()
                    )
                    .is_err()
                );
                converge_terminal_history_candidate(
                    &access,
                    &fresh,
                    thread,
                    turn,
                    SyndicTimestamp::from_unix_millis(1_000),
                    limit(),
                )
                .unwrap();
                let evidence = fresh
                    .terminal_history_evidence_candidate(&access, thread, turn, limit())
                    .unwrap()
                    .unwrap();
                (recovered.publish().unwrap(), fresh, evidence)
            } else {
                let access = publication.recovery_access().unwrap();
                assert_eq!(
                    storage
                        .terminal_history_evidence_candidate(&access, thread, turn, limit())
                        .unwrap(),
                    None
                );
                converge_terminal_history_candidate(
                    &access,
                    &storage,
                    thread,
                    turn,
                    SyndicTimestamp::from_unix_millis(1_000),
                    limit(),
                )
                .unwrap();
                let evidence = storage
                    .terminal_history_evidence_candidate(&access, thread, turn, limit())
                    .unwrap()
                    .unwrap();
                (publication.publish().unwrap(), storage, evidence)
            };
            assert_eq!(
                evidence.lifecycle(),
                if complete {
                    TurnLifecycle::Complete
                } else {
                    TurnLifecycle::Incomplete
                }
            );
            assert_eq!(
                storage
                    .terminal_history_evidence(&store, thread, turn, limit())
                    .unwrap(),
                Some(evidence)
            );
            assert_eq!(
                storage
                    .input_gate(&store, thread, limit())
                    .unwrap()
                    .unwrap()
                    .state(),
                &InputGateState::Idle
            );
            store.close().unwrap();
        }
    }
}

#[test]
fn candidate_history_convergence_preserves_failed_command_outcomes_and_reconciliation() {
    let _guard = TEST_LOCK.lock().unwrap();
    for fault in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
        FaultPoint::AfterPersist,
    ] {
        let (directory, thread, turn) = finalizing_home(true);
        let faults = FaultController::new();
        let (mut publication, storage) = open_candidate(directory.path(), faults.clone());
        let access = publication.recovery_access().unwrap();
        faults.fail_next(fault);
        let result = converge_terminal_history_candidate(
            &access,
            &storage,
            thread,
            turn,
            SyndicTimestamp::from_unix_millis(1_000),
            limit(),
        );
        match fault {
            FaultPoint::BeforeCommit => assert!(
                matches!(
                    result,
                    Err(OrdinaryTurnExecutionError::HomeCommandNotCommitted(_))
                ),
                "{result:?}"
            ),
            FaultPoint::AfterCommitBeforePersist => assert!(
                matches!(
                    result,
                    Err(OrdinaryTurnExecutionError::HomeCommandIndeterminate { .. })
                ),
                "{result:?}"
            ),
            FaultPoint::AfterPersist => assert!(
                matches!(
                    result,
                    Err(OrdinaryTurnExecutionError::HomeCommandCommitted { .. })
                ),
                "{result:?}"
            ),
            _ => unreachable!(),
        }
        if fault == FaultPoint::AfterCommitBeforePersist {
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
        } else {
            publication
                .publish()
                .unwrap_err()
                .into_parts()
                .1
                .close()
                .unwrap();
        }
        let (mut publication, storage) = open_candidate(directory.path(), FaultController::new());
        let access = publication.recovery_access().unwrap();
        converge_terminal_history_candidate(
            &access,
            &storage,
            thread,
            turn,
            SyndicTimestamp::from_unix_millis(1_000),
            limit(),
        )
        .unwrap();
        assert_eq!(
            storage
                .terminal_history_evidence_candidate(&access, thread, turn, limit())
                .unwrap()
                .unwrap()
                .lifecycle(),
            TurnLifecycle::Complete
        );
        publication.publish().unwrap().close().unwrap();
    }
}
