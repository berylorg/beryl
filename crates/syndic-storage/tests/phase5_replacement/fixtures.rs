use super::*;

pub(super) fn target_asset_reference_set(
    content: ContentReference,
) -> SealedAssetReferenceSetProof {
    let source = content.sealed_marker_summary().unwrap();
    SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([24; 16]),
        source,
        source.marker_count(),
        AssetReferenceSetDigest::from_bytes([23; 32]),
    )
    .unwrap()
}
