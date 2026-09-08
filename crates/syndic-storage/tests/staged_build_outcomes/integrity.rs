use super::{admitted::*, outcomes::*, *};
use syndic_storage::test_faults::{
    capture_draft_marker_source_endpoint_for_test, draft_marker_program_snapshot_for_test,
    restore_draft_marker_source_endpoint_for_test, staged_outcome_build_for_test,
};
use syndic_storage::{
    StagedDraftPieceDurableClassificationV1 as Durable, StagedDraftPieceOutcomeStateV1 as State,
    StagedDraftPieceTerminalElectionV1 as Election,
};

fn advance_to_pending(
    fixture: &AcceptedFixture,
    identity: DraftMutationStagingIdentityV1,
    pending: u8,
) -> syndic_storage::DraftPieceBuildRecordV1 {
    for _ in 0..256 {
        let build = staged_outcome_build_for_test(&fixture.storage, &fixture.store, identity);
        if draft_marker_program_snapshot_for_test(&build)
            .is_some_and(|program| program.pending == pending)
        {
            return build;
        }
        let prepared = fixture
            .storage
            .prepare_staged_draft_piece_advance(&fixture.store, identity, build.progress_receipt())
            .unwrap()
            .unwrap();
        complete(prepared.submit(&fixture.store), &fixture.store);
    }
    panic!("marker program did not reach pending state {pending}");
}

#[test]
fn byte_equal_occupied_surgery_effects_refuse_a_restored_source_without_advancing_custody() {
    let (fixture, _, identity) =
        fresh_staging("outcome-split-publication", 140, FaultController::new());
    let source = advance_to_pending(&fixture, identity, 5);
    let saved =
        capture_draft_marker_source_endpoint_for_test(&fixture.store, &fixture.storage, &source);
    let command = fixture
        .storage
        .prepare_staged_draft_piece_advance(&fixture.store, identity, source.progress_receipt())
        .unwrap()
        .unwrap();
    complete(command.submit(&fixture.store), &fixture.store);
    let target = staged_outcome_build_for_test(&fixture.storage, &fixture.store, identity);
    restore_draft_marker_source_endpoint_for_test(
        &fixture.store,
        &fixture.storage,
        &saved,
        target.progress_receipt(),
    );
    let revision = fixture.store.home_revision().unwrap();
    match fixture.storage.prepare_staged_draft_piece_advance(
        &fixture.store,
        identity,
        source.progress_receipt(),
    ) {
        Ok(Some(command)) => {
            let flight = command.submit(&fixture.store);
            assert_eq!(flight.classification(), Durable::NotCommitted, "{flight:?}");
            assert!(flight.original_failure().is_some());
            assert!(flight.result().is_none());
        }
        Err(_) => {}
        Ok(None) => panic!("split surgery was incorrectly complete"),
    }
    assert_eq!(fixture.store.home_revision().unwrap(), revision);
    assert_eq!(
        staged_outcome_build_for_test(&fixture.storage, &fixture.store, identity),
        source
    );
    let session = active_session(
        &fixture.storage,
        &fixture.store,
        fixture.session.draft_id(),
        fixture.session.session_id(),
    );
    assert!(session.active_operation().is_some());
    assert_eq!(session.newest_root(), fixture.session.newest_root());
    assert_eq!(session.newest_history(), fixture.session.newest_history());
}

fn history_limited_fixture(
    faults: FaultController,
) -> (TestHome, HomeStore, SyndicStorage, SyndicThreadId) {
    let home = TestHome::new("outcome-dynamic-settlement");
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([151; 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes([152; 16]),
                ExecutionBinding::new(
                    RuntimeId::from_bytes([171; 16]),
                    RootId::from_bytes([172; 16]),
                    RuntimeNativePath::from_admitted(
                        RuntimeMode::host(),
                        PathFlavor::Windows,
                        "C:\\syndic-outcome-history",
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(1_520, 1).unwrap(),
            ),
        ),
    ));
    (home, store, storage, thread)
}

