use beryl_home_store::{
    DomainMutation, DomainReader, MutationBuilder, PointReadLimit, ReconciliationReservation,
};
use beryl_model::{
    AssetId, AssetReferenceSetId, DomainRevision, ImageLabelOrdinal, OrderedMarkerAssetSummaryV1,
    SealedAssetReferenceSetProof, SequentialMarkerSummaryV1, SyndicDraftMarkerId,
    advance_ordered_marker_asset_digest, advance_sequential_marker_digest,
    ordered_marker_asset_digest_seed, sequential_marker_digest_seed,
};

use crate::RecordRevision;

use super::{
    ASSET_COMPLETION_EVIDENCE_LIMIT, ASSET_ENTRY_LIMIT, ASSET_INDEX_LIMIT, ASSET_MANIFEST_LIMIT,
    ASSET_METADATA_LIMIT, ASSET_REFERENCE_PAGE_MAX_ENTRIES, AssetDimensions, AssetDomain,
    AssetEntryKey, AssetLabelDisposition, AssetLabelFirstKey, AssetLabelFirstRecord,
    AssetMarkerKey, AssetMediaType, AssetMetadataRecord, AssetMutationError,
    AssetReferenceEntryRecord, AssetReferenceOrdinal, AssetReferencePageError,
    AssetReferenceSetBuildProof, AssetReferenceSetCompletionEvidence, AssetReferenceSetLifecycle,
    AssetReferenceSetManifest, AssetReferenceSetStagingAuthority, AssetSidecarState,
    codec::{
        AssetMetadataCodec, AssetReferenceCompletionEvidenceCodec, AssetReferenceEntryCodec,
        AssetReferenceLabelFirstCodec, AssetReferenceManifestCodec, AssetReferenceMarkerCodec,
    },
    completion, digest,
};

mod owner_heads;
mod reference_sets;

pub use owner_heads::{
    AssetOwnerHeadAssertion, AssetOwnerHeadUpdate, UpdateAssetOwnerHeads, ValidateAssetOwnerHeads,
};
pub use reference_sets::{
    AppendAssetReferencePage, AssetReferencePageEntry, BeginAssetReferenceSet,
    SealAssetReferenceSet,
};

/// Publishes immutable metadata after its exact sidecar has become durable.
pub struct PublishAssetMetadata {
    asset_id: AssetId,
    media_type: AssetMediaType,
    dimensions: Option<AssetDimensions>,
    creation_revision: DomainRevision,
}

impl PublishAssetMetadata {
    #[must_use]
    pub const fn new(
        asset_id: AssetId,
        media_type: AssetMediaType,
        dimensions: Option<AssetDimensions>,
        creation_revision: DomainRevision,
    ) -> Self {
        Self {
            asset_id,
            media_type,
            dimensions,
            creation_revision,
        }
    }

    #[must_use]
    pub const fn asset_id(&self) -> AssetId {
        self.asset_id
    }

    #[must_use]
    pub const fn creation_revision(&self) -> DomainRevision {
        self.creation_revision
    }
}

impl DomainMutation<AssetDomain> for PublishAssetMetadata {
    type Error = AssetMutationError;
    type Prepared = Self;

    fn prepare(
        self,
        reader: &DomainReader<'_, AssetDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if read_metadata(reader, self.asset_id)?.is_some() {
            return Err(AssetMutationError::MetadataAlreadyExists(self.asset_id));
        }
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<AssetMetadataCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<AssetMetadataCodec>(
            &prepared.asset_id,
            &AssetMetadataRecord {
                asset_id: prepared.asset_id,
                media_type: prepared.media_type,
                dimensions: prepared.dimensions,
                creation_revision: prepared.creation_revision,
                sidecar_state: AssetSidecarState::Committed,
                revision: RecordRevision::INITIAL,
            },
        )?;
        Ok(())
    }
}

fn require_build_proof(
    manifest: &AssetReferenceSetManifest,
    proof: AssetReferenceSetBuildProof,
) -> Result<(), AssetMutationError> {
    if manifest.lifecycle != AssetReferenceSetLifecycle::Building {
        return Err(AssetMutationError::ReferenceSetNotBuilding(manifest.set_id));
    }
    if manifest.build_proof() != proof {
        return Err(AssetMutationError::BuildProofMismatch(manifest.set_id));
    }
    Ok(())
}

fn require_complete_marker_summaries(
    manifest: &AssetReferenceSetManifest,
    sequential: SequentialMarkerSummaryV1,
    ordered_assets: OrderedMarkerAssetSummaryV1,
) -> Result<(), AssetMutationError> {
    if manifest.sequential != sequential
        || manifest.ordered_assets != ordered_assets
        || manifest.entry_frontier != sequential.marker_count()
    {
        return Err(AssetMutationError::MarkerSummaryMismatch(manifest.set_id));
    }
    Ok(())
}

fn read_metadata(
    reader: &DomainReader<'_, AssetDomain>,
    asset_id: AssetId,
) -> Result<Option<AssetMetadataRecord>, AssetMutationError> {
    reader
        .point::<AssetMetadataCodec>(&asset_id, metadata_limit())
        .map_err(Into::into)
}

fn require_metadata(
    reader: &DomainReader<'_, AssetDomain>,
    asset_id: AssetId,
) -> Result<AssetMetadataRecord, AssetMutationError> {
    read_metadata(reader, asset_id)?.ok_or(AssetMutationError::MetadataMissing(asset_id))
}

fn read_manifest(
    reader: &DomainReader<'_, AssetDomain>,
    set_id: AssetReferenceSetId,
) -> Result<Option<AssetReferenceSetManifest>, AssetMutationError> {
    reader
        .point::<AssetReferenceManifestCodec>(&set_id, manifest_limit())
        .map_err(Into::into)
}

fn require_building_evidence(
    reader: &DomainReader<'_, AssetDomain>,
    manifest: &AssetReferenceSetManifest,
) -> Result<AssetReferenceSetCompletionEvidence, AssetMutationError> {
    let evidence = reader
        .point::<AssetReferenceCompletionEvidenceCodec>(
            &manifest.set_id,
            completion_evidence_limit(),
        )?
        .ok_or(AssetMutationError::BuildProofMismatch(manifest.set_id))?;
    if !completion::building_matches(manifest, &evidence) {
        return Err(AssetMutationError::BuildProofMismatch(manifest.set_id));
    }
    Ok(evidence)
}

fn require_manifest(
    reader: &DomainReader<'_, AssetDomain>,
    set_id: AssetReferenceSetId,
) -> Result<AssetReferenceSetManifest, AssetMutationError> {
    read_manifest(reader, set_id)?.ok_or(AssetMutationError::ReferenceSetMissing(set_id))
}

fn metadata_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_METADATA_LIMIT + 4).expect("metadata point bound is nonzero")
}

fn manifest_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_MANIFEST_LIMIT + 4).expect("manifest point bound is nonzero")
}

fn completion_evidence_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_COMPLETION_EVIDENCE_LIMIT + 4)
        .expect("completion evidence point bound is nonzero")
}

fn entry_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_ENTRY_LIMIT + 4).expect("entry point bound is nonzero")
}

fn index_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_INDEX_LIMIT + 4).expect("index point bound is nonzero")
}
