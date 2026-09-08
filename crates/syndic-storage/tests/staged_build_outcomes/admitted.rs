use super::{outcomes::*, *};
use syndic_storage::test_faults::{
    draft_marker_program_snapshot_for_test, staged_outcome_build_for_test,
};
use syndic_storage::{
    StagedDraftPieceDurableClassificationV1 as Durable, StagedDraftPieceOutcomeStateV1 as State,
    StagedDraftPieceTerminalElectionV1 as Election,
};

pub(super) fn fresh_staging(
    name: &str,
    seed: u8,
    faults: FaultController,
) -> (
    AcceptedFixture,
    DraftMarkerAdmissionOwnerV1,
    DraftMutationStagingIdentityV1,
) {
    let fixture = AcceptedFixture::with_faults(name, seed, faults);
    let admission = owner(&fixture.session, seed + 1);
    ingest(
        &fixture,
        admission,
        seed + 2,
        1,
        true,
        vec![fresh(seed + 3, fixture.asset_id)],
        || Some(fresh_factory(&fixture)),
    );
    let proof = assign(&fixture, admission, seed + 4, 1);
    let marker = DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([seed + 3; 16]),
        1,
        label(&fixture, &proof, seed + 3),
        fixture.asset_id,
    );
    let replacement =
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Marker(marker)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    0,
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
                ),
            ));
    let identity = finish_admitted_staging(
        &fixture.storage,
        &fixture.store,
        &fixture.session,
        proof,
        &[replacement],
    );
    transfer(&fixture.storage, &fixture.store, identity);
    stage_windows(
        &fixture.storage,
        &fixture.store,
        identity,
        DraftPieceDurableBuildWindowLimitsV1::new(1, 1, 65_536).unwrap(),
    );
    (fixture, admission, identity)
}

pub(super) fn committed_cleanup(
    fixture: &AcceptedFixture,
    identity: DraftMutationStagingIdentityV1,
) -> syndic_storage::StagedDraftPieceOutcomeFlightV1 {
    let endpoint = advance_to_terminal(&fixture.storage, &fixture.store, identity);
    let prepared = fixture
        .storage
        .prepare_staged_draft_piece_terminal(&fixture.store, identity, endpoint, Election::Settle)
        .unwrap();
    let flight = prepared.submit(&fixture.store);
    assert_eq!(flight.state(), State::CleanupPending, "{flight:?}");
    assert_eq!(flight.classification(), Durable::Committed);
    assert_known_commit_verification(flight.verification_work());
    flight
}

#[test]
fn admitted_marker_ambiguous_advances_authenticate_every_insert_program_boundary() {
    let faults = FaultController::new();
    let (fixture, admission, identity) =
        fresh_staging("outcome-marker-program", 80, faults.clone());
    let mut boundaries = std::collections::BTreeSet::new();
    for _ in 0..256 {
        let before = staged_outcome_build_for_test(&fixture.storage, &fixture.store, identity);
        let Some(prepared) = fixture
            .storage
            .prepare_staged_draft_piece_advance(&fixture.store, identity, before.progress_receipt())
            .unwrap()
        else {
            break;
        };
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let flight = prepared.submit(&fixture.store);
        assert_eq!(flight.state(), State::Reconciling, "{flight:?}");
        assert!(flight.has_reconciliation_custody());
        let flight = flight.resume(&fixture.store);
        assert_eq!(
            flight.state(),
            State::Complete,
            "before {:?}, after {:?}: {flight:?}",
            draft_marker_program_snapshot_for_test(&before),
            draft_marker_program_snapshot_for_test(&staged_outcome_build_for_test(
                &fixture.storage,
                &fixture.store,
                identity
            ))
        );
        assert_eq!(flight.classification(), Durable::Committed);
        assert!(flight.original_failure().is_some());
        assert!(!flight.has_reconciliation_custody());
        assert_verification_budget(flight.verification_work());
        assert!(flight.verification_work().attempted_reads > 0);
        let after = staged_outcome_build_for_test(&fixture.storage, &fixture.store, identity);
        assert!(
            after.progress_receipt().key().transition_ordinal()
                > before.progress_receipt().key().transition_ordinal()
        );
        if let Some(program) = draft_marker_program_snapshot_for_test(&after) {
            boundaries.insert(program.pending);
        }
    }
    for pending in [1, 5, 6, 7] {
        assert!(
            boundaries.contains(&pending),
            "missing program state {pending}: {boundaries:?}"
        );
    }
    let flight = committed_cleanup(&fixture, identity);
    let completion = complete(flight, &fixture.store);
    assert!(completion.cleanup_receipt.is_some());
    let snapshot = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, admission, &[])
        .unwrap();
    assert!(snapshot.head().is_none());
    assert!(snapshot.receipt().is_none());
}

