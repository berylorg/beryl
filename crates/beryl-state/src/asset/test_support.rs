use beryl_home_store::{
    DomainMutation, DomainReader, MutationBuilder, PointReadLimit, ReconciliationReservation,
};
use beryl_model::{
    AssetReferenceSetDigest, AssetReferenceSetId, OrderedMarkerAssetSummaryV1,
    SealedAssetReferenceSetProof, SequentialMarkerSummaryV1, ordered_marker_asset_digest_seed,
    sequential_marker_digest_seed,
};

use super::{
    ASSET_HEAD_LIMIT, ASSET_MANIFEST_LIMIT, AssetDomain, AssetMutationError, AssetOwner,
    AssetOwnerHeadExpectation, AssetOwnerHeadRecord, AssetReferenceSetLifecycle,
    AssetReferenceSetManifestCorruption,
    codec::{
        AssetOwnerHeadCodec, AssetReferenceCompletionEvidenceCodec, AssetReferenceManifestCodec,
    },
};

pub(super) struct CorruptOwnerHeadProof {
    pub(super) owner: AssetOwner,
    pub(super) expected: AssetOwnerHeadExpectation,
    pub(super) replacement: SealedAssetReferenceSetProof,
}

impl DomainMutation<AssetDomain> for CorruptOwnerHeadProof {
    type Error = AssetMutationError;
    type Prepared = PreparedCorruptOwnerHeadProof;

    fn prepare(
        self,
        reader: &DomainReader<'_, AssetDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let current = require_expected(reader, self.owner, self.expected)?;
        Ok(PreparedCorruptOwnerHeadProof {
            owner: self.owner,
            replacement: self.replacement,
            owner_revision: current.owner_revision,
        })
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<AssetOwnerHeadCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<AssetOwnerHeadCodec>(
            &prepared.owner,
            &AssetOwnerHeadRecord {
                owner: prepared.owner,
                set: prepared.replacement,
                owner_revision: prepared.owner_revision,
            },
        )?;
        Ok(())
    }
}

pub(super) struct CorruptReferenceSetManifest {
    pub(super) set_id: AssetReferenceSetId,
    pub(super) corruption: AssetReferenceSetManifestCorruption,
}

impl DomainMutation<AssetDomain> for CorruptReferenceSetManifest {
    type Error = AssetMutationError;
    type Prepared = AssetReferenceSetManifest;

    fn prepare(
        self,
        reader: &DomainReader<'_, AssetDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let mut manifest = reader
            .point::<AssetReferenceManifestCodec>(&self.set_id, manifest_limit())?
            .ok_or(AssetMutationError::ReferenceSetMissing(self.set_id))?;
        match self.corruption {
            AssetReferenceSetManifestCorruption::Lifecycle => {
                manifest.lifecycle = match manifest.lifecycle {
                    AssetReferenceSetLifecycle::Building => AssetReferenceSetLifecycle::Sealed,
                    AssetReferenceSetLifecycle::Sealed => AssetReferenceSetLifecycle::Building,
                };
            }
            AssetReferenceSetManifestCorruption::Sequential => {
                manifest.sequential =
                    SequentialMarkerSummaryV1::new(sequential_marker_digest_seed(), 0, None)
                        .expect("canonical empty sequential marker summary is valid");
            }
            AssetReferenceSetManifestCorruption::OrderedAssets => {
                manifest.ordered_assets =
                    OrderedMarkerAssetSummaryV1::new(ordered_marker_asset_digest_seed(), 0);
            }
            AssetReferenceSetManifestCorruption::EntryFrontier => manifest.entry_frontier = 0,
            AssetReferenceSetManifestCorruption::AssetChain => {
                manifest.asset_chain_digest = AssetReferenceSetDigest::from_bytes([u8::MAX; 32]);
            }
        }
        Ok(manifest)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<AssetReferenceManifestCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<AssetReferenceManifestCodec>(&prepared.set_id, &prepared)?;
        Ok(())
    }
}

pub(super) struct RemoveReferenceSetCompletionEvidence {
    pub(super) set_id: AssetReferenceSetId,
}

impl DomainMutation<AssetDomain> for RemoveReferenceSetCompletionEvidence {
    type Error = AssetMutationError;
    type Prepared = AssetReferenceSetId;

    fn prepare(
        self,
        reader: &DomainReader<'_, AssetDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        reader
            .point::<AssetReferenceManifestCodec>(&self.set_id, manifest_limit())?
            .ok_or(AssetMutationError::ReferenceSetMissing(self.set_id))?;
        Ok(self.set_id)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<AssetReferenceCompletionEvidenceCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        mutations.delete::<AssetReferenceCompletionEvidenceCodec>(&prepared)?;
        Ok(())
    }
}

pub(crate) struct PreparedCorruptOwnerHeadProof {
    owner: AssetOwner,
    replacement: SealedAssetReferenceSetProof,
    owner_revision: crate::RecordRevision,
}

fn require_expected(
    reader: &DomainReader<'_, AssetDomain>,
    owner: AssetOwner,
    expected: AssetOwnerHeadExpectation,
) -> Result<AssetOwnerHeadRecord, AssetMutationError> {
    let current = reader.point::<AssetOwnerHeadCodec>(&owner, head_limit())?;
    match current {
        Some(current) if current.expectation() == expected => Ok(current),
        _ => Err(AssetMutationError::OwnerHeadMismatch(owner)),
    }
}

fn head_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_HEAD_LIMIT + 4).expect("head point bound is nonzero")
}

fn manifest_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_MANIFEST_LIMIT + 4).expect("manifest point bound is nonzero")
}
