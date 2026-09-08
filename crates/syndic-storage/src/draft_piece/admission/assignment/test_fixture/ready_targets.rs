use super::*;
use crate::{
    DRAFT_MARKER_ADMISSION_TREE_FANOUT, DraftMarkerAdmissionAssignmentContinuationV1,
    DraftMarkerAdmissionAssignmentGroupV1, DraftMarkerAdmissionChildV1,
    DraftMarkerAdmissionIndexTestStateV1, DraftMarkerAdmissionSourceKeyV1,
};
use sha2::{Digest, Sha256};

pub(super) fn ready_target_fixture_records(
    owner: DraftMarkerAdmissionOwnerV1,
    markers: &[DraftPieceMarkerV1],
    authority: &DraftMarkerLabelReadinessRequestAuthorityV1,
    command: DraftMarkerAdmissionCommandIdV1,
) -> Result<
    (
        Vec<DraftMarkerAdmissionNodeV1>,
        DraftMarkerAdmissionHeadV1,
        DraftMarkerAdmissionReplayReceiptV1,
    ),
    DraftMarkerLabelAssignmentErrorV1,
> {
    let empty_source =
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::SourceOrder);
    let mut markers = markers.to_vec();
    markers.sort_unstable_by_key(|marker| marker.marker_id());
    if markers
        .windows(2)
        .any(|pair| pair[0].marker_id() == pair[1].marker_id())
    {
        return Err(DraftMarkerLabelAssignmentErrorV1::Rejected);
    }
    let selected = *markers
        .iter()
        .max_by_key(|marker| (marker.label(), marker.marker_id()))
        .ok_or(DraftMarkerLabelAssignmentErrorV1::Rejected)?;
    let mut nodes = Vec::with_capacity(markers.len() + 3);
    let page = DraftMarkerAdmissionPageIdentityV1::new(command, NonZeroU64::MIN);
    for marker in &markers {
        let node_key = DraftMarkerAdmissionNodeKeyV1::new(
            owner,
            DraftMarkerAdmissionNodeKindV1::Leaf,
            DraftMarkerAdmissionNodeIdV1::from_bytes(*marker.marker_id().as_bytes()),
        );
        let mut evidence_bytes = vec![0; 145];
        evidence_bytes[0] = 1;
        evidence_bytes.extend_from_slice(&marker.label().get().to_le_bytes());
        evidence_bytes.push(marker.asset_id().version() as u8);
        evidence_bytes.extend_from_slice(&marker.asset_id().digest());
        evidence_bytes.extend_from_slice(&marker.asset_id().length().get().to_le_bytes());
        let evidence = DraftMarkerAdmissionEvidenceV1::new(evidence_bytes)
            .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
        let node = DraftMarkerAdmissionNodeV1::target_leaf(
            node_key,
            marker.marker_id(),
            page,
            evidence,
            marker.label(),
            marker.asset_id(),
            if marker.marker_id() == selected.marker_id() {
                DraftMarkerAdmissionTargetDispositionV1::Unassigned
            } else {
                DraftMarkerAdmissionTargetDispositionV1::Assigned(marker.label())
            },
        )
        .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
        nodes.push(node);
    }
    let target_before = build_target_root(owner, &mut nodes)?;
    let occurrence_count = target_before.count();
    let selected_evidence = nodes
        .iter()
        .find_map(|node| match node.payload() {
            DraftMarkerAdmissionNodePayloadV1::TargetLeaf {
                target_marker_id,
                evidence,
                ..
            } if *target_marker_id == selected.marker_id() => Some(evidence.clone()),
            _ => None,
        })
        .ok_or(DraftMarkerLabelAssignmentErrorV1::Rejected)?;
    let mut digest = Sha256::new();
    digest.update(b"syndic/test-ready-target-final-source/v1");
    digest.update(owner.operation_id().as_bytes());
    digest.update(selected.marker_id().as_bytes());
    let digest: [u8; 32] = digest.finalize().into();
    let source_id = DraftMarkerAdmissionNodeIdV1::from_bytes(digest[..16].try_into().unwrap());
    let group = DraftMarkerAdmissionAssignmentGroupV1::PreserveLabel(selected.label());
    let source = DraftMarkerAdmissionNodeV1::source_leaf(
        DraftMarkerAdmissionNodeKeyV1::new(owner, DraftMarkerAdmissionNodeKindV1::Leaf, source_id),
        DraftMarkerAdmissionSourceKeyV1::new(group, selected.marker_id()),
        selected_evidence,
        selected.asset_id(),
    )
    .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
    let source_before = DraftMarkerAdmissionRootV1::new(
        DraftMarkerAdmissionTreeV1::SourceOrder,
        source.key(),
        1,
        source.digest(),
        1,
    )
    .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
    nodes.push(source);
    let prior_source = markers
        .iter()
        .filter(|marker| marker.marker_id() != selected.marker_id())
        .max_by_key(|marker| (marker.label(), marker.marker_id()))
        .map(|marker| {
            (
                DraftMarkerAdmissionAssignmentGroupV1::PreserveLabel(marker.label()),
                marker.asset_id(),
                marker.label(),
            )
        });
    let before_head = DraftMarkerAdmissionHeadV1::new(
        owner,
        NonZeroU64::MIN,
        authority.home_generation,
        DraftMarkerAdmissionLifecycleV1::Assigning,
        authority.request_commitment(),
        authority.custody_commitment(),
        NonZeroU64::new(2).unwrap(),
        0,
        true,
        Some(DraftMarkerAdmissionCommandIdV1::from_bytes(
            digest[16..].try_into().unwrap(),
        )),
        source_before,
        target_before,
        authority.occurrence_commitment(),
        0,
        occurrence_count,
        1,
        Some(DraftMarkerAdmissionAssignmentContinuationV1::reuse(
            prior_source,
        )),
        0,
        DraftMarkerAdmissionRetainedChargeV1::new(1, occurrence_count, 0),
        None,
    )
    .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
    let mut state = DraftMarkerAdmissionIndexTestStateV1::from_parts(
        owner,
        source_before,
        target_before,
        nodes,
        Vec::new(),
    );
    let step = state
        .assign_next(command, occurrence_count - 1)
        .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
    let nodes = state.nodes().to_vec();
    let target_root = step.target_root();
    let mut source_bytes = before_head.digest().as_bytes().to_vec();
    source_bytes.extend_from_slice(&group.canonical_bytes());
    source_bytes.extend_from_slice(&selected.asset_id().digest());
    source_bytes.extend_from_slice(&selected.asset_id().length().get().to_le_bytes());
    let mut target_bytes = target_before.digest().as_bytes().to_vec();
    target_bytes.extend_from_slice(&target_before.count().to_le_bytes());
    target_bytes.extend_from_slice(&selected.label().get().to_le_bytes());
    let receipt = DraftMarkerAdmissionReplayReceiptV1::new(
        owner,
        command,
        NonZeroU64::MIN,
        authority.request_commitment(),
        source_bytes,
        target_bytes,
        source_before,
        empty_source,
        target_before,
        target_root,
        state.prior_replay_nodes().to_vec(),
        DraftMarkerAdmissionReceiptTransitionV1::Assignment,
    )
    .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
    let make_head = |charge| {
        DraftMarkerAdmissionHeadV1::new(
            owner,
            NonZeroU64::new(2).unwrap(),
            authority.home_generation,
            DraftMarkerAdmissionLifecycleV1::Ready,
            authority.request_commitment(),
            authority.custody_commitment(),
            NonZeroU64::new(2).unwrap(),
            0,
            true,
            Some(command),
            empty_source,
            target_root,
            authority.occurrence_commitment(),
            0,
            occurrence_count,
            0,
            None,
            occurrence_count,
            charge,
            None,
        )
    };
    let provisional = make_head(DraftMarkerAdmissionRetainedChargeV1::new(
        1,
        occurrence_count,
        0,
    ))
    .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
    let receipt_key = DraftMarkerAdmissionReceiptKeyV1::new(owner, command);
    let retained_bytes = nodes
        .iter()
        .try_fold(0_u64, |bytes, node| {
            bytes
                .checked_add(encoded_node_record_charge(&node.key(), node)?)
                .ok_or(super::super::DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)
        })
        .and_then(|bytes| {
            bytes
                .checked_add(encoded_head_record_charge(&owner, &provisional)?)
                .ok_or(super::super::DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)
        })
        .and_then(|bytes| {
            bytes
                .checked_add(encoded_receipt_record_charge(&receipt_key, &receipt)?)
                .ok_or(super::super::DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)
        })
        .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
    let head = make_head(DraftMarkerAdmissionRetainedChargeV1::new(
        1,
        occurrence_count,
        retained_bytes,
    ))
    .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
    Ok((nodes, head, receipt))
}

