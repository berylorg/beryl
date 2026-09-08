use super::*;

pub(super) fn admit_one_for_assignment(
    storage: &SyndicStorage,
    store: &HomeStore,
    operation: DraftMarkerAdmissionOwnerV1,
    command: u8,
    association: DraftMarkerReadinessSourceAssociationV1,
) {
    admit_page_for_assignment(
        storage,
        store,
        operation,
        command,
        true,
        Box::new([association]),
    );
}

pub(super) fn admit_page_for_assignment(
    storage: &SyndicStorage,
    store: &HomeStore,
    operation: DraftMarkerAdmissionOwnerV1,
    command: u8,
    eof: bool,
    associations: Box<[DraftMarkerReadinessSourceAssociationV1]>,
) {
    admit_page_for_assignment_at(
        storage,
        store,
        operation,
        command,
        NonZeroU64::MIN,
        eof,
        associations,
    );
}

pub(super) fn admit_page_for_assignment_at(
    storage: &SyndicStorage,
    store: &HomeStore,
    operation: DraftMarkerAdmissionOwnerV1,
    command: u8,
    ordinal: NonZeroU64,
    eof: bool,
    associations: Box<[DraftMarkerReadinessSourceAssociationV1]>,
) {
    let mut attempt = storage
        .prepare_draft_marker_label_readiness_page(
            store,
            DraftMarkerLabelReadinessPageRequestV1::new(
                operation,
                DraftMarkerAdmissionCommandIdV1::from_bytes([command; 16]),
                ordinal,
                eof,
                DraftMarkerLabelReadinessDispositionV1::Reuse,
                associations,
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

#[test]
pub(super) fn eof_ingestion_receipt_rejects_a_live_offpath_target_predecessor_before_assignment() {
    let (_home, store, storage, thread) = fixture("ingestion-retained-target", 42);
    let (session, source) = marked_session(&storage, &store, thread, 43);
    let operation = owner(&session, 44);
    let associations = (0..129)
        .map(|index| {
            DraftMarkerReadinessSourceAssociationV1::new(
                marker_id(30_000 + index),
                readiness_support::source(&session, source.marker_id()),
            )
        })
        .collect::<Vec<_>>();
    for (index, association) in associations.iter().take(128).cloned().enumerate() {
        admit_page_for_assignment_at(
            &storage,
            &store,
            operation,
            u8::try_from(45 + index).unwrap(),
            NonZeroU64::new(u64::try_from(index + 1).unwrap()).unwrap(),
            false,
            Box::new([association]),
        );
    }
    admit_page_for_assignment_at(
        &storage,
        &store,
        operation,
        200,
        NonZeroU64::new(129).unwrap(),
        true,
        vec![associations[128].clone()].into_boxed_slice(),
    );

    let before = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
        .unwrap();
    let head = before.head().unwrap().clone();
    let receipt = before.receipt().unwrap().clone();
    assert_eq!(
        receipt.transition(),
        DraftMarkerAdmissionReceiptTransitionV1::Ingestion
    );
    assert_eq!(head.source_root().count(), 129);
    let retained_keys = receipt
        .retained_predecessor_nodes()
        .iter()
        .map(|child| child.key())
        .collect::<Vec<_>>();
    let retained_snapshot = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation, &retained_keys)
        .unwrap();
    let current_target = storage
        .draft_marker_admission_publication_snapshot_for_test(
            &store,
            operation,
            &[head.target_root().node().unwrap()],
        )
        .unwrap();
    let root_node = current_target.nodes()[0].as_ref().unwrap();
    let DraftMarkerAdmissionNodePayloadV1::Internal { children, .. } = root_node.payload() else {
        panic!("expected a branching current target root");
    };
    assert!(children.len() >= 2);
    let live_right = *children.last().unwrap();
    assert!(
        !receipt
            .retained_predecessor_nodes()
            .iter()
            .any(|retained| retained.key() == live_right.key())
    );
    let live_right_snapshot = storage
        .draft_marker_admission_publication_snapshot_for_test(
            &store,
            operation,
            &[live_right.key()],
        )
        .unwrap();
    let live_right_node = live_right_snapshot.nodes()[0].as_ref().unwrap();
    assert_eq!(live_right_node.tree(), DraftMarkerAdmissionTreeV1::TargetId);
    assert_eq!(child(live_right_node), live_right);
    let target_position = retained_snapshot
        .nodes()
        .iter()
        .zip(receipt.retained_predecessor_nodes())
        .position(|(node, _)| {
            node.as_ref()
                .is_some_and(|node| node.tree() == DraftMarkerAdmissionTreeV1::TargetId)
        })
        .unwrap();
    let mut retained = receipt.retained_predecessor_nodes().to_vec();
    retained[target_position] = live_right;
    let forged = DraftMarkerAdmissionReplayReceiptV1::new(
        receipt.owner(),
        receipt.command_id(),
        receipt.page_ordinal(),
        receipt.request_commitment(),
        receipt.source_head_bytes(),
        receipt.target_head_bytes(),
        receipt.source_before(),
        receipt.source_after(),
        receipt.target_before(),
        receipt.target_after(),
        retained.into_boxed_slice(),
        DraftMarkerAdmissionReceiptTransitionV1::Ingestion,
    )
    .unwrap();
    assert_ne!(forged.digest(), receipt.digest());
    let mut nodes = retained_snapshot
        .nodes()
        .iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    nodes.push(live_right_node.clone());
    let revision = storage.revision(&store).unwrap();
    committed(execute(
        &store,
        draft_marker_admission_fixture_contribution(
            &storage,
            revision,
            DraftMarkerAdmissionFixtureSnapshotV1::new(
                before.capacity().unwrap().clone(),
                vec![head.clone()],
                nodes,
                vec![forged.clone()],
            ),
        ),
    ));
    let injected = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
        .unwrap();
    assert_eq!(injected.receipt(), Some(&forged));

    let flight = storage
        .prepare_draft_marker_label_assignment(
            &store,
            operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([47; 16]),
        )
        .unwrap();
    assert!(matches!(
        storage.submit_draft_marker_label_assignment(&store, flight),
        DraftMarkerLabelAssignmentOutcomeV1::Refused(_)
    ));
    let after = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
        .unwrap();
    assert_eq!(after.capacity(), before.capacity());
    assert_eq!(after.head(), Some(&head));
    assert_eq!(after.receipt(), Some(&forged));
}

#[test]
pub(super) fn complete_assignment_ledger_includes_outer_preparation_serialization_and_readiness() {
    let (_home, store, storage, thread) = fixture("deletion-ledger", 50);
    let (session, source) = marked_session(&storage, &store, thread, 51);
    let operation = owner(&session, 52);
    admit_one_for_assignment(
        &storage,
        &store,
        operation,
        53,
        association(54, &session, source.marker_id()),
    );
    reset_home_store_syndic_point_acquisition_count();
    let flight = storage
        .prepare_draft_marker_label_assignment(
            &store,
            operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([55; 16]),
        )
        .unwrap();
    let diagnostics = flight.work_diagnostics_for_test();
    assert_eq!(
        home_store_syndic_point_acquisition_count(),
        diagnostics.snapshot().point_attempts
    );
    assert!(matches!(
        storage.submit_draft_marker_label_assignment(&store, flight),
        DraftMarkerLabelAssignmentOutcomeV1::Ready { .. }
    ));
    let measured = diagnostics.snapshot();
    assert_eq!(
        home_store_syndic_point_acquisition_count(),
        measured.point_attempts
    );
    assert!(measured.point_attempts > 0);
    assert!(measured.stored_node_acquisitions > 0);
    assert!(measured.stored_node_emissions > 0);
    assert!(measured.point_attempts <= 153);
    assert!(measured.stored_node_acquisitions + measured.stored_node_emissions <= 140);
    assert!(measured.encoded_bytes <= 3_903_801);
    assert!(measured.peak_reserved_bytes <= 3_969_402);
}

#[test]
pub(super) fn pre_read_budget_rejection_has_no_serialized_work_or_mutation() {
    let (_home, store, storage, thread) = fixture("deletion-pre-read", 60);
    let (session, source) = marked_session(&storage, &store, thread, 61);
    let operation = owner(&session, 62);
    admit_one_for_assignment(
        &storage,
        &store,
        operation,
        63,
        association(64, &session, source.marker_id()),
    );
    let before = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
        .unwrap();
    let revision = storage.revision(&store).unwrap();
    let flight = storage
        .prepare_draft_marker_label_assignment_at_pre_authority_read_ceiling_for_test(
            &store,
            operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([65; 16]),
        )
        .unwrap();
    let diagnostics = flight.work_diagnostics_for_test();
    let prepared = diagnostics.snapshot();
    reset_home_store_syndic_point_acquisition_count();
    assert_eq!(home_store_syndic_point_acquisition_count(), 0);
    assert!(matches!(
        storage.submit_draft_marker_label_assignment(&store, flight),
        DraftMarkerLabelAssignmentOutcomeV1::Refused(_)
    ));
    assert_eq!(home_store_syndic_point_acquisition_count(), 0);
    assert_eq!(diagnostics.snapshot(), prepared);
    let after = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
        .unwrap();
    assert_eq!(storage.revision(&store).unwrap(), revision);
    assert_eq!(after.head(), before.head());
    assert_eq!(after.receipt(), before.receipt());
}

#[test]
pub(super) fn acknowledgement_loss_reconciles_the_captured_assignment_without_stateless_replay() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("deletion-captured-reconciliation", 70, faults.clone());
    let (session, source) = marked_session(&storage, &store, thread, 71);
    let operation = owner(&session, 72);
    admit_one_for_assignment(
        &storage,
        &store,
        operation,
        73,
        association(74, &session, source.marker_id()),
    );
    let command = DraftMarkerAdmissionCommandIdV1::from_bytes([75; 16]);
    let flight = storage
        .prepare_draft_marker_label_assignment(&store, operation, command)
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let pending = match storage.submit_draft_marker_label_assignment(&store, flight) {
        DraftMarkerLabelAssignmentOutcomeV1::ReconciliationPending(flight) => flight,
        _ => panic!("assignment did not retain captured reconciliation custody"),
    };
    let proof = match storage.submit_draft_marker_label_assignment(&store, pending) {
        DraftMarkerLabelAssignmentOutcomeV1::Ready { proof, .. } => proof,
        _ => panic!("captured reconciliation did not classify the committed assignment"),
    };
    assert_eq!(proof.owner(), operation);
    assert!(matches!(
        storage.prepare_draft_marker_label_assignment(&store, operation, command),
        Err(syndic_storage::DraftMarkerLabelAssignmentErrorV1::Rejected)
    ));
}
