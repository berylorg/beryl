use beryl_home_store::{
    DomainMutation, DomainReader, MutationBuilder, PointReadLimit, ReconciliationReservation,
};
use beryl_model::{
    AssetId, AssetReferenceSetId, DomainRevision, ImageLabelOrdinal, SealedContentMarkerSummary,
    SyndicDraftMarkerId, advance_content_marker_digest, content_marker_digest_seed,
};

use crate::RecordRevision;

use super::{
    ASSET_ENTRY_LIMIT, ASSET_INDEX_LIMIT, ASSET_MANIFEST_LIMIT, ASSET_METADATA_LIMIT,
    ASSET_REFERENCE_PAGE_MAX_ENTRIES, AssetDimensions, AssetDomain, AssetEntryKey,
    AssetLabelDisposition, AssetLabelFirstKey, AssetLabelFirstRecord, AssetMarkerKey,
    AssetMediaType, AssetMetadataRecord, AssetMutationError, AssetReferenceEntryRecord,
    AssetReferenceOrdinal, AssetReferencePageError, AssetReferenceSetBuildProof,
    AssetReferenceSetLifecycle, AssetReferenceSetManifest, AssetReferenceSetStagingAuthority,
    AssetSidecarState,
    codec::{
        AssetMetadataCodec, AssetReferenceEntryCodec, AssetReferenceLabelFirstCodec,
        AssetReferenceManifestCodec, AssetReferenceMarkerCodec,
    },
    digest,
};

mod owner_heads;

