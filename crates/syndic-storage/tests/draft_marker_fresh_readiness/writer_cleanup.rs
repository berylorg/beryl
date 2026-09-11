use super::*;
use syndic_storage::test_faults::{
    DraftMarkerAdmissionFixtureSnapshotV1, draft_marker_admission_fixture_contribution,
};
use syndic_storage::{
    DraftMarkerAdmissionLifecycleV1, DraftMarkerAdmissionNodeKeyV1,
    DraftMarkerAdmissionNodePayloadV1, DraftMarkerAdmissionPublicationSnapshotV1,
    DraftMarkerAdmissionReplayReceiptV1, DraftMarkerAdmissionTargetDispositionV1,
    DraftMarkerAdmissionTerminalOutcomeV1, DraftMutationStagingErrorV1,
    DraftMutationStagingLifecycleV1, DraftMutationStagingReconcileV1,
    DraftMutationStagingTerminalEvidenceV1, draft_marker_admission_head_encoded_charge_v1,
    draft_marker_admission_node_encoded_charge_v1,
    draft_marker_admission_receipt_encoded_charge_v1,
};

fn ready(
    fixture: &mut AcceptedFixture,
) -> (
    DraftMarkerAdmissionOwnerV1,
    DraftMarkerLabelReadinessProofV1,
) {
    fixture.session = complete_staged(
        &fixture.storage,
        &fixture.store,
        &fixture.session,
        210,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".to_owned())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    let operation = owner(&fixture.session, 211);
    ingest(
        fixture,
        operation,
        212,
        1,
        true,
        vec![fresh(213, fixture.asset_id)],
        || Some(fresh_factory(fixture)),
    );
    (operation, assign(fixture, operation, 214, 1))
}

fn retained_snapshot(
    fixture: &AcceptedFixture,
    operation: DraftMarkerAdmissionOwnerV1,
) -> (
    Vec<DraftMarkerAdmissionNodeKeyV1>,
    DraftMarkerAdmissionPublicationSnapshotV1,
) {
    let selected = writer_support::snapshot(&fixture.storage, &fixture.store, operation);
    let mut keys: Vec<_> = selected
        .receipt()
        .unwrap()
        .retained_predecessor_nodes()
        .iter()
        .map(|node| node.key())
        .collect();
    assert_eq!(keys.len(), 2);
    keys.push(selected.head().unwrap().target_root().node().unwrap());
    let snapshot = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &keys)
        .unwrap();
    assert!(snapshot.nodes().iter().all(Option::is_some));
    (keys, snapshot)
}

fn assert_handoff(
    before: &DraftMarkerAdmissionPublicationSnapshotV1,
    after: &DraftMarkerAdmissionPublicationSnapshotV1,
) {
    let old = before.head().unwrap();
    let new = after.head().unwrap();
    let physical_before = draft_marker_admission_head_encoded_charge_v1(old).unwrap()
        + draft_marker_admission_receipt_encoded_charge_v1(before.receipt().unwrap()).unwrap()
        + before
            .nodes()
            .iter()
            .flatten()
            .map(|node| draft_marker_admission_node_encoded_charge_v1(node).unwrap())
            .sum::<u64>();
    assert_eq!(old.charge().encoded_bytes(), physical_before);
    let mut deleted_bytes = 0;
    let mut deleted_count = 0;
    for (prior, next) in before.nodes().iter().zip(after.nodes()) {
        let prior = prior.as_ref().unwrap();
        if before
            .receipt()
            .unwrap()
            .retained_predecessor_nodes()
            .iter()
            .any(|node| node.key() == prior.key())
        {
            assert!(next.is_none());
            deleted_bytes += draft_marker_admission_node_encoded_charge_v1(prior).unwrap();
            deleted_count += 1;
        } else {
            assert_eq!(next.as_ref(), Some(prior));
        }
    }
    assert_eq!(deleted_count, 2);
    assert!(matches!(
        after.nodes().last().unwrap().as_ref().unwrap().payload(),
        DraftMarkerAdmissionNodePayloadV1::TargetLeaf {
            disposition: DraftMarkerAdmissionTargetDispositionV1::Assigned(_),
            ..
        }
    ));
    assert_eq!(new.lifecycle(), DraftMarkerAdmissionLifecycleV1::Staging);
    assert_eq!(new.revision().get(), old.revision().get() + 1);
    assert_eq!(
        after.capacity().unwrap().revision().get(),
        before.capacity().unwrap().revision().get() + 1
    );
    assert_eq!(new.target_root(), old.target_root());
    assert_eq!(new.remaining_builder_count(), 1);
    assert_eq!(new.charge().heads(), old.charge().heads());
    assert_eq!(new.charge().associations(), 1);
    let expected = old.charge().encoded_bytes()
        - draft_marker_admission_head_encoded_charge_v1(old).unwrap()
        - draft_marker_admission_receipt_encoded_charge_v1(before.receipt().unwrap()).unwrap()
        - deleted_bytes
        + draft_marker_admission_head_encoded_charge_v1(new).unwrap();
    assert_eq!(new.charge().encoded_bytes(), expected);
    let physical_after = draft_marker_admission_head_encoded_charge_v1(new).unwrap()
        + after
            .nodes()
            .iter()
            .flatten()
            .map(|node| draft_marker_admission_node_encoded_charge_v1(node).unwrap())
            .sum::<u64>();
    assert_eq!(new.charge().encoded_bytes(), physical_after);
    assert_eq!(after.capacity().unwrap().charge(), new.charge());
    assert_eq!(before.capacity().unwrap().charge(), old.charge());
    assert!(after.receipt().is_none());
}

