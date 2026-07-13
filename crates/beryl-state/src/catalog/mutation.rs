use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, PointReadLimit};
use beryl_model::SyndicThreadId;

use super::{
    CATALOG_RECORD_LIMIT, CatalogDomain, CatalogFacts, CatalogFreshness, CatalogMutationError,
    CatalogRevision, CatalogRow, CatalogRowExpectation, CatalogSourceRevisions,
    codec::{CatalogRecencyCodec, CatalogRowCodec},
};

/// Publish one complete current compact projection, creating or replacing its row atomically.
pub struct PublishCatalogRow {
    thread_id: SyndicThreadId,
    expectation: CatalogRowExpectation,
    sources: CatalogSourceRevisions,
    facts: CatalogFacts,
}

/// Atomically marks one existing catalog projection stale without treating it as authority.
pub struct MarkCatalogRowStale {
    thread_id: SyndicThreadId,
    expected_revision: CatalogRevision,
}

impl PublishCatalogRow {
    pub fn new(
        thread_id: SyndicThreadId,
        expectation: CatalogRowExpectation,
        sources: CatalogSourceRevisions,
        facts: CatalogFacts,
    ) -> Result<Self, super::CatalogValueError> {
        facts.validate_for(thread_id, sources)?;
        Ok(Self {
            thread_id,
            expectation,
            sources,
            facts,
        })
    }
}

impl DomainMutation<CatalogDomain> for PublishCatalogRow {
    type Error = CatalogMutationError;

    fn validate(&self, reader: &DomainReader<'_, CatalogDomain>) -> Result<(), Self::Error> {
        self.facts.validate_for(self.thread_id, self.sources)?;
        let current = read_pair(reader, self.thread_id)?;
        match (self.expectation, current.as_ref()) {
            (CatalogRowExpectation::Missing, None) => Ok(()),
            (CatalogRowExpectation::Missing, Some(_)) => Err(CatalogMutationError::RowExists {
                thread_id: self.thread_id,
            }),
            (CatalogRowExpectation::Revision(_), None) => Err(CatalogMutationError::RowMissing {
                thread_id: self.thread_id,
            }),
            (CatalogRowExpectation::Revision(expected), Some(current)) => {
                ensure_revision(expected, current.revision())?;
                if let Some(kind) = self.sources.regression_from(current.sources()) {
                    return Err(CatalogMutationError::SourceRevisionRegressed { kind });
                }
                Ok(())
            }
        }
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, CatalogDomain>,
        mutations: &mut MutationBuilder<'_, CatalogDomain>,
    ) -> Result<(), Self::Error> {
        let current = read_pair(reader, self.thread_id)?;
        let revision = match current.as_ref() {
            Some(current) => current.revision().checked_next()?,
            None => CatalogRevision::INITIAL,
        };
        let row = CatalogRow::current(self.thread_id, self.sources, self.facts.clone(), revision)?;
        mutations.put::<CatalogRowCodec>(&self.thread_id, &row)?;
        replace_recency_copy(mutations, current.as_ref(), &row)?;
        Ok(())
    }
}

impl MarkCatalogRowStale {
    #[must_use]
    pub const fn new(thread_id: SyndicThreadId, expected_revision: CatalogRevision) -> Self {
        Self {
            thread_id,
            expected_revision,
        }
    }
}

impl DomainMutation<CatalogDomain> for MarkCatalogRowStale {
    type Error = CatalogMutationError;

    fn validate(&self, reader: &DomainReader<'_, CatalogDomain>) -> Result<(), Self::Error> {
        let row = required_pair(reader, self.thread_id)?;
        ensure_revision(self.expected_revision, row.revision())?;
        if row.freshness() == CatalogFreshness::Stale {
            return Err(CatalogMutationError::AlreadyStale {
                thread_id: self.thread_id,
            });
        }
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, CatalogDomain>,
        mutations: &mut MutationBuilder<'_, CatalogDomain>,
    ) -> Result<(), Self::Error> {
        let current = required_pair(reader, self.thread_id)?;
        let stale = current.mark_stale(current.revision().checked_next()?)?;
        mutations.put::<CatalogRowCodec>(&self.thread_id, &stale)?;
        mutations.put::<CatalogRecencyCodec>(&stale.recency_cursor(), &stale)?;
        Ok(())
    }
}

fn read_pair(
    reader: &DomainReader<'_, CatalogDomain>,
    thread_id: SyndicThreadId,
) -> Result<Option<CatalogRow>, CatalogMutationError> {
    let Some(row) = reader.point::<CatalogRowCodec>(&thread_id, point_limit())? else {
        return Ok(None);
    };
    let index = reader
        .point::<CatalogRecencyCodec>(&row.recency_cursor(), point_limit())?
        .ok_or(CatalogMutationError::IndexMissing { thread_id })?;
    if index != row {
        return Err(CatalogMutationError::IndexMismatch { thread_id });
    }
    Ok(Some(row))
}

fn required_pair(
    reader: &DomainReader<'_, CatalogDomain>,
    thread_id: SyndicThreadId,
) -> Result<CatalogRow, CatalogMutationError> {
    read_pair(reader, thread_id)?.ok_or(CatalogMutationError::RowMissing { thread_id })
}

fn ensure_revision(
    expected: CatalogRevision,
    current: CatalogRevision,
) -> Result<(), CatalogMutationError> {
    if expected == current {
        Ok(())
    } else {
        Err(CatalogMutationError::RevisionConflict { expected, current })
    }
}

fn replace_recency_copy(
    mutations: &mut MutationBuilder<'_, CatalogDomain>,
    current: Option<&CatalogRow>,
    replacement: &CatalogRow,
) -> Result<(), CatalogMutationError> {
    let replacement_key = replacement.recency_cursor();
    if let Some(current) = current {
        let current_key = current.recency_cursor();
        if current_key != replacement_key {
            mutations.delete::<CatalogRecencyCodec>(&current_key)?;
        }
    }
    mutations.put::<CatalogRecencyCodec>(&replacement_key, replacement)?;
    Ok(())
}

fn point_limit() -> PointReadLimit {
    PointReadLimit::new(CATALOG_RECORD_LIMIT + 4).expect("catalog point limit is nonzero")
}
