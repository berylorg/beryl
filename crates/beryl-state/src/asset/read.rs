use beryl_home_store::{HomeStore, ReadError};
use beryl_model::{
    AssetReferenceSetId, OrderedMarkerAssetSummaryV1, SealedAssetReferenceSetProof,
    SequentialMarkerSummaryV1,
};

use super::{
    ASSET_COMPLETION_EVIDENCE_LIMIT, AssetDomain, AssetReadError, AssetReferenceSetCompletion,
    AssetReferenceSetCompletionEvidence, AssetReferenceSetLifecycle, AssetReferenceSetManifest,
    AssetReferenceSetStagingAuthority, AssetState,
    codec::{AssetReferenceCompletionEvidenceCodec, AssetReferenceManifestCodec},
    completion, manifest_point_limit,
};

impl AssetState {
    pub fn complete_reference_set(
        &self,
        store: &HomeStore,
        authority: AssetReferenceSetStagingAuthority,
        expected_sequential: SequentialMarkerSummaryV1,
        expected_ordered_assets: OrderedMarkerAssetSummaryV1,
    ) -> Result<AssetReferenceSetCompletion, AssetReadError> {
        let manifest = read_manifest(self, store, authority.set_id)?
            .ok_or(AssetReadError::ReferenceSetMissing(authority.set_id))?;
        let evidence = read_completion_evidence(self, store, authority.set_id)?
            .ok_or(AssetReadError::CompletionEvidenceMismatch(authority.set_id))?;
        if !completion::matches_authority(&evidence, authority) {
            return Err(AssetReadError::CompletionMismatch(authority.set_id));
        }
        if manifest.lifecycle() == AssetReferenceSetLifecycle::Building {
            if !completion::building_matches(&manifest, &evidence) {
                return Err(AssetReadError::CompletionEvidenceMismatch(authority.set_id));
            }
            return Ok(AssetReferenceSetCompletion::Building(manifest));
        }
        let proof = SealedAssetReferenceSetProof::new(
            manifest.set_id(),
            manifest.sequential(),
            manifest.ordered_assets(),
            manifest.entry_frontier(),
            manifest.asset_chain_digest(),
        )
        .map_err(|_| AssetReadError::CompletionEvidenceMismatch(authority.set_id))?;
        verify_sealed_manifest(&manifest, &evidence, proof)?;
        if manifest.set_id() != authority.set_id
            || manifest.sequential() != expected_sequential
            || manifest.ordered_assets() != expected_ordered_assets
            || manifest.entry_frontier() != expected_sequential.marker_count()
        {
            return Err(AssetReadError::CompletionMismatch(authority.set_id));
        }
        Ok(AssetReferenceSetCompletion::Sealed(proof))
    }
}

pub(super) fn require_sealed_manifest(
    state: &AssetState,
    store: &HomeStore,
    proof: SealedAssetReferenceSetProof,
) -> Result<AssetReferenceSetManifest, AssetReadError> {
    let manifest = read_manifest(state, store, proof.set_id())?
        .ok_or(AssetReadError::ReferenceSetMissing(proof.set_id()))?;
    let evidence = read_completion_evidence(state, store, proof.set_id())?
        .ok_or(AssetReadError::CompletionEvidenceMismatch(proof.set_id()))?;
    verify_sealed_manifest(&manifest, &evidence, proof)?;
    Ok(manifest)
}

pub(super) fn require_building_manifest(
    state: &AssetState,
    store: &HomeStore,
    authority: AssetReferenceSetStagingAuthority,
) -> Result<AssetReferenceSetManifest, AssetReadError> {
    let manifest = read_manifest(state, store, authority.set_id)?
        .ok_or(AssetReadError::ReferenceSetMissing(authority.set_id))?;
    let evidence = read_completion_evidence(state, store, authority.set_id)?
        .ok_or(AssetReadError::CompletionEvidenceMismatch(authority.set_id))?;
    if manifest.lifecycle() != AssetReferenceSetLifecycle::Building {
        return Err(AssetReadError::ReferenceSetNotBuilding(authority.set_id));
    }
    if !completion::matches_authority(&evidence, authority) {
        return Err(AssetReadError::CompletionMismatch(authority.set_id));
    }
    if !completion::building_matches(&manifest, &evidence) {
        return Err(AssetReadError::CompletionEvidenceMismatch(authority.set_id));
    }
    Ok(manifest)
}

pub(super) fn verify_sealed_manifest(
    manifest: &AssetReferenceSetManifest,
    evidence: &AssetReferenceSetCompletionEvidence,
    proof: SealedAssetReferenceSetProof,
) -> Result<(), AssetReadError> {
    if manifest.lifecycle() != AssetReferenceSetLifecycle::Sealed {
        return Err(AssetReadError::ReferenceSetNotSealed(proof.set_id()));
    }
    if SealedAssetReferenceSetProof::new(
        manifest.set_id(),
        manifest.sequential(),
        manifest.ordered_assets(),
        manifest.entry_frontier(),
        manifest.asset_chain_digest(),
    )
    .ok()
        != Some(proof)
    {
        return Err(AssetReadError::SealedProofMismatch(proof.set_id()));
    }
    if !completion::sealed_matches(manifest, evidence, proof) {
        return Err(AssetReadError::CompletionEvidenceMismatch(proof.set_id()));
    }
    Ok(())
}

fn read_completion_evidence(
    state: &AssetState,
    store: &HomeStore,
    set_id: AssetReferenceSetId,
) -> Result<Option<AssetReferenceSetCompletionEvidence>, ReadError> {
    store.read_point::<AssetDomain, AssetReferenceCompletionEvidenceCodec>(
        &state.handle,
        &set_id,
        completion_evidence_point_limit(),
    )
}

fn completion_evidence_point_limit() -> beryl_home_store::PointReadLimit {
    beryl_home_store::PointReadLimit::new(ASSET_COMPLETION_EVIDENCE_LIMIT + 4)
        .expect("completion evidence point bound is nonzero")
}

pub(super) fn read_manifest(
    state: &AssetState,
    store: &HomeStore,
    set_id: AssetReferenceSetId,
) -> Result<Option<AssetReferenceSetManifest>, ReadError> {
    store.read_point::<AssetDomain, AssetReferenceManifestCodec>(
        &state.handle,
        &set_id,
        manifest_point_limit(),
    )
}
