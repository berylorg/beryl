use super::*;
use syndic_storage::{
    DraftMarkerAdmissionStorageErrorV1, DraftMarkerAdmissionTerminalOutcomeV1,
    DraftMarkerLabelAssignmentErrorV1, DraftMarkerLabelAssignmentFlightV1,
    DraftMarkerLabelAssignmentRefusalV1 as Refusal,
};

#[test]
fn assignment_own_and_shared_limits_preserve_exact_durable_prefix() {
    let (_home, store, storage, thread) = fixture("typed-assignment-limits", 150);
    let (session, marker) = readiness_support::marked_session(&storage, &store, thread, 151);
    let operation = owner(&session, 152);
    eof_for_assignment(
        &storage,
        &store,
        operation,
        153,
        DraftMarkerLabelReadinessDispositionV1::Reuse,
        association(154, &session, marker.marker_id()),
    );
    let other = owner(&session, 155);
    eof_for_assignment(
        &storage,
        &store,
        other,
        156,
        DraftMarkerLabelReadinessDispositionV1::Reuse,
        association(157, &session, marker.marker_id()),
    );
    let before = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
        .unwrap();
    let bytes = before.head().unwrap().charge().encoded_bytes();
    for (limits, expected) in [
        (
            DraftMarkerAdmissionLimitsV1::new(64, 0, u64::MAX),
            Refusal::OperationTooLarge,
        ),
        (
            DraftMarkerAdmissionLimitsV1::new(64, u64::MAX, 0),
            Refusal::OperationTooLarge,
        ),
        (
            DraftMarkerAdmissionLimitsV1::new(1, 0, 0),
            Refusal::OperationTooLarge,
        ),
        (
            DraftMarkerAdmissionLimitsV1::new(1, u64::MAX, u64::MAX),
            Refusal::CapacityUnavailable,
        ),
        (
            DraftMarkerAdmissionLimitsV1::new(64, 1, u64::MAX),
            Refusal::CapacityUnavailable,
        ),
        (
            DraftMarkerAdmissionLimitsV1::new(64, u64::MAX, bytes),
            Refusal::CapacityUnavailable,
        ),
    ] {
        let flight = storage
            .prepare_draft_marker_label_assignment_with_limits_for_test(
                &store,
                operation,
                command(158),
                limits,
                u64::MAX,
            )
            .unwrap();
        let revision = store.home_revision().unwrap();
        let outcome = storage.submit_draft_marker_label_assignment(&store, flight);
        match outcome {
            DraftMarkerLabelAssignmentOutcomeV1::Refused(actual) => {
                assert_eq!(actual, expected, "limits {limits:?}")
            }
            DraftMarkerLabelAssignmentOutcomeV1::Ready { .. } => {
                panic!("unexpected Ready for {limits:?}")
            }
            _ => panic!("unexpected non-refusal for {limits:?}"),
        }
        assert_eq!(store.home_revision().unwrap(), revision);
        let after = storage
            .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
            .unwrap();
        assert_eq!(before.head(), after.head());
        assert_eq!(before.receipt(), after.receipt());
        assert_eq!(before.capacity(), after.capacity());
    }
    close(&storage, &store, operation);
    assert_eq!(
        storage
            .draft_editor_candidate_session(&store, session.draft_id(), session.session_id())
            .unwrap(),
        syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session)
    );
}

