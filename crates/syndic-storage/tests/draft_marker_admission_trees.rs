#![cfg(feature = "test-faults")]

use std::num::NonZeroU64;

use beryl_model::{AssetId, ImageLabelOrdinal, SyndicDraftId, SyndicDraftMarkerId};
use syndic_storage::{
    DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES, DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS,
    DRAFT_MARKER_ADMISSION_MAX_ENCODED_BYTES, DRAFT_MARKER_ADMISSION_MAX_HEADS,
    DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT, DraftEditorCandidateSessionIdV1,
    DraftMarkerAdmissionChildV1, DraftMarkerAdmissionCodecFixtureV1,
    DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionEvidenceV1, DraftMarkerAdmissionNodeIdV1,
    DraftMarkerAdmissionNodeKeyV1, DraftMarkerAdmissionNodeKindV1, DraftMarkerAdmissionNodeV1,
    DraftMarkerAdmissionOperationIdV1, DraftMarkerAdmissionOwnerV1,
    DraftMarkerAdmissionPageIdentityV1, DraftMarkerAdmissionRetainedChargeV1,
    DraftMarkerAdmissionRootV1, DraftMarkerAdmissionSchemaErrorV1, DraftMarkerAdmissionSourceKeyV1,
    DraftMarkerAdmissionTargetDispositionV1, DraftMarkerAdmissionTreeV1,
    canonical_empty_draft_marker_admission_root_v1,
    checked_draft_marker_admission_capacity_successor_v1,
    checked_draft_marker_admission_command_charge_v1, draft_marker_admission_codec_accepts,
    draft_marker_admission_corrupted_value_rejected,
};

fn owner() -> DraftMarkerAdmissionOwnerV1 {
    DraftMarkerAdmissionOwnerV1::new(
        SyndicDraftId::from_bytes([20; 16]),
        DraftEditorCandidateSessionIdV1::from_bytes([21; 16]),
        DraftMarkerAdmissionOperationIdV1::from_bytes([22; 16]),
    )
}

fn asset(seed: u8) -> AssetId {
    AssetId::sha256_v1([seed; 32], NonZeroU64::MIN)
}

fn label(value: u64) -> ImageLabelOrdinal {
    ImageLabelOrdinal::new(value).unwrap()
}

fn source_leaf(id: u8, label_value: u64, marker: u8) -> DraftMarkerAdmissionNodeV1 {
    DraftMarkerAdmissionNodeV1::source_leaf(
        DraftMarkerAdmissionNodeKeyV1::new(
            owner(),
            DraftMarkerAdmissionNodeKindV1::Leaf,
            DraftMarkerAdmissionNodeIdV1::from_bytes([id; 16]),
        ),
        DraftMarkerAdmissionSourceKeyV1::new(
            label(label_value),
            SyndicDraftMarkerId::from_bytes([marker; 16]),
        ),
        DraftMarkerAdmissionEvidenceV1::new([id, marker].as_slice()).unwrap(),
        asset(id),
    )
    .unwrap()
}

fn child(node: &DraftMarkerAdmissionNodeV1) -> DraftMarkerAdmissionChildV1 {
    DraftMarkerAdmissionChildV1::new(
        node.key(),
        node.digest(),
        node.count().unwrap(),
        node.envelope().unwrap(),
    )
}

#[test]
fn canonical_empty_roots_have_no_nodes_and_tree_specific_digests() {
    let source =
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::SourceOrder);
    let target =
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::TargetId);
    assert_eq!(source.node(), None);
    assert_eq!(source.height(), 0);
    assert_eq!(source.count(), 0);
    assert_eq!(target.node(), None);
    assert_ne!(source.digest(), target.digest());
    assert_eq!(
        DraftMarkerAdmissionRootV1::new(
            DraftMarkerAdmissionTreeV1::SourceOrder,
            DraftMarkerAdmissionNodeKeyV1::new(
                owner(),
                DraftMarkerAdmissionNodeKindV1::Leaf,
                DraftMarkerAdmissionNodeIdV1::from_bytes([1; 16]),
            ),
            0,
            source.digest(),
            1,
        ),
        Err(DraftMarkerAdmissionSchemaErrorV1::InvalidRoot)
    );
    assert_eq!(
        DraftMarkerAdmissionRootV1::new(
            DraftMarkerAdmissionTreeV1::SourceOrder,
            DraftMarkerAdmissionNodeKeyV1::new(
                owner(),
                DraftMarkerAdmissionNodeKindV1::Leaf,
                DraftMarkerAdmissionNodeIdV1::from_bytes([1; 16]),
            ),
            1,
            source.digest(),
            DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS + 1,
        ),
        Err(DraftMarkerAdmissionSchemaErrorV1::InvalidRoot)
    );
}

