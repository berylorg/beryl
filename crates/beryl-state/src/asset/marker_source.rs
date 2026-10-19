use beryl_home_store::{DomainReader, ReadError, VerifiedSidecar};
use beryl_model::{
    AssetId, ComposerRecallMarkerAssociationV1, HistoricalInputMarkerAssociationV1,
    ImageLabelOrdinal, SealedAssetReferenceSetProof, SyndicDraftMarkerId,
};

use super::{
    AssetDomain, AssetEntryKey, AssetMetadataCodec, AssetOwner, AssetOwnerHeadCodec,
    AssetReferenceCompletionEvidenceCodec, AssetReferenceEntryCodec, AssetReferenceManifestCodec,
    AssetReferenceOrdinal, AssetSidecarState, asset_sidecar_address, entry_point_limit,
    manifest_point_limit, metadata_point_limit, read,
};

pub(super) enum MarkerSourceError {
    Read(ReadError),
    Rejected,
}

impl From<ReadError> for MarkerSourceError {
    fn from(error: ReadError) -> Self {
        Self::Read(error)
    }
}

pub(super) struct MarkerSourceAssociation {
    ordinal: u64,
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
}

impl From<HistoricalInputMarkerAssociationV1> for MarkerSourceAssociation {
    fn from(value: HistoricalInputMarkerAssociationV1) -> Self {
        Self {
            ordinal: value.ordinal(),
            marker_id: value.marker_id(),
            label: value.label(),
            asset_id: value.asset_id(),
        }
    }
}

impl From<ComposerRecallMarkerAssociationV1> for MarkerSourceAssociation {
    fn from(value: ComposerRecallMarkerAssociationV1) -> Self {
        Self {
            ordinal: value.ordinal(),
            marker_id: value.marker_id(),
            label: value.label(),
            asset_id: value.asset_id(),
        }
    }
}

pub(super) fn prove_marker_source(
    reader: &DomainReader<'_, AssetDomain>,
    owner: AssetOwner,
    proof: SealedAssetReferenceSetProof,
    association: MarkerSourceAssociation,
    available: &VerifiedSidecar,
) -> Result<(), MarkerSourceError> {
    let head = reader
        .point::<AssetOwnerHeadCodec>(&owner, super::head_point_limit())?
        .ok_or(MarkerSourceError::Rejected)?;
    if head.owner() != owner || head.set() != proof {
        return Err(MarkerSourceError::Rejected);
    }
    let manifest = reader
        .point::<AssetReferenceManifestCodec>(&proof.set_id(), manifest_point_limit())?
        .ok_or(MarkerSourceError::Rejected)?;
    let completion = reader
        .point::<AssetReferenceCompletionEvidenceCodec>(
            &proof.set_id(),
            beryl_home_store::PointReadLimit::new(super::ASSET_COMPLETION_EVIDENCE_LIMIT + 4)
                .unwrap(),
        )?
        .ok_or(MarkerSourceError::Rejected)?;
    read::verify_sealed_manifest(&manifest, &completion, proof)
        .map_err(|_| MarkerSourceError::Rejected)?;
    let ordinal =
        AssetReferenceOrdinal::new(association.ordinal).map_err(|_| MarkerSourceError::Rejected)?;
    let entry = reader
        .point::<AssetReferenceEntryCodec>(
            &AssetEntryKey {
                set_id: proof.set_id(),
                ordinal,
            },
            entry_point_limit(),
        )?
        .ok_or(MarkerSourceError::Rejected)?;
    if entry.set_id() != proof.set_id()
        || entry.ordinal() != ordinal
        || entry.marker_id() != association.marker_id
        || entry.label() != association.label
        || entry.asset_id() != association.asset_id
    {
        return Err(MarkerSourceError::Rejected);
    }
    let metadata = reader
        .point::<AssetMetadataCodec>(&association.asset_id, metadata_point_limit())?
        .ok_or(MarkerSourceError::Rejected)?;
    if metadata.asset_id() != association.asset_id
        || metadata.sidecar_state() != AssetSidecarState::Committed
        || available.address() != &asset_sidecar_address(association.asset_id)
    {
        return Err(MarkerSourceError::Rejected);
    }
    Ok(())
}