fn cleanup(
    fixture: &AcceptedFixture,
    operation: DraftMarkerAdmissionOwnerV1,
    keys: &[DraftMarkerAdmissionNodeKeyV1],
) {
    let mut closed = false;
    for command in 220..228 {
        match fixture.storage.advance_draft_marker_admission_cleanup(
            &fixture.store,
            operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([command; 16]),
        ) {
            DraftMarkerAdmissionTerminalOutcomeV1::Advanced { .. } => {}
            DraftMarkerAdmissionTerminalOutcomeV1::RetainedClosure => {
                closed = true;
                break;
            }
            _ => panic!("fresh admission cleanup did not advance"),
        }
    }
    assert!(closed);
    let compact = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, keys)
        .unwrap();
    let head = compact.head().unwrap();
    assert_eq!(head.charge().associations(), 0);
    assert_eq!(head.remaining_builder_count(), 0);
    assert_eq!(head.source_root().count(), 0);
    assert_eq!(head.target_root().count(), 0);
    assert!(compact.nodes().iter().all(Option::is_none));
    let metadata = draft_marker_admission_head_encoded_charge_v1(head).unwrap()
        + compact
            .receipt()
            .map(|receipt| draft_marker_admission_receipt_encoded_charge_v1(receipt).unwrap())
            .unwrap_or(0);
    assert_eq!(head.charge().encoded_bytes(), metadata);
    assert_eq!(compact.capacity().unwrap().charge(), head.charge());
}

#[test]
fn fresh_writer_begin_reclaims_exact_shadow_bytes_and_cancellation_before_build_finishes_cleanup() {
    let mut fixture = AcceptedFixture::new("fresh-writer-cleanup", 140);
    let (operation, proof) = ready(&mut fixture);
    let (keys, before) = retained_snapshot(&fixture, operation);
    let (identity, active, staging) = writer_support::begin_admitted_marker_edit(
        &fixture.storage,
        &fixture.store,
        &fixture.session,
        operation,
        proof,
    );
    let after = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &keys)
        .unwrap();
    assert_handoff(&before, &after);
    assert!(
        fixture
            .storage
            .draft_marker_admission_receipt_for_test(
                &fixture.store,
                operation,
                before.receipt().unwrap().command_id()
            )
            .unwrap()
            .is_none()
    );
    let terminal = fixture
        .storage
        .prepare_draft_mutation_staging_terminal(
            &staging,
            &active,
            DraftMutationStagingTerminalEvidenceV1::Cancelled {
                request_id: identity.operation_id(),
                source_lifecycle: DraftMutationStagingLifecycleV1::Receiving,
                writer_admitted: true,
                candidate_generation: active.newest_candidate_generation(),
                root: active.newest_root(),
                history: active.newest_history(),
                session_revision: active.session_generation(),
            },
        )
        .unwrap();
    committed(execute(
        &fixture.store,
        fixture.storage.draft_mutation_staging_command(
            fixture.storage.revision(&fixture.store).unwrap(),
            terminal,
        ),
    ));
    assert!(matches!(
        fixture
            .storage
            .draft_mutation_staging_status(&fixture.store, identity)
            .unwrap(),
        DraftMutationStagingStatusV1::Cancelled { .. }
    ));
    cleanup(&fixture, operation, &keys);
}

