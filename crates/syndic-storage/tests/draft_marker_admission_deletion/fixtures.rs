use super::*;

pub(super) fn target_only_unary_height_eighteen() -> DraftMarkerAdmissionIndexTestStateV1 {
    const LEAF_COUNT: usize = 65_536;

    let owner = owner_for_test();
    let empty = DraftMarkerAdmissionIndexTestStateV1::new(owner);
    let mut nodes = Vec::with_capacity(LEAF_COUNT * 2);
    let mut level = Vec::with_capacity(LEAF_COUNT);
    for index in 0..LEAF_COUNT {
        let leaf = target_leaf(owner, index, true);
        level.push(child(&leaf));
        nodes.push(leaf);
    }
    let mut internal_ordinal = 0;
    for height in 2..=17 {
        let mut next = Vec::with_capacity(level.len() / 2);
        for pair in level.chunks_exact(2) {
            let node = internal(
                owner,
                DraftMarkerAdmissionTreeV1::TargetId,
                height,
                internal_ordinal,
                pair.to_vec(),
            );
            internal_ordinal += 1;
            next.push(child(&node));
            nodes.push(node);
        }
        level = next;
    }
    assert_eq!(level.len(), 1);
    let root_node = internal(
        owner,
        DraftMarkerAdmissionTreeV1::TargetId,
        18,
        internal_ordinal,
        vec![level[0]],
    );
    nodes.push(root_node.clone());
    assert_eq!(nodes.len(), 131_072);
    assert!(nodes.len() <= 250_000);
    DraftMarkerAdmissionIndexTestStateV1::from_parts(
        owner,
        empty.source_root(),
        root(&root_node),
        nodes,
        Vec::new(),
    )
}

pub(super) fn dual_tree_height_seventeen() -> DraftMarkerAdmissionIndexTestStateV1 {
    const LEAF_COUNT: usize = 32_768;

    let owner = owner_for_test();
    let empty = DraftMarkerAdmissionIndexTestStateV1::new(owner);
    let mut nodes = Vec::with_capacity(LEAF_COUNT * 4);
    let mut source_level = Vec::with_capacity(LEAF_COUNT);
    let mut target_level = Vec::with_capacity(LEAF_COUNT);
    for index in 0..LEAF_COUNT {
        let source = source_leaf(owner, index);
        let target = target_leaf(owner, index, false);
        source_level.push(child(&source));
        target_level.push(child(&target));
        nodes.push(source);
        nodes.push(target);
    }
    let mut source_ordinal = 0;
    let mut target_ordinal = 0;
    for height in 2..=16 {
        let mut next_source = Vec::with_capacity(source_level.len() / 2);
        let mut next_target = Vec::with_capacity(target_level.len() / 2);
        for pair in source_level.chunks_exact(2) {
            let node = internal(
                owner,
                DraftMarkerAdmissionTreeV1::SourceOrder,
                height,
                source_ordinal,
                pair.to_vec(),
            );
            source_ordinal += 1;
            next_source.push(child(&node));
            nodes.push(node);
        }
        for pair in target_level.chunks_exact(2) {
            let node = internal(
                owner,
                DraftMarkerAdmissionTreeV1::TargetId,
                height,
                target_ordinal,
                pair.to_vec(),
            );
            target_ordinal += 1;
            next_target.push(child(&node));
            nodes.push(node);
        }
        source_level = next_source;
        target_level = next_target;
    }
    assert_eq!(source_level.len(), 1);
    assert_eq!(target_level.len(), 1);
    let source_root = internal(
        owner,
        DraftMarkerAdmissionTreeV1::SourceOrder,
        17,
        source_ordinal,
        vec![source_level[0]],
    );
    let target_root = internal(
        owner,
        DraftMarkerAdmissionTreeV1::TargetId,
        17,
        target_ordinal,
        vec![target_level[0]],
    );
    nodes.extend([source_root.clone(), target_root.clone()]);
    assert_eq!(nodes.len(), 131_072);
    assert!(nodes.len() <= 250_000);
    DraftMarkerAdmissionIndexTestStateV1::from_parts(
        owner,
        root(&source_root),
        root(&target_root),
        nodes,
        Vec::new(),
    )
}

