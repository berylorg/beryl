use beryl_home_store::{
    DomainMutation, DomainReader, MutationBuilder, MutationContribution, ReconciliationReservation,
};
use beryl_model::{DomainRevision, SyndicThreadId};

use super::codec::{CatalogRecencyCodec, CatalogRowCodec};
use super::{
    CatalogClaimKind, CatalogClaimSummary, CatalogDomain, CatalogFacts, CatalogMutationError,
    CatalogRevision, CatalogRow, CatalogRowExpectation, CatalogSourceRevisions, CatalogState,
    CatalogValueError, PublishCatalogRow,
};

pub struct PublishCatalogClaimReplacement {
    target: PublishCatalogRow,
    predecessor: Option<PublishCatalogRow>,
}

pub struct CatalogClaimReplacementRow {
    publication: PublishCatalogRow,
    future: CatalogRow,
}

impl CatalogClaimReplacementRow {
    pub fn new(
        thread_id: SyndicThreadId,
        current: Option<CatalogRow>,
        sources: CatalogSourceRevisions,
        facts: CatalogFacts,
    ) -> Result<Self, CatalogValueError> {
        if current
            .as_ref()
            .is_some_and(|row| row.thread_id() != thread_id)
        {
            return Err(CatalogValueError::ClaimSourceMismatch);
        }
        let expectation = current
            .as_ref()
            .map_or(CatalogRowExpectation::Missing, |row| {
                CatalogRowExpectation::Revision(row.revision())
            });
        let revision = current
            .as_ref()
            .map_or(Ok(CatalogRevision::INITIAL), |row| {
                row.revision().checked_next()
            })?;
        Ok(Self {
            publication: PublishCatalogRow::new(thread_id, expectation, sources, facts.clone())?,
            future: CatalogRow::current(thread_id, sources, facts, revision)?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogClaimReplacementAudit {
    target: CatalogRow,
    predecessor: Option<CatalogRow>,
}

impl CatalogClaimReplacementAudit {
    pub fn thread_ids(&self) -> impl Iterator<Item = SyndicThreadId> + '_ {
        std::iter::once(self.target.thread_id())
            .chain(self.predecessor.as_ref().map(|row| row.thread_id()))
    }

    pub fn matches_row(&self, row: &CatalogRow) -> bool {
        row == &self.target || self.predecessor.as_ref() == Some(row)
    }
}

impl PublishCatalogClaimReplacement {
    pub fn from_planned_rows(
        target: CatalogClaimReplacementRow,
        predecessor: Option<CatalogClaimReplacementRow>,
    ) -> (Self, CatalogClaimReplacementAudit) {
        let (predecessor, future) = match predecessor {
            Some(row) => (Some(row.publication), Some(row.future)),
            None => (None, None),
        };
        (
            Self {
                target: target.publication,
                predecessor,
            },
            CatalogClaimReplacementAudit {
                target: target.future,
                predecessor: future,
            },
        )
    }

    pub fn new(target: PublishCatalogRow, predecessor: Option<PublishCatalogRow>) -> Self {
        Self {
            target,
            predecessor,
        }
    }
}

type PreparedRow = (SyndicThreadId, Option<CatalogRow>, CatalogRow);

impl DomainMutation<CatalogDomain> for PublishCatalogClaimReplacement {
    type Error = CatalogMutationError;
    type Prepared = (PreparedRow, Option<PreparedRow>);

    fn prepare(
        self,
        reader: &DomainReader<'_, CatalogDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let target = self.target.prepare(reader)?;
        let predecessor = self
            .predecessor
            .map(|row| row.prepare(reader))
            .transpose()?;
        if !matches!(
            target.2.facts().claim(),
            CatalogClaimSummary::Claimed {
                kind: CatalogClaimKind::Active,
                ..
            }
        ) || target.2.sources().claim().is_none()
            || predecessor.as_ref().is_some_and(|row| {
                row.0 == target.0
                    || row.2.facts().claim() != CatalogClaimSummary::Unclaimed
                    || row.2.sources().claim().is_some()
            })
        {
            return Err(CatalogValueError::ClaimSourceMismatch.into());
        }
        Ok((target, predecessor))
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, CatalogDomain>,
    ) -> Result<(), Self::Error> {
        let rows = 1 + usize::from(self.predecessor.is_some());
        reservation.reserve_records::<CatalogRowCodec>(rows)?;
        reservation.reserve_records::<CatalogRecencyCodec>(rows * 2)?;
        Ok(())
    }

    fn contribute(
        (target, predecessor): Self::Prepared,
        mutations: &mut MutationBuilder<'_, CatalogDomain>,
    ) -> Result<(), Self::Error> {
        PublishCatalogRow::contribute(target, mutations)?;
        if let Some(predecessor) = predecessor {
            PublishCatalogRow::contribute(predecessor, mutations)?;
        }
        Ok(())
    }
}

impl CatalogState {
    pub fn replace_claim_projection(
        &self,
        expected_revision: DomainRevision,
        command: PublishCatalogClaimReplacement,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }
}
