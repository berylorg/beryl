use super::*;
use syndic_storage::{
    DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS, DRAFT_MARKER_ADMISSION_MAX_ENCODED_BYTES,
    DraftMarkerAdmissionLimitsV1 as Limits, DraftMarkerAdmissionPublicationFixtureV1,
    DraftMarkerAdmissionRetainedChargeV1 as Charge, DraftMarkerAdmissionStorageErrorV1,
    DraftMarkerAdmissionTerminalOutcomeV1,
    DraftMarkerLabelReadinessPageSubmissionRefusalV1 as Refusal,
};

#[test]
fn isolated_charge_precedes_shared_charge_at_exact_production_boundaries() {
    let classify = DraftMarkerAdmissionPublicationFixtureV1::limits_refusal_for_test;
    let own = Charge::new(
        1,
        DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS,
        DRAFT_MARKER_ADMISSION_MAX_ENCODED_BYTES,
    );
    let full = Charge::new(
        DRAFT_MARKER_ADMISSION_MAX_HEADS,
        own.associations(),
        own.encoded_bytes(),
    );
    assert_eq!(classify(own, full), Ok(()));
    for excessive in [
        Charge::new(1, own.associations() + 1, own.encoded_bytes()),
        Charge::new(1, own.associations(), own.encoded_bytes() + 1),
    ] {
        assert_eq!(
            classify(excessive, excessive),
            Err(Refusal::OperationTooLarge)
        );
        assert_eq!(
            classify(excessive, Charge::new(u64::MAX, u64::MAX, u64::MAX)),
            Err(Refusal::OperationTooLarge)
        );
    }
    for saturated in [
        Charge::new(full.heads() + 1, full.associations(), full.encoded_bytes()),
        Charge::new(full.heads(), full.associations() + 1, full.encoded_bytes()),
        Charge::new(full.heads(), full.associations(), full.encoded_bytes() + 1),
    ] {
        assert_eq!(classify(own, saturated), Err(Refusal::CapacityUnavailable));
    }
    assert_eq!(
        classify(Charge::new(0, u64::MAX, u64::MAX), full),
        Err(Refusal::Rejected)
    );
}

#[test]
fn page_refusals_distinguish_isolated_size_and_each_shared_charge_without_mutation() {
    let (_home, store, storage, thread) = fixture("typed-page-limits", 170);
    let (session, marker) = marked_session(&storage, &store, thread, 171);
    let operation = admission_owner(&session, 172);
    for limits in [Limits::new(64, 0, u64::MAX), Limits::new(64, u64::MAX, 0)] {
        let flight = make_flight(
            &storage,
            &store,
            operation,
            173,
            1,
            false,
            174,
            &session,
            marker.marker_id(),
        )
        .with_retained_limits_for_test(limits);
        let revision = store.home_revision().unwrap();
        assert!(matches!(
            storage.submit_draft_marker_label_readiness_page(&store, flight),
            DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(Refusal::OperationTooLarge)
        ));
        assert_eq!(store.home_revision().unwrap(), revision);
        assert!(snapshot(&storage, &store, operation).head().is_none());
    }
    let occupied = admission_owner(&session, 175);
    let flight = make_flight(
        &storage,
        &store,
        occupied,
        176,
        1,
        false,
        177,
        &session,
        marker.marker_id(),
    );
    assert_advanced(
        "occupancy",
        &storage,
        &store,
        storage.submit_draft_marker_label_readiness_page(&store, flight),
        false,
    );
    let before = snapshot(&storage, &store, occupied);
    let bytes = before.capacity().unwrap().charge().encoded_bytes();
    for (limits, expected) in [
        (
            Limits::new(1, u64::MAX, u64::MAX),
            Refusal::CapacityUnavailable,
        ),
        (Limits::new(64, 1, u64::MAX), Refusal::CapacityUnavailable),
        (
            Limits::new(64, u64::MAX, bytes),
            Refusal::CapacityUnavailable,
        ),
        (Limits::new(1, 0, 0), Refusal::OperationTooLarge),
    ] {
        let flight = make_flight(
            &storage,
            &store,
            operation,
            173,
            1,
            false,
            174,
            &session,
            marker.marker_id(),
        )
        .with_retained_limits_for_test(limits);
        let revision = store.home_revision().unwrap();
        let result = storage.submit_draft_marker_label_readiness_page(&store, flight);
        assert!(
            matches!(result, DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(actual) if actual == expected)
        );
        assert_eq!(store.home_revision().unwrap(), revision);
        assert!(snapshot(&storage, &store, operation).head().is_none());
        let after = snapshot(&storage, &store, occupied);
        assert_eq!(before.head(), after.head());
        assert_eq!(before.receipt(), after.receipt());
        assert_eq!(before.capacity(), after.capacity());
    }
    assert_eq!(
        storage
            .draft_editor_candidate_session(&store, session.draft_id(), session.session_id())
            .unwrap(),
        syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session)
    );
}

