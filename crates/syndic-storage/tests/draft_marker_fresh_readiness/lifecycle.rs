use super::*;

#[test]
fn cancellation_releases_tentative_fresh_reservations_and_closes_each_durable_lifecycle() {
    let fixture = AcceptedFixture::new("fresh-cancellation", 180);
    let transient = owner(&fixture.session, 225);
    let attempt = fixture
        .storage
        .prepare_draft_marker_label_readiness_page(
            &fixture.store,
            request(
                transient,
                226,
                1,
                true,
                Disposition::Allocate,
                vec![fresh(227, fixture.asset_id)],
                Some(fresh_factory(&fixture)),
            ),
        )
        .unwrap();
    assert!(matches!(
        fixture.storage.cancel_draft_marker_admission(
            &fixture.store,
            transient,
            DraftMarkerAdmissionCommandIdV1::from_bytes([228; 16])
        ),
        syndic_storage::DraftMarkerAdmissionTerminalOutcomeV1::ReleasedTransient
    ));
    drop(attempt);
    let snapshot = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, transient, &[])
        .unwrap();
    assert!(snapshot.head().is_none());
    for (index, (eof, ready)) in [(false, false), (true, false), (true, true)]
        .into_iter()
        .enumerate()
    {
        let operation = owner(&fixture.session, 230 + index as u8);
        ingest(
            &fixture,
            operation,
            233,
            1,
            eof,
            vec![fresh(234, fixture.asset_id)],
            || Some(fresh_factory(&fixture)),
        );
        let proof = ready.then(|| assign(&fixture, operation, 235, 1));
        assert!(matches!(
            fixture.storage.cancel_draft_marker_admission(
                &fixture.store,
                operation,
                DraftMarkerAdmissionCommandIdV1::from_bytes([236; 16])
            ),
            syndic_storage::DraftMarkerAdmissionTerminalOutcomeV1::Advanced { .. }
        ));
        let snapshot = fixture
            .storage
            .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &[])
            .unwrap();
        assert_eq!(
            snapshot.head().unwrap().lifecycle(),
            syndic_storage::DraftMarkerAdmissionLifecycleV1::TerminalCleanup
        );
        assert_eq!(snapshot.head().unwrap().allocating_occurrence_count(), 1);
        if let Some(proof) = proof {
            let identity = DraftMutationStagingIdentityV1::new(
                fixture.session.draft_id(),
                fixture.session.session_id(),
                DraftMutationOperationIdV1::from_bytes(*operation.operation_id().as_bytes()),
            );
            if let Ok(begin) = fixture.storage.prepare_draft_mutation_staging_marker_begin(
                begin_input(identity, &fixture.session),
                &fixture.session,
                proof,
            ) {
                assert!(matches!(
                    execute(
                        &fixture.store,
                        fixture.storage.draft_mutation_staging_command(
                            fixture.storage.revision(&fixture.store).unwrap(),
                            begin
                        )
                    ),
                    CommandOutcome::NotCommitted { .. }
                ));
            }
        }
        let after_rejection = fixture
            .storage
            .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &[])
            .unwrap();
        assert_eq!(snapshot.head(), after_rejection.head());
        assert_eq!(snapshot.receipt(), after_rejection.receipt());
        assert_eq!(snapshot.capacity(), after_rejection.capacity());
        for command in 237..245 {
            match fixture.storage.advance_draft_marker_admission_cleanup(
                &fixture.store,
                operation,
                DraftMarkerAdmissionCommandIdV1::from_bytes([command; 16]),
            ) {
                syndic_storage::DraftMarkerAdmissionTerminalOutcomeV1::Advanced { .. } => {}
                syndic_storage::DraftMarkerAdmissionTerminalOutcomeV1::RetainedClosure => break,
                syndic_storage::DraftMarkerAdmissionTerminalOutcomeV1::Refused(reason) => {
                    panic!("fresh terminal cleanup refused {reason:?} in lifecycle {index}")
                }
                syndic_storage::DraftMarkerAdmissionTerminalOutcomeV1::Collision => {
                    panic!("fresh terminal cleanup collided in lifecycle {index}")
                }
                _ => panic!("fresh terminal cleanup did not advance in lifecycle {index}"),
            }
        }
        let compact = fixture
            .storage
            .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &[])
            .unwrap();
        assert_eq!(compact.head().unwrap().target_root().count(), 0);
        assert_eq!(compact.head().unwrap().charge().associations(), 0);
        assert_eq!(compact.head().unwrap().allocating_occurrence_count(), 1);
    }
}

#[test]
fn cancellation_after_partial_fresh_assignment_reclaims_only_replay_shadows_before_cleanup() {
    let fixture = AcceptedFixture::new("fresh-partial-cancellation", 185);
    let operation = owner(&fixture.session, 236);
    ingest(
        &fixture,
        operation,
        237,
        1,
        true,
        vec![fresh(238, fixture.asset_id), fresh(239, fixture.asset_id)],
        || Some(fresh_factory(&fixture)),
    );
    let flight = fixture
        .storage
        .prepare_draft_marker_label_assignment(
            &fixture.store,
            operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([240; 16]),
        )
        .unwrap();
    assert!(matches!(
        fixture
            .storage
            .submit_draft_marker_label_assignment(&fixture.store, flight),
        DraftMarkerLabelAssignmentOutcomeV1::Advanced { .. }
    ));
    assert!(matches!(
        fixture.storage.cancel_draft_marker_admission(
            &fixture.store,
            operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([241; 16])
        ),
        syndic_storage::DraftMarkerAdmissionTerminalOutcomeV1::Advanced { .. }
    ));
    let terminal = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &[])
        .unwrap();
    assert_eq!(terminal.head().unwrap().charge().associations(), 2);
    for command in 242..250 {
        match fixture.storage.advance_draft_marker_admission_cleanup(
            &fixture.store,
            operation,
            DraftMarkerAdmissionCommandIdV1::from_bytes([command; 16]),
        ) {
            syndic_storage::DraftMarkerAdmissionTerminalOutcomeV1::Advanced { .. } => {}
            syndic_storage::DraftMarkerAdmissionTerminalOutcomeV1::RetainedClosure => break,
            _ => panic!("partially assigned fresh cleanup did not advance"),
        }
    }
    let compact = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &[])
        .unwrap();
    assert_eq!(compact.head().unwrap().charge().associations(), 0);
    assert_eq!(compact.head().unwrap().allocating_occurrence_count(), 2);
}

