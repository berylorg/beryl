use beryl_app::cas_projection::{
    test_faults::{
        converge_compaction_restart, converge_compaction_restart_candidate,
        install_compaction_recovery_confirmation_barrier,
    },
    ProjectionCoordinatorError,
};
use beryl_home_store::{
    test_faults::{FaultController, FaultPoint},
    CommandOutcome, HomeCandidateError, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    ReconciliationResolution,
};
use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration, CasTurnId,
    SyndicItemId,
};
use syndic_storage::{
    ClaimCompactionDispatch, CompactionAbandonmentReason, CompactionAdmissionRead,
    CompactionAttemptNonce, CompactionOperationId, CompactionOperationNonce,
    CompactionOperationState, CompactionProviderEvent, CompactionProviderSequence,
    CompactionRequestDisposition, CompactionSettlement, PublishCompactionProviderEvent,
    PublishCompactionRequestDisposition, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
    TurnEndStatus, TurnTerminalOutcome,
};

#[derive(Clone, Copy)]
enum RestartCase {
    CancelBeforeDispatch,
    LocalNondispatch,
    Rejected,
    PossibleDispatch,
    Success,
    Interrupted,
    Failure,
}

struct DeferredCompaction {
    directory: tempfile::TempDir,
    service: beryl_app::cas_projection::ProjectionConnectionService,
    storage: SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    turn: beryl_model::SyndicTurnId,
    operation: CompactionOperationId,
    provider_frontier: Option<CompactionProviderSequence>,
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn committed(outcome: CommandOutcome) {
    match outcome {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        other => panic!("fixture command must commit cleanly: {other:?}"),
    }
}

fn deferred(case: RestartCase, seed: u8) -> DeferredCompaction {
    let mut source = crate::syndic::Fixture::new(seed);
    let submitted = source.submit_text("candidate deferred compaction");
    source.complete_with_assistant(submitted, "completed before restart");
    let thread = source.thread;
    let CompactionAdmissionRead::Admissible(admission) = source
        .storage
        .compaction_admission_read(&source.home(), thread, point_limit())
        .unwrap()
    else {
        panic!("completed fixture must admit compaction")
    };
    let attempt = CompactionAttemptNonce::from_bytes([seed.wrapping_add(1); 16]);
    let operation = admission.admission(
        CompactionOperationNonce::from_bytes([seed; 16]),
        attempt,
        CasLoadedSessionGeneration::new(
            CasProcessGeneration::new(u64::from(seed) + 1).unwrap(),
            CasLoadedThreadGeneration::new(u64::from(seed) + 2).unwrap(),
        ),
        SyndicTimestamp::from_unix_millis(70_000),
    );
    let operation_id = operation.operation_id();
    committed(
        source
            .home()
            .execute_current(source.storage.current_admit_compaction_operation(operation)),
    );
    if !matches!(case, RestartCase::CancelBeforeDispatch) {
        let record = source
            .storage
            .compaction_operation(&source.home(), operation_id, point_limit())
            .unwrap()
            .unwrap();
        committed(
            source
                .home()
                .execute_current(source.storage.current_claim_compaction_dispatch(
                    ClaimCompactionDispatch::new(operation_id, record.revision(), attempt),
                )),
        );
    }
    match case {
        RestartCase::CancelBeforeDispatch | RestartCase::PossibleDispatch => {}
        RestartCase::LocalNondispatch | RestartCase::Rejected => publish_request(
            &source,
            operation_id,
            if matches!(case, RestartCase::LocalNondispatch) {
                CompactionRequestDisposition::ProvenLocalNondispatch
            } else {
                CompactionRequestDisposition::RejectedBeforeCore
            },
        ),
        RestartCase::Success | RestartCase::Interrupted | RestartCase::Failure => {
            publish_event(
                &source,
                operation_id,
                CompactionProviderEvent::ThreadStatus(
                    syndic_storage::CompactionThreadStatus::Active,
                ),
                70_001,
            );
            publish_event(
                &source,
                operation_id,
                CompactionProviderEvent::TurnStarted(
                    CasTurnId::new(format!("restart-{seed}")).unwrap(),
                ),
                70_002,
            );
            if matches!(case, RestartCase::Success) {
                let item = SyndicItemId::from_bytes([seed; 16]);
                publish_event(
                    &source,
                    operation_id,
                    CompactionProviderEvent::Marker {
                        item_id: item,
                        lifecycle: syndic_storage::CompactionMarkerLifecycle::Started,
                    },
                    70_003,
                );
                publish_event(
                    &source,
                    operation_id,
                    CompactionProviderEvent::Marker {
                        item_id: item,
                        lifecycle: syndic_storage::CompactionMarkerLifecycle::Completed,
                    },
                    70_004,
                );
            }
            publish_event(
                &source,
                operation_id,
                CompactionProviderEvent::ThreadStatus(syndic_storage::CompactionThreadStatus::Idle),
                70_005,
            );
            let outcome = match case {
                RestartCase::Success => TurnTerminalOutcome::Complete,
                RestartCase::Interrupted => TurnTerminalOutcome::Interrupted,
                RestartCase::Failure => TurnTerminalOutcome::Failed,
                _ => unreachable!(),
            };
            publish_event(
                &source,
                operation_id,
                CompactionProviderEvent::Terminal(TurnEndStatus::new(outcome, None).unwrap()),
                70_006,
            );
        }
    }
    let provider_frontier = source
        .storage
        .compaction_operation(&source.home(), operation_id, point_limit())
        .unwrap()
        .unwrap()
        .provider_frontier();
    let storage = source.storage.clone();
    let (directory, service) = source.into_service();
    DeferredCompaction {
        directory,
        service,
        storage,
        thread,
        turn: operation_id.provider_turn_id(),
        operation: operation_id,
        provider_frontier,
    }
}

fn publish_request(
    source: &crate::syndic::Fixture,
    operation: CompactionOperationId,
    disposition: CompactionRequestDisposition,
) {
    let record = source
        .storage
        .compaction_operation(&source.home(), operation, point_limit())
        .unwrap()
        .unwrap();
    committed(
        source.home().execute_current(
            source
                .storage
                .current_publish_compaction_request_disposition(
                    PublishCompactionRequestDisposition::new(
                        operation,
                        record.revision(),
                        record.attempt(),
                        disposition,
                    ),
                ),
        ),
    );
}

fn publish_event(
    source: &crate::syndic::Fixture,
    operation: CompactionOperationId,
    event: CompactionProviderEvent,
    at: u64,
) {
    let record = source
        .storage
        .compaction_operation(&source.home(), operation, point_limit())
        .unwrap()
        .unwrap();
    let sequence = record
        .provider_frontier()
        .map_or(CompactionProviderSequence::FIRST, |frontier| {
            frontier.checked_next().unwrap()
        });
    committed(source.home().execute_current(
        source.storage.current_publish_compaction_provider_event(
            PublishCompactionProviderEvent::new(
                operation,
                record.revision(),
                sequence,
                event,
                SyndicTimestamp::from_unix_millis(at),
            ),
        ),
    ));
}

fn expected(case: RestartCase) -> Result<CompactionSettlement, CompactionAbandonmentReason> {
    match case {
        RestartCase::CancelBeforeDispatch => Ok(CompactionSettlement::CancelledBeforeDispatch),
        RestartCase::LocalNondispatch => Ok(CompactionSettlement::LocalNondispatch),
        RestartCase::Rejected => Err(CompactionAbandonmentReason::ProviderRejectedBeforeCore),
        RestartCase::PossibleDispatch => {
            Err(CompactionAbandonmentReason::StartupProcessGenerationLost)
        }
        RestartCase::Success => Ok(CompactionSettlement::ManualSuccess),
        RestartCase::Interrupted | RestartCase::Failure => Ok(CompactionSettlement::ManualFailure),
    }
}

#[test]
fn candidate_deferred_compaction_matches_ordinary_restart_settlement_without_provider_replay() {
    for (offset, case) in [
        RestartCase::CancelBeforeDispatch,
        RestartCase::LocalNondispatch,
        RestartCase::Rejected,
        RestartCase::PossibleDispatch,
        RestartCase::Success,
        RestartCase::Interrupted,
        RestartCase::Failure,
    ]
    .into_iter()
    .enumerate()
    {
        let ordinary = deferred(case, 120 + offset as u8);
        let expected = expected(case);
        converge_compaction_restart(
            ordinary.service.home_for_shutdown_test(),
            &ordinary.storage,
            ordinary.thread,
            ordinary.turn,
        )
        .unwrap();
        let ordinary_after = ordinary
            .storage
            .compaction_operation(
                ordinary.service.home_for_shutdown_test(),
                ordinary.operation,
                point_limit(),
            )
            .unwrap()
            .unwrap();
        assert_settlement(&ordinary_after, expected.clone());
        ordinary.service.close().unwrap();

        let candidate = deferred(case, 160 + offset as u8);
        candidate.service.close().unwrap();
        let mut opening = HomeOpenCandidate::open(HomeOpenOptions::new(
            candidate.directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let storage = SyndicStorage::register(&mut opening).unwrap();
        let mut publication = opening
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap();
        let access = publication.recovery_access().unwrap();
        assert!(
            candidate
                .storage
                .compaction_recovery_read_candidate(&access, candidate.operation, point_limit())
                .is_err(),
            "stale storage handle must fail"
        );
        converge_compaction_restart_candidate(&access, &storage, candidate.thread, candidate.turn)
            .unwrap();
        let recovered = storage
            .compaction_recovery_read_candidate(&access, candidate.operation, point_limit())
            .unwrap()
            .unwrap();
        assert_settlement(recovered.record(), expected);
        assert_eq!(
            recovered.record().provider_frontier(),
            candidate.provider_frontier
        );
        publication.close().unwrap();
    }
}

fn assert_settlement(
    record: &syndic_storage::CompactionOperationRecord,
    expected: Result<CompactionSettlement, CompactionAbandonmentReason>,
) {
    let settlement = match expected {
        Ok(settlement) => settlement,
        Err(reason) => CompactionSettlement::Abandoned(reason),
    };
    assert!(
        matches!(record.state(), CompactionOperationState::Consumed(witness) if witness.settlement() == &settlement)
    );
}

fn assert_reopened_settlement(
    path: &std::path::Path,
    operation: CompactionOperationId,
    expected: CompactionSettlement,
) {
    let mut opening =
        HomeOpenCandidate::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut opening).unwrap();
    let mut publication = opening
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap();
    let access = publication.recovery_access().unwrap();
    let record = storage
        .compaction_recovery_read_candidate(&access, operation, point_limit())
        .unwrap()
        .unwrap();
    assert_settlement(record.record(), Ok(expected));
    drop(access);
    publication.close().unwrap();
}

#[test]
fn candidate_restart_keeps_command_failure_and_reconciliation_custody() {
    let fixture = deferred(RestartCase::CancelBeforeDispatch, 210);
    let _ = fixture.service.close().unwrap();
    let faults = FaultController::new();
    let mut opening = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(fixture.directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut opening).unwrap();
    let mut publication = opening
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap();
    let access = publication.recovery_access().unwrap();
    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(matches!(
        converge_compaction_restart_candidate(&access, &storage, fixture.thread, fixture.turn),
        Err(ProjectionCoordinatorError::CommandNotCommitted(_))
    ));
    publication.close().unwrap();

    let fixture = deferred(RestartCase::CancelBeforeDispatch, 211);
    let _ = fixture.service.close().unwrap();
    let faults = FaultController::new();
    let mut opening = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(fixture.directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut opening).unwrap();
    let mut publication = opening
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap();
    let access = publication.recovery_access().unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        converge_compaction_restart_candidate(&access, &storage, fixture.thread, fixture.turn),
        Err(ProjectionCoordinatorError::CommandIndeterminate { .. })
    ));
    assert_eq!(access.pending_reconciliations().len(), 1);
    drop(access);
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
    drop(access);
    publication.close().unwrap();
}