#[test]
fn cleanup_exact_new_retains_settlement_and_releases_only_the_empty_writer() {
    let faults = FaultController::new();
    let (fixture, admission, identity) = fresh_staging("outcome-cleanup-new", 90, faults.clone());
    let flight = committed_cleanup(&fixture, identity);
    let result = flight.result().unwrap().clone();
    let revision = fixture.store.home_revision().unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let flight = flight.resume(&fixture.store);
    assert_eq!(flight.state(), State::CleanupReconciling, "{flight:?}");
    assert_eq!(flight.classification(), Durable::Committed);
    assert_eq!(flight.result(), Some(&result));
    assert!(flight.cleanup_failure().is_some());
    assert!(flight.has_reconciliation_custody());
    let flight = flight.resume(&fixture.store);
    assert_eq!(flight.state(), State::Complete, "{flight:?}");
    assert_eq!(flight.result(), Some(&result));
    assert!(flight.cleanup_receipt().is_some());
    assert!(!flight.has_reconciliation_custody());
    assert_verification_budget(flight.verification_work());
    assert!(flight.verification_work().attempted_reads > 0);
    assert_eq!(
        fixture.store.home_revision().unwrap().get(),
        revision.get() + 1
    );
    let completion = complete(flight, &fixture.store);
    assert!(completion.inert_cleanup.is_none());
    let snapshot = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, admission, &[])
        .unwrap();
    assert!(snapshot.head().is_none());
    assert!(snapshot.receipt().is_none());
}

#[test]
fn committed_cleanup_exact_new_preserves_terminal_mapping_evidence() {
    use syndic_storage::test_faults::{
        DraftBuildMappingRootForTest, draft_build_mapping_record_for_test,
        draft_build_mapping_root_key_for_test, draft_build_mapping_snapshot,
    };
    let faults = FaultController::new();
    let (fixture, admission, identity) =
        fresh_staging("mapping-cleanup-retention", 176, faults.clone());
    let flight = committed_cleanup(&fixture, identity);
    let terminal = staged_outcome_build_for_test(&fixture.storage, &fixture.store, identity);
    let mapping = draft_build_mapping_snapshot(&terminal).unwrap();
    assert_eq!(mapping.stage_tag, 0);
    assert_eq!(mapping.pending_target_units, None);
    let key = draft_build_mapping_root_key_for_test(
        &fixture.storage,
        &fixture.store,
        &terminal,
        DraftBuildMappingRootForTest::Current,
    )
    .unwrap();
    let encoded =
        draft_build_mapping_record_for_test(&fixture.storage, &fixture.store, key).unwrap();
    let result = flight.result().unwrap().clone();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let flight = flight.resume(&fixture.store);
    assert_eq!(flight.state(), State::CleanupReconciling, "{flight:?}");
    assert_eq!(flight.classification(), Durable::Committed);
    assert_eq!(flight.result(), Some(&result));
    assert!(flight.has_reconciliation_custody());
    let flight = flight.resume(&fixture.store);
    assert_eq!(flight.state(), State::Complete, "{flight:?}");
    assert!(flight.verification_work().attempted_reads > 0);
    assert!(flight.verification_work().attempted_reads <= 126);
    let completion = complete(flight, &fixture.store);
    assert_eq!(completion.result, result);
    assert!(completion.cleanup_receipt.is_some());
    assert!(completion.cleanup_failure.is_some());
    assert_eq!(
        staged_outcome_build_for_test(&fixture.storage, &fixture.store, identity),
        terminal
    );
    assert_eq!(draft_build_mapping_snapshot(&terminal), Some(mapping));
    assert_eq!(
        draft_build_mapping_record_for_test(&fixture.storage, &fixture.store, key),
        Some(encoded)
    );
    let admission = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, admission, &[])
        .unwrap();
    assert!(admission.head().is_none());
    assert!(admission.receipt().is_none());
}

#[test]
fn cleanup_exact_old_retriggers_its_failed_handle_before_explicit_bounded_retry() {
    let faults = FaultController::new();
    let (fixture, admission, identity) = fresh_staging("outcome-cleanup-old", 100, faults);
    let flight = committed_cleanup(&fixture, identity);
    let result = flight.result().unwrap().clone();
    let revision = fixture.store.home_revision().unwrap();
    let fault = beryl_home_store::test_faults::fail_next_journal_write();
    let mut flight = flight.resume(&fixture.store);
    drop(fault);
    assert_eq!(flight.state(), State::CleanupReconciling, "{flight:?}");
    for _ in 0..2 {
        flight = flight.resume(&fixture.store);
        assert_eq!(flight.state(), State::CleanupReconciling, "{flight:?}");
        assert_eq!(flight.classification(), Durable::Committed);
        assert_eq!(flight.result(), Some(&result));
        assert!(flight.has_reconciliation_custody());
        assert!(flight.failure().is_some());
    }
    let recovery = fixture.store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish();
    let flight = flight.resume(&store);
    assert_eq!(flight.state(), State::CleanupPending, "{flight:?}");
    assert_eq!(flight.classification(), Durable::Committed);
    assert_eq!(flight.result(), Some(&result));
    assert_eq!(store.home_revision().unwrap(), revision);
    assert!(!flight.has_reconciliation_custody());
    let snapshot = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, admission, &[])
        .unwrap();
    assert!(snapshot.head().is_some());
    let flight = flight.resume(&store);
    assert_eq!(flight.classification(), Durable::Committed);
    assert_eq!(flight.result(), Some(&result));
    assert!(flight.cleanup_failure().is_some());
    assert_eq!(flight.state(), State::Unavailable, "{flight:?}");
    assert!(matches!(
        flight.failure(),
        Some(syndic_storage::StagedDraftPieceOutcomeErrorV1::Unavailable)
    ));
}

