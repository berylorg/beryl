use std::num::NonZeroU64;

use beryl_model::{AssetId, AssetIdentityVersion, ImageLabelOrdinal, ImageLabelOrdinalError};
use beryl_model::{
    AssetProofError, AssetReferenceSetDigest, AssetReferenceSetId, DraftMarkerCommitmentV1,
    OrderedMarkerAssetSummaryV1, SealedAssetReferenceSetProof, SealedContentMarkerSummary,
    SequentialMarkerSummaryV1, SyndicContentDigest, SyndicContentId, SyndicDraftMarkerId,
    advance_ordered_marker_asset_digest, advance_sequential_marker_digest,
    ordered_marker_asset_digest_seed, sequential_marker_digest_seed,
};

#[test]
fn asset_identity_retains_version_digest_and_exact_length() {
    let digest = [0xab; 32];
    let length = NonZeroU64::new(42).unwrap();
    let asset = AssetId::sha256_v1(digest, length);

    assert_eq!(asset.version(), AssetIdentityVersion::Sha256V1);
    assert_eq!(asset.digest(), digest);
    assert_eq!(asset.length(), length);
}

#[test]
fn asset_identity_round_trips_through_serde() {
    let asset = AssetId::sha256_v1([7; 32], NonZeroU64::new(1_024).unwrap());
    let encoded = serde_json::to_string(&asset).unwrap();

    assert_eq!(serde_json::from_str::<AssetId>(&encoded).unwrap(), asset);
}

#[test]
fn image_label_ordinals_use_checked_bijective_letters() {
    assert_eq!(ImageLabelOrdinal::FIRST.to_string(), "A");
    assert_eq!(ImageLabelOrdinal::new(26).unwrap().to_string(), "Z");
    assert_eq!(ImageLabelOrdinal::new(27).unwrap().to_string(), "AA");
    assert_eq!(
        ImageLabelOrdinal::new(u64::MAX).unwrap().to_string(),
        "GKGWBYLWRXTLPO"
    );
    assert_eq!(ImageLabelOrdinal::new(0), Err(ImageLabelOrdinalError::Zero));
    assert_eq!(
        ImageLabelOrdinal::new(u64::MAX).unwrap().checked_next(),
        Err(ImageLabelOrdinalError::Exhausted)
    );
}

#[test]
fn marker_summaries_and_commitments_are_exact() {
    let content_id = SyndicContentId::from_bytes([8; 16]);
    let content_digest = SyndicContentDigest::from_bytes([9; 32]);
    let empty = SequentialMarkerSummaryV1::new(sequential_marker_digest_seed(), 0, None).unwrap();
    assert_eq!(empty.marker_count(), 0);
    assert_eq!(empty.maximum_image_label(), None);

    let label = ImageLabelOrdinal::new(27).unwrap();
    let marker_id = SyndicDraftMarkerId::from_bytes([10; 16]);
    let digest =
        advance_sequential_marker_digest(sequential_marker_digest_seed(), marker_id, label);
    let bearing = SequentialMarkerSummaryV1::new(digest, 1, Some(label)).unwrap();
    assert_eq!(bearing.marker_digest(), digest);
    assert_eq!(bearing.maximum_image_label(), Some(label));
    assert_eq!(
        SequentialMarkerSummaryV1::new(digest, 1, None),
        Err(AssetProofError::MarkerMaximumMismatch)
    );

    let commitment = DraftMarkerCommitmentV1::new([13; 32], 1, Some(label)).unwrap();
    assert_eq!(commitment.tree_root_digest(), [13; 32]);
    assert_eq!(commitment.maximum_image_label(), Some(label));
    assert_eq!(
        DraftMarkerCommitmentV1::new([13; 32], 0, Some(label)),
        Err(AssetProofError::MarkerMaximumMismatch)
    );

    let sealed = SealedContentMarkerSummary::new(content_id, content_digest, bearing);
    assert_eq!(sealed.sequential(), bearing);

    let asset = AssetId::sha256_v1([14; 32], NonZeroU64::new(15).unwrap());
    let ordered_assets = OrderedMarkerAssetSummaryV1::new(
        advance_ordered_marker_asset_digest(
            ordered_marker_asset_digest_seed(),
            marker_id,
            label,
            asset,
        ),
        1,
    );
    let set_id = AssetReferenceSetId::from_bytes([11; 16]);
    let chain = AssetReferenceSetDigest::from_bytes([12; 32]);
    let proof =
        SealedAssetReferenceSetProof::new(set_id, bearing, ordered_assets, 1, chain).unwrap();
    assert_eq!(proof.sequential(), bearing);
    assert_eq!(proof.ordered_assets(), ordered_assets);
    assert_eq!(proof.asset_chain_digest(), chain);
    assert_eq!(
        SealedAssetReferenceSetProof::new(set_id, bearing, ordered_assets, 0, chain),
        Err(AssetProofError::EntryFrontierMismatch)
    );
    assert_eq!(
        SealedAssetReferenceSetProof::new(
            set_id,
            bearing,
            OrderedMarkerAssetSummaryV1::new(ordered_assets.marker_asset_digest(), 2),
            1,
            chain,
        ),
        Err(AssetProofError::OrderedMarkerAssetCountMismatch)
    );
}