#[test]
fn uncertain_fresh_assignment_reconciles_once_and_retired_custody_cannot_mint_readiness() {
    for retired in [false, true] {
        let faults = FaultController::new();
        let fixture = AcceptedFixture::with_faults("fresh-uncertain", 190, faults.clone());
        let operation = owner(&fixture.session, 240);
        ingest(
            &fixture,
            operation,
            241,
            1,
            true,
            vec![fresh(242, fixture.asset_id)],
            || Some(fresh_factory(&fixture)),
        );
        let flight = fixture
            .storage
            .prepare_draft_marker_label_assignment(
                &fixture.store,
                operation,
                DraftMarkerAdmissionCommandIdV1::from_bytes([243; 16]),
            )
            .unwrap();
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let pending = match fixture
            .storage
            .submit_draft_marker_label_assignment(&fixture.store, flight)
        {
            DraftMarkerLabelAssignmentOutcomeV1::ReconciliationPending(pending) => pending,
            _ => panic!("uncertain fresh assignment lost reconciliation custody"),
        };
        if retired {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(fixture.store.home_revision().is_err());
            let recovery = fixture.store.recover_same_home().unwrap();
            let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
            let store = recovery.publish();
            assert!(!matches!(
                storage.submit_draft_marker_label_assignment(&store, pending),
                DraftMarkerLabelAssignmentOutcomeV1::Ready { .. }
            ));
        } else {
            let proof = match fixture
                .storage
                .submit_draft_marker_label_assignment(&fixture.store, pending)
            {
                DraftMarkerLabelAssignmentOutcomeV1::Ready { proof, .. } => proof,
                _ => panic!("exact-new fresh reconciliation did not issue readiness"),
            };
            assert_eq!(proof.allocation_range().unwrap().count(), 1);
            assert!(
                fixture
                    .storage
                    .prepare_draft_marker_label_assignment(
                        &fixture.store,
                        operation,
                        DraftMarkerAdmissionCommandIdV1::from_bytes([244; 16])
                    )
                    .is_err()
            );
        }
    }
}

#[test]
fn duplicate_targets_across_fresh_and_accepted_pages_leave_the_existing_prefix_unchanged() {
    let fixture = AcceptedFixture::new("fresh-cross-page-duplicate", 200);
    let operation = owner(&fixture.session, 245);
    ingest(
        &fixture,
        operation,
        246,
        1,
        false,
        vec![fresh(247, fixture.asset_id)],
        || Some(fresh_factory(&fixture)),
    );
    let before = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &[])
        .unwrap();
    let mut attempt = fixture
        .storage
        .prepare_draft_marker_label_readiness_page(
            &fixture.store,
            request(
                operation,
                248,
                2,
                true,
                Disposition::Allocate,
                vec![fixture.association(247, fixture.thread)],
                Some(fixture.factory()),
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
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Collision
    ));
    let after = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &[])
        .unwrap();
    assert_eq!(before.head().unwrap(), after.head().unwrap());
}

#[test]
fn fresh_assigned_target_is_consumed_once_and_historical_counts_survive_writer_settlement() {
    let mut fixture = AcceptedFixture::new("fresh-writer", 210);
    fixture.session = complete_staged(
        &fixture.storage,
        &fixture.store,
        &fixture.session,
        249,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".to_owned())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    let operation = owner(&fixture.session, 250);
    ingest(
        &fixture,
        operation,
        251,
        1,
        true,
        vec![fresh(252, fixture.asset_id)],
        || Some(fresh_factory(&fixture)),
    );
    let proof = assign(&fixture, operation, 253, 1);
    let assigned = label(&fixture, &proof, 252);
    let target = DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([252; 16]),
        0,
        assigned,
        fixture.asset_id,
    );
    let boundary = point(1);
    let replacement =
        DraftPieceReplacementV1::new(boundary, boundary, vec![DraftPieceV1::Marker(target)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    target,
                    DraftPieceMarkerEffectChargesV1::for_marker(target),
                ),
            ));
    let next = writer_support::complete_admitted_marker_edit(
        &fixture.storage,
        &fixture.store,
        &fixture.session,
        operation,
        proof,
        replacement,
    );
    let settled = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &[])
        .unwrap();
    let head = settled.head().unwrap();
    assert_eq!(
        head.lifecycle(),
        syndic_storage::DraftMarkerAdmissionLifecycleV1::Settled
    );
    assert_eq!(head.allocating_occurrence_count(), 1);
    assert_eq!(head.occurrence_count(), 1);
    assert_eq!(head.target_root().count(), 0);
    assert_eq!(head.remaining_builder_count(), 0);
    assert_eq!(
        next.newest_candidate_generation(),
        fixture.session.newest_candidate_generation() + 1
    );
    fixture
        .storage
        .release_settled_draft_marker_writer(&fixture.store, operation)
        .unwrap();
}
