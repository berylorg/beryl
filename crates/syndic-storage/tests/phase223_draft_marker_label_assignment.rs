#![cfg(feature = "test-faults")]

include!("phase154_durable_builder/support.rs");

use std::num::NonZeroU64;

use sha2::{Digest, Sha256};
use syndic_storage::{
    DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionLimitsV1,
    DraftMarkerAdmissionOperationIdV1, DraftMarkerAdmissionOwnerV1,
    DraftMarkerLabelAssignmentOutcomeV1, DraftMarkerLabelReadinessDispositionV1,
    DraftMarkerLabelReadinessPageRequestV1, DraftMarkerLabelReadinessProofV1,
    DraftMarkerReadinessCandidateSourceV1, DraftMarkerReadinessSourceAssociationV1,
    DraftMarkerReadinessSourceSelectorV1, DraftPieceRootBuildIdentityV1, DraftPieceRootReferenceV1,
    DraftPieceSettlementKeyV1,
};

#[path = "phase216_draft_marker_readiness_source_proof/support.rs"]
mod readiness_support;

use readiness_support::{association, owner, two_marked_session};

#[test]
fn allocate_assigns_least_source_order_and_reuses_equal_label_asset() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("phase223-allocate", 1, faults.clone());
    let (session, first, second) = two_marked_session(&storage, &store, thread, 2);
    let operation = owner(&session, 10);
    let proof = eof_then_assign(
        &storage,
        &store,
        operation,
        20,
        DraftMarkerLabelReadinessDispositionV1::Allocate,
        vec![
            association(30, &session, first.marker_id()),
            association(31, &session, second.marker_id()),
            association(32, &session, first.marker_id()),
        ],
    );

    let range = proof
        .allocation_range()
        .expect("allocation proof retains its package-derived range");
    assert!(range.first().get() > proof.protection().protected_maximum().get());
    assert_eq!(range.count(), 3);
    let associations = storage
        .inspect_draft_marker_label_readiness_proof_for_test(&store, &proof)
        .unwrap();
    assert_eq!(associations.len(), 3);
    let first_assigned = assigned_label(&associations, 30);
    let second_assigned = assigned_label(&associations, 31);
    let repeated_assigned = assigned_label(&associations, 32);
    assert_eq!(repeated_assigned, first_assigned);
    assert_eq!(first_assigned, range.first());
    assert_eq!(second_assigned, range.first().checked_next().unwrap());
    assert_eq!(proof.owner(), operation);
    assert_eq!(proof.assigned_target_root().count(), 3);
    let ready = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
        .unwrap();
    let head = ready.head().unwrap();
    assert_eq!(
        head.lifecycle(),
        syndic_storage::DraftMarkerAdmissionLifecycleV1::Ready
    );
    assert_eq!(head.source_root().count(), 0);
    assert!(head.source_root().node().is_none());
    assert_eq!(head.unassigned_count(), 0);
    assert!(ready.receipt().is_some());

    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let recovery = store.recover_same_home().unwrap();
    let reopened_storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let reopened = recovery.publish();
    assert!(
        reopened_storage
            .inspect_draft_marker_label_readiness_proof_for_test(&reopened, &proof)
            .is_err()
    );
}

