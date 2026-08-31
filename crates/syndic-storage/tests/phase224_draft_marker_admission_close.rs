#![cfg(feature = "test-faults")]

include!("phase154_durable_builder/support.rs");

use std::num::NonZeroU64;

use sha2::{Digest, Sha256};
use syndic_storage::{
    DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionLifecycleV1,
    DraftMarkerAdmissionOperationIdV1, DraftMarkerAdmissionOwnerV1,
    DraftMarkerAdmissionTerminalOutcomeV1, DraftMarkerAdmissionTerminalReceiptFaultV1,
    DraftMarkerLabelAssignmentOutcomeV1, DraftMarkerLabelReadinessDispositionV1,
    DraftMarkerLabelReadinessPageRequestV1, DraftMarkerLabelReadinessPageSubmissionFlightV1,
    DraftMarkerLabelReadinessPageSubmissionOutcomeV1,
    DraftMarkerLabelReadinessPageSubmissionRefusalV1, DraftMarkerReadinessCandidateSourceV1,
    DraftMarkerReadinessSourceAssociationV1, DraftMarkerReadinessSourceSelectorV1,
    DraftPieceRootBuildIdentityV1, DraftPieceRootReferenceV1, DraftPieceSettlementKeyV1,
};

#[path = "phase216_draft_marker_readiness_source_proof/support.rs"]
mod readiness_support;
#[path = "phase224_draft_marker_admission_close/support.rs"]
mod support;

use readiness_support::{association, marked_session, owner};
use support::*;

#[test]
fn cancel_before_durable_admission_releases_only_the_transient_reservation() {
    let (_home, store, storage, thread) = fixture("phase224-transient", 1);
    let (session, marker) = marked_session(&storage, &store, thread, 2);
    let admission = owner(&session, 3);
    let attempt = storage
        .prepare_draft_marker_label_readiness_page(
            &store,
            request(
                admission,
                4,
                false,
                vec![association(5, &session, marker.marker_id())],
            ),
        )
        .unwrap();

    assert!(matches!(
        storage.cancel_draft_marker_admission(&store, admission, command(6)),
        DraftMarkerAdmissionTerminalOutcomeV1::ReleasedTransient
    ));
    drop(attempt);

    let snapshot = snapshot(&storage, &store, admission);
    assert!(snapshot.head().is_none());
    assert!(snapshot.capacity().is_none());
    assert!(snapshot.receipt().is_none());
    assert!(
        storage
            .prepare_draft_marker_label_readiness_page(
                &store,
                request(
                    admission,
                    7,
                    false,
                    vec![association(8, &session, marker.marker_id())],
                ),
            )
            .is_ok()
    );
}

#[test]
fn dispatch_election_keeps_reservation_until_first_durable_head_or_refusal() {
    let (_home, store, storage, thread) = fixture("phase224-dispatch-election", 9);
    let (session, marker) = marked_session(&storage, &store, thread, 10);
    let elected = owner(&session, 11);
    let mut flight = submission_flight(
        &storage,
        &store,
        elected,
        12,
        false,
        vec![association(13, &session, marker.marker_id())],
    );
    assert!(flight.dispatch_attachment_reservation_for_test());

    assert!(!matches!(
        storage.cancel_draft_marker_admission(&store, elected, command(14)),
        DraftMarkerAdmissionTerminalOutcomeV1::ReleasedTransient
    ));
    assert!(matches!(
        storage.submit_draft_marker_label_readiness_page(&store, flight),
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Advanced { .. }
    ));
    assert!(snapshot(&storage, &store, elected).head().is_some());
    assert_cancelled_terminal(&storage, &store, elected, 15);

    let invalidated = owner(&session, 16);
    let flight = submission_flight(
        &storage,
        &store,
        invalidated,
        17,
        false,
        vec![association(18, &session, marker.marker_id())],
    );
    assert!(
        storage.invalidate_draft_marker_admission_submission_reservation_for_test(&store, &flight,)
    );
    assert!(matches!(
        storage.submit_draft_marker_label_readiness_page(&store, flight),
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
            DraftMarkerLabelReadinessPageSubmissionRefusalV1::Rejected
        )
    ));
    let refused = snapshot(&storage, &store, invalidated);
    assert!(refused.head().is_none());
    assert!(refused.receipt().is_none());
}