#[test]
fn committed_readiness_keeps_exclusive_custody_until_retry_without_another_mutation() {
    let (_home, store, storage, thread) = fixture("typed-exclusive-readiness", 160);
    let (session, marker) = readiness_support::marked_session(&storage, &store, thread, 161);
    let operation = owner(&session, 162);
    ingest_two(&storage, &store, operation, &session, marker.marker_id());
    let flight = storage
        .prepare_draft_marker_label_assignment(&store, operation, command(165))
        .unwrap()
        .defer_committed_readiness_once_for_test();
    let pending = take_pending(storage.submit_draft_marker_label_assignment(&store, flight));
    let revision = store.home_revision().unwrap();
    assert_eq!(pending.owner(), operation);
    assert_eq!(
        pending.committed_receipt().unwrap().home_revision(),
        revision
    );
    assert!(matches!(
        storage.prepare_draft_marker_label_assignment(&store, operation, command(166)),
        Err(DraftMarkerLabelAssignmentErrorV1::CapacityUnavailable)
    ));
    assert!(matches!(
        storage.submit_draft_marker_label_assignment(&store, pending),
        DraftMarkerLabelAssignmentOutcomeV1::Advanced { .. }
    ));
    assert_eq!(store.home_revision().unwrap(), revision);
    let final_flight = storage
        .prepare_draft_marker_label_assignment(&store, operation, command(166))
        .unwrap()
        .defer_committed_readiness_once_for_test();
    let final_pending =
        take_pending(storage.submit_draft_marker_label_assignment(&store, final_flight));
    let final_revision = store.home_revision().unwrap();
    let proof = match storage.submit_draft_marker_label_assignment(&store, final_pending) {
        DraftMarkerLabelAssignmentOutcomeV1::Ready { proof, .. } => proof,
        _ => panic!("exact retained readiness did not issue its one proof"),
    };
    assert_eq!(store.home_revision().unwrap(), final_revision);
    assert_eq!(proof.owner(), operation);
    assert_eq!(proof.assigned_target_root().count(), 2);
    assert!(matches!(
        storage.prepare_draft_marker_label_assignment(&store, operation, command(167)),
        Err(DraftMarkerLabelAssignmentErrorV1::Rejected)
    ));
}

#[test]
fn dropping_committed_readiness_releases_the_attempt_for_exact_terminal_cleanup() {
    let (_home, store, storage, thread) = fixture("typed-drop-readiness", 170);
    let (session, marker) = readiness_support::marked_session(&storage, &store, thread, 171);
    let operation = owner(&session, 172);
    ingest_two(&storage, &store, operation, &session, marker.marker_id());
    let flight = storage
        .prepare_draft_marker_label_assignment(&store, operation, command(175))
        .unwrap()
        .defer_committed_readiness_once_for_test();
    let pending = take_pending(storage.submit_draft_marker_label_assignment(&store, flight));
    assert!(matches!(
        storage.prepare_draft_marker_label_assignment(&store, operation, command(176)),
        Err(DraftMarkerLabelAssignmentErrorV1::CapacityUnavailable)
    ));
    drop(pending);
    close(&storage, &store, operation);
    assert_eq!(
        storage
            .draft_editor_candidate_session(&store, session.draft_id(), session.session_id())
            .unwrap(),
        syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session)
    );
}

#[test]
fn assignment_exact_old_retains_storage_failure_and_prior_custody_for_retry() {
    let (_home, store, storage, thread) = fixture("typed-assignment-old", 180);
    let (session, marker) = readiness_support::marked_session(&storage, &store, thread, 181);
    let operation = owner(&session, 182);
    eof_for_assignment(
        &storage,
        &store,
        operation,
        183,
        DraftMarkerLabelReadinessDispositionV1::Reuse,
        association(184, &session, marker.marker_id()),
    );
    let before = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
        .unwrap();
    let revision = store.home_revision().unwrap();
    let flight = storage
        .prepare_draft_marker_label_assignment(&store, operation, command(185))
        .unwrap();
    let fault = beryl_home_store::test_faults::fail_next_journal_write();
    let pending = match storage.submit_draft_marker_label_assignment(&store, flight) {
        DraftMarkerLabelAssignmentOutcomeV1::ReconciliationPending(flight) => flight,
        _ => panic!("journal failure discarded exact assignment custody"),
    };
    drop(fault);
    assert!(store.home_revision().is_err());
    let recovery = store.recover_same_home().unwrap();
    let retired_storage = storage;
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish();
    match retired_storage.submit_draft_marker_label_assignment(&store, pending) {
        DraftMarkerLabelAssignmentOutcomeV1::StorageError(
            DraftMarkerAdmissionStorageErrorV1::Command(CommandError::Commit { .. }),
        ) => {}
        DraftMarkerLabelAssignmentOutcomeV1::StorageError(error) => {
            panic!("unexpected storage {error:?}")
        }
        DraftMarkerLabelAssignmentOutcomeV1::Refused(error) => {
            panic!("unexpected refusal {error:?}")
        }
        DraftMarkerLabelAssignmentOutcomeV1::ReconciliationPending(_) => panic!("still pending"),
        _ => panic!("unexpected other outcome"),
    }
    assert_eq!(store.home_revision().unwrap(), revision);
    let after = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
        .unwrap();
    assert_eq!(before.head(), after.head());
    assert_eq!(before.receipt(), after.receipt());
    assert_eq!(before.capacity(), after.capacity());
    assert!(matches!(
        storage.prepare_draft_marker_label_assignment(&store, operation, command(185)),
        Err(DraftMarkerLabelAssignmentErrorV1::Rejected)
    ));
    close_retired(&storage, &store, operation);
}

