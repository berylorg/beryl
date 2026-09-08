use super::*;

pub(super) struct TreeFixture {
    pub(super) state: DraftMarkerAdmissionIndexTestStateV1,
    pub(super) source_first_descent: Vec<DraftMarkerAdmissionNodeKeyV1>,
    pub(super) target_first_descent: Vec<DraftMarkerAdmissionNodeKeyV1>,
    pub(super) source_right_sibling: DraftMarkerAdmissionNodeKeyV1,
    pub(super) target_left_sibling: DraftMarkerAdmissionNodeKeyV1,
    pub(super) target_right_sibling: DraftMarkerAdmissionNodeKeyV1,
}

pub(super) fn owner_for_test() -> DraftMarkerAdmissionOwnerV1 {
    DraftMarkerAdmissionOwnerV1::new(
        SyndicDraftId::from_bytes([10; 16]),
        DraftEditorCandidateSessionIdV1::from_bytes([11; 16]),
        DraftMarkerAdmissionOperationIdV1::from_bytes([12; 16]),
    )
}

pub(super) fn label(index: usize) -> ImageLabelOrdinal {
    ImageLabelOrdinal::new(u64::try_from(index + 1).unwrap()).unwrap()
}

pub(super) fn marker_id(index: usize) -> SyndicDraftMarkerId {
    let mut bytes = [0; 16];
    bytes[8..].copy_from_slice(&u64::try_from(index).unwrap().to_be_bytes());
    SyndicDraftMarkerId::from_bytes(bytes)
}

pub(super) fn asset(index: usize) -> AssetId {
    let mut digest = [0; 32];
    digest[..8].copy_from_slice(&u64::try_from(index).unwrap().to_be_bytes());
    AssetId::sha256_v1(digest, NonZeroU64::MIN)
}

pub(super) fn evidence(label: ImageLabelOrdinal, asset: AssetId) -> DraftMarkerAdmissionEvidenceV1 {
    let mut bytes = vec![0; 194];
    bytes[0] = 1;
    bytes[145..153].copy_from_slice(&label.get().to_le_bytes());
    bytes[153] = asset.version() as u8;
    bytes[154..186].copy_from_slice(&asset.digest());
    bytes[186..].copy_from_slice(&asset.length().get().to_le_bytes());
    DraftMarkerAdmissionEvidenceV1::new(bytes).unwrap()
}

pub(super) fn key(
    owner: DraftMarkerAdmissionOwnerV1,
    kind: DraftMarkerAdmissionNodeKindV1,
    domain: u8,
    ordinal: usize,
) -> DraftMarkerAdmissionNodeKeyV1 {
    let mut id = [domain; 16];
    id[..8].copy_from_slice(&u64::try_from(ordinal).unwrap().to_be_bytes());
    DraftMarkerAdmissionNodeKeyV1::new(owner, kind, DraftMarkerAdmissionNodeIdV1::from_bytes(id))
}

pub(super) fn source_leaf(
    owner: DraftMarkerAdmissionOwnerV1,
    index: usize,
) -> DraftMarkerAdmissionNodeV1 {
    DraftMarkerAdmissionNodeV1::source_leaf(
        key(owner, DraftMarkerAdmissionNodeKindV1::Leaf, 1, index),
        DraftMarkerAdmissionSourceKeyV1::new(
            DraftMarkerAdmissionAssignmentGroupV1::PreserveLabel(label(index)),
            marker_id(index),
        ),
        evidence(label(index), asset(index)),
        asset(index),
    )
    .unwrap()
}

pub(super) fn target_leaf(
    owner: DraftMarkerAdmissionOwnerV1,
    index: usize,
    assigned: bool,
) -> DraftMarkerAdmissionNodeV1 {
    DraftMarkerAdmissionNodeV1::target_leaf(
        key(owner, DraftMarkerAdmissionNodeKindV1::Leaf, 2, index),
        marker_id(index),
        DraftMarkerAdmissionPageIdentityV1::new(
            DraftMarkerAdmissionCommandIdV1::from_bytes([13; 16]),
            NonZeroU64::MIN,
        ),
        evidence(label(index), asset(index)),
        DraftMarkerAdmissionAssignmentGroupV1::PreserveLabel(label(index)),
        asset(index),
        if assigned {
            DraftMarkerAdmissionTargetDispositionV1::Assigned(label(index))
        } else {
            DraftMarkerAdmissionTargetDispositionV1::Unassigned
        },
    )
    .unwrap()
}

pub(super) fn child(node: &DraftMarkerAdmissionNodeV1) -> DraftMarkerAdmissionChildV1 {
    DraftMarkerAdmissionChildV1::new(
        node.key(),
        node.digest(),
        node.count().unwrap(),
        node.envelope().unwrap(),
    )
}

