use super::*;

#[test]
fn mapping_creation_acknowledgement_loss_returns_actual_captured_target_once() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("mapping-exact-new", 40, faults.clone());
    let session = open_session(&storage, &store, &current(&storage, &store, thread), 41, 42);
    let identity = stage_text(&storage, &store, &session, 43, 32);
    let source = advance_until(&storage, &store, identity, |_, mapping| {
        mapping.stage_tag == 19
    });
    let command = storage
        .prepare_staged_draft_piece_advance(&store, identity, source.progress_receipt())
        .unwrap()
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let flight = command.submit(&store);
    assert_eq!(flight.classification(), Durable::Unresolved);
    assert_eq!(flight.state(), State::Reconciling);
    assert!(flight.has_reconciliation_custody());
    let completion = complete(flight, &store);
    assert!(completion.original_failure.is_some());
    assert!(completion.verification.attempted_reads > 0);
    assert!(completion.verification.attempted_reads <= 68);
    let DraftPieceReconciledCommandV1::Pending(DraftPieceOperationStatusV1::Open(target)) =
        completion.result
    else {
        panic!("mapping creation did not return its captured open endpoint");
    };
    assert_eq!(draft_build_mapping_snapshot(&target).unwrap().stage_tag, 20);
    assert_eq!(
        target.progress_receipt().key().transition_ordinal(),
        source.progress_receipt().key().transition_ordinal() + 1
    );
    assert_eq!(
        staged_outcome_build_for_test(&storage, &store, identity),
        target
    );
    let key = draft_build_mapping_root_key_for_test(
        &storage,
        &store,
        &target,
        DraftBuildMappingRootForTest::Pending,
    )
    .unwrap();
    assert!(draft_build_mapping_record_for_test(&storage, &store, key).is_some());
    assert_unadopted(&storage, &store, &session);
}

#[test]
fn known_mapping_commit_keeps_its_historical_endpoint_after_later_progress() {
    let (_home, store, storage, thread) = fixture("mapping-captured-history", 50);
    let session = open_session(&storage, &store, &current(&storage, &store, thread), 51, 52);
    let identity = stage_text(&storage, &store, &session, 53, 32);
    let source = advance_until(&storage, &store, identity, |_, mapping| {
        mapping.stage_tag == 19
    });
    let command = storage
        .prepare_staged_draft_piece_advance(&store, identity, source.progress_receipt())
        .unwrap()
        .unwrap();
    let flight = command.submit(&store);
    assert_eq!(flight.classification(), Durable::Committed);
    assert_eq!(flight.verification_work().attempted_reads, 0);
    let historical = staged_outcome_build_for_test(&storage, &store, identity);
    assert_eq!(
        draft_build_mapping_snapshot(&historical).unwrap().stage_tag,
        20
    );
    advance_once(&storage, &store, identity, &historical);
    let latest = staged_outcome_build_for_test(&storage, &store, identity);
    assert_eq!(draft_build_mapping_snapshot(&latest).unwrap().stage_tag, 21);
    let completion = complete(flight, &store);
    assert_eq!(completion.verification.attempted_reads, 0);
    assert_eq!(
        completion.result,
        DraftPieceReconciledCommandV1::Pending(DraftPieceOperationStatusV1::Open(historical))
    );
    assert_ne!(latest.progress_receipt(), source.progress_receipt());
}

#[test]
fn mapping_exact_old_preserves_failure_and_retires_old_generation_custody() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) = fixture_with_faults("mapping-exact-old", 60, faults);
    let session = open_session(&storage, &store, &current(&storage, &store, thread), 61, 62);
    let identity = stage_text(&storage, &store, &session, 63, 32);
    let source = advance_until(&storage, &store, identity, |_, mapping| {
        mapping.stage_tag == 19
    });
    let command = storage
        .prepare_staged_draft_piece_advance(&store, identity, source.progress_receipt())
        .unwrap()
        .unwrap();
    let revision = store.home_revision().unwrap();
    let fault = beryl_home_store::test_faults::fail_next_journal_write();
    let flight = command.submit(&store);
    drop(fault);
    assert_eq!(flight.state(), State::Reconciling);
    assert!(matches!(
        flight.original_failure(),
        Some(CommandError::Commit { .. })
    ));
    let flight = flight.resume(&store);
    assert_eq!(flight.state(), State::Reconciling);
    assert!(flight.has_reconciliation_custody());
    let recovery = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish();
    let flight = flight.resume(&store);
    assert_eq!(flight.classification(), Durable::NotCommitted);
    assert_eq!(
        flight.state(),
        State::Verifying,
        "{flight:?}, verification failure: {:?}",
        flight.failure()
    );
    assert!(matches!(
        flight.failure(),
        Some(syndic_storage::StagedDraftPieceOutcomeErrorV1::Read(_))
    ));
    assert!(!flight.has_reconciliation_custody());
    assert!(matches!(
        flight.original_failure(),
        Some(CommandError::Commit { .. })
    ));
    assert!(flight.verification_work().attempted_reads > 0);
    assert_verification(flight.verification_work());
    assert_eq!(store.home_revision().unwrap(), revision);
    assert_eq!(
        staged_outcome_build_for_test(&storage, &store, identity),
        source
    );
    let Err(flight) = flight.into_noncommit() else {
        panic!("durable classification alone must not release unverified source custody");
    };
    let flight = flight.resume(&store);
    assert_eq!(flight.classification(), Durable::NotCommitted);
    assert_eq!(flight.state(), State::Unavailable);
    assert!(matches!(
        flight.failure(),
        Some(syndic_storage::StagedDraftPieceOutcomeErrorV1::Unavailable)
    ));
    assert!(matches!(
        flight.original_failure(),
        Some(CommandError::Commit { .. })
    ));
    assert!(flight.into_noncommit().is_err());
    assert_eq!(store.home_revision().unwrap(), revision);
    assert_eq!(
        staged_outcome_build_for_test(&storage, &store, identity),
        source
    );
    assert_unadopted(&storage, &store, &session);
}