#[test]
fn ordered_marker_asset_summary_is_deterministic_and_order_sensitive() {
    let first_marker = SyndicDraftMarkerId::from_bytes([1; 16]);
    let second_marker = SyndicDraftMarkerId::from_bytes([2; 16]);
    let first_label = ImageLabelOrdinal::new(1).unwrap();
    let second_label = ImageLabelOrdinal::new(2).unwrap();
    let first_asset = AssetId::sha256_v1([3; 32], NonZeroU64::new(4).unwrap());
    let second_asset = AssetId::sha256_v1([5; 32], NonZeroU64::new(6).unwrap());

    let forward = advance_ordered_marker_asset_digest(
        advance_ordered_marker_asset_digest(
            ordered_marker_asset_digest_seed(),
            first_marker,
            first_label,
            first_asset,
        ),
        second_marker,
        second_label,
        second_asset,
    );
    let repeated_forward = advance_ordered_marker_asset_digest(
        advance_ordered_marker_asset_digest(
            ordered_marker_asset_digest_seed(),
            first_marker,
            first_label,
            first_asset,
        ),
        second_marker,
        second_label,
        second_asset,
    );
    let reverse = advance_ordered_marker_asset_digest(
        advance_ordered_marker_asset_digest(
            ordered_marker_asset_digest_seed(),
            second_marker,
            second_label,
            second_asset,
        ),
        first_marker,
        first_label,
        first_asset,
    );

    assert_eq!(forward, repeated_forward);
    assert_ne!(forward, reverse);
}

#[test]
fn ordered_marker_asset_summary_binds_asset_identity_but_not_sequential_summary() {
    let marker_id = SyndicDraftMarkerId::from_bytes([7; 16]);
    let label = ImageLabelOrdinal::new(8).unwrap();
    let first_asset = AssetId::sha256_v1([9; 32], NonZeroU64::new(10).unwrap());
    let second_asset = AssetId::sha256_v1([11; 32], NonZeroU64::new(10).unwrap());

    let first_sequential = SequentialMarkerSummaryV1::new(
        advance_sequential_marker_digest(sequential_marker_digest_seed(), marker_id, label),
        1,
        Some(label),
    )
    .unwrap();
    let second_sequential = SequentialMarkerSummaryV1::new(
        advance_sequential_marker_digest(sequential_marker_digest_seed(), marker_id, label),
        1,
        Some(label),
    )
    .unwrap();
    assert_eq!(first_sequential, second_sequential);

    let first_ordered_assets = OrderedMarkerAssetSummaryV1::new(
        advance_ordered_marker_asset_digest(
            ordered_marker_asset_digest_seed(),
            marker_id,
            label,
            first_asset,
        ),
        1,
    );
    let second_ordered_assets = OrderedMarkerAssetSummaryV1::new(
        advance_ordered_marker_asset_digest(
            ordered_marker_asset_digest_seed(),
            marker_id,
            label,
            second_asset,
        ),
        1,
    );
    assert_ne!(first_ordered_assets, second_ordered_assets);
}

#[test]
fn ordered_marker_asset_digest_has_the_canonical_v1_encoding() {
    let marker_id = SyndicDraftMarkerId::from_bytes([7; 16]);
    let label = ImageLabelOrdinal::new(8).unwrap();
    let asset = AssetId::sha256_v1([9; 32], NonZeroU64::new(10).unwrap());

    assert_eq!(
        ordered_marker_asset_digest_seed(),
        [
            0xe5, 0xd2, 0x51, 0xa2, 0x01, 0xcd, 0xbe, 0x8b, 0x13, 0x90, 0x6d, 0x6e, 0x15, 0xc4,
            0x88, 0x28, 0xe6, 0x50, 0x7b, 0x25, 0x2f, 0x55, 0x79, 0xd4, 0x21, 0xc6, 0x36, 0x3d,
            0x31, 0x4d, 0x28, 0x17,
        ]
    );
    assert_eq!(
        advance_ordered_marker_asset_digest(
            ordered_marker_asset_digest_seed(),
            marker_id,
            label,
            asset,
        ),
        [
            0x69, 0x0b, 0x6d, 0x13, 0xcc, 0xb1, 0x39, 0xbc, 0x0f, 0x8a, 0xe1, 0x27, 0xb9, 0x62,
            0xda, 0xe1, 0x53, 0x89, 0xb1, 0xc9, 0xb6, 0x26, 0x62, 0x4e, 0x93, 0x01, 0xea, 0xad,
            0x59, 0xf6, 0x64, 0x90,
        ]
    );
}
