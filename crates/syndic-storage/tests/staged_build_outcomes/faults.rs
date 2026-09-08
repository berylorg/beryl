use super::outcomes::*;
use super::*;
use syndic_storage::{
    StagedDraftPieceDurableClassificationV1 as Durable, StagedDraftPieceOutcomeStateV1 as State,
};

#[test]
fn transfer_faults_preserve_exact_commit_classification_and_retry_custody() {
    for (index, fault_point) in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
        FaultPoint::AfterPersist,
    ]
    .into_iter()
    .enumerate()
    {
        let faults = FaultController::new();
        let (_home, store, storage, thread) =
            fixture_with_faults("staged-transfer-fault", 31 + index as u8, faults.clone());
        let initial = current(&storage, &store, thread);
        let session = open_session(&storage, &store, &initial, 41, 42);
        let identity = finish_plain_staging(
            &storage,
            &store,
            &session,
            43,
            false,
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("a".into())],
            )],
            DraftLogicalExtentV1::new(1, 1),
        );
        let head = storage
            .draft_mutation_staging_head(&store, identity)
            .unwrap()
            .unwrap();
        let prepared = storage
            .prepare_staged_draft_piece_transfer(&store, identity, head.receipt())
            .unwrap();
        faults.fail_next(fault_point);
        let flight = prepared.submit(&store);
        match fault_point {
            FaultPoint::BeforeCommit => {
                assert_eq!(flight.classification(), Durable::NotCommitted);
                assert_eq!(flight.state(), State::NotCommitted);
                assert!(flight.original_failure().is_some());
                assert!(flight.receipt().is_none());
                assert_eq!(flight.verification_work().attempted_reads, 0);
            }
            FaultPoint::AfterCommitBeforePersist => {
                assert_eq!(flight.classification(), Durable::Unresolved);
                assert_eq!(flight.state(), State::Reconciling);
                assert!(flight.has_reconciliation_custody());
                let flight = flight.resume(&store);
                assert_eq!(flight.state(), State::Complete, "{flight:?}");
                assert_eq!(flight.classification(), Durable::Committed);
                assert!(flight.original_failure().is_some());
                assert!(flight.receipt().is_some());
                assert!(flight.verification_work().attempted_reads > 0);
                assert!(
                    flight.verification_work().attempted_reads
                        <= syndic_storage::STAGED_DRAFT_PIECE_OUTCOME_MAX_READS
                );
            }
            FaultPoint::AfterPersist => {
                assert_eq!(flight.classification(), Durable::Committed);
                assert!(flight.receipt().is_some());
                assert!(flight.later_failure().is_some());
                assert_eq!(flight.verification_work().attempted_reads, 0);
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn transfer_exact_old_requires_explicit_resubmission_and_preserves_first_failure() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("staged-transfer-old", 51, faults.clone());
    let initial = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &initial, 52, 53);
    let identity = finish_plain_staging(
        &storage,
        &store,
        &session,
        54,
        false,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("a".into())],
        )],
        DraftLogicalExtentV1::new(1, 1),
    );
    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    let prepared = storage
        .prepare_staged_draft_piece_transfer(&store, identity, head.receipt())
        .unwrap();
    let revision = store.home_revision().unwrap();
    let fault = beryl_home_store::test_faults::fail_next_journal_write();
    let flight = prepared.submit(&store);
    drop(fault);
    assert_eq!(flight.state(), State::Reconciling, "{flight:?}");
    assert!(matches!(
        flight.original_failure(),
        Some(CommandError::Commit { .. })
    ));
    let flight = flight.resume(&store);
    assert_eq!(flight.state(), State::Reconciling, "{flight:?}");
    assert!(flight.has_reconciliation_custody());
    assert!(flight.failure().is_some());
    let flight = flight.resume(&store);
    assert_eq!(flight.state(), State::Reconciling, "{flight:?}");
    assert!(flight.has_reconciliation_custody());
    let recovery = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish();
    let flight = flight.resume(&store);
    assert_eq!(flight.classification(), Durable::NotCommitted);
    assert!(matches!(
        flight.original_failure(),
        Some(CommandError::Commit { .. })
    ));
    assert!(flight.verification_work().attempted_reads > 0);
    assert_eq!(store.home_revision().unwrap(), revision);
    assert_eq!(
        storage
            .draft_mutation_staging_head(&store, identity)
            .unwrap()
            .unwrap(),
        head
    );
    let prepared = storage
        .prepare_staged_draft_piece_transfer(&store, identity, head.receipt())
        .unwrap();
    let completion = complete(prepared.submit(&store), &store);
    assert!(matches!(
        completion.result,
        DraftPieceReconciledCommandV1::Pending(_)
    ));
    assert_eq!(store.home_revision().unwrap().get(), revision.get() + 1);
}

