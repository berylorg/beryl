use super::*;
use crate::test_faults::MarkerLocatorBoundsForTest;

pub(crate) fn run_marker_locator_fixture(
    storage: &SyndicStorage,
    store: &HomeStore,
    draft_id: SyndicDraftId,
    prototype: DraftPieceMarkerV1,
) -> MarkerLocatorBoundsForTest {
    let session = DraftEditorCandidateSessionIdV1::from_bytes([0xc1; 16]);
    let operation = DraftPieceOperationIdV1::from_bytes([0xc2; 16]);
    let mut context = BuildContext::new(storage, store, draft_id, Some(session), operation);
    let a = marker(0, prototype);
    let b = marker(10, prototype);
    let c = marker(5, prototype);
    let a_leaf = context
        .new_sequence_leaf(DraftPieceLeafValueV1::Marker(a))
        .unwrap();
    let a_text = context
        .new_sequence_leaf(DraftPieceLeafValueV1::Text("a".to_owned()))
        .unwrap();
    let b_leaf = context
        .new_sequence_leaf(DraftPieceLeafValueV1::Marker(b))
        .unwrap();
    let b_text = context
        .new_sequence_leaf(DraftPieceLeafValueV1::Text("b".to_owned()))
        .unwrap();
    let left = context
        .new_sequence_node(1, vec![a_leaf.link, a_text.link])
        .unwrap();
    let right = context
        .new_sequence_node(1, vec![b_leaf.link, b_text.link])
        .unwrap();
    let tree = context
        .new_sequence_node(2, vec![left.link, right.link])
        .unwrap();
    for record in context.sequence_nodes.values() {
        persist::<DraftPieceNodesFamily>(storage, store, &record.key(), record);
    }
    for record in context.sequence_leaves.values() {
        persist::<DraftPieceLeavesFamily>(storage, store, &record.key(), record);
    }
    let new_context = || {
        let mut context = BuildContext::new(
            storage,
            store,
            draft_id,
            Some(session),
            DraftPieceOperationIdV1::from_bytes([0xc3; 16]),
        );
        context.acquisition = Some(BuildAcquisition::new(storage, store));
        context
    };
    let exact_key = DraftCompositeSearchKeyV1::Marker {
        anchor: 1,
        order_key: b.order_key(),
        marker_id: b.marker_id(),
    };
    let mut exact = new_context();
    let exact_fact =
        marker_program::locate_boundary_for_test(&mut exact, tree, exact_key, false).unwrap();
    let mut insertion = new_context();
    let insert_key = DraftCompositeSearchKeyV1::Marker {
        anchor: 1,
        order_key: c.order_key(),
        marker_id: SyndicDraftMarkerId::from_bytes([0; 16]),
    };
    let insertion_fact =
        marker_program::locate_boundary_for_test(&mut insertion, tree, insert_key, true).unwrap();
    let mut inserted_population = None;
    if let Some((rank, _, Some(_))) = insertion_fact {
        let inserted = insertion
            .new_sequence_leaf(DraftPieceLeafValueV1::Marker(c))
            .unwrap();
        inserted_population = insert_sequence_leaf(
            &mut insertion,
            Some(tree),
            Boundary { rank, inner: 0 },
            inserted,
        )
        .ok()
        .map(|tree| tree.link.piece_count());
    }
    let mut removed_population = None;
    if let Some((rank, _, Some(found))) = exact_fact {
        let expected = DraftMarkerIdentityOccurrenceV1::new(
            found.marker_id(),
            found.label(),
            found.asset_id(),
            found.order_key(),
            b_leaf.link.id(),
            b_leaf.link.digest(),
        );
        removed_population = sequence_edit::remove_marker(&mut exact, tree, rank, expected)
            .ok()
            .flatten()
            .map(|tree| tree.link.piece_count());
    }
    MarkerLocatorBoundsForTest {
        exact_rank_and_ordinal: exact_fact
            .filter(|(_, _, marker)| marker == &Some(b))
            .map(|(rank, ordinal, _)| (rank, ordinal)),
        insertion_rank_and_ordinal: insertion_fact
            .filter(|(_, _, marker)| marker == &Some(b))
            .map(|(rank, ordinal, _)| (rank, ordinal)),
        inserted_population,
        removed_population,
        work: [
            exact.acquisition.as_ref().unwrap().budget.work(),
            insertion.acquisition.as_ref().unwrap().budget.work(),
        ],
    }
}