#[test]
fn after_persist_assignment_keeps_selected_receipt_and_failure_without_minting_readiness() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("typed-assignment-after-persist", 210, faults.clone());
    let (session, marker) = readiness_support::marked_session(&storage, &store, thread, 211);
    let operation = owner(&session, 212);
    eof_for_assignment(
        &storage,
        &store,
        operation,
        213,
        DraftMarkerLabelReadinessDispositionV1::Reuse,
        association(214, &session, marker.marker_id()),
    );
    let flight = storage
        .prepare_draft_marker_label_assignment(&store, operation, command(215))
        .unwrap();
    let revision = store.home_revision().unwrap();
    faults.fail_next(FaultPoint::AfterPersist);
    match storage.submit_draft_marker_label_assignment(&store, flight) {
        DraftMarkerLabelAssignmentOutcomeV1::CommittedUnavailable {
            receipt,
            later_failure: Some(CommandError::Persistence { .. }),
            reason: syndic_storage::DraftMarkerAdmissionCommittedUnavailableReasonV1::LaterFailure,
        } => assert_eq!(receipt.home_revision().get(), revision.get() + 1),
        _ => panic!("durable assignment lost its selected receipt or later storage failure"),
    }
    let recovery = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish();
    let head = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
        .unwrap();
    assert_eq!(head.head().unwrap().selected_receipt(), Some(command(215)));
    assert_eq!(
        head.head().unwrap().lifecycle(),
        syndic_storage::DraftMarkerAdmissionLifecycleV1::Ready
    );
    assert!(matches!(
        storage.prepare_draft_marker_label_assignment(&store, operation, command(216)),
        Err(DraftMarkerLabelAssignmentErrorV1::Rejected)
    ));
    close_retired(&storage, &store, operation);
    assert_eq!(
        storage
            .draft_editor_candidate_session(&store, session.draft_id(), session.session_id())
            .unwrap(),
        syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session)
    );
}

fn close_retired(
    storage: &SyndicStorage,
    store: &HomeStore,
    operation: DraftMarkerAdmissionOwnerV1,
) {
    let mut closed = false;
    for seed in 230..250 {
        match storage.advance_draft_marker_admission_cleanup(store, operation, command(seed)) {
            DraftMarkerAdmissionTerminalOutcomeV1::Advanced { .. } => {}
            DraftMarkerAdmissionTerminalOutcomeV1::RetainedClosure => {
                closed = true;
                break;
            }
            _ => panic!("retired committed custody did not reach terminal closure"),
        }
    }
    assert!(closed);
    let closed = storage
        .draft_marker_admission_publication_snapshot_for_test(store, operation, &[])
        .unwrap();
    assert_eq!(closed.head().unwrap().charge().associations(), 0);
    assert_eq!(closed.head().unwrap().target_root().count(), 0);
}

#[test]
fn retired_committed_readiness_cannot_issue_a_proof() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("typed-retired-readiness", 190, faults.clone());
    let (session, marker) = readiness_support::marked_session(&storage, &store, thread, 191);
    let operation = owner(&session, 192);
    eof_for_assignment(
        &storage,
        &store,
        operation,
        193,
        DraftMarkerLabelReadinessDispositionV1::Reuse,
        association(194, &session, marker.marker_id()),
    );
    let flight = storage
        .prepare_draft_marker_label_assignment(&store, operation, command(195))
        .unwrap()
        .defer_committed_readiness_once_for_test();
    let pending = take_pending(storage.submit_draft_marker_label_assignment(&store, flight));
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let recovery = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish();
    let pending = take_pending(storage.submit_draft_marker_label_assignment(&store, pending));
    drop(pending);
    assert!(matches!(
        storage.prepare_draft_marker_label_assignment(&store, operation, command(196)),
        Err(DraftMarkerLabelAssignmentErrorV1::Rejected)
    ));
}