pub use owner_heads::{
    AssetOwnerHeadAssertion, AssetOwnerHeadUpdate, UpdateAssetOwnerHeads, ValidateAssetOwnerHeads,
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

    fn validate(&self, reader: &DomainReader<'_, AssetDomain>) -> Result<(), Self::Error> {
        if read_metadata(reader, self.asset_id)?.is_some() {
            return Err(AssetMutationError::MetadataAlreadyExists(self.asset_id));
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<AssetMetadataCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, AssetDomain>,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<AssetMetadataCodec>(
            &self.asset_id,
            &AssetMetadataRecord {
                asset_id: self.asset_id,
                media_type: self.media_type.clone(),
                dimensions: self.dimensions,
                creation_revision: self.creation_revision,
                sidecar_state: AssetSidecarState::Committed,
                revision: RecordRevision::INITIAL,
            },
        )?;
        Ok(())
    }
}

/// Starts one unpublished owner-neutral reference-set build.
pub struct BeginAssetReferenceSet {
    set_id: AssetReferenceSetId,
    source: SealedContentMarkerSummary,
}

impl BeginAssetReferenceSet {
    #[must_use]
    pub const fn new(set_id: AssetReferenceSetId, source: SealedContentMarkerSummary) -> Self {
        Self { set_id, source }
    }

    /// Returns the only public authority that can inspect this unpublished build.
    #[must_use]
    pub const fn staging_authority(&self) -> AssetReferenceSetStagingAuthority {
        AssetReferenceSetStagingAuthority {
            set_id: self.set_id,
            source: self.source,
        }
    }
}

impl DomainMutation<AssetDomain> for BeginAssetReferenceSet {
    type Error = AssetMutationError;

    fn validate(&self, reader: &DomainReader<'_, AssetDomain>) -> Result<(), Self::Error> {
        if read_manifest(reader, self.set_id)?.is_some() {
            return Err(AssetMutationError::ReferenceSetAlreadyExists(self.set_id));
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<AssetReferenceManifestCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, AssetDomain>,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<AssetReferenceManifestCodec>(
            &self.set_id,
            &AssetReferenceSetManifest {
                set_id: self.set_id,
                source: self.source,
                lifecycle: AssetReferenceSetLifecycle::Building,
                marker_count: 0,
                marker_digest: content_marker_digest_seed(),
                maximum_image_label: None,
                entry_frontier: 0,
                asset_chain_digest: digest::seed(self.set_id, self.source),
                revision: RecordRevision::INITIAL,
            },
        )?;
        Ok(())
    }
}

/// One marker reference carried by a fixed-capacity append page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetReferencePageEntry {
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
}

impl AssetReferencePageEntry {
    #[must_use]
    pub const fn new(
        marker_id: SyndicDraftMarkerId,
        label: ImageLabelOrdinal,
        asset_id: AssetId,
    ) -> Self {
        Self {
            marker_id,
            label,
            asset_id,
        }
    }
}

/// Appends one nonempty contiguous page to an exact building manifest.
pub struct AppendAssetReferencePage {
    expected: AssetReferenceSetBuildProof,
    entries: Box<[AssetReferencePageEntry]>,
}

impl AppendAssetReferencePage {
    pub fn new(
        expected: AssetReferenceSetBuildProof,
        entries: Box<[AssetReferencePageEntry]>,
    ) -> Result<Self, AssetReferencePageError> {
        if entries.is_empty() {
            return Err(AssetReferencePageError::Empty);
        }
        if entries.len() > ASSET_REFERENCE_PAGE_MAX_ENTRIES {
            return Err(AssetReferencePageError::TooMany {
                actual: entries.len(),
            });
        }
        for (index, entry) in entries.iter().enumerate() {
            if entries[..index]
                .iter()
                .any(|prior| prior.marker_id == entry.marker_id)
            {
                return Err(AssetReferencePageError::DuplicateMarker(entry.marker_id));
            }
        }
        Ok(Self { expected, entries })
    }

    #[must_use]
    pub fn entries(&self) -> &[AssetReferencePageEntry] {
        &self.entries
    }
}

struct PreparedPage {
    manifest: AssetReferenceSetManifest,
    entries: Vec<AssetReferenceEntryRecord>,
    first_labels: Vec<(AssetLabelFirstKey, AssetLabelFirstRecord)>,
}

impl DomainMutation<AssetDomain> for AppendAssetReferencePage {
    type Error = AssetMutationError;

    fn validate(&self, reader: &DomainReader<'_, AssetDomain>) -> Result<(), Self::Error> {
        self.prepare(reader).map(drop)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        let count = self.entries.len();
        reservation.reserve_records::<AssetReferenceEntryCodec>(count)?;
        reservation.reserve_records::<AssetReferenceMarkerCodec>(count)?;
        reservation.reserve_records::<AssetReferenceLabelFirstCodec>(count)?;
        reservation.reserve_records::<AssetReferenceManifestCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, AssetDomain>,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        let prepared = self.prepare(reader)?;
        for entry in &prepared.entries {
            let entry_key = AssetEntryKey {
                set_id: entry.set_id,
                ordinal: entry.ordinal,
            };
            mutations.put::<AssetReferenceEntryCodec>(&entry_key, entry)?;
            mutations.put::<AssetReferenceMarkerCodec>(
                &AssetMarkerKey {
                    set_id: entry.set_id,
                    marker_id: entry.marker_id,
                },
                &entry.ordinal,
            )?;
        }
        for (key, value) in &prepared.first_labels {
            mutations.put::<AssetReferenceLabelFirstCodec>(key, value)?;
        }
        mutations
            .put::<AssetReferenceManifestCodec>(&prepared.manifest.set_id, &prepared.manifest)?;
        Ok(())
    }
}

impl AppendAssetReferencePage {
    fn prepare(
        &self,
        reader: &DomainReader<'_, AssetDomain>,
    ) -> Result<PreparedPage, AssetMutationError> {
        let mut manifest = require_manifest(reader, self.expected.set_id)?;
        require_build_proof(&manifest, self.expected)?;
        let mut prepared_entries = Vec::with_capacity(self.entries.len());
        let mut first_labels = Vec::with_capacity(self.entries.len());

        for requested in &self.entries {
            require_metadata(reader, requested.asset_id)?;
            let next = manifest
                .entry_frontier
                .checked_add(1)
                .ok_or(AssetMutationError::CountOverflow)?;
            let ordinal = AssetReferenceOrdinal::new(next)?;
            let entry_key = AssetEntryKey {
                set_id: manifest.set_id,
                ordinal,
            };
            if reader
                .point::<AssetReferenceEntryCodec>(&entry_key, entry_limit())?
                .is_some()
            {
                return Err(AssetMutationError::EntryAlreadyExists);
            }
            let marker_key = AssetMarkerKey {
                set_id: manifest.set_id,
                marker_id: requested.marker_id,
            };
            if reader
                .point::<AssetReferenceMarkerCodec>(&marker_key, index_limit())?
                .is_some()
            {
                return Err(AssetMutationError::MarkerAlreadyExists(requested.marker_id));
            }

            let label_key = AssetLabelFirstKey {
                set_id: manifest.set_id,
                label: requested.label,
            };
            let first = first_labels
                .iter()
                .find_map(|(key, value)| (*key == label_key).then_some(*value))
                .or(reader.point::<AssetReferenceLabelFirstCodec>(&label_key, index_limit())?);
            let (first_ordinal, label_disposition) = match first {
                Some(first) => {
                    if first.asset_id != requested.asset_id {
                        return Err(AssetMutationError::LabelAssetMismatch {
                            label: requested.label,
                        });
                    }
                    (
                        first.first_ordinal,
                        AssetLabelDisposition::Repeated {
                            first_ordinal: first.first_ordinal,
                        },
                    )
                }
                None => {
                    first_labels.push((
                        label_key,
                        AssetLabelFirstRecord {
                            first_ordinal: ordinal,
                            asset_id: requested.asset_id,
                        },
                    ));
                    (ordinal, AssetLabelDisposition::First)
                }
            };
            let chain_digest = digest::advance(
                manifest.asset_chain_digest,
                ordinal,
                requested.marker_id,
                requested.label,
                requested.asset_id,
                first_ordinal,
            );
            prepared_entries.push(AssetReferenceEntryRecord {
                set_id: manifest.set_id,
                ordinal,
                marker_id: requested.marker_id,
                label: requested.label,
                asset_id: requested.asset_id,
                label_disposition,
                chain_digest,
            });
            manifest.entry_frontier = next;
            manifest.marker_count = manifest
                .marker_count
                .checked_add(1)
                .ok_or(AssetMutationError::CountOverflow)?;
            manifest.marker_digest = advance_content_marker_digest(
                manifest.marker_digest,
                requested.marker_id,
                requested.label,
            );
            manifest.maximum_image_label = Some(
                manifest
                    .maximum_image_label
                    .map_or(requested.label, |maximum| maximum.max(requested.label)),
            );
            manifest.asset_chain_digest = chain_digest;
        }
        manifest.revision = manifest
            .revision
            .checked_next()
            .map_err(|_| AssetMutationError::ManifestRevisionExhausted(manifest.set_id))?;
        Ok(PreparedPage {
            manifest,
            entries: prepared_entries,
            first_labels,
        })
    }
}

/// Seals an exact staged set without rewriting any entry or index record.
pub struct SealAssetReferenceSet {
    expected: AssetReferenceSetBuildProof,
    source: SealedContentMarkerSummary,
}

impl SealAssetReferenceSet {
    #[must_use]
    pub const fn new(
        expected: AssetReferenceSetBuildProof,
        source: SealedContentMarkerSummary,
    ) -> Self {
        Self { expected, source }
    }
}

impl DomainMutation<AssetDomain> for SealAssetReferenceSet {
    type Error = AssetMutationError;

    fn validate(&self, reader: &DomainReader<'_, AssetDomain>) -> Result<(), Self::Error> {
        let manifest = require_manifest(reader, self.expected.set_id)?;
        require_build_proof(&manifest, self.expected)?;
        require_complete_marker_summary(&manifest, self.source)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<AssetReferenceManifestCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, AssetDomain>,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        let mut manifest = require_manifest(reader, self.expected.set_id)?;
        require_build_proof(&manifest, self.expected)?;
        require_complete_marker_summary(&manifest, self.source)?;
        manifest.lifecycle = AssetReferenceSetLifecycle::Sealed;
        manifest.revision = manifest
            .revision
            .checked_next()
            .map_err(|_| AssetMutationError::ManifestRevisionExhausted(manifest.set_id))?;
        mutations.put::<AssetReferenceManifestCodec>(&manifest.set_id, &manifest)?;
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

fn require_complete_marker_summary(
    manifest: &AssetReferenceSetManifest,
    source: SealedContentMarkerSummary,
) -> Result<(), AssetMutationError> {
    if manifest.source != source
        || manifest.marker_count != source.marker_count()
        || manifest.entry_frontier != source.marker_count()
        || manifest.marker_digest != source.marker_digest()
        || manifest.maximum_image_label != source.maximum_image_label()
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

fn entry_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_ENTRY_LIMIT + 4).expect("entry point bound is nonzero")
}

fn index_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_INDEX_LIMIT + 4).expect("index point bound is nonzero")
}