#[test]
fn reuse_preserves_source_labels_and_proof_moves_once_ready() {
    let (_home, store, storage, thread) = fixture("phase223-reuse-retirement", 40);
    let (session, first, second) = two_marked_session(&storage, &store, thread, 41);
    let operation = owner(&session, 42);
    let proof = eof_then_assign(
        &storage,
        &store,
        operation,
        50,
        DraftMarkerLabelReadinessDispositionV1::Reuse,
        vec![
            association(51, &session, first.marker_id()),
            association(52, &session, second.marker_id()),
        ],
    );
    assert!(proof.allocation_range().is_none());
    let assigned = storage
        .inspect_draft_marker_label_readiness_proof_for_test(&store, &proof)
        .unwrap();
    assert_eq!(assigned_label(&assigned, 51), first.label());
    assert_eq!(assigned_label(&assigned, 52), second.label());

    let moved = proof;
    assert_eq!(moved.owner(), operation);
    assert_eq!(
        storage
            .inspect_draft_marker_label_readiness_proof_for_test(&store, &moved)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn concurrent_allocate_operations_reserve_disjoint_destination_ranges() {
    let (_home, store, storage, thread) = fixture("phase223-concurrent-allocation", 60);
    let (session, marker) = readiness_support::marked_session(&storage, &store, thread, 61);
    let first_operation = owner(&session, 62);
    let second_operation = owner(&session, 63);
    eof_for_assignment(
        &storage,
        &store,
        first_operation,
        64,
        DraftMarkerLabelReadinessDispositionV1::Allocate,
        association(65, &session, marker.marker_id()),
    );
    eof_for_assignment(
        &storage,
        &store,
        second_operation,
        66,
        DraftMarkerLabelReadinessDispositionV1::Allocate,
        association(67, &session, marker.marker_id()),
    );

    let first_flight = storage
        .prepare_draft_marker_label_assignment(
            &store,
            first_operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([68; 16]),
        )
        .unwrap();
    let second_flight = storage
        .prepare_draft_marker_label_assignment(
            &store,
            second_operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([69; 16]),
        )
        .unwrap();
    let first = match storage.submit_draft_marker_label_assignment(&store, first_flight) {
        DraftMarkerLabelAssignmentOutcomeV1::Ready { proof, .. } => proof,
        _ => panic!("first live Allocate operation did not become ready"),
    };
    let second = match storage.submit_draft_marker_label_assignment(&store, second_flight) {
        DraftMarkerLabelAssignmentOutcomeV1::Ready { proof, .. } => proof,
        _ => panic!("second live Allocate operation did not become ready"),
    };
    let first_range = first.allocation_range().unwrap();
    let second_range = second.allocation_range().unwrap();
    assert!(first_range.first().get() > first.protection().protected_maximum().get());
    assert!(second_range.first().get() > second.protection().protected_maximum().get());
    assert!(second_range.first() > first_range.last());
}

#[test]
fn allocation_exhaustion_refuses_before_eof_creates_assignment_custody() {
    let (_home, store, storage, thread) = fixture("phase223-allocation-exhaustion", 70);
    let (session, marker) = readiness_support::marked_session(&storage, &store, thread, 71);
    let operation = owner(&session, 72);
    storage
        .seed_draft_marker_label_allocation_frontier_for_test(
            &store,
            thread,
            ImageLabelOrdinal::new(u64::MAX).unwrap(),
        )
        .unwrap();
    assert!(
        storage
            .prepare_draft_marker_label_readiness_page(
                &store,
                DraftMarkerLabelReadinessPageRequestV1::new(
                    operation,
                    DraftMarkerAdmissionCommandIdV1::from_bytes([73; 16]),
                    NonZeroU64::MIN,
                    true,
                    DraftMarkerLabelReadinessDispositionV1::Allocate,
                    Box::new([association(74, &session, marker.marker_id())]),
                    None,
                ),
            )
            .is_err()
    );
    let snapshot = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
        .unwrap();
    assert!(snapshot.head().is_none());
    assert!(snapshot.capacity().is_none());
}

#[test]
fn assignment_retained_and_command_limits_refuse_without_consuming_the_source_leaf() {
    let (_home, store, storage, thread) = fixture("phase223-assignment-limits", 80);
    let (session, marker) = readiness_support::marked_session(&storage, &store, thread, 81);
    let operation = owner(&session, 82);
    eof_for_assignment(
        &storage,
        &store,
        operation,
        83,
        DraftMarkerLabelReadinessDispositionV1::Reuse,
        association(84, &session, marker.marker_id()),
    );
    let before = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
        .unwrap();
    let head = before.head().unwrap();
    let revision = storage.revision(&store).unwrap();

    for (command, limits, command_limit) in [
        (85, DraftMarkerAdmissionLimitsV1::new(64, 0, 0), u64::MAX),
        (86, DraftMarkerAdmissionLimitsV1::PRODUCTION, 1),
    ] {
        let flight = storage
            .prepare_draft_marker_label_assignment_with_limits_for_test(
                &store,
                operation,
                DraftMarkerAdmissionCommandIdV1::from_bytes([command; 16]),
                limits,
                command_limit,
            )
            .unwrap();
        assert!(matches!(
            storage.submit_draft_marker_label_assignment(&store, flight),
            DraftMarkerLabelAssignmentOutcomeV1::Refused(_)
        ));
        let after = storage
            .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
            .unwrap();
        assert_eq!(storage.revision(&store).unwrap(), revision);
        assert_eq!(after.head().unwrap().digest(), head.digest());
    }
}

#[test]
fn assignment_authority_read_bytes_are_preflighted_before_mutation() {
    let (_home, store, storage, thread) = fixture("phase223-assignment-authority-bytes", 96);
    let (session, marker) = readiness_support::marked_session(&storage, &store, thread, 97);
    let operation = owner(&session, 98);
    eof_for_assignment(
        &storage,
        &store,
        operation,
        99,
        DraftMarkerLabelReadinessDispositionV1::Reuse,
        association(100, &session, marker.marker_id()),
    );
    let before = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
        .unwrap();
    let head = before.head().unwrap();
    let revision = storage.revision(&store).unwrap();
    let flight = storage
        .prepare_draft_marker_label_assignment_at_pre_authority_read_ceiling_for_test(
            &store,
            operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([101; 16]),
        )
        .unwrap();

    assert!(matches!(
        storage.submit_draft_marker_label_assignment(&store, flight),
        DraftMarkerLabelAssignmentOutcomeV1::Refused(
            syndic_storage::DraftMarkerLabelAssignmentRefusalV1::Unavailable
        )
    ));
    let after = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
        .unwrap();
    let after_head = after.head().unwrap();
    assert_eq!(storage.revision(&store).unwrap(), revision);
    assert_eq!(after_head.digest(), head.digest());
    assert_eq!(after_head.source_root(), head.source_root());
    assert_eq!(after_head.target_root(), head.target_root());
}

#[test]
fn equal_label_different_asset_refuses_without_advancing_the_rejecting_quantum() {
    let (_home, store, storage, thread) = fixture("phase223-label-asset-disagreement", 87);
    let (session, first, second) =
        equal_label_different_asset_session(&storage, &store, thread, 88);
    assert_eq!(first.label(), second.label());
    assert_ne!(first.asset_id(), second.asset_id());
    let operation = owner(&session, 89);
    let associations = vec![
        association(90, &session, first.marker_id()),
        association(91, &session, second.marker_id()),
    ];
    for _ in 0..associations.len() {
        let mut attempt = storage
            .prepare_draft_marker_label_readiness_page(
                &store,
                DraftMarkerLabelReadinessPageRequestV1::new(
                    operation,
                    DraftMarkerAdmissionCommandIdV1::from_bytes([92; 16]),
                    NonZeroU64::MIN,
                    true,
                    DraftMarkerLabelReadinessDispositionV1::Allocate,
                    associations.clone().into_boxed_slice(),
                    None,
                ),
            )
            .unwrap();
        let receipt = store
            .compose_proof(attempt.take_command().unwrap())
            .unwrap();
        let flight = attempt.into_submission_flight(&store, receipt).unwrap();
        assert!(matches!(
            storage.submit_draft_marker_label_readiness_page(&store, flight),
            syndic_storage::DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Advanced { .. }
        ));
    }

    let first_flight = storage
        .prepare_draft_marker_label_assignment(
            &store,
            operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([93; 16]),
        )
        .unwrap();
    assert!(matches!(
        storage.submit_draft_marker_label_assignment(&store, first_flight),
        DraftMarkerLabelAssignmentOutcomeV1::Advanced { .. }
    ));
    let before = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
        .unwrap();
    let before_head = before.head().unwrap();
    let revision = storage.revision(&store).unwrap();

    let rejecting_flight = storage
        .prepare_draft_marker_label_assignment(
            &store,
            operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([94; 16]),
        )
        .unwrap();
    assert!(matches!(
        storage.submit_draft_marker_label_assignment(&store, rejecting_flight),
        DraftMarkerLabelAssignmentOutcomeV1::Refused(
            syndic_storage::DraftMarkerLabelAssignmentRefusalV1::Rejected
        )
    ));
    let after = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
        .unwrap();
    assert_eq!(storage.revision(&store).unwrap(), revision);
    assert_eq!(after.head().unwrap().digest(), before_head.digest());
    assert!(matches!(
        storage.prepare_draft_marker_label_assignment(
            &store,
            operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([95; 16]),
        ),
        Err(syndic_storage::DraftMarkerLabelAssignmentErrorV1::Rejected)
    ));
}

#[test]
fn indeterminate_assignment_exact_new_issues_ready_once() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("phase223-assignment-exact-new", 90, faults.clone());
    let (session, marker) = readiness_support::marked_session(&storage, &store, thread, 91);
    let operation = owner(&session, 92);
    eof_for_assignment(
        &storage,
        &store,
        operation,
        93,
        DraftMarkerLabelReadinessDispositionV1::Reuse,
        association(94, &session, marker.marker_id()),
    );
    let flight = storage
        .prepare_draft_marker_label_assignment(
            &store,
            operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([95; 16]),
        )
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let pending = match storage.submit_draft_marker_label_assignment(&store, flight) {
        DraftMarkerLabelAssignmentOutcomeV1::ReconciliationPending(flight) => flight,
        _ => panic!("indeterminate assignment did not retain exact reconciliation custody"),
    };
    let proof = match storage.submit_draft_marker_label_assignment(&store, pending) {
        DraftMarkerLabelAssignmentOutcomeV1::Ready { proof, .. } => proof,
        _ => panic!("exact-new assignment reconciliation did not issue readiness"),
    };
    assert_eq!(proof.owner(), operation);
    let ready = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
        .unwrap();
    assert_eq!(
        ready.head().unwrap().lifecycle(),
        syndic_storage::DraftMarkerAdmissionLifecycleV1::Ready
    );
}

#[test]
fn retired_generation_pending_assignment_cannot_mint_readiness() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("phase223-assignment-retired", 100, faults.clone());
    let (session, marker) = readiness_support::marked_session(&storage, &store, thread, 101);
    let operation = owner(&session, 102);
    eof_for_assignment(
        &storage,
        &store,
        operation,
        103,
        DraftMarkerLabelReadinessDispositionV1::Reuse,
        association(104, &session, marker.marker_id()),
    );
    let flight = storage
        .prepare_draft_marker_label_assignment(
            &store,
            operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([105; 16]),
        )
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let pending = match storage.submit_draft_marker_label_assignment(&store, flight) {
        DraftMarkerLabelAssignmentOutcomeV1::ReconciliationPending(flight) => flight,
        _ => panic!("indeterminate assignment did not retain exact reconciliation custody"),
    };
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let recovery = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish();
    match storage.submit_draft_marker_label_assignment(&store, pending) {
        DraftMarkerLabelAssignmentOutcomeV1::Ready { .. } => {
            panic!("retired-generation flight minted readiness")
        }
        _ => {}
    }
}

