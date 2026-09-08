use super::*;
use crate::codec::Family;
use crate::draft_piece::mutation::advance_budget::BuildAcquisition;

use crate::test_faults::MarkerRemovalBoundsForTest;

#[path = "locator.rs"]
mod locator;
pub(crate) use locator::run_marker_locator_fixture;

#[derive(Clone, Copy)]
struct Triple {
    sequence: SequenceRef,
    identity: IndexRef,
    order: MarkerOrderRef,
}

fn marker_id(value: u64) -> SyndicDraftMarkerId {
    SyndicDraftMarkerId::from_bytes((u128::from(value) + 1).to_be_bytes())
}

fn marker(value: u64, prototype: DraftPieceMarkerV1) -> DraftPieceMarkerV1 {
    DraftPieceMarkerV1::new(
        marker_id(value),
        value,
        prototype.label(),
        prototype.asset_id(),
    )
}

fn leaf(context: &mut BuildContext<'_>, value: DraftPieceMarkerV1) -> Triple {
    let sequence = context
        .new_sequence_leaf(DraftPieceLeafValueV1::Marker(value))
        .unwrap();
    let occurrence = DraftMarkerIdentityOccurrenceV1::new(
        value.marker_id(),
        value.label(),
        value.asset_id(),
        value.order_key(),
        sequence.link.id(),
        sequence.link.digest(),
    );
    Triple {
        sequence,
        identity: context.new_index_leaf(occurrence).unwrap(),
        order: context
            .new_marker_order_leaf(value.marker_id(), value.label(), value.asset_id())
            .unwrap(),
    }
}

fn node(context: &mut BuildContext<'_>, height: u8, children: &[Triple]) -> Triple {
    Triple {
        sequence: context
            .new_sequence_node(height, children.iter().map(|v| v.sequence.link).collect())
            .unwrap(),
        identity: context
            .new_index_node(height, children.iter().map(|v| v.identity.link).collect())
            .unwrap(),
        order: context
            .new_marker_order_node(height, children.iter().map(|v| v.order.link).collect())
            .unwrap(),
    }
}

fn opaque(
    context: &mut BuildContext<'_>,
    height: u8,
    first: u64,
    count: u64,
    prototype: DraftPieceMarkerV1,
) -> Triple {
    let last = first.checked_add(count - 1).unwrap();
    let digest = DraftPieceDigestV1::from_bytes([0xa5; 32]);
    let id = context.next_id(digest).unwrap();
    let key = |value| DraftCompositeSearchKeyV1::Marker {
        anchor: 0,
        order_key: value,
        marker_id: marker_id(value),
    };
    Triple {
        sequence: SequenceRef {
            link: DraftPieceChildV1::new(
                id,
                digest,
                0,
                0,
                0,
                count,
                count,
                digest,
                key(first),
                key(last),
            ),
            height,
            selected_root: false,
        },
        identity: IndexRef {
            link: DraftMarkerIdentityChildV1::new(
                id,
                digest,
                count,
                marker_id(first),
                marker_id(last),
            ),
            height,
            selected_root: false,
        },
        order: MarkerOrderRef {
            link: DraftMarkerOrderChildV1::new(id, digest, count, Some(prototype.label())).unwrap(),
            height,
            selected_root: false,
        },
    }
}

fn sibling(
    context: &mut BuildContext<'_>,
    height: u8,
    first: u64,
    fanout: usize,
    complete: bool,
    prototype: DraftPieceMarkerV1,
) -> Triple {
    if height == 0 {
        return leaf(context, marker(first, prototype));
    }
    let child_count = 1_u64.checked_shl(u32::from(height - 1)).unwrap();
    let children = (0..fanout)
        .map(|index| {
            let first = first.checked_add(index as u64 * child_count).unwrap();
            if complete {
                sibling(context, height - 1, first, 2, true, prototype)
            } else {
                opaque(context, height - 1, first, child_count, prototype)
            }
        })
        .collect::<Vec<_>>();
    node(context, height, &children)
}

fn persist<F: Family>(storage: &SyndicStorage, store: &HomeStore, key: &F::Key, value: &F::Value) {
    crate::test_faults::put_marker_bounds_fixture_record::<F>(storage, store, key, value);
}

fn width<F: Family>(key: &F::Key, value: &F::Value) -> u64 {
    (F::encode_key(key).unwrap().len() + F::encode_value(value).unwrap().len()) as u64
}