#[test]
fn candidate_restart_reports_committed_and_confirmation_read_failures() {
    let fixture = deferred(RestartCase::CancelBeforeDispatch, 212);
    let _ = fixture.service.close().unwrap();
    let faults = FaultController::new();
    let mut opening = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(fixture.directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut opening).unwrap();
    let mut publication = opening
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap();
    let access = publication.recovery_access().unwrap();
    faults.fail_next(FaultPoint::AfterPersist);
    assert!(matches!(
        converge_compaction_restart_candidate(&access, &storage, fixture.thread, fixture.turn),
        Err(ProjectionCoordinatorError::CommandCommitted { .. })
    ));
    drop(access);
    publication.close().unwrap();
    assert_reopened_settlement(
        fixture.directory.path(),
        fixture.operation,
        CompactionSettlement::CancelledBeforeDispatch,
    );

    let fixture = deferred(RestartCase::CancelBeforeDispatch, 213);
    let _ = fixture.service.close().unwrap();
    let faults = FaultController::new();
    let mut opening = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(fixture.directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut opening).unwrap();
    let mut publication = opening
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap();
    let access = publication.recovery_access().unwrap();
    let barrier = install_compaction_recovery_confirmation_barrier(fixture.thread);
    let result = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            converge_compaction_restart_candidate(&access, &storage, fixture.thread, fixture.turn)
        });
        barrier.wait();
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        barrier.release();
        worker.join().unwrap()
    });
    assert!(matches!(
        result,
        Err(ProjectionCoordinatorError::AcceptedDeliveryRecoveryRead)
    ));
    drop(access);
    publication.close().unwrap();
    assert_reopened_settlement(
        fixture.directory.path(),
        fixture.operation,
        CompactionSettlement::CancelledBeforeDispatch,
    );
}

