use beryl_model::{
    OrderedMarkerAssetSummaryV1, SealedAssetReferenceSetProof, SequentialMarkerSummaryV1,
    ordered_marker_asset_digest_seed, sequential_marker_digest_seed,
};

use super::{
    AssetReferenceSetCompletionEvidence, AssetReferenceSetLifecycle, AssetReferenceSetManifest,
    AssetReferenceSetStagingAuthority, digest,
};

pub(super) fn initial_evidence(
    authority: AssetReferenceSetStagingAuthority,
    manifest: &AssetReferenceSetManifest,
) -> AssetReferenceSetCompletionEvidence {
    let authority_commitment = digest::authority_commitment(authority);
    AssetReferenceSetCompletionEvidence {
        set_id: manifest.set_id,
        authority_commitment,
        manifest_commitment: digest::manifest_commitment(authority_commitment, manifest),
        sealed_proof_commitment: None,
    }
}

pub(super) fn advance_building_evidence(
    evidence: AssetReferenceSetCompletionEvidence,
    manifest: &AssetReferenceSetManifest,
) -> AssetReferenceSetCompletionEvidence {
    AssetReferenceSetCompletionEvidence {
        set_id: manifest.set_id,
        authority_commitment: evidence.authority_commitment,
        manifest_commitment: digest::manifest_commitment(evidence.authority_commitment, manifest),
        sealed_proof_commitment: None,
    }
}

pub(super) fn seal_evidence(
    evidence: AssetReferenceSetCompletionEvidence,
    manifest: &AssetReferenceSetManifest,
    proof: SealedAssetReferenceSetProof,
) -> AssetReferenceSetCompletionEvidence {
    AssetReferenceSetCompletionEvidence {
        set_id: manifest.set_id,
        authority_commitment: evidence.authority_commitment,
        manifest_commitment: digest::manifest_commitment(evidence.authority_commitment, manifest),
        sealed_proof_commitment: Some(digest::sealed_proof_commitment(
            evidence.authority_commitment,
            proof,
        )),
    }
}

pub(super) fn matches_authority(
    evidence: &AssetReferenceSetCompletionEvidence,
    authority: AssetReferenceSetStagingAuthority,
) -> bool {
    evidence.set_id == authority.set_id
        && evidence.authority_commitment == digest::authority_commitment(authority)
}

pub(super) fn building_matches(
    manifest: &AssetReferenceSetManifest,
    evidence: &AssetReferenceSetCompletionEvidence,
) -> bool {
    manifest.lifecycle == AssetReferenceSetLifecycle::Building
        && evidence.set_id == manifest.set_id
        && evidence.sealed_proof_commitment.is_none()
        && evidence.manifest_commitment
            == digest::manifest_commitment(evidence.authority_commitment, manifest)
        && counts_match(manifest)
        && (manifest.entry_frontier != 0 || canonical_empty_manifest(manifest))
}

pub(super) fn sealed_matches(
    manifest: &AssetReferenceSetManifest,
    evidence: &AssetReferenceSetCompletionEvidence,
    proof: SealedAssetReferenceSetProof,
) -> bool {
    manifest.lifecycle == AssetReferenceSetLifecycle::Sealed
        && evidence.set_id == manifest.set_id
        && evidence.manifest_commitment
            == digest::manifest_commitment(evidence.authority_commitment, manifest)
        && counts_match(manifest)
        && evidence.sealed_proof_commitment
            == Some(digest::sealed_proof_commitment(
                evidence.authority_commitment,
                proof,
            ))
}

fn counts_match(manifest: &AssetReferenceSetManifest) -> bool {
    manifest.sequential.marker_count() == manifest.entry_frontier
        && manifest.ordered_assets.marker_count() == manifest.entry_frontier
}

fn canonical_empty_manifest(manifest: &AssetReferenceSetManifest) -> bool {
    manifest.sequential
        == SequentialMarkerSummaryV1::new(sequential_marker_digest_seed(), 0, None)
            .expect("canonical empty sequential marker summary is valid")
        && manifest.ordered_assets
            == OrderedMarkerAssetSummaryV1::new(ordered_marker_asset_digest_seed(), 0)
        && manifest.asset_chain_digest == digest::seed(manifest.set_id)
}