pub fn run_marker_removal_bounds_fixture(
    storage: &SyndicStorage,
    store: &HomeStore,
    draft_id: SyndicDraftId,
    prototype: DraftPieceMarkerV1,
    height: u8,
    sibling_fanout: usize,
    complete_closure: bool,
) -> MarkerRemovalBoundsForTest {
    assert!((1..=64).contains(&height));
    assert!((2..=128).contains(&sibling_fanout));
    assert!(!complete_closure || height <= 2);
    assert!(height <= 2 || sibling_fanout == 2);
    let session = DraftEditorCandidateSessionIdV1::from_bytes([0xf1; 16]);
    let operation = DraftPieceOperationIdV1::from_bytes([0xf2; 16]);
    let mut fixture = BuildContext::new(storage, store, draft_id, Some(session), operation);
    let target = marker(0, prototype);
    let target_leaf = leaf(&mut fixture, target);
    let expected = DraftMarkerIdentityOccurrenceV1::new(
        target.marker_id(),
        target.label(),
        target.asset_id(),
        target.order_key(),
        target_leaf.sequence.link.id(),
        target_leaf.sequence.link.digest(),
    );
    let mut root = target_leaf;
    for level in 1..=height.min(63) {
        let first = root.sequence.link.piece_count();
        let other = sibling(
            &mut fixture,
            level - 1,
            first,
            sibling_fanout,
            complete_closure,
            prototype,
        );
        root = node(&mut fixture, level, &[root, other]);
    }
    if height == 64 {
        root = node(&mut fixture, height, &[root]);
    }
    for record in fixture.sequence_nodes.values() {
        persist::<DraftPieceNodesFamily>(storage, store, &record.key(), record);
    }
    for record in fixture.sequence_leaves.values() {
        persist::<DraftPieceLeavesFamily>(storage, store, &record.key(), record);
    }
    for record in fixture.index_records.values() {
        persist::<DraftMarkerIdentityIndexFamily>(storage, store, &record.key(), record);
    }
    for record in fixture.marker_order_records.values() {
        persist::<DraftMarkerOrderCommitmentsFamily>(storage, store, &record.key(), record);
    }
    let measured_max_internal_bytes = [
        fixture
            .sequence_nodes
            .values()
            .map(|r| width::<DraftPieceNodesFamily>(&r.key(), r))
            .max()
            .unwrap(),
        fixture
            .index_records
            .values()
            .filter(|r| r.children().is_some())
            .map(|r| width::<DraftMarkerIdentityIndexFamily>(&r.key(), r))
            .max()
            .unwrap(),
        fixture
            .marker_order_records
            .values()
            .filter(|r| r.children().is_some())
            .map(|r| width::<DraftMarkerOrderCommitmentsFamily>(&r.key(), r))
            .max()
            .unwrap(),
    ];
    let mut contexts = std::array::from_fn::<_, 3, _>(|index| {
        let mut context = BuildContext::with_ordinal(
            storage,
            store,
            draft_id,
            Some(session),
            DraftPieceOperationIdV1::from_bytes([0xe0 + index as u8; 16]),
            1,
        );
        context.acquisition = Some(BuildAcquisition::new(storage, store));
        context
    });
    let sequence = sequence_edit::remove_marker(&mut contexts[0], root.sequence, 0, expected)
        .unwrap()
        .unwrap();
    let identity = marker_tree_edit::remove_identity(&mut contexts[1], root.identity, expected)
        .unwrap()
        .unwrap();
    let order = marker_tree_edit::remove_order(
        &mut contexts[2],
        root.order,
        0,
        (target.marker_id(), target.label(), target.asset_id()),
    )
    .unwrap()
    .unwrap();
    MarkerRemovalBoundsForTest {
        complete_closure,
        input_height: height,
        output_heights: [sequence.height, identity.height, order.height],
        input_population: root.sequence.link.piece_count(),
        output_populations: [
            sequence.link.piece_count(),
            identity.link.record_count(),
            order.link.marker_count(),
        ],
        acquired: std::array::from_fn(|i| contexts[i].records_read),
        emitted: [
            contexts[0].sequence_nodes.len() + contexts[0].sequence_leaves.len(),
            contexts[1].index_records.len(),
            contexts[2].marker_order_records.len(),
        ],
        emitted_internal_children: [
            contexts[0]
                .sequence_nodes
                .values()
                .map(|v| v.children().len())
                .collect(),
            contexts[1]
                .index_records
                .values()
                .filter_map(|v| v.children().map(<[_]>::len))
                .collect(),
            contexts[2]
                .marker_order_records
                .values()
                .filter_map(|v| v.children().map(<[_]>::len))
                .collect(),
        ],
        work: std::array::from_fn(|i| contexts[i].acquisition.as_ref().unwrap().budget.work()),
        measured_max_internal_bytes,
    }
}

pub fn run_marker_constructor_reservation_fixture(
    storage: &SyndicStorage,
    store: &HomeStore,
    draft_id: SyndicDraftId,
    prototype: DraftPieceMarkerV1,
) -> [bool; 6] {
    let session = DraftEditorCandidateSessionIdV1::from_bytes([0xb1; 16]);
    let operation = DraftPieceOperationIdV1::from_bytes([0xb2; 16]);
    let mut fixture = BuildContext::new(storage, store, draft_id, Some(session), operation);
    let first = leaf(&mut fixture, marker(0, prototype));
    let second = leaf(&mut fixture, marker(1, prototype));
    let occurrence = fixture
        .index_records
        .values()
        .find_map(|v| v.occurrence())
        .unwrap();
    std::array::from_fn(|index| {
        let mut context = BuildContext::new(storage, store, draft_id, Some(session), operation);
        let acquisition = BuildAcquisition::new(storage, store);
        acquisition
            .budget
            .encoded_effect(4_194_304 - 1, false)
            .unwrap();
        let before = acquisition.budget.work();
        context.acquisition = Some(acquisition.clone());
        let rejected = match index {
            0 => context
                .new_sequence_leaf(DraftPieceLeafValueV1::Marker(prototype))
                .is_err(),
            1 => context
                .new_sequence_node(1, vec![first.sequence.link, second.sequence.link])
                .is_err(),
            2 => context.new_index_leaf(occurrence).is_err(),
            3 => context
                .new_index_node(1, vec![first.identity.link, second.identity.link])
                .is_err(),
            4 => context
                .new_marker_order_leaf(
                    prototype.marker_id(),
                    prototype.label(),
                    prototype.asset_id(),
                )
                .is_err(),
            5 => context
                .new_marker_order_node(1, vec![first.order.link, second.order.link])
                .is_err(),
            _ => unreachable!(),
        };
        rejected
            && context.sequence_leaves.is_empty()
            && context.sequence_nodes.is_empty()
            && context.index_records.is_empty()
            && context.marker_order_records.is_empty()
            && acquisition.budget.work() == before
    })
}