pub(super) fn dual_tree_high_fanout() -> DraftMarkerAdmissionIndexTestStateV1 {
    const FANOUT: usize = 128;
    const LEAF_COUNT: usize = FANOUT * FANOUT;

    let owner = owner_for_test();
    let mut nodes = Vec::with_capacity(LEAF_COUNT * 2 + FANOUT * 2 + 2);
    let mut source_leaves = Vec::with_capacity(LEAF_COUNT);
    let mut target_leaves = Vec::with_capacity(LEAF_COUNT);
    for index in 0..LEAF_COUNT {
        let source = source_leaf(owner, index);
        let target = target_leaf(owner, index, false);
        source_leaves.push(child(&source));
        target_leaves.push(child(&target));
        nodes.push(source);
        nodes.push(target);
    }
    let source_children = source_leaves
        .chunks_exact(FANOUT)
        .enumerate()
        .map(|(ordinal, children)| {
            let node = internal(
                owner,
                DraftMarkerAdmissionTreeV1::SourceOrder,
                2,
                ordinal,
                children.to_vec(),
            );
            let child = child(&node);
            nodes.push(node);
            child
        })
        .collect::<Vec<_>>();
    let target_children = target_leaves
        .chunks_exact(FANOUT)
        .enumerate()
        .map(|(ordinal, children)| {
            let node = internal(
                owner,
                DraftMarkerAdmissionTreeV1::TargetId,
                2,
                ordinal,
                children.to_vec(),
            );
            let child = child(&node);
            nodes.push(node);
            child
        })
        .collect::<Vec<_>>();
    let source_root = internal(
        owner,
        DraftMarkerAdmissionTreeV1::SourceOrder,
        3,
        FANOUT,
        source_children,
    );
    let target_root = internal(
        owner,
        DraftMarkerAdmissionTreeV1::TargetId,
        3,
        FANOUT,
        target_children,
    );
    nodes.extend([source_root.clone(), target_root.clone()]);
    assert_eq!(nodes.len(), 33_026);
    DraftMarkerAdmissionIndexTestStateV1::from_parts(
        owner,
        root(&source_root),
        root(&target_root),
        nodes,
        Vec::new(),
    )
}

pub(super) fn binary_subtree(
    owner: DraftMarkerAdmissionOwnerV1,
    tree: DraftMarkerAdmissionTreeV1,
    leaves: &[DraftMarkerAdmissionChildV1],
    height: u8,
    ordinal: &mut usize,
    nodes: &mut Vec<DraftMarkerAdmissionNodeV1>,
) -> DraftMarkerAdmissionChildV1 {
    let mut level = leaves.to_vec();
    for node_height in 2..=height {
        let mut next = Vec::with_capacity(level.len() / 2);
        for pair in level.chunks_exact(2) {
            let node = internal(owner, tree, node_height, *ordinal, pair.to_vec());
            *ordinal += 1;
            next.push(child(&node));
            nodes.push(node);
        }
        level = next;
    }
    assert_eq!(level.len(), 1);
    level[0]
}

