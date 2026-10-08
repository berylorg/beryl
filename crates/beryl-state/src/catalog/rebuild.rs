use super::*;
use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};

pub struct RebuildCatalogRow {
    thread_id: SyndicThreadId,
    expected: Option<CatalogRow>,
    sources: CatalogSourceRevisions,
    facts: CatalogFacts,
}

impl RebuildCatalogRow {
    pub fn new(
        thread_id: SyndicThreadId,
        expected: Option<CatalogRow>,
        sources: CatalogSourceRevisions,
        facts: CatalogFacts,
    ) -> Result<Self, CatalogValueError> {
        facts.validate_for(thread_id, sources)?;
        Ok(Self {
            thread_id,
            expected,
            sources,
            facts,
        })
    }
}

impl DomainMutation<CatalogDomain> for RebuildCatalogRow {
    type Error = CatalogMutationError;
    type Prepared = (SyndicThreadId, Option<CatalogRow>, CatalogRow);

    fn prepare(
        self,
        reader: &DomainReader<'_, CatalogDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let limit = PointReadLimit::new(CATALOG_POINT_READ_MAX_BYTES)
            .expect("catalog point limit is nonzero");
        let current = reader.point::<CatalogRowCodec>(&self.thread_id, limit)?;
        match (self.expected.as_ref(), current.as_ref()) {
            (None, Some(_)) => {
                return Err(CatalogMutationError::RowExists {
                    thread_id: self.thread_id,
                });
            }
            (Some(_), None) => {
                return Err(CatalogMutationError::RowMissing {
                    thread_id: self.thread_id,
                });
            }
            (Some(expected), Some(current)) => {
                if current.thread_id() != self.thread_id || expected != current {
                    return Err(CatalogMutationError::CurrentRowChanged {
                        thread_id: self.thread_id,
                    });
                }
                let reverse = reader
                    .point::<CatalogRecencyCodec>(&current.recency_cursor(), limit)?
                    .ok_or(CatalogMutationError::IndexMissing {
                        thread_id: self.thread_id,
                    })?;
                if reverse != *current {
                    return Err(CatalogMutationError::IndexMismatch {
                        thread_id: self.thread_id,
                    });
                }
                if current.facts().execution().runtime_id() != self.facts.execution().runtime_id()
                    || current.facts().execution().root_id() != self.facts.execution().root_id()
                {
                    return Err(CatalogMutationError::ExecutionIdentityChanged {
                        thread_id: self.thread_id,
                    });
                }
                if self.sources.runtime() < current.sources().runtime() {
                    return Err(CatalogMutationError::SourceRevisionRegressed { kind: "runtime" });
                }
                if self.sources.root() < current.sources().root() {
                    return Err(CatalogMutationError::SourceRevisionRegressed { kind: "root" });
                }
            }
            (None, None) => {}
        }
        let revision = current
            .as_ref()
            .map_or(Ok(CatalogRevision::INITIAL), |row| {
                row.revision().checked_next()
            })?;
        let row = CatalogRow::current(self.thread_id, self.sources, self.facts, revision)?;
        let planned = reader.point::<CatalogRecencyCodec>(&row.recency_cursor(), limit)?;
        if let Some(planned) = planned {
            if current.as_ref() != Some(&planned)
                || planned.recency_cursor() != row.recency_cursor()
            {
                return Err(CatalogMutationError::IndexExists {
                    thread_id: self.thread_id,
                });
            }
        }
        Ok((self.thread_id, current, row))
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, CatalogDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<CatalogRowCodec>(1)?;
        reservation.reserve_records::<CatalogRecencyCodec>(2)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, CatalogDomain>,
    ) -> Result<(), Self::Error> {
        <PublishCatalogRow as DomainMutation<CatalogDomain>>::contribute(prepared, mutations)
    }
}
