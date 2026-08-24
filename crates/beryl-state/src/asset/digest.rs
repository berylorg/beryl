use beryl_model::{
    AssetId, AssetReferenceSetDigest, AssetReferenceSetId, ImageLabelOrdinal, SyndicDraftMarkerId,
};
use sha2::{Digest, Sha256};

use super::AssetReferenceOrdinal;

pub(super) fn seed(set_id: AssetReferenceSetId) -> AssetReferenceSetDigest {
    let mut hash = Sha256::new();
    hash.update(b"beryl.asset-reference-set.v3\0");
    hash.update(set_id.as_bytes());
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
    hash.update(b"beryl.asset-reference-entry.v3\0");
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