#[test]
fn cancelled_writer_replay_preserves_result_without_duplicating_inert_cleanup() {
    let faults = FaultController::new();
    let (fixture, admission, identity) = fresh_staging("outcome-cancel-replay", 110, faults);
    let endpoint = building_endpoint(&fixture.storage, &fixture.store, identity);
    let first = fixture
        .storage
        .prepare_staged_draft_piece_terminal(&fixture.store, identity, endpoint, Election::Cancel)
        .unwrap();
    let replay = fixture
        .storage
        .prepare_staged_draft_piece_terminal(&fixture.store, identity, endpoint, Election::Cancel)
        .unwrap();
    let first = complete(first.submit(&fixture.store), &fixture.store);
    assert!(matches!(
        first.result,
        DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Cancelled(_))
    ));
    assert_eq!(first.inert_cleanup.as_ref().unwrap().owner(), admission);
    assert!(first.cleanup_receipt.is_none());
    let replay = complete(replay.submit(&fixture.store), &fixture.store);
    assert_eq!(replay.result, first.result);
    assert!(
        replay.inert_cleanup.is_none(),
        "replay duplicated the operation-owned cleanup authority"
    );
    assert!(replay.cleanup_receipt.is_none());
    assert_known_commit_verification(replay.verification);
    let session = active_session(
        &fixture.storage,
        &fixture.store,
        fixture.session.draft_id(),
        fixture.session.session_id(),
    );
    assert_eq!(session.newest_root(), fixture.session.newest_root());
    assert_eq!(session.newest_history(), fixture.session.newest_history());
    assert!(session.active_operation().is_none());
}

#[test]
fn known_committed_writer_finalization_is_consumed_once_despite_later_failure() {
    let faults = FaultController::new();
    let (fixture, _, identity) = fresh_staging("outcome-local-finalization", 120, faults.clone());
    let endpoint = building_endpoint(&fixture.storage, &fixture.store, identity);
    let prepared = fixture
        .storage
        .prepare_staged_draft_piece_advance(&fixture.store, identity, endpoint)
        .unwrap()
        .unwrap();
    faults.fail_next(FaultPoint::AfterPersist);
    let flight = prepared.submit(&fixture.store);
    assert_eq!(flight.classification(), Durable::Committed);
    assert_eq!(
        flight.local_finalization(),
        syndic_storage::StagedDraftPieceLocalFinalizationV1::Consumed
    );
    assert!(flight.later_failure().is_some());
    assert_known_commit_verification(flight.verification_work());
    let historical = flight.result().unwrap().clone();
    let flight = flight.resume(&fixture.store);
    assert_eq!(flight.state(), State::Finalizing, "{flight:?}");
    assert_eq!(flight.result(), Some(&historical));
    assert_eq!(
        flight.local_finalization(),
        syndic_storage::StagedDraftPieceLocalFinalizationV1::Consumed
    );
    assert!(matches!(
        flight.failure(),
        Some(syndic_storage::StagedDraftPieceOutcomeErrorV1::LocalCustody)
    ));
    let recovery = fixture.store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish();
    let flight = flight.resume(&store);
    assert_eq!(flight.state(), State::Unavailable);
    assert_eq!(flight.classification(), Durable::Committed);
    assert_eq!(flight.result(), Some(&historical));
    assert_eq!(
        flight.local_finalization(),
        syndic_storage::StagedDraftPieceLocalFinalizationV1::Consumed
    );
    assert_ne!(
        staged_outcome_build_for_test(&storage, &store, identity).progress_receipt(),
        endpoint
    );
}

#[test]
fn cleanup_local_finalization_releases_writer_once_despite_later_failure() {
    let faults = FaultController::new();
    let (fixture, admission, identity) =
        fresh_staging("outcome-cleanup-finalization", 130, faults.clone());
    let cleanup = committed_cleanup(&fixture, identity);
    faults.fail_next(FaultPoint::AfterPersist);
    let cleanup = cleanup.resume(&fixture.store);
    assert_eq!(cleanup.classification(), Durable::Committed);
    assert_eq!(
        cleanup.cleanup_local_finalization(),
        syndic_storage::StagedDraftPieceLocalFinalizationV1::Consumed
    );
    assert!(cleanup.cleanup_failure().is_some());
    assert_eq!(cleanup.state(), State::Complete, "{cleanup:?}");
    assert_known_commit_verification(cleanup.verification_work());
    let cleanup = cleanup.resume(&fixture.store);
    assert_eq!(cleanup.state(), State::Complete);
    assert_eq!(
        cleanup.cleanup_local_finalization(),
        syndic_storage::StagedDraftPieceLocalFinalizationV1::Consumed
    );
    let recovery = fixture.store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish();
    let snapshot = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, admission, &[])
        .unwrap();
    assert!(snapshot.head().is_none());
}
