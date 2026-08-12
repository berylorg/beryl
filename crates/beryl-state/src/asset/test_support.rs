use beryl_home_store::{
    DomainMutation, DomainReader, MutationBuilder, PointReadLimit, ReconciliationReservation,
};
use beryl_model::SealedAssetReferenceSetProof;

use super::{
    ASSET_HEAD_LIMIT, AssetDomain, AssetMutationError, AssetOwner, AssetOwnerHeadExpectation,
    AssetOwnerHeadRecord, codec::AssetOwnerHeadCodec,
};

pub(super) struct CorruptOwnerHeadProof {
    pub(super) owner: AssetOwner,
    pub(super) expected: AssetOwnerHeadExpectation,
    pub(super) replacement: SealedAssetReferenceSetProof,
}

impl DomainMutation<AssetDomain> for CorruptOwnerHeadProof {
    type Error = AssetMutationError;

    fn validate(&self, reader: &DomainReader<'_, AssetDomain>) -> Result<(), Self::Error> {
        require_expected(reader, self.owner, self.expected).map(drop)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<AssetOwnerHeadCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, AssetDomain>,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        let current = require_expected(reader, self.owner, self.expected)?;
        mutations.put::<AssetOwnerHeadCodec>(
            &self.owner,
            &AssetOwnerHeadRecord {
                owner: self.owner,
                set: self.replacement,
                owner_revision: current.owner_revision,
            },
        )?;
        Ok(())
    }
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