#[test]
fn occupied_byte_equal_mapping_record_cannot_publish_from_restored_source() {
    use syndic_storage::test_faults::{
        capture_draft_marker_source_endpoint_for_test,
        restore_draft_marker_source_endpoint_for_test,
    };
    let (_home, store, storage, thread) = fixture("mapping-occupied-source", 70);
    let session = open_session(&storage, &store, &current(&storage, &store, thread), 71, 72);
    let identity = stage_text(&storage, &store, &session, 73, 32);
    let source = advance_until(&storage, &store, identity, |_, mapping| {
        mapping.stage_tag == 19
    });
    let saved = capture_draft_marker_source_endpoint_for_test(&store, &storage, &source);
    let completion = advance_once(&storage, &store, identity, &source);
    let DraftPieceReconciledCommandV1::Pending(DraftPieceOperationStatusV1::Open(target)) =
        completion.result
    else {
        panic!("mapping insertion did not return its target");
    };
    let key = draft_build_mapping_root_key_for_test(
        &storage,
        &store,
        &target,
        DraftBuildMappingRootForTest::Pending,
    )
    .unwrap();
    let encoded = draft_build_mapping_record_for_test(&storage, &store, key).unwrap();
    restore_draft_marker_source_endpoint_for_test(
        &store,
        &storage,
        &saved,
        target.progress_receipt(),
    );
    let revision = store.home_revision().unwrap();
    assert!(
        storage
            .prepare_staged_draft_piece_advance(&store, identity, source.progress_receipt())
            .is_err()
    );
    assert_eq!(store.home_revision().unwrap(), revision);
    assert_eq!(
        draft_build_mapping_record_for_test(&storage, &store, key),
        Some(encoded)
    );
    assert_eq!(
        staged_outcome_build_for_test(&storage, &store, identity),
        source
    );
    assert_unadopted(&storage, &store, &session);
}

#[test]
fn ambiguous_sequence_installation_retains_custody_when_unchanged_map_reference_is_corrupt() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("mapping-reference-outcome", 80, faults.clone());
    let session = open_session(&storage, &store, &current(&storage, &store, thread), 81, 82);
    let identity = stage_text(&storage, &store, &session, 83, 32);
    let source = advance_until(&storage, &store, identity, |_, mapping| {
        mapping.stage_tag == 21
    });
    let key = draft_build_mapping_root_key_for_test(
        &storage,
        &store,
        &source,
        DraftBuildMappingRootForTest::Pending,
    )
    .unwrap();
    let command = storage
        .prepare_staged_draft_piece_advance(&store, identity, source.progress_receipt())
        .unwrap()
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let flight = command.submit(&store);
    assert_eq!(flight.state(), State::Reconciling);
    assert_unadopted(&storage, &store, &session);
    syndic_storage::test_faults::corrupt_draft_build_mapping_record_for_test(&storage, &store, key);
    let flight = flight.resume(&store);
    assert_eq!(
        flight.classification(),
        Durable::Committed,
        "changed effects must reconcile before referenced-map authentication: {flight:?}"
    );
    assert!(
        matches!(flight.state(), State::Verifying | State::Unavailable),
        "{flight:?}"
    );
    assert!(flight.original_failure().is_some());
    assert!(flight.failure().is_some());
    assert!(flight.verification_work().attempted_reads > 0);
    assert_verification(flight.verification_work());
    let Err(retained) = flight.into_completion() else {
        panic!("corrupt mapping reference released completion custody");
    };
    assert!(matches!(
        retained.state(),
        State::Verifying | State::Unavailable
    ));
}
