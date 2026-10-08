use super::*;

#[test]
fn fresh_recovery_opening_follows_exact_disposal_and_reads_without_ordinary_admission() {
    let (_home, store, storage, faults, thread) = fault_fixture("fresh-candidate-open", 10, 65_536);
    let selected = current(&storage, &store, thread);
    let old = open_session(&storage, &store, &selected, 12, 13);
    let old_binding = DraftEditorCandidateActivationBindingV1::from_head(&old);
    let expected_current = storage
        .current_draft_piece_text_demand(&store, thread, DraftPieceTextDemandV1::Validate(0), 4)
        .unwrap()
        .unwrap();
    let expected_text = storage
        .candidate_draft_piece_text_demand(
            &store,
            old_binding,
            DraftPieceTextDemandV1::Forward(0),
            4,
        )
        .unwrap();
    let expected_markers = storage
        .candidate_draft_piece_marker_demand(&store, old_binding, marker_demand(1, 128))
        .unwrap();
    let expected_edge = storage
        .candidate_draft_piece_marker_edge_proof(
            &store,
            old_binding,
            DraftPieceMarkerEdgeProofRequestV1::Absence { anchor: 0 },
            9,
        )
        .unwrap();
    let request = open_request(&selected, 15, 16);
    fail_home(&store, &faults);
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let ordinary = recovery.service_reference();
    assert!(
        fresh
            .prepare_open_draft_editor_candidate_session(&ordinary, request)
            .is_err()
    );
    assert!(
        fresh
            .current_draft_piece_text_demand(
                &ordinary,
                thread,
                DraftPieceTextDemandV1::Validate(0),
                4
            )
            .is_err()
    );
    drop(ordinary);
    let access = recovery.recovery_access().unwrap();
    let disposed = fresh
        .prepare_abandon_fresh_draft_editor_candidate_session_candidate(
            &access,
            abandon_request(&old, 14),
        )
        .unwrap();
    let outcome = candidate_execute(
        &access,
        fresh.abandon_fresh_draft_editor_candidate_session(
            fresh.revision_candidate(&access).unwrap(),
            disposed.clone(),
        ),
    );
    assert!(matches!(
        fresh
            .reconcile_abandon_fresh_draft_editor_candidate_session_candidate(
                &access, &disposed, outcome,
            )
            .unwrap(),
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(_)
    ));
    assert!(matches!(
        fresh.candidate_draft_piece_text_demand_candidate(
            &access,
            old_binding,
            DraftPieceTextDemandV1::Forward(0),
            4,
        ),
        Err(DraftPieceRangeSourceErrorV1::Disposed(_))
    ));
    assert_eq!(
        fresh
            .current_draft_piece_text_demand_candidate(
                &access,
                thread,
                DraftPieceTextDemandV1::Validate(0),
                4,
            )
            .unwrap()
            .unwrap(),
        expected_current
    );
    let prepared = fresh
        .prepare_open_draft_editor_candidate_session_candidate(&access, request)
        .unwrap();
    let outcome = candidate_execute(
        &access,
        fresh.open_draft_editor_candidate_session(
            fresh.revision_candidate(&access).unwrap(),
            prepared.clone(),
        ),
    );
    let head = match fresh
        .reconcile_fresh_draft_editor_candidate_session_open_candidate(&access, &prepared, outcome)
        .unwrap()
    {
        DraftEditorCandidateSessionOpenOutcomeV1::Opened(head) => head,
        other => panic!("fresh candidate opening unavailable: {other:?}"),
    };
    let binding = DraftEditorCandidateActivationBindingV1::from_head(&head);
    assert_eq!(head.durable_base_root(), selected.draft().piece_root());
    assert_ne!(
        head.newest_history().key(),
        selected.draft().history().key()
    );
    let revision = fresh.revision_candidate(&access).unwrap();
    for _ in 0..2 {
        assert_eq!(
            fresh
                .candidate_draft_piece_text_demand_candidate(
                    &access,
                    binding,
                    DraftPieceTextDemandV1::Forward(0),
                    4,
                )
                .unwrap()
                .value(),
            expected_text.value()
        );
        assert_eq!(
            fresh
                .candidate_draft_piece_marker_demand_candidate(
                    &access,
                    binding,
                    marker_demand(1, 128),
                )
                .unwrap()
                .value(),
            expected_markers.value()
        );
        assert_eq!(
            fresh
                .candidate_draft_piece_marker_edge_proof_candidate(
                    &access,
                    binding,
                    DraftPieceMarkerEdgeProofRequestV1::Absence { anchor: 0 },
                    9,
                )
                .unwrap()
                .value(),
            expected_edge.value()
        );
    }
    assert_eq!(fresh.revision_candidate(&access).unwrap(), revision);
    assert!(
        fresh
            .prepare_open_draft_editor_candidate_session_candidate(&access, request)
            .is_err()
    );
    for _ in 0..2 {
        assert_eq!(
            fresh
                .qualify_fresh_draft_editor_candidate_session_open_candidate(
                    &access,
                    access.home_id(),
                    DraftPieceOperationIdV1::from_bytes([17; 16]),
                    &prepared,
                )
                .unwrap(),
            DraftEditorCandidateSessionReadOutcomeV1::Active(head.clone())
        );
    }
    assert_eq!(fresh.revision_candidate(&access).unwrap(), revision);
    recovery.publish().unwrap().close().unwrap();
}
