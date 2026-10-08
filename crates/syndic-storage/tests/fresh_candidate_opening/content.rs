use super::*;

#[test]
fn fresh_candidate_content_retains_bounds_and_refuses_invalid_bindings() {
    let (_home, store, storage, faults, thread) =
        fault_fixture("fresh-candidate-bounds", 70, 65_536);
    let selected = current(&storage, &store, thread);
    let head = open_session(&storage, &store, &selected, 72, 73);
    let binding = DraftEditorCandidateActivationBindingV1::from_head(&head);
    fail_home(&store, &faults);
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let (_foreign_home, foreign_store, foreign, _) =
        fixture("fresh-candidate-bounds-foreign", 80, 65_536);
    let access = recovery.recovery_access().unwrap();
    let revision = fresh.revision_candidate(&access).unwrap();
    for bytes in [0, 3, 65_537] {
        assert!(
            fresh
                .current_draft_piece_text_demand_candidate(
                    &access,
                    thread,
                    DraftPieceTextDemandV1::Validate(0),
                    bytes
                )
                .is_err()
        );
        assert!(
            fresh
                .candidate_draft_piece_text_demand_candidate(
                    &access,
                    binding,
                    DraftPieceTextDemandV1::Forward(0),
                    bytes
                )
                .is_err()
        );
    }
    for (objects, bytes) in [(0, 1), (257, 1), (1, 0), (1, 65_537)] {
        assert!(
            fresh
                .candidate_draft_piece_marker_demand_candidate(
                    &access,
                    binding,
                    marker_demand(objects, bytes)
                )
                .is_err()
        );
    }
    for bytes in [0, 8, 65_537] {
        assert!(
            fresh
                .candidate_draft_piece_marker_edge_proof_candidate(
                    &access,
                    binding,
                    DraftPieceMarkerEdgeProofRequestV1::Absence { anchor: 0 },
                    bytes
                )
                .is_err()
        );
    }
    for rejected in [&storage, &foreign] {
        assert!(
            rejected
                .current_draft_piece_text_demand_candidate(
                    &access,
                    thread,
                    DraftPieceTextDemandV1::Validate(0),
                    4
                )
                .is_err()
        );
        assert!(
            rejected
                .candidate_draft_piece_text_demand_candidate(
                    &access,
                    binding,
                    DraftPieceTextDemandV1::Forward(0),
                    4
                )
                .is_err()
        );
        assert!(
            rejected
                .candidate_draft_piece_marker_demand_candidate(
                    &access,
                    binding,
                    marker_demand(1, 128)
                )
                .is_err()
        );
        assert!(
            rejected
                .candidate_draft_piece_marker_edge_proof_candidate(
                    &access,
                    binding,
                    DraftPieceMarkerEdgeProofRequestV1::Absence { anchor: 0 },
                    9
                )
                .is_err()
        );
    }
    assert_eq!(fresh.revision_candidate(&access).unwrap(), revision);
    foreign_store.close().unwrap();
    recovery.publish().unwrap().close().unwrap();
}
