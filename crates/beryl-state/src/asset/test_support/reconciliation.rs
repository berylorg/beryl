use beryl_home_store::{
    DomainMutation, DomainReader, DomainValidator, MutationBuilder, MutationContribution,
    ReconciliationReservation,
};
use beryl_model::DomainRevision;

use super::super::{
    AssetDomain, AssetMutationError, AssetOwner, AssetOwnerHeadRecord, AssetState,
    ValidateAssetOwnerHeads, codec::AssetOwnerHeadCodec,
};

impl AssetState {
    pub fn unchanged_owner_heads_for_test(
        &self,
        expected_revision: DomainRevision,
        assertions: ValidateAssetOwnerHeads,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_revision, UnchangedOwnerHeads(assertions))
    }
}

struct UnchangedOwnerHeads(ValidateAssetOwnerHeads);

impl DomainMutation<AssetDomain> for UnchangedOwnerHeads {
    type Error = AssetMutationError;
    type Prepared = Box<[(AssetOwner, Option<AssetOwnerHeadRecord>)]>;

    fn prepare(
        self,
        reader: &DomainReader<'_, AssetDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        DomainValidator::validate(&self.0, reader)?;
        self.0
            .assertions()
            .iter()
            .map(|assertion| {
                let owner = assertion.owner();
                let head = reader
                    .point::<AssetOwnerHeadCodec>(&owner, super::super::head_point_limit())?;
                Ok((owner, head))
            })
            .collect()
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<AssetOwnerHeadCodec>(self.0.assertions().len())?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        for (owner, head) in prepared {
            match head {
                Some(head) => mutations.put::<AssetOwnerHeadCodec>(&owner, &head)?,
                None => mutations.delete::<AssetOwnerHeadCodec>(&owner)?,
            }
        }
        Ok(())
    }
}
