use beryl_model::{
    AssetId, AssetReferenceSetDigest, AssetReferenceSetId, ImageLabelOrdinal,
    SealedContentMarkerSummary, SyndicDraftMarkerId,
};
use sha2::{Digest, Sha256};

use super::AssetReferenceOrdinal;

pub(super) fn seed(
    set_id: AssetReferenceSetId,
    source: SealedContentMarkerSummary,
) -> AssetReferenceSetDigest {
    let mut hash = Sha256::new();
    hash.update(b"beryl.asset-reference-set.v2\0");
    hash.update(set_id.as_bytes());
    hash.update(source.content_id().as_bytes());
    hash.update(source.content_digest().as_bytes());
    hash.update(source.marker_digest());
    hash.update(source.marker_count().to_be_bytes());
    match source.maximum_image_label() {
        Some(label) => {
            hash.update([1]);
            hash.update(label.get().to_be_bytes());
        }
        None => hash.update([0]),
    }
    AssetReferenceSetDigest::from_bytes(hash.finalize().into())
}

pub(super) fn advance(
    previous: AssetReferenceSetDigest,
    ordinal: AssetReferenceOrdinal,
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
    first_ordinal: AssetReferenceOrdinal,
) -> AssetReferenceSetDigest {
    let mut hash = Sha256::new();
    hash.update(b"beryl.asset-reference-entry.v2\0");
    hash.update(previous.as_bytes());
    hash.update(ordinal.get().to_be_bytes());
    hash.update(marker_id.as_bytes());
    hash.update(label.get().to_be_bytes());
    hash.update([asset_id.version() as u8]);
    hash.update(asset_id.digest());
    hash.update(asset_id.length().get().to_be_bytes());
    hash.update(first_ordinal.get().to_be_bytes());
    AssetReferenceSetDigest::from_bytes(hash.finalize().into())
}
