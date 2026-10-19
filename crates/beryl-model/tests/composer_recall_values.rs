use std::{collections::BTreeSet, num::NonZeroU64};

use beryl_model::{
    AssetId, AssetReferenceSetDigest, AssetReferenceSetId, ComposerRecallMarkerAssociationV1,
    ComposerRecallMarkerPageV1, ComposerRecallSourceOwnerV1, ImageLabelOrdinal,
    OrderedMarkerAssetSummaryV1, SealedAssetReferenceSetProof, SequentialMarkerSummaryV1,
    SyndicAcceptedInputId, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId, SyndicTurnId,
    composer_recall_marker_correlation,
};

fn association(ordinal: u64) -> ComposerRecallMarkerAssociationV1 {
    ComposerRecallMarkerAssociationV1::new(
        ordinal,
        SyndicDraftMarkerId::from_bytes([1; 16]),
        ImageLabelOrdinal::new(7).unwrap(),
        AssetId::sha256_v1([2; 32], NonZeroU64::new(8).unwrap()),
    )
    .unwrap()
}

fn proof(id: u8) -> SealedAssetReferenceSetProof {
    SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([id; 16]),
        SequentialMarkerSummaryV1::new([3; 32], 2, Some(ImageLabelOrdinal::new(7).unwrap()))
            .unwrap(),
        OrderedMarkerAssetSummaryV1::new([4; 32], 2),
        2,
        AssetReferenceSetDigest::from_bytes([5; 32]),
    )
    .unwrap()
}

fn page_with(
    header: Vec<u8>,
    receipt_ordinal: u64,
    encoded_start: u64,
    source_piece: u64,
    association: ComposerRecallMarkerAssociationV1,
) -> Option<ComposerRecallMarkerPageV1> {
    ComposerRecallMarkerPageV1::new(
        SyndicDraftId::from_bytes([6; 16]),
        [7; 16],
        [8; 16],
        receipt_ordinal,
        [9; 32],
        header,
        SyndicDraftId::from_bytes([10; 16]),
        [11; 16],
        [12; 16],
        [13; 32],
        ComposerRecallSourceOwnerV1::AcceptedInput(SyndicAcceptedInputId::from_bytes([14; 16])),
        proof(15),
        encoded_start,
        source_piece,
        association,
    )
}

#[test]
fn recall_pages_bound_header_and_source_window_without_granting_authority() {
    assert!(
        ComposerRecallMarkerAssociationV1::new(
            0,
            association(1).marker_id(),
            association(1).label(),
            association(1).asset_id(),
        )
        .is_none()
    );
    assert!(page_with(Vec::new(), 1, 0, 1, association(1)).is_none());
    assert!(page_with(vec![0; 65_537], 1, 0, 1, association(1)).is_none());
    assert!(page_with(vec![0; 65_536], 1, u64::MAX - 25, 1, association(2)).is_some());
    assert!(page_with(vec![0], 0, 0, 1, association(1)).is_none());
    assert!(page_with(vec![0], 1, u64::MAX - 24, 1, association(1)).is_none());
    assert!(page_with(vec![0], 1, 0, 0, association(1)).is_none());
    assert!(page_with(vec![0], 1, 0, 1, association(3)).is_none());
}

fn rebind(page: &ComposerRecallMarkerPageV1, changed: usize) -> ComposerRecallMarkerPageV1 {
    let a = page.association();
    let association = ComposerRecallMarkerAssociationV1::new(
        if changed == 13 { 2 } else { a.ordinal() },
        if changed == 14 {
            SyndicDraftMarkerId::from_bytes([99; 16])
        } else {
            a.marker_id()
        },
        if changed == 15 {
            ImageLabelOrdinal::new(8).unwrap()
        } else {
            a.label()
        },
        if changed == 16 {
            AssetId::sha256_v1([99; 32], a.asset_id().length())
        } else if changed == 17 {
            AssetId::sha256_v1(a.asset_id().digest(), NonZeroU64::new(9).unwrap())
        } else {
            a.asset_id()
        },
    )
    .unwrap();
    let owner = match changed {
        9 => {
            ComposerRecallSourceOwnerV1::AcceptedInput(SyndicAcceptedInputId::from_bytes([99; 16]))
        }
        18 => ComposerRecallSourceOwnerV1::SubmittedTurnItem {
            turn_id: SyndicTurnId::from_bytes([14; 16]),
            item_id: SyndicItemId::from_bytes([14; 16]),
        },
        19 => ComposerRecallSourceOwnerV1::SubmittedTurnItem {
            turn_id: SyndicTurnId::from_bytes([99; 16]),
            item_id: SyndicItemId::from_bytes([14; 16]),
        },
        20 => ComposerRecallSourceOwnerV1::SubmittedTurnItem {
            turn_id: SyndicTurnId::from_bytes([14; 16]),
            item_id: SyndicItemId::from_bytes([99; 16]),
        },
        _ => page.source_owner(),
    };
    ComposerRecallMarkerPageV1::new(
        if changed == 0 {
            SyndicDraftId::from_bytes([99; 16])
        } else {
            page.draft_id()
        },
        if changed == 1 {
            [99; 16]
        } else {
            page.session_id()
        },
        if changed == 2 {
            [99; 16]
        } else {
            page.operation_id()
        },
        if changed == 3 {
            2
        } else {
            page.receipt_ordinal()
        },
        if changed == 4 {
            [99; 32]
        } else {
            page.receipt_digest()
        },
        if changed == 5 {
            b"different-header".to_vec()
        } else {
            page.canonical_header().to_vec()
        },
        if changed == 6 {
            SyndicDraftId::from_bytes([99; 16])
        } else {
            page.source_draft_id()
        },
        if changed == 7 {
            [99; 16]
        } else {
            page.source_session_id()
        },
        if changed == 8 {
            [99; 16]
        } else {
            page.source_operation_id()
        },
        if changed == 21 {
            [99; 32]
        } else {
            page.acceptance_digest()
        },
        owner,
        if changed == 10 {
            proof(99)
        } else {
            page.proof()
        },
        if changed == 11 {
            25
        } else {
            page.encoded_start()
        },
        if changed == 12 {
            2
        } else {
            page.source_piece()
        },
        association,
    )
    .unwrap()
}

#[test]
fn correlation_binds_acceptance_owner_operation_progress_and_every_marker_fact() {
    let page = page_with(b"header".to_vec(), 1, 0, 1, association(1)).unwrap();
    let original = composer_recall_marker_correlation(&page);
    assert_eq!(composer_recall_marker_correlation(&page.clone()), original);
    let mut distinct = BTreeSet::from([original]);
    for changed in 0..22 {
        assert!(
            distinct.insert(composer_recall_marker_correlation(&rebind(&page, changed))),
            "changed field {changed} must have its own correlation"
        );
    }
}
