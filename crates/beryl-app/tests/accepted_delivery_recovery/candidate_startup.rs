use beryl_app::cas_projection::{
    ProjectionCoordinatorError,
    test_faults::{
        StartupRecoverySnapshot, abandon_active_candidate, install_startup_classification_barrier,
        publish_source_less_terminal_candidate, recover_startup_candidate,
    },
};
use beryl_home_store::{
    CommandOutcome, CursorReadLimits, HomeCandidateError, HomeOpenCandidate, HomeOpenOptions,
    HomeOpenPublication, HomeSchemaVersion, ReconciliationResolution,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration, CasTurnId,
    SyndicDraftId, SyndicThreadId, SyndicTurnId,
};
use beryl_state::BerylState;
use syndic_storage::{
    BindingState, ClaimCompactionDispatch, CompactionAdmissionRead, CompactionAttemptNonce,
    CompactionOperationNonce, CompactionProviderEvent, CompactionProviderSequence,
    DeliveryRecoveryCase, InputGateState, PublishCompactionProviderEvent, StopAdmissionRead,
    StopCause, StopCauseSet, StopOperationNonce, SyndicPointReadLimit, SyndicStorage,
    SyndicTimestamp, TurnEndStatus, TurnIncompleteReason, TurnLifecycle,
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

#[derive(Clone, Copy)]
enum StartupCase {
    Pending,
    Active,
    Stopping,
    PostAbandonment,
    Finalizing,
    DeferredCompaction,
    ProviderStopping,
    Settled,
}

fn assert_snapshot(case: StartupCase, snapshot: StartupRecoverySnapshot) {
    let expected = match case {
        StartupCase::Pending => (1, 1, 1, 0, 0, 0),
        StartupCase::Active | StartupCase::Stopping | StartupCase::ProviderStopping => {
            (1, 1, 0, 1, 1, 0)
        }
        StartupCase::PostAbandonment | StartupCase::Finalizing => (1, 1, 0, 0, 1, 0),
        StartupCase::DeferredCompaction => (1, 1, 0, 0, 0, 1),
        StartupCase::Settled => (1, 0, 0, 0, 0, 0),
    };
    assert_eq!(
        (
            snapshot.page_reads,
            snapshot.cases,
            snapshot.pending_turns,
            snapshot.active_convergences,
            snapshot.terminal_convergences,
            snapshot.deferred_compactions,
        ),
        expected
    );
}

fn source_case(case: StartupCase, seed: u8) -> (tempfile::TempDir, SyndicThreadId, SyndicTurnId) {
    let mut fixture = Fixture::new(seed);
    let submitted = fixture.submit_text("candidate startup recovery");
    let thread = fixture.thread;
    match case {
        StartupCase::Pending => {}
        StartupCase::Active => {
            fixture.activate_without_terminal(submitted);
        }
        StartupCase::Stopping => {
            fixture.activate_without_terminal(submitted);
            let home = fixture.home();
            let StopAdmissionRead::Admissible(candidate) = fixture
                .storage
                .stop_admission_read(&home, thread, limit())
                .unwrap()
            else {
                panic!("active startup fixture must admit a stop")
            };
            let admission = candidate.admission(
                StopOperationNonce::from_bytes([seed; 16]),
                StopCauseSet::from(StopCause::SelectedOperationControl),
            );
            assert!(matches!(
                home.execute_current(fixture.storage.current_admit_stop_operation(admission)),
                CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
        }
        StartupCase::PostAbandonment => {
            fixture.activate_without_terminal(submitted);
            let home = fixture.home();
            let source = fixture
                .storage
                .delivery_recovery_startup_page(
                    &home,
                    None,
                    CursorReadLimits::new(1, 65_536).unwrap(),
                )
                .unwrap()
                .records()[0]
                .clone();
            let DeliveryRecoveryCase::Active(active) = fixture
                .storage
                .classify_delivery_recovery(&home, &source, limit())
                .unwrap()
            else {
                panic!("active startup fixture must classify as active")
            };
            let request = active
                .generic_abandonment(
                    "candidate startup post-abandonment",
                    SyndicTimestamp::from_unix_millis(1_000),
                )
                .unwrap();
            assert!(matches!(
                home.execute_current(fixture.storage.current_abandon_active_binding(request)),
                CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
        }
        StartupCase::Finalizing => {
            let source = fixture.activate_without_terminal(submitted);
            fixture.publish_terminal_without_convergence(
                submitted,
                &source,
                TurnEndStatus::incomplete(TurnIncompleteReason::AuthorityLost),
            );
        }
        StartupCase::DeferredCompaction => {
            fixture.complete_with_assistant(submitted, "completed before startup");
            let home = fixture.home();
            let CompactionAdmissionRead::Admissible(candidate) = fixture
                .storage
                .compaction_admission_read(&home, thread, limit())
                .unwrap()
            else {
                panic!("completed startup fixture must admit compaction")
            };
            let admission = candidate.admission(
                CompactionOperationNonce::from_bytes([seed; 16]),
                CompactionAttemptNonce::from_bytes([seed.wrapping_add(1); 16]),
                CasLoadedSessionGeneration::new(
                    CasProcessGeneration::new(u64::from(seed) + 1).unwrap(),
                    CasLoadedThreadGeneration::new(1).unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(70_000),
            );
            assert!(matches!(
                home.execute_current(
                    fixture
                        .storage
                        .current_admit_compaction_operation(admission)
                ),
                CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
        }
        StartupCase::ProviderStopping => {
            fixture.complete_with_assistant(submitted, "completed before provider stop");
            let home = fixture.home();
            let CompactionAdmissionRead::Admissible(candidate) = fixture
                .storage
                .compaction_admission_read(&home, thread, limit())
                .unwrap()
            else {
                panic!("completed startup fixture must admit provider compaction")
            };
            let attempt = CompactionAttemptNonce::from_bytes([seed.wrapping_add(1); 16]);
            let admission = candidate.admission(
                CompactionOperationNonce::from_bytes([seed; 16]),
                attempt,
                CasLoadedSessionGeneration::new(
                    CasProcessGeneration::new(u64::from(seed) + 1).unwrap(),
                    CasLoadedThreadGeneration::new(1).unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(70_000),
            );
            let operation = admission.operation_id();
            assert!(matches!(
                home.execute_current(
                    fixture
                        .storage
                        .current_admit_compaction_operation(admission)
                ),
                CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
            let record = fixture
                .storage
                .compaction_operation(&home, operation, limit())
                .unwrap()
                .unwrap();
            assert!(matches!(
                home.execute_current(fixture.storage.current_claim_compaction_dispatch(
                    ClaimCompactionDispatch::new(operation, record.revision(), attempt),
                )),
                CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
            publish_compaction_event(
                &home,
                &fixture.storage,
                operation,
                CompactionProviderEvent::ThreadStatus(
                    syndic_storage::CompactionThreadStatus::Active,
                ),
                70_001,
            );
            publish_compaction_event(
                &home,
                &fixture.storage,
                operation,
                CompactionProviderEvent::TurnStarted(
                    CasTurnId::new("candidate-provider-stop").unwrap(),
                ),
                70_002,
            );
            let StopAdmissionRead::Admissible(candidate) = fixture
                .storage
                .stop_admission_read(&home, thread, limit())
                .unwrap()
            else {
                panic!("active provider operation must admit a stop")
            };
            let stop = candidate.admission(
                StopOperationNonce::from_bytes([seed.wrapping_add(2); 16]),
                StopCauseSet::from(StopCause::SelectedOperationControl),
            );
            assert!(matches!(
                home.execute_current(fixture.storage.current_admit_stop_operation(stop)),
                CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
        }
        StartupCase::Settled => {
            fixture.complete_with_assistant(submitted, "already settled");
        }
    }
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    (directory, thread, submitted.turn)
}

fn publish_compaction_event(
    home: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
    operation: syndic_storage::CompactionOperationId,
    event: CompactionProviderEvent,
    observed_at: u64,
) {
    let record = storage
        .compaction_operation(home, operation, limit())
        .unwrap()
        .unwrap();
    let sequence = record
        .provider_frontier()
        .map_or(CompactionProviderSequence::FIRST, |frontier| {
            frontier.checked_next().unwrap()
        });
    assert!(matches!(
        home.execute_current(storage.current_publish_compaction_provider_event(
            PublishCompactionProviderEvent::new(
                operation,
                record.revision(),
                sequence,
                event,
                SyndicTimestamp::from_unix_millis(observed_at),
            ),
        )),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

fn assert_case_converged(
    case: StartupCase,
    storage: &SyndicStorage,
    store: &beryl_home_store::HomeStore,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
) {
    let gate = storage.input_gate(store, thread, limit()).unwrap().unwrap();
    match case {
        StartupCase::Pending => assert_eq!(gate.state(), &InputGateState::PendingTurn(turn)),
        StartupCase::Active
        | StartupCase::Stopping
        | StartupCase::PostAbandonment
        | StartupCase::Finalizing => {
            assert_eq!(gate.state(), &InputGateState::Idle);
            let state = storage.turn_state(store, turn, limit()).unwrap().unwrap();
            assert_eq!(state.lifecycle(), TurnLifecycle::Incomplete);
            assert_eq!(
                state.incomplete_reason(),
                Some(TurnIncompleteReason::AuthorityLost)
            );
        }
        StartupCase::DeferredCompaction => {
            assert_eq!(gate.state(), &InputGateState::Idle);
            let operation = storage
                .compaction_admission_read(store, thread, limit())
                .unwrap();
            assert!(matches!(operation, CompactionAdmissionRead::Admissible(_)));
        }
        StartupCase::ProviderStopping => assert_eq!(gate.state(), &InputGateState::Idle),
        StartupCase::Settled => assert_eq!(gate.state(), &InputGateState::Idle),
    }
    if matches!(case, StartupCase::Stopping | StartupCase::ProviderStopping) {
        let stop = storage.stop_admission_read(store, thread, limit()).unwrap();
        assert!(!matches!(stop, StopAdmissionRead::Stopping(_)));
    }
}

#[test]
fn candidate_startup_restarts_once_for_source_drift_and_rejects_a_second_drift() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (directory, thread, turn) = source_case(StartupCase::Active, 110);
    let (mut publication, storage) = open_candidate(directory.path(), FaultController::new());
    let access = publication.recovery_access().unwrap();
    let source = storage
        .delivery_recovery_startup_page_candidate(
            &access,
            None,
            CursorReadLimits::new(1, 65_536).unwrap(),
        )
        .unwrap()
        .records()[0]
        .clone();
    let DeliveryRecoveryCase::Active(active) = storage
        .classify_delivery_recovery_candidate(&access, &source, limit())
        .unwrap()
    else {
        panic!("active startup fixture must classify as active")
    };
    let abandonment = active
        .generic_abandonment(
            "candidate startup source drift",
            SyndicTimestamp::from_unix_millis(2_000),
        )
        .unwrap();
    let first = install_startup_classification_barrier(thread, 1);
    let result = std::thread::scope(|scope| {
        let worker = scope.spawn(|| recover_startup_candidate(&access, &storage));
        first.wait();
        abandon_active_candidate(&access, &storage, &abandonment).unwrap();
        let second = install_startup_classification_barrier(thread, 2);
        first.release();
        second.wait();
        publish_source_less_terminal_candidate(
            &access,
            &storage,
            thread,
            turn,
            SyndicTimestamp::from_unix_millis(3_000),
            limit(),
        )
        .unwrap();
        second.release();
        worker.join().unwrap()
    });
    assert!(matches!(
        result,
        Err(ProjectionCoordinatorError::AcceptedDeliveryRecoveryRead)
    ));
    drop(access);
    publication.close().unwrap();
}

#[test]
fn candidate_startup_rebases_a_multi_page_pending_scan_after_one_active_gate_mutation() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::new(120);
    let submitted = fixture.submit_text("candidate startup rebase active");
    fixture.activate_without_terminal(submitted);
    fixture.set_submission_identity_counter(300);
    for ordinal in 0_u16..257 {
        let mut thread_bytes = [255; 16];
        thread_bytes[14..].copy_from_slice(&ordinal.to_le_bytes());
        let mut draft_bytes = [254; 16];
        draft_bytes[14..].copy_from_slice(&ordinal.to_le_bytes());
        let thread = fixture.create_ordinary_with_ids(
            SyndicThreadId::from_bytes(thread_bytes),
            SyndicDraftId::from_bytes(draft_bytes),
        );
        fixture.submit_text_on(thread, "candidate startup rebase pending");
    }
    let thread = fixture.thread;
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();

    let (mut publication, storage) = open_candidate(directory.path(), FaultController::new());
    let access = publication.recovery_access().unwrap();
    let snapshot = recover_startup_candidate(&access, &storage).unwrap();
    assert_eq!(snapshot.page_reads, 2);
    assert_eq!(snapshot.cases, 258);
    assert_eq!(snapshot.pending_turns, 257);
    assert_eq!(snapshot.active_convergences, 1);
    assert_eq!(snapshot.terminal_convergences, 1);
    drop(access);
    let store = publication.publish().unwrap();
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

#[test]
fn candidate_startup_converges_every_delivery_case_before_publication_and_after_recovery() {
    let _guard = TEST_LOCK.lock().unwrap();
    for (index, case) in [
        StartupCase::Pending,
        StartupCase::Active,
        StartupCase::Stopping,
        StartupCase::PostAbandonment,
        StartupCase::Finalizing,
        StartupCase::DeferredCompaction,
        StartupCase::ProviderStopping,
        StartupCase::Settled,
    ]
    .into_iter()
    .enumerate()
    {
        for recovered in [false, true] {
            let (directory, thread, turn) = source_case(case, 30 + index as u8);
            let faults = FaultController::new();
            let (mut publication, storage) = open_candidate(directory.path(), faults.clone());
            if recovered {
                let store = publication.publish().unwrap();
                faults.fail_next(FaultPoint::BeforeReadConfirmation);
                assert!(store.home_revision().is_err());
                let mut recovered = store.recover_same_home().unwrap();
                let fresh = SyndicStorage::reacquire_candidate(&recovered).unwrap();
                let access = recovered.recovery_access().unwrap();
                let snapshot = recover_startup_candidate(&access, &fresh).unwrap();
                assert_snapshot(case, snapshot);
                drop(access);
                let store = recovered.publish().unwrap();
                assert_case_converged(case, &fresh, &store, thread, turn);
                store.close().unwrap();
            } else {
                let access = publication.recovery_access().unwrap();
                let snapshot = recover_startup_candidate(&access, &storage).unwrap();
                assert_snapshot(case, snapshot);
                drop(access);
                beryl_home_store::test_faults::with_initial_publication_store(
                    &publication,
                    |store| {
                        assert!(storage.input_gate(store, thread, limit()).is_err());
                    },
                );
                let store = publication.publish().unwrap();
                assert_case_converged(case, &storage, &store, thread, turn);
                store.close().unwrap();
            }
        }
    }
}

#[test]
fn candidate_startup_preserves_command_failure_custody_and_candidate_identity() {
    let _guard = TEST_LOCK.lock().unwrap();
    for fault in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
        FaultPoint::AfterPersist,
    ] {
        let (directory, thread, turn) = source_case(StartupCase::Active, 90);
        let faults = FaultController::new();
        let (mut publication, storage) = open_candidate(directory.path(), faults.clone());
        let access = publication.recovery_access().unwrap();
        faults.fail_next(fault);
        assert!(matches!(
            recover_startup_candidate(&access, &storage),
            Err(ProjectionCoordinatorError::AcceptedDeliveryRecoveryPublication)
        ));
        if fault != FaultPoint::AfterCommitBeforePersist {
            assert!(access.pending_reconciliations().is_empty());
        }
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
            publication.close().unwrap();
        } else {
            drop(access);
            publication.close().unwrap();
        }

        let (mut retry, fresh) = open_candidate(directory.path(), FaultController::new());
        let access = retry.recovery_access().unwrap();
        recover_startup_candidate(&access, &fresh).unwrap();
        drop(access);
        let store = retry.publish().unwrap();
        assert_case_converged(StartupCase::Active, &fresh, &store, thread, turn);
        assert!(matches!(
            fresh
                .current_binding(&store, thread, limit())
                .unwrap()
                .unwrap()
                .binding()
                .state(),
            BindingState::Stale(_)
        ));
        store.close().unwrap();
    }
}

#[test]
fn candidate_startup_rejects_foreign_and_stale_storage_before_scan() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (directory, _, _) = source_case(StartupCase::Pending, 115);
    let (foreign_directory, _, _) = source_case(StartupCase::Pending, 116);
    let faults = FaultController::new();
    let (mut publication, storage) = open_candidate(directory.path(), faults.clone());
    let (foreign_publication, foreign_storage) =
        open_candidate(foreign_directory.path(), FaultController::new());
    let access = publication.recovery_access().unwrap();
    assert!(matches!(
        recover_startup_candidate(&access, &foreign_storage),
        Err(ProjectionCoordinatorError::AcceptedDeliveryRecoveryRead)
    ));
    drop(access);
    foreign_publication.close().unwrap();
    let store = publication.publish().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovered = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovered).unwrap();
    let access = recovered.recovery_access().unwrap();
    assert!(matches!(
        recover_startup_candidate(&access, &storage),
        Err(ProjectionCoordinatorError::AcceptedDeliveryRecoveryRead)
    ));
    recover_startup_candidate(&access, &fresh).unwrap();
    drop(access);
    recovered.publish().unwrap().close().unwrap();
}