#[test]
fn actual_postcommit_read_failure_preserves_the_committed_receipt_and_error() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("typed-postcommit-read", 200, faults.clone());
    let (session, marker) = readiness_support::marked_session(&storage, &store, thread, 201);
    let operation = owner(&session, 202);
    eof_for_assignment(
        &storage,
        &store,
        operation,
        203,
        DraftMarkerLabelReadinessDispositionV1::Reuse,
        association(204, &session, marker.marker_id()),
    );
    let revision = store.home_revision().unwrap();
    let flight = storage
        .prepare_draft_marker_label_assignment(&store, operation, command(205))
        .unwrap();
    let block = faults.block_next(FaultPoint::AfterPersist);
    let outcome = std::thread::scope(|scope| {
        let worker = scope.spawn(|| storage.submit_draft_marker_label_assignment(&store, flight));
        let reached = block.wait_until_reached(std::time::Duration::from_secs(10));
        if reached {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
        }
        block.release();
        assert!(
            reached,
            "assignment did not reach its committed read boundary"
        );
        worker.join().unwrap()
    });
    let pending = match outcome {
        DraftMarkerLabelAssignmentOutcomeV1::CommittedReadinessPending {
            flight,
            error:
                DraftMarkerLabelAssignmentErrorV1::Read(syndic_storage::SyndicReadError::Read(
                    beryl_home_store::ReadError::Storage { .. },
                )),
        } => flight,
        _ => panic!("postcommit storage failure lost its committed receipt or source"),
    };
    assert_eq!(pending.owner(), operation);
    assert_eq!(
        pending.committed_receipt().unwrap().home_revision().get(),
        revision.get() + 1
    );
    drop(pending);
}

fn command(seed: u8) -> DraftMarkerAdmissionCommandIdV1 {
    DraftMarkerAdmissionCommandIdV1::from_bytes([seed; 16])
}

fn take_pending(
    outcome: DraftMarkerLabelAssignmentOutcomeV1,
) -> DraftMarkerLabelAssignmentFlightV1 {
    match outcome {
        DraftMarkerLabelAssignmentOutcomeV1::CommittedReadinessPending { flight, .. } => flight,
        _ => panic!("committed readiness did not retain its receipt and exclusive custody"),
    }
}

fn ingest_two(
    storage: &SyndicStorage,
    store: &HomeStore,
    operation: DraftMarkerAdmissionOwnerV1,
    session: &DraftEditorCandidateSessionV1,
    marker: SyndicDraftMarkerId,
) {
    let associations = Box::new([
        association(163, session, marker),
        association(164, session, marker),
    ]);
    for _ in 0..2 {
        let mut attempt = storage
            .prepare_draft_marker_label_readiness_page(
                store,
                DraftMarkerLabelReadinessPageRequestV1::new(
                    operation,
                    command(160),
                    NonZeroU64::MIN,
                    true,
                    DraftMarkerLabelReadinessDispositionV1::Reuse,
                    associations.clone(),
                    None,
                ),
            )
            .unwrap();
        let receipt = store
            .compose_proof(attempt.take_command().unwrap())
            .unwrap();
        let flight = attempt.into_submission_flight(store, receipt).unwrap();
        assert!(matches!(
            storage.submit_draft_marker_label_readiness_page(store, flight),
            syndic_storage::DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Advanced { .. }
        ));
    }
}

fn close(storage: &SyndicStorage, store: &HomeStore, operation: DraftMarkerAdmissionOwnerV1) {
    assert!(matches!(
        storage.cancel_draft_marker_admission(store, operation, command(230)),
        DraftMarkerAdmissionTerminalOutcomeV1::Advanced { .. }
    ));
    let mut closed = false;
    for seed in 231..250 {
        match storage.advance_draft_marker_admission_cleanup(store, operation, command(seed)) {
            DraftMarkerAdmissionTerminalOutcomeV1::Advanced { .. } => {}
            DraftMarkerAdmissionTerminalOutcomeV1::RetainedClosure => {
                closed = true;
                break;
            }
            _ => panic!("assignment refusal did not retain cleanable custody"),
        }
    }
    assert!(closed);
    let after = storage
        .draft_marker_admission_publication_snapshot_for_test(store, operation, &[])
        .unwrap();
    assert_eq!(after.head().unwrap().charge().associations(), 0);
    assert_eq!(after.head().unwrap().target_root().count(), 0);
}