#[test]
fn malformed_compact_terminal_receipts_remain_inert_and_scheduled_for_cleanup() {
    for (name, fault, seed) in [
        (
            "phase224-terminal-receipt-missing",
            DraftMarkerAdmissionTerminalReceiptFaultV1::Missing,
            90,
        ),
        (
            "phase224-terminal-receipt-mismatched",
            DraftMarkerAdmissionTerminalReceiptFaultV1::Mismatched,
            100,
        ),
        (
            "phase224-terminal-receipt-extra",
            DraftMarkerAdmissionTerminalReceiptFaultV1::Extra,
            110,
        ),
    ] {
        assert_terminal_receipt_fault_remains_inert(name, fault, seed);
    }
}

#[test]
fn compact_terminal_charge_mismatch_remains_inert_and_cannot_settle() {
    let (home, store, storage, thread) = fixture("phase224-terminal-charge-mismatch", 120);
    let (session, marker) = marked_session(&storage, &store, thread, 121);
    let admission = owner(&session, 122);
    let terminal_command = command(125);
    submit_page(
        &storage,
        &store,
        admission,
        123,
        false,
        vec![association(124, &session, marker.marker_id())],
    );
    assert_advanced(storage.cancel_draft_marker_admission(&store, admission, terminal_command));
    compact_terminal_without_retaining(&storage, &store, admission, 126);

    let exact = snapshot(&storage, &store, admission);
    let exact_charge = exact.head().unwrap().charge();
    assert_eq!(exact.capacity().unwrap().charge(), exact_charge);
    let receipt_digest = exact.receipt().unwrap().digest();
    assert!(matches!(
        storage.inject_draft_marker_admission_terminal_receipt_fault_for_test(
            &store,
            admission,
            DraftMarkerAdmissionTerminalReceiptFaultV1::ChargeMismatch,
        ),
        beryl_home_store::CommandOutcome::Committed { .. }
    ));
    let mismatched = snapshot(&storage, &store, admission);
    assert_eq!(
        mismatched.head().unwrap().charge(),
        mismatched.capacity().unwrap().charge()
    );
    assert_ne!(
        mismatched.head().unwrap().charge().encoded_bytes(),
        exact_charge.encoded_bytes()
    );
    assert_eq!(mismatched.receipt().unwrap().digest(), receipt_digest);
    drop(storage);
    drop(store);

    let mut reopened =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_inert_cleanup_refuses_without_mutation(&storage, &reopened, admission, 127);
    let before_settlement = snapshot(&storage, &reopened, admission);
    let head_digest = before_settlement.head().unwrap().digest();
    let capacity_digest = before_settlement.capacity().unwrap().digest();
    assert!(
        storage
            .transfer_draft_marker_admission_terminal_to_settlement_for_test(
                &reopened,
                admission,
                terminal_command,
            )
            .is_err()
    );
    let after = snapshot(&storage, &reopened, admission);
    assert_eq!(after.head().unwrap().digest(), head_digest);
    assert_eq!(after.capacity().unwrap().digest(), capacity_digest);
}