fn build_target_root(
    owner: DraftMarkerAdmissionOwnerV1,
    nodes: &mut Vec<DraftMarkerAdmissionNodeV1>,
) -> Result<DraftMarkerAdmissionRootV1, DraftMarkerLabelAssignmentErrorV1> {
    let mut level = nodes
        .iter()
        .map(target_child)
        .collect::<Result<Vec<_>, _>>()?;
    let mut height = 1_u8;
    let mut internal_ordinal = 0_u128;
    while level.len() > 1 {
        height += 1;
        let parent_count = level.len().div_ceil(DRAFT_MARKER_ADMISSION_TREE_FANOUT);
        let chunk_size = level.len().div_ceil(parent_count);
        let mut next = Vec::with_capacity(parent_count);
        for children in level.chunks(chunk_size) {
            let key = DraftMarkerAdmissionNodeKeyV1::new(
                owner,
                DraftMarkerAdmissionNodeKindV1::Internal,
                DraftMarkerAdmissionNodeIdV1::from_bytes(internal_ordinal.to_be_bytes()),
            );
            internal_ordinal += 1;
            let node = DraftMarkerAdmissionNodeV1::internal(
                key,
                DraftMarkerAdmissionTreeV1::TargetId,
                height,
                children.to_vec().into_boxed_slice(),
            )
            .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?;
            next.push(target_child(&node)?);
            nodes.push(node);
        }
        level = next;
    }
    let root = level
        .first()
        .ok_or(DraftMarkerLabelAssignmentErrorV1::Rejected)?;
    DraftMarkerAdmissionRootV1::new(
        DraftMarkerAdmissionTreeV1::TargetId,
        root.key(),
        height,
        root.digest(),
        root.count(),
    )
    .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)
}

fn target_child(
    node: &DraftMarkerAdmissionNodeV1,
) -> Result<DraftMarkerAdmissionChildV1, DraftMarkerLabelAssignmentErrorV1> {
    Ok(DraftMarkerAdmissionChildV1::new(
        node.key(),
        node.digest(),
        node.count()
            .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?,
        node.envelope()
            .map_err(|_| DraftMarkerLabelAssignmentErrorV1::Rejected)?,
    ))
}