#[test]
fn refusal_after_a_page_prefix_retains_exact_custody_until_terminal_cleanup() {
    let (_home, store, storage, thread) = fixture("typed-prefix-limits", 180);
    let (session, marker) = marked_session(&storage, &store, thread, 181);
    let occupied = admission_owner(&session, 182);
    let flight = make_flight(
        &storage,
        &store,
        occupied,
        183,
        1,
        false,
        184,
        &session,
        marker.marker_id(),
    );
    assert_advanced(
        "occupancy",
        &storage,
        &store,
        storage.submit_draft_marker_label_readiness_page(&store, flight),
        false,
    );
    let operation = admission_owner(&session, 185);
    let associations = Box::new([
        association(186, &session, marker.marker_id()),
        association(187, &session, marker.marker_id()),
    ]);
    let first = page_flight(
        &storage,
        &store,
        operation,
        188,
        1,
        false,
        associations.clone(),
    );
    assert_advanced(
        "prefix",
        &storage,
        &store,
        storage.submit_draft_marker_label_readiness_page(&store, first),
        false,
    );
    let before = snapshot(&storage, &store, operation);
    assert_eq!(before.head().unwrap().ingestion_association_cursor(), 1);
    for (limit, expected) in [
        (1, Refusal::OperationTooLarge),
        (2, Refusal::CapacityUnavailable),
    ] {
        let next = page_flight(
            &storage,
            &store,
            operation,
            188,
            1,
            false,
            associations.clone(),
        )
        .with_retained_limits_for_test(Limits::new(64, limit, u64::MAX));
        let revision = store.home_revision().unwrap();
        assert!(
            matches!(storage.submit_draft_marker_label_readiness_page(&store, next), DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(actual) if actual == expected)
        );
        assert_eq!(store.home_revision().unwrap(), revision);
        let after = snapshot(&storage, &store, operation);
        assert_eq!(before.head(), after.head());
        assert_eq!(before.receipt(), after.receipt());
        assert_eq!(before.capacity(), after.capacity());
    }
    assert!(matches!(
        storage.cancel_draft_marker_admission(
            &store,
            operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([189; 16])
        ),
        DraftMarkerAdmissionTerminalOutcomeV1::Advanced { .. }
    ));
    let mut closed = false;
    for command in 190..200 {
        match storage.advance_draft_marker_admission_cleanup(
            &store,
            operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([command; 16]),
        ) {
            DraftMarkerAdmissionTerminalOutcomeV1::Advanced { .. } => {}
            DraftMarkerAdmissionTerminalOutcomeV1::RetainedClosure => {
                closed = true;
                break;
            }
            _ => panic!("refused prefix did not retain cleanable custody"),
        }
    }
    assert!(closed);
    let after = snapshot(&storage, &store, operation);
    assert_eq!(after.head().unwrap().charge().associations(), 0);
    assert_eq!(after.head().unwrap().target_root().count(), 0);
    assert_eq!(
        storage
            .draft_editor_candidate_session(&store, session.draft_id(), session.session_id())
            .unwrap(),
        syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session)
    );
}

#[test]
fn proven_noncommit_retains_storage_failure_and_releases_the_transient_slot() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("typed-exact-old", 200, faults.clone());
    let (session, marker) = marked_session(&storage, &store, thread, 201);
    let operation = admission_owner(&session, 202);
    let flight = make_flight(
        &storage,
        &store,
        operation,
        203,
        1,
        false,
        204,
        &session,
        marker.marker_id(),
    );
    let revision = store.home_revision().unwrap();
    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(matches!(
        storage.submit_draft_marker_label_readiness_page(&store, flight),
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::StorageError(
            DraftMarkerAdmissionStorageErrorV1::Command(CommandError::Commit { .. })
        )
    ));
    let recovery = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish();
    assert_eq!(store.home_revision().unwrap(), revision);
    assert!(snapshot(&storage, &store, operation).head().is_none());
    let next = make_flight(
        &storage,
        &store,
        operation,
        203,
        1,
        false,
        204,
        &session,
        marker.marker_id(),
    );
    assert_advanced(
        "retry after exact old",
        &storage,
        &store,
        storage.submit_draft_marker_label_readiness_page(&store, next),
        false,
    );
}