#[test]
fn cancellation_from_ingesting_assigning_and_ready_never_publishes_readiness() {
    let (_home, store, storage, thread) = fixture("phase224-lifecycles", 10);
    let (session, marker) = marked_session(&storage, &store, thread, 11);

    let ingesting = owner(&session, 12);
    submit_page(
        &storage,
        &store,
        ingesting,
        13,
        false,
        vec![association(14, &session, marker.marker_id())],
    );
    assert_cancelled_terminal(&storage, &store, ingesting, 15);

    let assigning = owner(&session, 16);
    submit_page(
        &storage,
        &store,
        assigning,
        17,
        true,
        vec![association(18, &session, marker.marker_id())],
    );
    assert_eq!(
        snapshot(&storage, &store, assigning)
            .head()
            .unwrap()
            .lifecycle(),
        DraftMarkerAdmissionLifecycleV1::Assigning
    );
    assert_cancelled_terminal(&storage, &store, assigning, 19);

    let ready = owner(&session, 20);
    submit_page(
        &storage,
        &store,
        ready,
        21,
        true,
        vec![association(22, &session, marker.marker_id())],
    );
    let flight = storage
        .prepare_draft_marker_label_assignment(&store, ready, command(23))
        .unwrap();
    assert!(matches!(
        storage.submit_draft_marker_label_assignment(&store, flight),
        DraftMarkerLabelAssignmentOutcomeV1::Ready { .. }
    ));
    assert_eq!(
        snapshot(&storage, &store, ready)
            .head()
            .unwrap()
            .lifecycle(),
        DraftMarkerAdmissionLifecycleV1::Ready
    );
    assert_cancelled_terminal(&storage, &store, ready, 24);
}

#[test]
fn terminal_cleanup_is_bounded_reopens_and_retains_only_exact_replay_closure() {
    let (home, store, storage, thread) = fixture("phase224-cleanup", 30);
    let (session, marker) = marked_session(&storage, &store, thread, 31);
    let admission = owner(&session, 32);
    let associations = vec![
        association(33, &session, marker.marker_id()),
        association(34, &session, marker.marker_id()),
    ];
    submit_page(&storage, &store, admission, 35, false, associations.clone());
    submit_page(&storage, &store, admission, 35, false, associations);
    let before = snapshot(&storage, &store, admission);
    assert_eq!(before.head().unwrap().charge().associations(), 2);
    let cancellation = command(36);
    assert_advanced(storage.cancel_draft_marker_admission(&store, admission, cancellation));
    let terminal = terminal_cleanup_snapshot(&storage, &store, admission);
    assert_eq!(
        terminal.capacity().unwrap().charge(),
        terminal.head().unwrap().charge()
    );

    assert_advanced(storage.advance_draft_marker_admission_cleanup(&store, admission, command(37)));
    let after_first = terminal_cleanup_snapshot(&storage, &store, admission);
    assert_eq!(
        after_first.capacity().unwrap().charge(),
        after_first.head().unwrap().charge()
    );

    drop(storage);
    drop(store);
    let mut reopened =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        storage
            .next_inert_draft_marker_admission_cleanup(&reopened)
            .unwrap(),
        Some(admission)
    );
    for cleanup_command in [38, 39, 40, 41] {
        match storage.advance_draft_marker_admission_cleanup(
            &reopened,
            admission,
            command(cleanup_command),
        ) {
            DraftMarkerAdmissionTerminalOutcomeV1::Advanced { .. } => {}
            DraftMarkerAdmissionTerminalOutcomeV1::RetainedClosure => break,
            _ => panic!("terminal cleanup did not make bounded progress"),
        }
    }
    let compact = terminal_cleanup_snapshot(&storage, &reopened, admission);
    let head = compact.head().unwrap();
    assert_eq!(head.charge().associations(), 0);
    assert_eq!(head.source_root().count(), 0);
    assert_eq!(head.target_root().count(), 0);
    assert!(compact.receipt().is_some());
    assert_eq!(compact.capacity().unwrap().charge(), head.charge());
    assert!(matches!(
        storage.cancel_draft_marker_admission(&reopened, admission, cancellation),
        DraftMarkerAdmissionTerminalOutcomeV1::Replayed
    ));
    assert!(matches!(
        storage.cancel_draft_marker_admission(&reopened, admission, command(42)),
        DraftMarkerAdmissionTerminalOutcomeV1::Collision
    ));
}

#[test]
fn acknowledgement_uncertainty_is_reconciled_without_reactivating_terminal_custody() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) = fixture_with_faults("phase224-ack", 50, faults.clone());
    let (session, marker) = marked_session(&storage, &store, thread, 51);
    let admission = owner(&session, 52);
    submit_page(
        &storage,
        &store,
        admission,
        53,
        false,
        vec![association(54, &session, marker.marker_id())],
    );
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let pending = match storage.cancel_draft_marker_admission(&store, admission, command(55)) {
        DraftMarkerAdmissionTerminalOutcomeV1::ReconciliationPending(flight) => flight,
        _ => panic!("lost acknowledgement did not retain exact terminal reconciliation"),
    };
    assert_advanced(storage.resolve_draft_marker_admission_terminal(&store, pending));
    terminal_cleanup_snapshot(&storage, &store, admission);
}

