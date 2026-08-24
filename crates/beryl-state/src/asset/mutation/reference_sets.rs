use super::*;

pub struct BeginAssetReferenceSet {
    authority: AssetReferenceSetStagingAuthority,
}

impl BeginAssetReferenceSet {
    #[must_use]
    pub const fn new(authority: AssetReferenceSetStagingAuthority) -> Self {
        Self { authority }
    }

    #[must_use]
    pub const fn staging_authority(&self) -> AssetReferenceSetStagingAuthority {
        self.authority
    }
}

impl DomainMutation<AssetDomain> for BeginAssetReferenceSet {
    type Error = AssetMutationError;

    fn validate(&self, reader: &DomainReader<'_, AssetDomain>) -> Result<(), Self::Error> {
        if read_manifest(reader, self.authority.set_id)?.is_some() {
            return Err(AssetMutationError::ReferenceSetAlreadyExists(
                self.authority.set_id,
            ));
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<AssetReferenceManifestCodec>(1)?;
        reservation.reserve_records::<AssetReferenceCompletionEvidenceCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, AssetDomain>,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        let manifest = AssetReferenceSetManifest {
            set_id: self.authority.set_id,
            sequential: SequentialMarkerSummaryV1::new(sequential_marker_digest_seed(), 0, None)
                .expect("canonical empty sequential marker summary is valid"),
            ordered_assets: OrderedMarkerAssetSummaryV1::new(ordered_marker_asset_digest_seed(), 0),
            lifecycle: AssetReferenceSetLifecycle::Building,
            entry_frontier: 0,
            asset_chain_digest: digest::seed(self.authority.set_id),
            revision: RecordRevision::INITIAL,
        };
        mutations.put::<AssetReferenceManifestCodec>(&manifest.set_id, &manifest)?;
        mutations.put::<AssetReferenceCompletionEvidenceCodec>(
            &manifest.set_id,
            &completion::initial_evidence(self.authority, &manifest),
        )?;
        Ok(())
    }
}

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
    evidence: AssetReferenceSetCompletionEvidence,
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
        reservation.reserve_records::<AssetReferenceCompletionEvidenceCodec>(1)?;
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
        mutations.put::<AssetReferenceCompletionEvidenceCodec>(
            &prepared.manifest.set_id,
            &prepared.evidence,
        )?;
        Ok(())
    }
}

impl AppendAssetReferencePage {
    fn prepare(
        &self,
        reader: &DomainReader<'_, AssetDomain>,
    ) -> Result<PreparedPage, AssetMutationError> {
        let mut manifest = require_manifest(reader, self.expected.set_id)?;
        let evidence = require_building_evidence(reader, &manifest)?;
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
            let marker_count = manifest
                .sequential
                .marker_count()
                .checked_add(1)
                .ok_or(AssetMutationError::CountOverflow)?;
            let marker_digest = advance_sequential_marker_digest(
                manifest.sequential.marker_digest(),
                requested.marker_id,
                requested.label,
            );
            let maximum_image_label = Some(
                manifest
                    .sequential
                    .maximum_image_label()
                    .map_or(requested.label, |maximum| maximum.max(requested.label)),
            );
            manifest.sequential =
                SequentialMarkerSummaryV1::new(marker_digest, marker_count, maximum_image_label)
                    .expect("incrementally derived sequential marker summary is valid");
            manifest.ordered_assets = OrderedMarkerAssetSummaryV1::new(
                advance_ordered_marker_asset_digest(
                    manifest.ordered_assets.marker_asset_digest(),
                    requested.marker_id,
                    requested.label,
                    requested.asset_id,
                ),
                marker_count,
            );
            manifest.asset_chain_digest = chain_digest;
        }
        manifest.revision = manifest
            .revision
            .checked_next()
            .map_err(|_| AssetMutationError::ManifestRevisionExhausted(manifest.set_id))?;
        Ok(PreparedPage {
            evidence: completion::advance_building_evidence(evidence, &manifest),
            manifest,
            entries: prepared_entries,
            first_labels,
        })
    }
}

pub struct SealAssetReferenceSet {
    expected: AssetReferenceSetBuildProof,
    sequential: SequentialMarkerSummaryV1,
    ordered_assets: OrderedMarkerAssetSummaryV1,
    sealed: SealedAssetReferenceSetProof,
}

impl SealAssetReferenceSet {
    #[must_use]
    pub fn new(
        expected: AssetReferenceSetBuildProof,
        sequential: SequentialMarkerSummaryV1,
        ordered_assets: OrderedMarkerAssetSummaryV1,
    ) -> Result<Self, beryl_model::AssetProofError> {
        let sealed = SealedAssetReferenceSetProof::new(
            expected.set_id(),
            sequential,
            ordered_assets,
            expected.entry_frontier(),
            expected.asset_chain_digest(),
        )?;
        Ok(Self {
            expected,
            sequential,
            ordered_assets,
            sealed,
        })
    }

    #[must_use]
    pub const fn sealed_proof(&self) -> SealedAssetReferenceSetProof {
        self.sealed
    }
}

impl DomainMutation<AssetDomain> for SealAssetReferenceSet {
    type Error = AssetMutationError;

    fn validate(&self, reader: &DomainReader<'_, AssetDomain>) -> Result<(), Self::Error> {
        let manifest = require_manifest(reader, self.expected.set_id)?;
        require_building_evidence(reader, &manifest)?;
        require_build_proof(&manifest, self.expected)?;
        require_complete_marker_summaries(&manifest, self.sequential, self.ordered_assets)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<AssetReferenceManifestCodec>(1)?;
        reservation.reserve_records::<AssetReferenceCompletionEvidenceCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, AssetDomain>,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        let mut manifest = require_manifest(reader, self.expected.set_id)?;
        let evidence = require_building_evidence(reader, &manifest)?;
        require_build_proof(&manifest, self.expected)?;
        require_complete_marker_summaries(&manifest, self.sequential, self.ordered_assets)?;
        manifest.lifecycle = AssetReferenceSetLifecycle::Sealed;
        manifest.revision = manifest
            .revision
            .checked_next()
            .map_err(|_| AssetMutationError::ManifestRevisionExhausted(manifest.set_id))?;
        mutations.put::<AssetReferenceManifestCodec>(&manifest.set_id, &manifest)?;
        mutations.put::<AssetReferenceCompletionEvidenceCodec>(
            &manifest.set_id,
            &completion::seal_evidence(evidence, &manifest, self.sealed),
        )?;
        Ok(())
    }
}
