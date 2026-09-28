use super::{edit_support::commit_edit, support::*};
use syndic_storage::DraftPieceRestorationV1;

fn markers(objects: usize, bytes: usize) -> DraftPieceMarkerDemandV1 {
    DraftPieceMarkerDemandV1::new(
        DraftPieceMarkerScopeV1::InclusiveRange { start: 0, end: 4 },
        DraftPieceMarkerDirectionV1::Forward,
        None,
        objects,
        bytes,
    )
}

#[test]
fn unpublished_candidate_reads_share_exact_results_bounds_and_generation_fences() {
    let (_home, store, storage, faults, thread) = fault_fixture("candidate-ranges", 150, 1_048_576);
    let initial = current(&storage, &store, thread);
    let opened = open_session(&storage, &store, &initial, 152, 153);
    let edit = commit_edit(&storage, &store, &opened, 154, &"é日\n".repeat(1_000));
    let head = edit.adopted_session();
    let root = head.newest_root();
    let restoration =
        DraftPieceRestorationV1::new(root, head.newest_history(), point(2), point(5), point(0));
    storage
        .validate_draft_piece_restoration(&store, restoration.clone())
        .unwrap();
    let demands = [
        DraftPieceTextDemandV1::Forward(0),
        DraftPieceTextDemandV1::Backward(root.summary().logical_utf8_bytes()),
        DraftPieceTextDemandV1::Validate(2),
    ];
    let expected = demands.map(|demand| {
        storage
            .draft_piece_text_demand(&store, root, demand, 4)
            .unwrap()
    });
    let expected_markers = storage
        .draft_piece_marker_demand(&store, root, markers(1, 128))
        .unwrap();
    let proof = DraftPieceMarkerEdgeProofRequestV1::Absence { anchor: 0 };
    let expected_proof = storage
        .draft_piece_marker_edge_proof(&store, root, proof, 9)
        .unwrap();
    let revision = storage.revision(&store).unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let ordinary = recovery.service_reference();
    assert!(
        fresh
            .draft_piece_text_demand(&ordinary, root, demands[0], 4)
            .is_err()
    );
    drop(ordinary);
    let (_foreign_home, _foreign_store, foreign, _) =
        fixture("candidate-ranges-foreign", 160, 65_536);
    let access = recovery.recovery_access().unwrap();
    for (demand, expected) in demands.into_iter().zip(expected) {
        assert_eq!(
            fresh
                .draft_piece_text_demand_candidate(&access, root, demand, 4)
                .unwrap(),
            expected
        );
    }
    assert_eq!(
        fresh
            .draft_piece_marker_demand_candidate(&access, root, markers(1, 128))
            .unwrap(),
        expected_markers
    );
    assert_eq!(
        fresh
            .draft_piece_marker_edge_proof_candidate(&access, root, proof, 9)
            .unwrap(),
        expected_proof
    );
    assert_eq!(
        fresh
            .validate_draft_piece_restoration_candidate(&access, restoration.clone())
            .unwrap(),
        restoration
    );
    for rejected in [&storage, &foreign] {
        assert!(
            rejected
                .draft_piece_text_demand_candidate(&access, root, demands[0], 4)
                .is_err()
        );
        assert!(
            rejected
                .draft_piece_marker_demand_candidate(&access, root, markers(1, 128))
                .is_err()
        );
        assert!(
            rejected
                .draft_piece_marker_edge_proof_candidate(&access, root, proof, 9)
                .is_err()
        );
        assert!(
            rejected
                .validate_draft_piece_restoration_candidate(&access, restoration.clone())
                .is_err()
        );
    }
    for bytes in [0, 3, 65_537] {
        assert!(
            fresh
                .draft_piece_text_demand_candidate(&access, root, demands[0], bytes)
                .is_err()
        );
    }
    for (objects, bytes) in [(0, 1), (257, 1), (1, 0), (1, 65_537)] {
        assert!(
            fresh
                .draft_piece_marker_demand_candidate(&access, root, markers(objects, bytes))
                .is_err()
        );
    }
    for bytes in [0, 8, 65_537] {
        assert!(
            fresh
                .draft_piece_marker_edge_proof_candidate(&access, root, proof, bytes)
                .is_err()
        );
    }
    let invalid =
        DraftPieceRestorationV1::new(root, head.newest_history(), point(1), point(0), point(0));
    assert!(
        fresh
            .validate_draft_piece_restoration_candidate(&access, invalid)
            .is_err()
    );
    let store = recovery.publish().unwrap();
    assert_eq!(fresh.revision(&store).unwrap(), revision);
}

#[test]
fn candidate_restoration_rejects_missing_history_and_read_failure() {
    for missing in [true, false] {
        let (_home, store, storage, faults, thread) =
            fault_fixture("candidate-restoration-failure", 170, 65_536);
        let initial = current(&storage, &store, thread);
        let opened = open_session(&storage, &store, &initial, 172, 173);
        let restoration = DraftPieceRestorationV1::new(
            opened.newest_root(),
            opened.newest_history(),
            point(0),
            point(0),
            point(0),
        );
        if missing {
            committed(execute(
                &store,
                delete_draft_edit_history_frontier(
                    &store,
                    storage.clone(),
                    opened.newest_history().key(),
                ),
            ));
        }
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        if !missing {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
        }
        assert!(
            fresh
                .validate_draft_piece_restoration_candidate(&access, restoration.clone())
                .is_err()
        );
        if missing {
            recovery.abort().close().unwrap();
        } else {
            assert!(recovery.publish().is_err());
        }
    }
}
