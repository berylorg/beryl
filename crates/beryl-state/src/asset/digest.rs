use beryl_model::{
    AssetId, AssetReferenceSetDigest, AssetReferenceSetId, ImageLabelOrdinal,
    SealedAssetReferenceSetProof, SyndicDraftMarkerId,
};
use sha2::{Digest, Sha256};

use super::{
    AssetReferenceOrdinal, AssetReferenceSetLifecycle, AssetReferenceSetManifest,
    AssetReferenceSetStagingAuthority,
};

pub(super) fn authority_commitment(authority: AssetReferenceSetStagingAuthority) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"beryl.asset-reference-set.staging-authority.v3\0");
    hash.update(authority.set_id.as_bytes());
    hash.update(authority.secret);
    hash.finalize().into()
}

pub(super) fn manifest_commitment(
    authority_commitment: [u8; 32],
    manifest: &AssetReferenceSetManifest,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"beryl.asset-reference-set.completion-manifest.v3\0");
    hash.update(authority_commitment);
    hash.update(manifest.set_id.as_bytes());
    hash.update([match manifest.lifecycle {
        AssetReferenceSetLifecycle::Building => 0,
        AssetReferenceSetLifecycle::Sealed => 1,
    }]);
    hash.update(manifest.sequential.marker_digest());
    hash.update(manifest.sequential.marker_count().to_be_bytes());
    match manifest.sequential.maximum_image_label() {
        Some(label) => {
            hash.update([1]);
            hash.update(label.get().to_be_bytes());
        }
        None => hash.update([0]),
    }
    hash.update(manifest.ordered_assets.marker_asset_digest());
    hash.update(manifest.ordered_assets.marker_count().to_be_bytes());
    hash.update(manifest.entry_frontier.to_be_bytes());
    hash.update(manifest.asset_chain_digest.as_bytes());
    hash.update(manifest.revision.get().to_be_bytes());
    hash.finalize().into()
}

pub(super) fn sealed_proof_commitment(
    authority_commitment: [u8; 32],
    proof: SealedAssetReferenceSetProof,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"beryl.asset-reference-set.completion-seal.v3\0");
    hash.update(authority_commitment);
    hash.update(proof.set_id().as_bytes());
    hash.update(proof.sequential().marker_digest());
    hash.update(proof.sequential().marker_count().to_be_bytes());
    match proof.sequential().maximum_image_label() {
        Some(label) => {
            hash.update([1]);
            hash.update(label.get().to_be_bytes());
        }
        None => hash.update([0]),
    }
    hash.update(proof.ordered_assets().marker_asset_digest());
    hash.update(proof.ordered_assets().marker_count().to_be_bytes());
    hash.update(proof.entry_frontier().to_be_bytes());
    hash.update(proof.asset_chain_digest().as_bytes());
    hash.finalize().into()
}

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
