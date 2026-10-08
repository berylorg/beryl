use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};
use beryl_model::SyndicThreadId;

use super::{
    CatalogDomain, CatalogMutationError, CatalogRow,
    codec::{CatalogRecencyCodec, CatalogRowCodec},
    mutation::required_pair,
};

pub(super) struct InvalidateCurrentCatalogRow {
    pub(super) thread_id: SyndicThreadId,
}

impl DomainMutation<CatalogDomain> for InvalidateCurrentCatalogRow {
    type Error = CatalogMutationError;
    type Prepared = CatalogRow;

    fn prepare(
        self,
        reader: &DomainReader<'_, CatalogDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let row = required_pair(reader, self.thread_id)?;
        if row.thread_id() != self.thread_id {
            return Err(CatalogMutationError::IndexMismatch {
                thread_id: self.thread_id,
            });
        }
        Ok(row.mark_stale(row.revision().checked_next()?)?)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, CatalogDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<CatalogRowCodec>(1)?;
        reservation.reserve_records::<CatalogRecencyCodec>(1)?;
        Ok(())
    }

    fn contribute(
        stale: Self::Prepared,
        builder: &mut MutationBuilder<'_, CatalogDomain>,
    ) -> Result<(), Self::Error> {
        builder.put::<CatalogRowCodec>(&stale.thread_id(), &stale)?;
        builder.put::<CatalogRecencyCodec>(&stale.recency_cursor(), &stale)?;
        Ok(())
    }
}
