use super::{
    ASSET_COMPLETION_EVIDENCE_LIMIT, AssetDomain, AssetHistoricalInputError, AssetMutationError,
    AssetOwner, AssetOwnerHeadCodec, AssetOwnerHeadRecord, AssetReferenceCompletionEvidenceCodec,
    AssetReferenceManifestCodec, AssetState, head_point_limit, manifest_point_limit, read,
};
use crate::RecordRevision;
use beryl_home_store::{
    DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation,
};
use beryl_model::{SealedAssetReferenceSetProof, SyndicDraftId, SyndicItemId};

impl AssetState {
    pub fn historical_input_owner_selection(
        &self,
        store: &HomeStore,
        draft: SyndicDraftId,
        item: SyndicItemId,
        proof: Option<SealedAssetReferenceSetProof>,
    ) -> Result<MutationContribution, AssetHistoricalInputError> {
        let revision = self.revision(store)?;
        let historical = self.owner_head(store, AssetOwner::SubmittedTurnItem(item))?;
        if historical
            .as_ref()
            .is_some_and(|h| h.owner() != AssetOwner::SubmittedTurnItem(item))
            || historical.as_ref().map(|h| h.set()) != proof
            || self
                .owner_head(store, AssetOwner::CurrentDraft(draft))?
                .is_some()
            || self.revision(store)? != revision
        {
            return Err(AssetHistoricalInputError::Rejected);
        }
        Ok(self.handle.contribution(
            revision,
            SelectHistoricalInput {
                draft,
                item,
                proof,
                historical,
            },
        ))
    }
}
struct SelectHistoricalInput {
    draft: SyndicDraftId,
    item: SyndicItemId,
    proof: Option<SealedAssetReferenceSetProof>,
    historical: Option<AssetOwnerHeadRecord>,
}
impl DomainMutation<AssetDomain> for SelectHistoricalInput {
    type Error = AssetMutationError;
    type Prepared = Self;
    fn prepare(self, r: &DomainReader<'_, AssetDomain>) -> Result<Self, Self::Error> {
        let source = AssetOwner::SubmittedTurnItem(self.item);
        let target = AssetOwner::CurrentDraft(self.draft);
        if r.point::<AssetOwnerHeadCodec>(&source, head_point_limit())? != self.historical
            || r.point::<AssetOwnerHeadCodec>(&target, head_point_limit())?
                .is_some()
            || self.historical.as_ref().map(|h| h.set()) != self.proof
        {
            return Err(AssetMutationError::OwnerHeadMismatch(source));
        }
        if let Some(proof) = self.proof {
            let manifest = r
                .point::<AssetReferenceManifestCodec>(&proof.set_id(), manifest_point_limit())?
                .ok_or(AssetMutationError::ReferenceSetMissing(proof.set_id()))?;
            let completion = r
                .point::<AssetReferenceCompletionEvidenceCodec>(
                    &proof.set_id(),
                    beryl_home_store::PointReadLimit::new(ASSET_COMPLETION_EVIDENCE_LIMIT + 4)
                        .unwrap(),
                )?
                .ok_or(AssetMutationError::ReferenceSetNotSealed(proof.set_id()))?;
            read::verify_sealed_manifest(&manifest, &completion, proof)
                .map_err(|_| AssetMutationError::BuildProofMismatch(proof.set_id()))?;
        }
        Ok(self)
    }
    fn reserve_reconciliation(
        &self,
        r: &mut ReconciliationReservation<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        r.reserve_records::<AssetOwnerHeadCodec>(2)?;
        Ok(())
    }
    fn contribute(p: Self, w: &mut MutationBuilder<'_, AssetDomain>) -> Result<(), Self::Error> {
        let source = AssetOwner::SubmittedTurnItem(p.item);
        let target = AssetOwner::CurrentDraft(p.draft);
        match (p.historical, p.proof) {
            (Some(historical), Some(set)) => {
                w.put::<AssetOwnerHeadCodec>(&source, &historical)?;
                w.put::<AssetOwnerHeadCodec>(
                    &target,
                    &AssetOwnerHeadRecord {
                        owner: target,
                        set,
                        owner_revision: RecordRevision::INITIAL,
                    },
                )?;
            }
            (None, None) => {
                w.delete::<AssetOwnerHeadCodec>(&source)?;
                w.delete::<AssetOwnerHeadCodec>(&target)?;
            }
            _ => return Err(AssetMutationError::OwnerHeadMismatch(source)),
        }
        Ok(())
    }
}