#[test]
fn exact_old_reconciliation_retains_failure_across_pending_retry() {
    let (_home, store, storage, thread) = fixture("typed-reconciled-old", 210);
    let (_foreign_home, foreign_store, _, _) = fixture("typed-reconciled-old-foreign", 211);
    let (session, marker) = marked_session(&storage, &store, thread, 212);
    let operation = admission_owner(&session, 213);
    let flight = make_flight(
        &storage,
        &store,
        operation,
        214,
        1,
        false,
        215,
        &session,
        marker.marker_id(),
    );
    let revision = store.home_revision().unwrap();
    let fault = beryl_home_store::test_faults::fail_next_journal_write();
    let pending = match storage.submit_draft_marker_label_readiness_page(&store, flight) {
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::ReconciliationPending(flight) => flight,
        _ => panic!("journal failure did not retain exact reconciliation custody"),
    };
    drop(fault);
    let pending = match storage.submit_draft_marker_label_readiness_page(&foreign_store, pending) {
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::ReconciliationPending(flight) => flight,
        _ => panic!("foreign reconciliation discarded original failure or custody"),
    };
    assert!(store.home_revision().is_err());
    let recovery = store.recover_same_home().unwrap();
    let retired_storage = storage;
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish();
    match retired_storage.submit_draft_marker_label_readiness_page(&store, pending) {
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::StorageError(
            DraftMarkerAdmissionStorageErrorV1::Command(CommandError::Commit { .. }),
        ) => {}
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::StorageError(error) => {
            panic!("unexpected storage {error:?}")
        }
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(error) => {
            panic!("unexpected refusal {error:?}")
        }
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::ReconciliationPending(_) => {
            panic!("still pending")
        }
        _ => panic!("unexpected other outcome"),
    }
    assert_eq!(store.home_revision().unwrap(), revision);
    assert!(snapshot(&storage, &store, operation).head().is_none());
    assert!(store.pending_reconciliations().is_empty());
    let next = make_flight(
        &storage,
        &store,
        operation,
        214,
        1,
        false,
        215,
        &session,
        marker.marker_id(),
    );
    assert_advanced(
        "retry after reconciled old",
        &storage,
        &store,
        storage.submit_draft_marker_label_readiness_page(&store, next),
        false,
    );
}

#[test]
fn readiness_preflight_read_failure_is_not_an_admission_capacity_refusal() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("typed-source-read", 220, faults.clone());
    let (session, marker) = marked_session(&storage, &store, thread, 221);
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    let result = storage.prepare_draft_marker_label_readiness_page(
        &store,
        DraftMarkerLabelReadinessPageRequestV1::new(
            admission_owner(&session, 222),
            DraftMarkerAdmissionCommandIdV1::from_bytes([223; 16]),
            NonZeroU64::MIN,
            false,
            DraftMarkerLabelReadinessDispositionV1::Reuse,
            Box::new([association(224, &session, marker.marker_id())]),
            None,
        ),
    );
    assert!(matches!(
        result,
        Err(
            syndic_storage::DraftMarkerReadinessSourceErrorV1::PreflightRead(
                syndic_storage::SyndicReadError::Read(beryl_home_store::ReadError::Storage { .. })
            )
        )
    ));
}

#[test]
fn submission_preflight_read_failure_preserves_its_typed_storage_source() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("typed-submission-read", 230, faults.clone());
    let (session, marker) = marked_session(&storage, &store, thread, 231);
    let operation = admission_owner(&session, 232);
    let flight = make_flight(
        &storage,
        &store,
        operation,
        233,
        1,
        false,
        234,
        &session,
        marker.marker_id(),
    );
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(matches!(
        storage.submit_draft_marker_label_readiness_page(&store, flight),
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::StorageError(
            DraftMarkerAdmissionStorageErrorV1::Read(syndic_storage::SyndicReadError::Read(
                beryl_home_store::ReadError::Storage { .. }
            ))
        )
    ));
}

#[test]
fn committed_page_preserves_receipt_confirmation_and_concurrent_health_failures() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("typed-committed-confirmation", 240, faults.clone());
    let (session, marker) = marked_session(&storage, &store, thread, 241);
    let operation = admission_owner(&session, 242);
    let flight = make_flight(
        &storage,
        &store,
        operation,
        243,
        1,
        false,
        244,
        &session,
        marker.marker_id(),
    );
    let revision = store.home_revision().unwrap();
    let block = faults.block_next(FaultPoint::AfterPersist);
    let outcome = std::thread::scope(|scope| {
        let worker =
            scope.spawn(|| storage.submit_draft_marker_label_readiness_page(&store, flight));
        let reached = block.wait_until_reached(std::time::Duration::from_secs(10));
        if reached {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            let failed = store.home_revision().is_err();
            block.release();
            assert!(failed);
        }
        block.release();
        assert!(
            reached,
            "page did not reach its committed confirmation boundary"
        );
        worker.join().unwrap()
    });
    match outcome {
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::CommittedUnavailable {
            receipt,
            later_failure: Some(CommandError::HealthGate(_)),
            reason:
                syndic_storage::DraftMarkerAdmissionCommittedUnavailableReasonV1::Receipt(
                    beryl_home_store::CommitReceiptError::HealthGate(_),
                ),
        } => assert_eq!(receipt.home_revision().get(), revision.get() + 1),
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::CommittedUnavailable {
            later_failure,
            reason,
            ..
        } => panic!("unexpected committed reason {reason:?}, later {later_failure:?}"),
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Advanced { .. } => {
            panic!("unexpected advanced")
        }
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(reason) => {
            panic!("unexpected refusal {reason:?}")
        }
        _ => panic!("committed page confirmation lost its read failure or selected receipt"),
    }
}