#[test]
fn stale_generation_schedules_inert_cleanup_and_retirement_never_reactivates_admission() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) = fixture_with_faults("phase224-stale", 60, faults.clone());
    let (session, marker) = marked_session(&storage, &store, thread, 61);
    let admission = owner(&session, 62);
    submit_page(
        &storage,
        &store,
        admission,
        63,
        false,
        vec![association(64, &session, marker.marker_id())],
    );
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let recovery = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let reopened = recovery.publish();
    assert_eq!(
        storage
            .next_inert_draft_marker_admission_cleanup(&reopened)
            .unwrap(),
        Some(admission)
    );
    assert!(matches!(
        storage.cancel_draft_marker_admission(&reopened, admission, command(65)),
        DraftMarkerAdmissionTerminalOutcomeV1::Refused(
            syndic_storage::DraftMarkerAdmissionTerminalRefusalV1::Stale
        )
    ));
    assert_advanced(storage.advance_draft_marker_admission_cleanup(
        &reopened,
        admission,
        command(66),
    ));
    terminal_cleanup_snapshot(&storage, &reopened, admission);
}

#[test]
fn exact_compact_terminal_closure_transfers_final_charge_to_settlement_once() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("phase224-settlement-release", 70, faults.clone());
    let (session, marker) = marked_session(&storage, &store, thread, 71);
    let admission = owner(&session, 72);
    let terminal_command = command(73);
    submit_page(
        &storage,
        &store,
        admission,
        74,
        false,
        vec![association(75, &session, marker.marker_id())],
    );
    assert_advanced(storage.cancel_draft_marker_admission(&store, admission, terminal_command));
    assert!(
        storage
            .transfer_draft_marker_admission_terminal_to_settlement_for_test(
                &store,
                admission,
                terminal_command,
            )
            .is_err()
    );

    for cleanup_command in [76, 77, 78, 79] {
        match storage.advance_draft_marker_admission_cleanup(
            &store,
            admission,
            command(cleanup_command),
        ) {
            DraftMarkerAdmissionTerminalOutcomeV1::Advanced { .. } => {}
            DraftMarkerAdmissionTerminalOutcomeV1::RetainedClosure => break,
            _ => panic!("terminal cleanup did not reach compact settlement closure"),
        }
    }
    let compact = snapshot(&storage, &store, admission);
    let exact_final_charge = compact.head().unwrap().charge();
    assert_eq!(exact_final_charge.associations(), 0);
    assert!(compact.receipt().is_some());
    assert!(
        storage
            .transfer_draft_marker_admission_terminal_to_settlement_for_test(
                &store,
                admission,
                command(80),
            )
            .is_err()
    );

    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let recovery = store.recover_same_home().unwrap();
    let fresh_storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let reopened = recovery.publish();
    assert!(
        storage
            .transfer_draft_marker_admission_terminal_to_settlement_for_test(
                &reopened,
                admission,
                terminal_command,
            )
            .is_err()
    );
    assert!(matches!(
        fresh_storage.transfer_draft_marker_admission_terminal_to_settlement_for_test(
            &reopened,
            admission,
            terminal_command,
        ),
        Ok(beryl_home_store::CommandOutcome::Committed { .. })
    ));

    let released = snapshot(&fresh_storage, &reopened, admission);
    assert!(released.head().is_none());
    assert!(released.receipt().is_none());
    let aggregate = released.capacity().unwrap().charge();
    assert_eq!(aggregate.heads(), 0);
    assert_eq!(aggregate.associations(), 0);
    assert_eq!(aggregate.encoded_bytes(), 0);
    assert!(
        fresh_storage
            .transfer_draft_marker_admission_terminal_to_settlement_for_test(
                &reopened,
                admission,
                terminal_command,
            )
            .is_err()
    );
}