#[test]
fn recovered_candidate_rejects_wrong_and_foreign_convergence_before_exact_settlement() {
    let fixture = deferred(RestartCase::CancelBeforeDispatch, 220);
    let foreign = deferred(RestartCase::CancelBeforeDispatch, 221);
    let _ = fixture.service.close().unwrap();
    let faults = FaultController::new();
    let mut opening = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(fixture.directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let stale = SyndicStorage::register(&mut opening).unwrap();
    let publication = opening
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap();
    let store = publication.publish().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovered = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovered).unwrap();
    {
        let access = recovered.recovery_access().unwrap();
        assert!(matches!(
            converge_compaction_restart_candidate(
                &access,
                &fresh,
                beryl_model::SyndicThreadId::from_bytes([222; 16]),
                fixture.turn,
            ),
            Err(ProjectionCoordinatorError::AcceptedDeliveryRecoveryInvariant)
        ));
        assert!(matches!(
            converge_compaction_restart_candidate(
                &access,
                &fresh,
                fixture.thread,
                beryl_model::SyndicTurnId::from_bytes([223; 16]),
            ),
            Err(ProjectionCoordinatorError::AcceptedDeliveryRecoveryInvariant)
        ));
        for wrong in [&fixture.storage, &stale, &foreign.storage] {
            assert!(matches!(
                converge_compaction_restart_candidate(&access, wrong, fixture.thread, fixture.turn),
                Err(ProjectionCoordinatorError::AcceptedDeliveryRecoveryRead)
            ));
        }
        converge_compaction_restart_candidate(&access, &fresh, fixture.thread, fixture.turn)
            .unwrap();
        assert_settlement(
            fresh
                .compaction_recovery_read_candidate(&access, fixture.operation, point_limit())
                .unwrap()
                .unwrap()
                .record(),
            Ok(CompactionSettlement::CancelledBeforeDispatch),
        );
    }
    let store = recovered.publish().unwrap();
    let record = fresh
        .compaction_operation(&store, fixture.operation, point_limit())
        .unwrap()
        .unwrap();
    assert_settlement(&record, Ok(CompactionSettlement::CancelledBeforeDispatch));
    store.close().unwrap();
    let _ = foreign.service.close().unwrap();
}
