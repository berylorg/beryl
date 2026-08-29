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
    type Prepared = (CatalogRecencyCursor, CatalogRow);

    fn prepare(
        self,
        _reader: &DomainReader<'_, CatalogDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        Ok((self.key, self.row))
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, CatalogDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<CatalogRecencyCodec>(1)?;
        Ok(())
    }

    fn contribute(
        (key, row): Self::Prepared,
        mutations: &mut MutationBuilder<'_, CatalogDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<CatalogRecencyCodec>(&key, &row)?;
        Ok(())
    }
}