pub(super) fn internal(
    owner: DraftMarkerAdmissionOwnerV1,
    tree: DraftMarkerAdmissionTreeV1,
    height: u8,
    ordinal: usize,
    children: Vec<DraftMarkerAdmissionChildV1>,
) -> DraftMarkerAdmissionNodeV1 {
    DraftMarkerAdmissionNodeV1::internal(
        key(
            owner,
            DraftMarkerAdmissionNodeKindV1::Internal,
            if tree == DraftMarkerAdmissionTreeV1::SourceOrder {
                3
            } else {
                4
            },
            ordinal,
        ),
        tree,
        height,
        children.into_boxed_slice(),
    )
    .unwrap()
}

pub(super) fn root(node: &DraftMarkerAdmissionNodeV1) -> DraftMarkerAdmissionRootV1 {
    DraftMarkerAdmissionRootV1::new(
        node.tree(),
        node.key(),
        node.height(),
        node.digest(),
        node.count().unwrap(),
    )
    .unwrap()
}

pub(super) fn height_three_binary(left: usize, right: usize, assigned: bool) -> TreeFixture {
    assert!(left >= 2 && right >= 2);
    let owner = owner_for_test();
    let source_leaves = (0..left + right)
        .map(|index| source_leaf(owner, index))
        .collect::<Vec<_>>();
    let target_leaves = (0..left + right)
        .map(|index| target_leaf(owner, index, assigned))
        .collect::<Vec<_>>();
    let source_left = internal(
        owner,
        DraftMarkerAdmissionTreeV1::SourceOrder,
        2,
        0,
        source_leaves[..left].iter().map(child).collect(),
    );
    let source_right = internal(
        owner,
        DraftMarkerAdmissionTreeV1::SourceOrder,
        2,
        1,
        source_leaves[left..].iter().map(child).collect(),
    );
    let target_left = internal(
        owner,
        DraftMarkerAdmissionTreeV1::TargetId,
        2,
        0,
        target_leaves[..left].iter().map(child).collect(),
    );
    let target_right = internal(
        owner,
        DraftMarkerAdmissionTreeV1::TargetId,
        2,
        1,
        target_leaves[left..].iter().map(child).collect(),
    );
    let source_root = internal(
        owner,
        DraftMarkerAdmissionTreeV1::SourceOrder,
        3,
        2,
        vec![child(&source_left), child(&source_right)],
    );
    let target_root = internal(
        owner,
        DraftMarkerAdmissionTreeV1::TargetId,
        3,
        2,
        vec![child(&target_left), child(&target_right)],
    );
    let source_first_descent = vec![source_root.key(), source_left.key(), source_leaves[0].key()];
    let target_first_descent = vec![target_root.key(), target_left.key(), target_leaves[0].key()];
    let mut nodes = source_leaves;
    nodes.extend(target_leaves);
    nodes.extend([
        source_left.clone(),
        source_right.clone(),
        target_left.clone(),
        target_right.clone(),
    ]);
    nodes.extend([source_root.clone(), target_root.clone()]);
    TreeFixture {
        state: DraftMarkerAdmissionIndexTestStateV1::from_parts(
            owner,
            root(&source_root),
            root(&target_root),
            nodes,
            Vec::new(),
        ),
        source_first_descent,
        target_first_descent,
        source_right_sibling: source_right.key(),
        target_left_sibling: target_left.key(),
        target_right_sibling: target_right.key(),
    }
}

pub(super) fn consume_marker(index: usize) -> DraftPieceMarkerV1 {
    DraftPieceMarkerV1::new(marker_id(index), 0, label(index), asset(index))
}

pub(super) fn consume_identity(index: usize) -> DraftMarkerAdmissionPageIdentityV1 {
    DraftMarkerAdmissionPageIdentityV1::new(
        DraftMarkerAdmissionCommandIdV1::from_bytes([220; 16]),
        NonZeroU64::new(u64::try_from(index + 1).unwrap()).unwrap(),
    )
}

pub(super) fn replacement_receipt(
    receipt: &DraftMarkerAdmissionReplayReceiptV1,
    source_before: DraftMarkerAdmissionRootV1,
    target_before: DraftMarkerAdmissionRootV1,
    source_after: DraftMarkerAdmissionRootV1,
    target_after: DraftMarkerAdmissionRootV1,
    retained: Vec<DraftMarkerAdmissionChildV1>,
) -> DraftMarkerAdmissionReplayReceiptV1 {
    DraftMarkerAdmissionReplayReceiptV1::new(
        receipt.owner(),
        receipt.command_id(),
        receipt.page_ordinal(),
        receipt.request_commitment(),
        receipt.source_head_bytes(),
        receipt.target_head_bytes(),
        source_before,
        source_after,
        target_before,
        target_after,
        retained.into_boxed_slice(),
        receipt.transition(),
    )
    .unwrap()
}

