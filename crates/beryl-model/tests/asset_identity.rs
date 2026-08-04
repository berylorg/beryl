use std::num::NonZeroU64;

use beryl_model::{AssetId, AssetIdentityVersion, ImageLabelOrdinal, ImageLabelOrdinalError};
use beryl_model::{
    AssetProofError, AssetReferenceSetDigest, AssetReferenceSetId, SealedAssetReferenceSetProof,
    SealedContentMarkerSummary, SyndicContentDigest, SyndicContentId, SyndicDraftMarkerId,
    advance_content_marker_digest, content_marker_digest_seed,
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
fn marker_free_and_marker_bearing_summaries_are_exact() {
    let content_id = SyndicContentId::from_bytes([8; 16]);
    let content_digest = SyndicContentDigest::from_bytes([9; 32]);
    let empty = SealedContentMarkerSummary::new(
        content_id,
        content_digest,
        content_marker_digest_seed(),
        0,
        None,
    )
    .unwrap();
    assert_eq!(empty.marker_count(), 0);
    assert_eq!(empty.maximum_image_label(), None);

    let label = ImageLabelOrdinal::new(27).unwrap();
    let marker_id = SyndicDraftMarkerId::from_bytes([10; 16]);
    let digest = advance_content_marker_digest(content_marker_digest_seed(), marker_id, label);
    let bearing =
        SealedContentMarkerSummary::new(content_id, content_digest, digest, 1, Some(label))
            .unwrap();
    assert_eq!(bearing.marker_digest(), digest);
    assert_eq!(bearing.maximum_image_label(), Some(label));
    assert_eq!(
        SealedContentMarkerSummary::new(content_id, content_digest, digest, 1, None),
        Err(AssetProofError::MarkerMaximumMismatch)
    );

    let set_id = AssetReferenceSetId::from_bytes([11; 16]);
    let chain = AssetReferenceSetDigest::from_bytes([12; 32]);
    let proof = SealedAssetReferenceSetProof::new(set_id, bearing, 1, chain).unwrap();
    assert_eq!(proof.source(), bearing);
    assert_eq!(proof.asset_chain_digest(), chain);
    assert_eq!(
        SealedAssetReferenceSetProof::new(set_id, bearing, 0, chain),
        Err(AssetProofError::EntryFrontierMismatch)
    );
}