#[test]
fn fresh_writer_begin_acknowledgement_loss_reconciles_exact_deleted_predecessors() {
    for (index, fault) in [
        FaultPoint::AfterCommitBeforePersist,
        FaultPoint::AfterPersist,
    ]
    .into_iter()
    .enumerate()
    {
        let faults = FaultController::new();
        let mut fixture = AcceptedFixture::with_faults(
            &format!("fresh-begin-fault-{index}"),
            150 + index as u8,
            faults.clone(),
        );
        let (operation, proof) = ready(&mut fixture);
        let (keys, before) = retained_snapshot(&fixture, operation);
        let identity = DraftMutationStagingIdentityV1::new(
            fixture.session.draft_id(),
            fixture.session.session_id(),
            DraftMutationOperationIdV1::from_bytes(*operation.operation_id().as_bytes()),
        );
        let begin = fixture
            .storage
            .prepare_draft_mutation_staging_marker_begin(
                begin_input(identity, &fixture.session),
                &fixture.session,
                proof,
            )
            .unwrap();
        faults.fail_next(fault);
        let outcome = execute(
            &fixture.store,
            fixture.storage.draft_mutation_staging_command(
                fixture.storage.revision(&fixture.store).unwrap(),
                begin.clone(),
            ),
        );
        match fault {
            FaultPoint::AfterCommitBeforePersist => {
                assert!(matches!(&outcome, CommandOutcome::Indeterminate { .. }))
            }
            FaultPoint::AfterPersist => assert!(matches!(
                &outcome,
                CommandOutcome::Committed {
                    later_failure: Some(CommandError::Persistence { .. }),
                    ..
                }
            )),
            _ => unreachable!(),
        }
        if fault == FaultPoint::AfterPersist {
            assert!(matches!(
                fixture
                    .storage
                    .reconcile_draft_mutation_staging_command_outcome(
                        &fixture.store,
                        &begin,
                        outcome
                    ),
                Err(DraftMutationStagingErrorV1::Read(_))
            ));
        } else {
            if fixture.store.health().state() == HomeHealthState::Failed {
                let recovery = fixture.store.recover_same_home().unwrap();
                fixture.storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
                fixture.store = recovery.publish();
            }
            assert_eq!(
                fixture
                    .storage
                    .reconcile_draft_mutation_staging_command_outcome(
                        &fixture.store,
                        &begin,
                        outcome
                    )
                    .unwrap(),
                DraftMutationStagingReconcileV1::TargetSelected
            );
        }
        if fixture.store.health().state() == HomeHealthState::Failed {
            let recovery = fixture.store.recover_same_home().unwrap();
            fixture.storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
            fixture.store = recovery.publish();
        }
        let after = fixture
            .storage
            .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &keys)
            .unwrap();
        assert_handoff(&before, &after);
        let replay = execute(
            &fixture.store,
            fixture.storage.draft_mutation_staging_command(
                fixture.storage.revision(&fixture.store).unwrap(),
                begin.clone(),
            ),
        );
        if fault == FaultPoint::AfterPersist {
            let CommandOutcome::NotCommitted {
                evidence: CommandError::ContributorValidation { source, .. },
            } = replay
            else {
                panic!("recovered writer accepted stale begin custody: {replay:?}");
            };
            assert!(matches!(
                source.downcast_ref::<syndic_storage::SyndicMutationError>(),
                Some(syndic_storage::SyndicMutationError::IdentityCollision)
            ));
        } else {
            assert!(matches!(
                &replay,
                CommandOutcome::NotCommitted {
                    evidence: CommandError::EmptyContribution { .. }
                }
            ));
            assert_eq!(
                fixture
                    .storage
                    .reconcile_draft_mutation_staging_command_outcome(
                        &fixture.store,
                        &begin,
                        replay
                    )
                    .unwrap(),
                DraftMutationStagingReconcileV1::TargetSelected
            );
        }
        let replayed = fixture
            .storage
            .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &keys)
            .unwrap();
        assert_eq!(after.head(), replayed.head());
        assert_eq!(after.receipt(), replayed.receipt());
        assert_eq!(after.capacity(), replayed.capacity());
        assert_eq!(after.nodes(), replayed.nodes());
        assert_eq!(
            fixture
                .storage
                .draft_mutation_staging_head(&fixture.store, identity)
                .unwrap()
                .as_ref(),
            Some(begin.target_head())
        );
    }
}