#[test]
fn ambiguous_window_advance_and_settlement_verify_only_the_selected_endpoint() {
    for command in 0..3 {
        let faults = FaultController::new();
        let (_home, store, storage, thread) =
            fixture_with_faults("staged-command-new", 61 + command, faults.clone());
        let initial = current(&storage, &store, thread);
        let session = open_session(&storage, &store, &initial, 65, 66);
        let identity = finish_plain_staging(
            &storage,
            &store,
            &session,
            67,
            false,
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("a".into())],
            )],
            DraftLogicalExtentV1::new(1, 1),
        );
        let mut endpoint = transfer(&storage, &store, identity);
        let limits = DraftPieceDurableBuildWindowLimitsV1::new(1, 1, 65_536).unwrap();
        let prepared = match command {
            0 => storage
                .prepare_staged_draft_piece_window(&store, identity, endpoint, limits)
                .unwrap()
                .unwrap(),
            1 => {
                stage_windows(&storage, &store, identity, limits);
                endpoint = building_endpoint(&storage, &store, identity);
                storage
                    .prepare_staged_draft_piece_advance(&store, identity, endpoint)
                    .unwrap()
                    .unwrap()
            }
            _ => {
                stage_windows(&storage, &store, identity, limits);
                endpoint = advance_to_terminal(&storage, &store, identity);
                storage
                    .prepare_staged_draft_piece_terminal(
                        &store,
                        identity,
                        endpoint,
                        syndic_storage::StagedDraftPieceTerminalElectionV1::Settle,
                    )
                    .unwrap()
            }
        };
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let flight = prepared.submit(&store);
        assert_eq!(
            flight.state(),
            State::Reconciling,
            "command {command}: {flight:?}"
        );
        let flight = flight.resume(&store);
        assert_eq!(
            flight.state(),
            State::Complete,
            "command {command}: {flight:?}"
        );
        assert_eq!(flight.classification(), Durable::Committed);
        assert!(flight.original_failure().is_some());
        let work = flight.verification_work();
        assert!(
            work.attempted_reads > 0
                && work.attempted_reads <= syndic_storage::STAGED_DRAFT_PIECE_OUTCOME_MAX_READS
        );
        assert_eq!(
            work.charged_encoded_value_bytes,
            work.attempted_reads * 65_536
        );
    }
}

#[test]
fn prepared_settlement_replay_captures_the_already_committed_historical_result() {
    let (_home, store, storage, thread) = fixture("staged-settlement-replay", 71);
    let initial = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &initial, 72, 73);
    let identity = finish_plain_staging(
        &storage,
        &store,
        &session,
        74,
        false,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("a".into())],
        )],
        DraftLogicalExtentV1::new(1, 1),
    );
    transfer(&storage, &store, identity);
    stage_windows(
        &storage,
        &store,
        identity,
        DraftPieceDurableBuildWindowLimitsV1::new(1, 1, 65_536).unwrap(),
    );
    let endpoint = advance_to_terminal(&storage, &store, identity);
    let election = syndic_storage::StagedDraftPieceTerminalElectionV1::Settle;
    let first = storage
        .prepare_staged_draft_piece_terminal(&store, identity, endpoint, election)
        .unwrap();
    let replay = storage
        .prepare_staged_draft_piece_terminal(&store, identity, endpoint, election)
        .unwrap();
    let first = complete(first.submit(&store), &store);
    let replay = complete(replay.submit(&store), &store);
    assert_eq!(first.result, replay.result);
    assert_eq!(first.verification.attempted_reads, 0);
    assert_eq!(replay.verification.attempted_reads, 0);
}