fn eof_then_assign(
    storage: &SyndicStorage,
    store: &HomeStore,
    owner: syndic_storage::DraftMarkerAdmissionOwnerV1,
    command_seed: u8,
    disposition: DraftMarkerLabelReadinessDispositionV1,
    associations: Vec<DraftMarkerReadinessSourceAssociationV1>,
) -> DraftMarkerLabelReadinessProofV1 {
    let association_count = u8::try_from(associations.len()).unwrap();
    for _ in 0..association_count {
        let mut attempt = storage
            .prepare_draft_marker_label_readiness_page(
                store,
                DraftMarkerLabelReadinessPageRequestV1::new(
                    owner,
                    DraftMarkerAdmissionCommandIdV1::from_bytes([command_seed; 16]),
                    NonZeroU64::MIN,
                    true,
                    disposition,
                    associations.clone().into_boxed_slice(),
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

    for command in 1..=association_count {
        let flight = storage
            .prepare_draft_marker_label_assignment(
                store,
                owner,
                DraftMarkerAdmissionCommandIdV1::from_bytes(
                    [command_seed.wrapping_add(command); 16],
                ),
            )
            .unwrap();
        match storage.submit_draft_marker_label_assignment(store, flight) {
            DraftMarkerLabelAssignmentOutcomeV1::Advanced { .. } => {}
            DraftMarkerLabelAssignmentOutcomeV1::Ready { proof, .. }
                if command == association_count =>
            {
                return proof;
            }
            _ => panic!("assignment quantum {command} did not make exact progress"),
        }
    }
    panic!("assignment did not issue readiness proof after every durable occurrence was assigned")
}

fn eof_for_assignment(
    storage: &SyndicStorage,
    store: &HomeStore,
    owner: DraftMarkerAdmissionOwnerV1,
    command: u8,
    disposition: DraftMarkerLabelReadinessDispositionV1,
    association: DraftMarkerReadinessSourceAssociationV1,
) {
    let mut attempt = storage
        .prepare_draft_marker_label_readiness_page(
            store,
            DraftMarkerLabelReadinessPageRequestV1::new(
                owner,
                DraftMarkerAdmissionCommandIdV1::from_bytes([command; 16]),
                NonZeroU64::MIN,
                true,
                disposition,
                Box::new([association]),
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

fn equal_label_different_asset_session(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    seed: u8,
) -> (
    DraftEditorCandidateSessionV1,
    DraftPieceMarkerV1,
    DraftPieceMarkerV1,
) {
    let (session, first) = readiness_support::marked_session(storage, store, thread, seed);
    let second = marker(seed.wrapping_add(10), 0, first.label().get());
    let before_all = DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll);
    let session = readiness_support::complete_marker_edit(
        storage,
        store,
        &session,
        seed.wrapping_add(11),
        DraftPieceReplacementV1::new(before_all, before_all, vec![DraftPieceV1::Marker(second)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    second,
                    DraftPieceMarkerEffectChargesV1::for_marker(second),
                ),
            )),
    );
    (session, first, second)
}

fn assigned_label(
    associations: &[syndic_storage::DraftMarkerAssignedAssociationV1],
    target: u8,
) -> ImageLabelOrdinal {
    associations
        .iter()
        .find(|association| {
            association.target_marker_id() == SyndicDraftMarkerId::from_bytes([target; 16])
        })
        .unwrap()
        .assigned_label()
}
