use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};

use super::{
    CatalogDomain, CatalogMutationError, CatalogRecencyCursor, CatalogRow,
    codec::{CatalogRecencyCodec, CatalogRowCodec},
};

pub(super) struct CorruptRecencyCopy {
    pub(super) key: CatalogRecencyCursor,
    pub(super) row: CatalogRow,
}

pub(super) struct RemoveCatalogCopy {
    pub(super) row: CatalogRow,
    pub(super) primary: bool,
}

impl DomainMutation<CatalogDomain> for RemoveCatalogCopy {
    type Error = CatalogMutationError;
    type Prepared = Self;

    fn prepare(self, _: &DomainReader<'_, CatalogDomain>) -> Result<Self, Self::Error> {
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, CatalogDomain>,
    ) -> Result<(), Self::Error> {
        if self.primary {
            reservation.reserve_records::<CatalogRowCodec>(1)?;
        } else {
            reservation.reserve_records::<CatalogRecencyCodec>(1)?;
        }
        Ok(())
    }

    fn contribute(
        self_: Self,
        mutations: &mut MutationBuilder<'_, CatalogDomain>,
    ) -> Result<(), Self::Error> {
        if self_.primary {
            mutations.delete::<CatalogRowCodec>(&self_.row.thread_id())?;
        } else {
            mutations.delete::<CatalogRecencyCodec>(&self_.row.recency_cursor())?;
        }
        Ok(())
    }
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