#[test]
fn ambiguous_identity_surgery_rejects_its_unchanged_pending_sequence_reference() {
    let faults = FaultController::new();
    let (fixture, _, identity) = fresh_staging("outcome-pending-reference", 160, faults.clone());
    let source = advance_to_pending(&fixture, identity, 6);
    let command = fixture
        .storage
        .prepare_staged_draft_piece_advance(&fixture.store, identity, source.progress_receipt())
        .unwrap()
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let flight = command.submit(&fixture.store);
    assert_eq!(flight.state(), State::Reconciling);
    syndic_storage::test_faults::corrupt_staged_outcome_pending_sequence_for_test(
        &fixture.storage,
        &fixture.store,
        &source,
    );
    let flight = flight.resume(&fixture.store);
    assert_eq!(
        flight.classification(),
        Durable::Committed,
        "changed effects should reconcile before referenced closure fails: {flight:?}"
    );
    assert!(
        matches!(flight.state(), State::Verifying | State::Unavailable),
        "{flight:?}"
    );
    assert!(flight.failure().is_some());
    assert!(flight.original_failure().is_some());
    assert!(flight.verification_work().attempted_reads > 0);
    assert_verification_budget(flight.verification_work());
    let Err(retained) = flight.into_completion() else {
        panic!("a corrupted referenced closure released a completion");
    };
    assert!(matches!(
        retained.state(),
        State::Verifying | State::Unavailable
    ));
    assert!(retained.failure().is_some());
}

#[test]
fn settlement_captures_dynamic_history_refusal_and_exact_inert_writer_cleanup() {
    for ambiguous in [false, true] {
        let faults = FaultController::new();
        let (_home, store, storage, thread) = history_limited_fixture(faults.clone());
        let session = open_session(
            &storage,
            &store,
            &current(&storage, &store, thread),
            153,
            154,
        );
        let session = complete_staged(
            &storage,
            &store,
            &session,
            155,
            DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".into())]),
            DraftLogicalExtentV1::new(1, 1),
        );
        let target = marker(156, 1, 7);
        let admission = owner(&session, 157);
        let proof = storage
            .seed_draft_marker_writer_ready_target_for_test(&store, &session, admission, target)
            .unwrap();
        let replacement =
            DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Marker(target)])
                .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                    DraftPieceMarkerInsertionV1::new(
                        0,
                        target,
                        DraftPieceMarkerEffectChargesV1::for_marker(target),
                    ),
                ));
        let identity = finish_admitted_staging(&storage, &store, &session, proof, &[replacement]);
        transfer(&storage, &store, identity);
        stage_windows(
            &storage,
            &store,
            identity,
            DraftPieceDurableBuildWindowLimitsV1::new(1, 1, 65_536).unwrap(),
        );
        let endpoint = advance_to_terminal(&storage, &store, identity);
        let command = storage
            .prepare_staged_draft_piece_terminal(&store, identity, endpoint, Election::Settle)
            .unwrap();
        if ambiguous {
            faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        }
        let completion = complete(command.submit(&store), &store);
        let DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Error(error)) =
            completion.result
        else {
            panic!(
                "Settle did not capture the dynamically elected error: {:?}",
                completion.result
            );
        };
        let syndic_storage::DraftPieceSettlementProofV1::Settlement(settlement) = error else {
            panic!("dynamic settlement returned occupied-identity authority");
        };
        assert_eq!(
            settlement.outcome(),
            &syndic_storage::DraftPieceSettlementOutcomeV1::Error(
                DraftPieceErrorReasonV1::HistoryCapacityUnavailable
            )
        );
        assert_eq!(
            completion.inert_cleanup.as_ref().unwrap().owner(),
            admission
        );
        assert!(completion.cleanup_receipt.is_none());
        assert_eq!(completion.original_failure.is_some(), ambiguous);
        assert_verification_budget(completion.verification);
        assert_eq!(completion.verification.attempted_reads > 0, ambiguous);
        let preserved = active_session(&storage, &store, session.draft_id(), session.session_id());
        assert_eq!(preserved.newest_root(), session.newest_root());
        assert_eq!(preserved.newest_history(), session.newest_history());
        assert_eq!(
            preserved.newest_candidate_generation(),
            session.newest_candidate_generation()
        );
        assert!(preserved.active_operation().is_none());
    }
}
