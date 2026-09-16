use beryl_app::cas_projection::{
    OrdinaryTurnExecutionError,
    test_faults::{
        TerminalHistoryBarrierStage, converge_terminal_history,
        converge_terminal_history_candidate, install_terminal_history_barrier,
        publish_source_less_terminal_candidate,
    },
};
use beryl_home_store::{
    CommandOutcome, CursorReadLimits, HomeCandidateError, HomeCommand, HomeOpenCandidate,
    HomeOpenOptions, HomeOpenPublication, HomeSchemaVersion, ReconciliationResolution,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{DomainRevision, SyndicThreadId, SyndicTurnId};
use beryl_state::BerylState;
use syndic_storage::{
    BindingState, DeliveryRecoveryCase, InputGateState, SourceEventPayload, SourceEventSequence,
    SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, TurnEndStatus, TurnIncompleteReason,
    TurnLifecycle,
    test_faults::{FixtureBatch, FixtureDelete},
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

fn abandoned_home() -> (
    tempfile::TempDir,
    SyndicThreadId,
    SyndicTurnId,
    DomainRevision,
) {
    let mut fixture = Fixture::new(171);
    let submitted = fixture.submit_text("candidate source-less terminal");
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
        panic!("source-less candidate fixture must classify its active turn")
    };
    let request = active
        .generic_abandonment(
            "candidate source-less terminal authority lost",
            SyndicTimestamp::from_unix_millis(1_000),
        )
        .unwrap();
    assert!(matches!(
        fixture
            .home()
            .execute_current(fixture.storage.current_abandon_active_binding(request)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let revision = fixture.storage.revision(&fixture.home()).unwrap();
    let thread = fixture.thread;
    assert!(matches!(
        fixture
            .storage
            .current_binding(&fixture.home(), thread, limit())
            .unwrap()
            .unwrap()
            .binding()
            .state(),
        BindingState::Stale(_)
    ));
    assert!(matches!(
        fixture
            .storage
            .input_gate(&fixture.home(), thread, limit())
            .unwrap()
            .unwrap()
            .state(),
        InputGateState::PendingTurn(turn) if *turn == submitted.turn
    ));
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    (directory, thread, submitted.turn, revision)
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

#[test]
fn candidate_source_less_terminal_publication_preserves_exact_event_and_publication_parity() {
    let _guard = TEST_LOCK.lock().unwrap();
    for recovered in [false, true] {
        let (directory, thread, turn, _) = abandoned_home();
        let faults = FaultController::new();
        let (mut publication, storage) = open_candidate(directory.path(), faults.clone());
        let minimum = SyndicTimestamp::from_unix_millis(4_000_000_000_000);
        let (store, storage, prior_state, prior_summary, candidate_state) = if recovered {
            let store = publication.publish().unwrap();
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
            let mut recovered = store.recover_same_home().unwrap();
            let fresh = SyndicStorage::reacquire_candidate(&recovered).unwrap();
            let access = recovered.recovery_access().unwrap();
            assert!(
                publish_source_less_terminal_candidate(
                    &access,
                    &storage,
                    thread,
                    turn,
                    minimum,
                    limit()
                )
                .is_err()
            );
            let prior_state = fresh
                .turn_state_candidate(&access, turn, limit())
                .unwrap()
                .unwrap();
            let prior_summary = fresh
                .history_summary_candidate(&access, thread, limit())
                .unwrap()
                .unwrap();
            assert!(
                publish_source_less_terminal_candidate(
                    &access,
                    &fresh,
                    thread,
                    turn,
                    minimum,
                    SyndicPointReadLimit::new(1).unwrap(),
                )
                .is_err()
            );
            syndic_storage::test_faults::reset_syndic_point_read_count();
            publish_source_less_terminal_candidate(&access, &fresh, thread, turn, minimum, limit())
                .unwrap();
            assert!(syndic_storage::test_faults::syndic_point_read_count() <= 64);
            let state = fresh
                .turn_state_candidate(&access, turn, limit())
                .unwrap()
                .unwrap();
            (
                recovered.publish().unwrap(),
                fresh,
                prior_state,
                prior_summary,
                state,
            )
        } else {
            beryl_home_store::test_faults::with_initial_publication_store(&publication, |store| {
                assert!(storage.turn_state(store, turn, limit()).is_err());
            });
            let access = publication.recovery_access().unwrap();
            let (foreign_directory, _, _, _) = abandoned_home();
            let (foreign_publication, foreign_storage) =
                open_candidate(foreign_directory.path(), FaultController::new());
            assert!(
                publish_source_less_terminal_candidate(
                    &access,
                    &foreign_storage,
                    thread,
                    turn,
                    minimum,
                    limit(),
                )
                .is_err()
            );
            foreign_publication.close().unwrap();
            let prior_state = storage
                .turn_state_candidate(&access, turn, limit())
                .unwrap()
                .unwrap();
            let prior_summary = storage
                .history_summary_candidate(&access, thread, limit())
                .unwrap()
                .unwrap();
            assert!(
                publish_source_less_terminal_candidate(
                    &access,
                    &storage,
                    thread,
                    turn,
                    minimum,
                    SyndicPointReadLimit::new(1).unwrap(),
                )
                .is_err()
            );
            syndic_storage::test_faults::reset_syndic_point_read_count();
            publish_source_less_terminal_candidate(
                &access,
                &storage,
                thread,
                turn,
                minimum,
                limit(),
            )
            .unwrap();
            assert!(syndic_storage::test_faults::syndic_point_read_count() <= 64);
            let state = storage
                .turn_state_candidate(&access, turn, limit())
                .unwrap()
                .unwrap();
            (
                publication.publish().unwrap(),
                storage,
                prior_state,
                prior_summary,
                state,
            )
        };
        assert_eq!(candidate_state.lifecycle(), TurnLifecycle::Incomplete);
        assert_eq!(
            candidate_state.incomplete_reason(),
            Some(TurnIncompleteReason::AuthorityLost)
        );
        let persisted_state = storage.turn_state(&store, turn, limit()).unwrap().unwrap();
        assert_eq!(persisted_state, candidate_state);
        let event = storage
            .source_event(
                &store,
                turn,
                SourceEventSequence::new(persisted_state.source_event_count()).unwrap(),
                limit(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            persisted_state.source_event_count(),
            prior_state.source_event_count().checked_add(1).unwrap()
        );
        assert_eq!(event.source(), None);
        assert_eq!(
            event.payload(),
            &SourceEventPayload::TurnEnded(TurnEndStatus::incomplete(
                TurnIncompleteReason::AuthorityLost
            ))
        );
        assert!(persisted_state.updated_at() >= minimum);
        assert!(persisted_state.updated_at() >= prior_state.updated_at());
        let persisted_summary = storage
            .history_summary(&store, thread, limit())
            .unwrap()
            .unwrap();
        assert!(persisted_state.updated_at() >= prior_summary.last_activity_at());
        assert!(persisted_summary.last_activity_at() >= persisted_state.updated_at());
        assert_eq!(
            storage
                .terminal_history_evidence(&store, thread, turn, limit())
                .unwrap(),
            None
        );
        store.close().unwrap();
    }
}

#[test]
fn candidate_source_less_terminal_publication_preserves_command_custody_and_read_stabilization() {
    let _guard = TEST_LOCK.lock().unwrap();
    for fault in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
        FaultPoint::AfterPersist,
    ] {
        let (directory, thread, turn, _) = abandoned_home();
        let faults = FaultController::new();
        let (mut publication, storage) = open_candidate(directory.path(), faults.clone());
        let access = publication.recovery_access().unwrap();
        faults.fail_next(fault);
        let result = publish_source_less_terminal_candidate(
            &access,
            &storage,
            thread,
            turn,
            SyndicTimestamp::from_unix_millis(2_000),
            limit(),
        );
        match fault {
            FaultPoint::BeforeCommit => {
                assert!(result.unwrap_err().contains("Publication(Command("));
                publication.close().unwrap();
            }
            FaultPoint::AfterCommitBeforePersist => {
                assert!(result.unwrap_err().contains("CommandIndeterminate"));
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
                assert!(result.unwrap_err().contains("CommandCommitted"));
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

    let (directory, thread, turn, revision) = abandoned_home();
    let (mut publication, storage) = open_candidate(directory.path(), FaultController::new());
    let access = publication.recovery_access().unwrap();
    let barrier = install_terminal_history_barrier(
        thread,
        TerminalHistoryBarrierStage::SourceFrontierObserved,
    );
    let result = std::thread::scope(|scope| {
        let task = scope.spawn(|| {
            publish_source_less_terminal_candidate(
                &access,
                &storage,
                thread,
                turn,
                SyndicTimestamp::from_unix_millis(2_000),
                limit(),
            )
        });
        barrier.wait();
        let mut batch = FixtureBatch::new();
        batch.delete(FixtureDelete::HistorySummary(thread)).unwrap();
        let mut command = HomeCommand::new(access.home_revision().unwrap());
        command
            .add(storage.clone().fixture_contribution(revision, batch))
            .unwrap();
        assert!(matches!(
            access.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        barrier.release();
        task.join().unwrap()
    });
    assert!(result.unwrap_err().contains("ConcurrentChange"));
    publication.close().unwrap();
}