pub(super) fn weighted_source_with_dense_target() -> DraftMarkerAdmissionIndexTestStateV1 {
    const ASSOCIATIONS: usize = 65_410;
    const WIDE_LEVELS: std::ops::RangeInclusive<u8> = 3..=10;

    let owner = owner_for_test();
    let mut nodes = Vec::with_capacity(200_000);
    let mut source_leaves = Vec::with_capacity(ASSOCIATIONS);
    let mut target_level = Vec::with_capacity(ASSOCIATIONS);
    for index in 0..ASSOCIATIONS {
        let source = source_leaf(owner, index);
        let target = target_leaf(owner, index, false);
        source_leaves.push(child(&source));
        target_level.push(child(&target));
        nodes.push(source);
        nodes.push(target);
    }

    let mut source_ordinal = 0;
    let mut source = internal(
        owner,
        DraftMarkerAdmissionTreeV1::SourceOrder,
        2,
        source_ordinal,
        source_leaves[..128].to_vec(),
    );
    source_ordinal += 1;
    nodes.push(source.clone());
    let mut next_leaf = 128;
    for height in WIDE_LEVELS {
        let sibling_leaves = 1usize << (height - 2);
        let mut children = Vec::with_capacity(128);
        children.push(child(&source));
        for _ in 0..127 {
            let end = next_leaf + sibling_leaves;
            children.push(binary_subtree(
                owner,
                DraftMarkerAdmissionTreeV1::SourceOrder,
                &source_leaves[next_leaf..end],
                height - 1,
                &mut source_ordinal,
                &mut nodes,
            ));
            next_leaf = end;
        }
        source = internal(
            owner,
            DraftMarkerAdmissionTreeV1::SourceOrder,
            height,
            source_ordinal,
            children,
        );
        source_ordinal += 1;
        nodes.push(source.clone());
    }
    let sibling_end = next_leaf + 512;
    let sibling = binary_subtree(
        owner,
        DraftMarkerAdmissionTreeV1::SourceOrder,
        &source_leaves[next_leaf..sibling_end],
        10,
        &mut source_ordinal,
        &mut nodes,
    );
    next_leaf = sibling_end;
    source = internal(
        owner,
        DraftMarkerAdmissionTreeV1::SourceOrder,
        11,
        source_ordinal,
        vec![child(&source), sibling],
    );
    source_ordinal += 1;
    nodes.push(source.clone());
    let source_root = internal(
        owner,
        DraftMarkerAdmissionTreeV1::SourceOrder,
        12,
        source_ordinal,
        vec![child(&source)],
    );
    nodes.push(source_root.clone());
    assert_eq!(next_leaf, ASSOCIATIONS);

    let mut target_ordinal = 0;
    let mut target_height = 2;
    while target_level.len() > 1 {
        let mut next = Vec::with_capacity(target_level.len().div_ceil(128));
        for children in target_level.chunks(128) {
            let node = internal(
                owner,
                DraftMarkerAdmissionTreeV1::TargetId,
                target_height,
                target_ordinal,
                children.to_vec(),
            );
            target_ordinal += 1;
            next.push(child(&node));
            nodes.push(node);
        }
        target_level = next;
        target_height += 1;
    }
    let target_root = DraftMarkerAdmissionRootV1::new(
        DraftMarkerAdmissionTreeV1::TargetId,
        target_level[0].key(),
        target_height - 1,
        target_level[0].digest(),
        target_level[0].count(),
    )
    .unwrap();
    assert_eq!(source_root.count().unwrap(), ASSOCIATIONS as u64);
    assert_eq!(target_root.count(), ASSOCIATIONS as u64);
    assert!(nodes.len() < 250_000);
    DraftMarkerAdmissionIndexTestStateV1::from_parts(
        owner,
        root(&source_root),
        target_root,
        nodes,
        Vec::new(),
    )
}

pub(super) fn assignment_receipt(
    state: &DraftMarkerAdmissionIndexTestStateV1,
    command: DraftMarkerAdmissionCommandIdV1,
    source_before: DraftMarkerAdmissionRootV1,
    source_after: DraftMarkerAdmissionRootV1,
    target_before: DraftMarkerAdmissionRootV1,
    target_after: DraftMarkerAdmissionRootV1,
    retained: &[DraftMarkerAdmissionNodeKeyV1],
) -> DraftMarkerAdmissionReplayReceiptV1 {
    let source = state
        .nodes()
        .iter()
        .find(|node| {
            node.key() == key(owner_for_test(), DraftMarkerAdmissionNodeKindV1::Leaf, 1, 0)
        })
        .unwrap();
    let DraftMarkerAdmissionNodePayloadV1::SourceLeaf {
        source_key,
        asset_id,
        ..
    } = source.payload()
    else {
        panic!("expected source leaf");
    };
    let mut source_head = vec![0; 32];
    source_head.extend_from_slice(&source_key.group().canonical_bytes());
    source_head.extend_from_slice(&asset_id.digest());
    source_head.extend_from_slice(&asset_id.length().get().to_le_bytes());
    let DraftMarkerAdmissionAssignmentGroupV1::PreserveLabel(label) = source_key.group() else {
        panic!("expected preserve-label source");
    };
    let mut target_head = Vec::new();
    target_head.extend_from_slice(target_before.digest().as_bytes());
    target_head.extend_from_slice(&target_before.count().to_le_bytes());
    target_head.extend_from_slice(&label.get().to_le_bytes());
    DraftMarkerAdmissionReplayReceiptV1::new(
        owner_for_test(),
        command,
        NonZeroU64::MIN,
        syndic_storage::DraftMarkerAdmissionDigestV1::from_bytes([240; 32]),
        source_head,
        target_head,
        source_before,
        source_after,
        target_before,
        target_after,
        retained_children(state, retained).into_boxed_slice(),
        DraftMarkerAdmissionReceiptTransitionV1::Assignment,
    )
    .unwrap()
}
