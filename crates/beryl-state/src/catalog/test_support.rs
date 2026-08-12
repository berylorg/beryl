use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};

use super::{
    CatalogDomain, CatalogMutationError, CatalogRecencyCursor, CatalogRow,
    codec::CatalogRecencyCodec,
};

pub(super) struct CorruptRecencyCopy {
    pub(super) key: CatalogRecencyCursor,
    pub(super) row: CatalogRow,
}

impl DomainMutation<CatalogDomain> for CorruptRecencyCopy {
    type Error = CatalogMutationError;

    fn validate(&self, _reader: &DomainReader<'_, CatalogDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, CatalogDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<CatalogRecencyCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, CatalogDomain>,
        mutations: &mut MutationBuilder<'_, CatalogDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<CatalogRecencyCodec>(&self.key, &self.row)?;
        Ok(())
    }
}