#[test]
fn substituted_receipt_predecessor_order_rejects_writer_begin_and_terminalization_without_publication()
 {
    let mut fixture = AcceptedFixture::new("fresh-substituted-predecessors", 160);
    let (operation, proof) = ready(&mut fixture);
    let (keys, selected) = retained_snapshot(&fixture, operation);
    let prior = selected.receipt().unwrap();
    let mut retained = prior.retained_predecessor_nodes().to_vec();
    retained.reverse();
    let substituted = DraftMarkerAdmissionReplayReceiptV1::new(
        prior.owner(),
        prior.command_id(),
        prior.page_ordinal(),
        prior.request_commitment(),
        prior.source_head_bytes(),
        prior.target_head_bytes(),
        prior.source_before(),
        prior.source_after(),
        prior.target_before(),
        prior.target_after(),
        retained,
        prior.transition(),
    )
    .unwrap();
    assert_ne!(substituted.digest(), prior.digest());
    assert_eq!(
        draft_marker_admission_receipt_encoded_charge_v1(&substituted).unwrap(),
        draft_marker_admission_receipt_encoded_charge_v1(prior).unwrap()
    );
    let injected = DraftMarkerAdmissionFixtureSnapshotV1::new(
        selected.capacity().unwrap().clone(),
        vec![selected.head().unwrap().clone()],
        selected
            .nodes()
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>(),
        vec![substituted],
    );
    committed(execute(
        &fixture.store,
        draft_marker_admission_fixture_contribution(
            &fixture.storage,
            fixture.storage.revision(&fixture.store).unwrap(),
            injected,
        ),
    ));
    let before = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &keys)
        .unwrap();
    let revision = fixture.storage.revision(&fixture.store).unwrap();
    let identity = DraftMutationStagingIdentityV1::new(
        fixture.session.draft_id(),
        fixture.session.session_id(),
        DraftMutationOperationIdV1::from_bytes(*operation.operation_id().as_bytes()),
    );
    let begin = fixture
        .storage
        .prepare_draft_mutation_staging_marker_begin(
            begin_input(identity, &fixture.session),
            &fixture.session,
            proof,
        )
        .unwrap();
    assert!(matches!(
        execute(
            &fixture.store,
            fixture
                .storage
                .draft_mutation_staging_command(revision, begin)
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        fixture.storage.cancel_draft_marker_admission(
            &fixture.store,
            operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([219; 16])
        ),
        DraftMarkerAdmissionTerminalOutcomeV1::Collision
    ));
    assert_eq!(fixture.storage.revision(&fixture.store).unwrap(), revision);
    let after = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &keys)
        .unwrap();
    assert_eq!(before.head(), after.head());
    assert_eq!(before.receipt(), after.receipt());
    assert_eq!(before.capacity(), after.capacity());
    assert_eq!(before.nodes(), after.nodes());
    assert!(
        fixture
            .storage
            .draft_mutation_staging_head(&fixture.store, identity)
            .unwrap()
            .is_none()
    );
}

#[test]
fn cancellation_midway_through_multi_entry_ingestion_page_authenticates_selected_cursor() {
    let fixture = AcceptedFixture::new("fresh-midpage-cancellation", 170);
    let operation = owner(&fixture.session, 211);
    let mut attempt = fixture
        .storage
        .prepare_draft_marker_label_readiness_page(
            &fixture.store,
            request(
                operation,
                212,
                1,
                true,
                Disposition::Allocate,
                vec![fresh(213, fixture.asset_id), fresh(214, fixture.asset_id)],
                Some(fresh_factory(&fixture)),
            ),
        )
        .unwrap();
    let receipt = fixture
        .store
        .compose_proof(attempt.take_command().unwrap())
        .unwrap();
    let flight = attempt
        .into_submission_flight(&fixture.store, receipt)
        .unwrap();
    assert!(matches!(
        fixture
            .storage
            .submit_draft_marker_label_readiness_page(&fixture.store, flight),
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Advanced { .. }
    ));
    let before = writer_support::snapshot(&fixture.storage, &fixture.store, operation);
    let head = before.head().unwrap();
    assert_eq!(head.ingestion_association_cursor(), 1);
    assert!(!head.evidence_eof());
    assert_eq!(head.charge().associations(), 1);
    let keys = [
        head.source_root().node().unwrap(),
        head.target_root().node().unwrap(),
    ];
    assert!(matches!(
        fixture.storage.cancel_draft_marker_admission(
            &fixture.store,
            operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([219; 16])
        ),
        DraftMarkerAdmissionTerminalOutcomeV1::Advanced { .. }
    ));
    cleanup(&fixture, operation, &keys);
}