#[test]
fn authenticated_nodes_enforce_tree_kind_fanout_height_count_envelopes_and_digest() {
    let first = source_leaf(1, 1, 1);
    let second = source_leaf(2, 2, 2);
    let root = DraftMarkerAdmissionNodeV1::internal(
        DraftMarkerAdmissionNodeKeyV1::new(
            owner(),
            DraftMarkerAdmissionNodeKindV1::Internal,
            DraftMarkerAdmissionNodeIdV1::from_bytes([3; 16]),
        ),
        DraftMarkerAdmissionTreeV1::SourceOrder,
        2,
        vec![child(&first), child(&second)],
    )
    .unwrap();
    assert_eq!(root.count().unwrap(), 2);
    assert!(draft_marker_admission_codec_accepts(
        DraftMarkerAdmissionCodecFixtureV1::Node(root.clone())
    ));
    assert!(draft_marker_admission_corrupted_value_rejected(
        DraftMarkerAdmissionCodecFixtureV1::Node(root)
    ));

    let internal_key = DraftMarkerAdmissionNodeKeyV1::new(
        owner(),
        DraftMarkerAdmissionNodeKindV1::Internal,
        DraftMarkerAdmissionNodeIdV1::from_bytes([4; 16]),
    );
    assert_eq!(
        DraftMarkerAdmissionNodeV1::internal(
            internal_key,
            DraftMarkerAdmissionTreeV1::SourceOrder,
            2,
            Vec::new(),
        ),
        Err(DraftMarkerAdmissionSchemaErrorV1::NodeFanout)
    );
    assert_eq!(
        DraftMarkerAdmissionNodeV1::internal(
            internal_key,
            DraftMarkerAdmissionTreeV1::SourceOrder,
            DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT + 1,
            vec![child(&first)],
        ),
        Err(DraftMarkerAdmissionSchemaErrorV1::TreeHeight)
    );
    assert_eq!(
        DraftMarkerAdmissionNodeV1::internal(
            internal_key,
            DraftMarkerAdmissionTreeV1::SourceOrder,
            2,
            vec![child(&second), child(&first)],
        ),
        Err(DraftMarkerAdmissionSchemaErrorV1::InvalidEnvelope)
    );
    let overflowing = [
        DraftMarkerAdmissionChildV1::new(
            first.key(),
            first.digest(),
            u64::MAX,
            first.envelope().unwrap(),
        ),
        child(&second),
    ];
    assert_eq!(
        DraftMarkerAdmissionNodeV1::internal(
            internal_key,
            DraftMarkerAdmissionTreeV1::SourceOrder,
            2,
            overflowing,
        ),
        Err(DraftMarkerAdmissionSchemaErrorV1::InvalidCount)
    );
}

#[test]
fn target_leaf_and_evidence_bounds_are_canonical() {
    let target = DraftMarkerAdmissionNodeV1::target_leaf(
        DraftMarkerAdmissionNodeKeyV1::new(
            owner(),
            DraftMarkerAdmissionNodeKindV1::Leaf,
            DraftMarkerAdmissionNodeIdV1::from_bytes([10; 16]),
        ),
        SyndicDraftMarkerId::from_bytes([11; 16]),
        DraftMarkerAdmissionPageIdentityV1::new(
            DraftMarkerAdmissionCommandIdV1::from_bytes([12; 16]),
            NonZeroU64::MIN,
        ),
        DraftMarkerAdmissionEvidenceV1::new([13, 14].as_slice()).unwrap(),
        label(15),
        asset(16),
        DraftMarkerAdmissionTargetDispositionV1::Assigned(label(17)),
    )
    .unwrap();
    assert!(draft_marker_admission_codec_accepts(
        DraftMarkerAdmissionCodecFixtureV1::Node(target)
    ));
    assert_eq!(
        DraftMarkerAdmissionEvidenceV1::new(Vec::<u8>::new()),
        Err(DraftMarkerAdmissionSchemaErrorV1::EvidenceLength)
    );
    assert_eq!(
        DraftMarkerAdmissionEvidenceV1::new(vec![0; 65_537]),
        Err(DraftMarkerAdmissionSchemaErrorV1::EvidenceLength)
    );
}

#[test]
fn checked_charge_primitives_enforce_aggregate_and_command_envelopes() {
    let maximum = DraftMarkerAdmissionRetainedChargeV1::new(
        DRAFT_MARKER_ADMISSION_MAX_HEADS,
        DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS,
        DRAFT_MARKER_ADMISSION_MAX_ENCODED_BYTES,
    );
    assert_eq!(
        checked_draft_marker_admission_capacity_successor_v1(
            maximum,
            DraftMarkerAdmissionRetainedChargeV1::new(1, 1, 1),
            DraftMarkerAdmissionRetainedChargeV1::new(1, 1, 1),
        ),
        Ok(maximum)
    );
    assert_eq!(
        checked_draft_marker_admission_capacity_successor_v1(
            maximum,
            DraftMarkerAdmissionRetainedChargeV1::ZERO,
            DraftMarkerAdmissionRetainedChargeV1::new(1, 0, 0),
        ),
        Err(DraftMarkerAdmissionSchemaErrorV1::CapacityExceeded)
    );
    for successor in [
        DraftMarkerAdmissionRetainedChargeV1::new(
            0,
            DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS + 1,
            0,
        ),
        DraftMarkerAdmissionRetainedChargeV1::new(
            0,
            0,
            DRAFT_MARKER_ADMISSION_MAX_ENCODED_BYTES + 1,
        ),
    ] {
        assert_eq!(
            checked_draft_marker_admission_capacity_successor_v1(
                DraftMarkerAdmissionRetainedChargeV1::ZERO,
                DraftMarkerAdmissionRetainedChargeV1::ZERO,
                successor,
            ),
            Err(DraftMarkerAdmissionSchemaErrorV1::CapacityExceeded)
        );
    }
    assert_eq!(
        checked_draft_marker_admission_capacity_successor_v1(
            DraftMarkerAdmissionRetainedChargeV1::ZERO,
            DraftMarkerAdmissionRetainedChargeV1::new(1, 0, 0),
            DraftMarkerAdmissionRetainedChargeV1::ZERO,
        ),
        Err(DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)
    );
    assert_eq!(
        checked_draft_marker_admission_command_charge_v1([
            DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES - 1,
            1,
        ]),
        Ok(DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES)
    );
    assert_eq!(
        checked_draft_marker_admission_command_charge_v1([
            DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES,
            1,
        ]),
        Err(DraftMarkerAdmissionSchemaErrorV1::CommandTooLarge)
    );
    assert_eq!(
        checked_draft_marker_admission_command_charge_v1([u64::MAX, 1]),
        Err(DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)
    );
}