pub(super) fn retained_children(
    state: &DraftMarkerAdmissionIndexTestStateV1,
    keys: &[DraftMarkerAdmissionNodeKeyV1],
) -> Vec<DraftMarkerAdmissionChildV1> {
    keys.iter()
        .map(|key| {
            child(
                state
                    .nodes()
                    .iter()
                    .find(|node| node.key() == *key)
                    .unwrap(),
            )
        })
        .collect()
}

pub(super) fn root_child_fanouts(
    state: &DraftMarkerAdmissionIndexTestStateV1,
    root: DraftMarkerAdmissionRootV1,
) -> Vec<usize> {
    let node = state
        .nodes()
        .iter()
        .find(|node| Some(node.key()) == root.node())
        .unwrap();
    let DraftMarkerAdmissionNodePayloadV1::Internal { children, .. } = node.payload() else {
        panic!("expected internal root");
    };
    children
        .iter()
        .map(|child| {
            let node = state
                .nodes()
                .iter()
                .find(|node| node.key() == child.key())
                .unwrap();
            let DraftMarkerAdmissionNodePayloadV1::Internal { children, .. } = node.payload()
            else {
                panic!("expected internal root child");
            };
            children.len()
        })
        .collect()
}

pub(super) fn selected_path_child_references(
    state: &DraftMarkerAdmissionIndexTestStateV1,
    root: DraftMarkerAdmissionRootV1,
) -> usize {
    let mut key = root.node().unwrap();
    let mut references = 0;
    loop {
        let node = state.nodes().iter().find(|node| node.key() == key).unwrap();
        let DraftMarkerAdmissionNodePayloadV1::Internal { children, .. } = node.payload() else {
            return references;
        };
        references += children.len();
        key = children[0].key();
    }
}

pub(super) fn live_right_target_subtree(
    state: &DraftMarkerAdmissionIndexTestStateV1,
) -> DraftMarkerAdmissionChildV1 {
    let root = state.target_root();
    let node = state
        .nodes()
        .iter()
        .find(|node| Some(node.key()) == root.node())
        .unwrap();
    let DraftMarkerAdmissionNodePayloadV1::Internal { children, .. } = node.payload() else {
        panic!("expected a branching target root");
    };
    let right = *children.last().unwrap();
    assert!(children.len() >= 2);
    assert!(state.nodes().iter().any(|node| {
        node.key() == right.key()
            && node.tree() == DraftMarkerAdmissionTreeV1::TargetId
            && child(node) == right
    }));
    right
}

pub(super) fn assigned_receipt_fixture() -> (
    TreeFixture,
    DraftMarkerAdmissionReplayReceiptV1,
    DraftMarkerAdmissionRootV1,
    DraftMarkerAdmissionRootV1,
) {
    let mut fixture = height_three_binary(2, 2, false);
    let source_before = fixture.state.source_root();
    let target_before = fixture.state.target_root();
    let source_leaf = fixture
        .state
        .nodes()
        .iter()
        .find(|node| node.key() == *fixture.source_first_descent.last().unwrap())
        .unwrap();
    let (source_group, source_asset) = match source_leaf.payload() {
        DraftMarkerAdmissionNodePayloadV1::SourceLeaf {
            source_key,
            asset_id,
            ..
        } => (source_key.group(), *asset_id),
        _ => panic!("expected source leaf"),
    };
    let step = fixture
        .state
        .assign_next(DraftMarkerAdmissionCommandIdV1::from_bytes([180; 16]), 0)
        .unwrap();
    let retained = retained_children(&fixture.state, step.retained_predecessor_nodes());
    let mut source_head = vec![0; 32];
    source_head.extend_from_slice(&source_group.canonical_bytes());
    source_head.extend_from_slice(&source_asset.digest());
    source_head.extend_from_slice(&source_asset.length().get().to_le_bytes());
    let mut target_head = Vec::new();
    target_head.extend_from_slice(target_before.digest().as_bytes());
    target_head.extend_from_slice(&target_before.count().to_le_bytes());
    let DraftMarkerAdmissionAssignmentGroupV1::PreserveLabel(label) = source_group else {
        panic!("expected preserve-label source");
    };
    target_head.extend_from_slice(&label.get().to_le_bytes());
    let receipt = DraftMarkerAdmissionReplayReceiptV1::new(
        owner_for_test(),
        DraftMarkerAdmissionCommandIdV1::from_bytes([180; 16]),
        NonZeroU64::MIN,
        syndic_storage::DraftMarkerAdmissionDigestV1::from_bytes([181; 32]),
        source_head,
        target_head,
        source_before,
        step.source_root(),
        target_before,
        step.target_root(),
        retained.into_boxed_slice(),
        DraftMarkerAdmissionReceiptTransitionV1::Assignment,
    )
    .unwrap();
    (fixture, receipt, source_before, target_before)
}
