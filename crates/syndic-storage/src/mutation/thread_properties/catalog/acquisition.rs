use super::*;
use crate::mutation::InitialThreadRecords;
use crate::{CreateThread, EligibleEmptyThreadCandidate};

pub enum ThreadAcquisitionContribution {
    Validation(ValidationContribution),
    Mutation(MutationContribution),
}

enum AcquisitionTarget {
    Reuse(
        EligibleEmptyThreadCandidate,
        ThreadCatalogSummaryPreparation,
    ),
    Create(CreateThread),
}

struct AcquireThread {
    target: AcquisitionTarget,
    predecessor: Option<ThreadCatalogSummaryPreparation>,
}

impl SyndicStorage {
    pub fn reuse_empty_thread_with_catalog_predecessor(
        &self,
        candidate: EligibleEmptyThreadCandidate,
        target_summary: ThreadCatalogSummaryPreparation,
        predecessor: Option<ThreadCatalogSummaryPreparation>,
    ) -> Result<ThreadAcquisitionContribution, SyndicMutationError> {
        let revision = candidate.source_revision();
        if source_revision(&target_summary) != revision
            || summary_id(&target_summary) != candidate.thread_id()
        {
            return Err(SyndicMutationError::ThreadCatalogSummaryConflict);
        }
        qualify_predecessor(revision, candidate.thread_id(), predecessor.as_ref())?;
        let acquisition = AcquireThread {
            target: AcquisitionTarget::Reuse(candidate, target_summary),
            predecessor,
        };
        Ok(
            if acquisition.summary_sources().any(|source| {
                matches!(
                    source,
                    ThreadCatalogSummaryPreparation::PreparedReplacement(_)
                )
            }) {
                ThreadAcquisitionContribution::Mutation(
                    self.handle.contribution(revision, acquisition),
                )
            } else {
                ThreadAcquisitionContribution::Validation(
                    self.handle.validation(revision, acquisition),
                )
            },
        )
    }

    pub fn create_thread_with_catalog_predecessor(
        &self,
        revision: beryl_model::DomainRevision,
        creation: CreateThread,
        predecessor: Option<ThreadCatalogSummaryPreparation>,
    ) -> Result<MutationContribution, SyndicMutationError> {
        qualify_predecessor(revision, creation.thread_id(), predecessor.as_ref())?;
        Ok(self.handle.contribution(
            revision,
            AcquireThread {
                target: AcquisitionTarget::Create(creation),
                predecessor,
            },
        ))
    }
}

impl AcquireThread {
    fn summary_sources(&self) -> impl Iterator<Item = &ThreadCatalogSummaryPreparation> {
        let target = match &self.target {
            AcquisitionTarget::Reuse(_, source) => Some(source),
            AcquisitionTarget::Create(_) => None,
        };
        target.into_iter().chain(self.predecessor.as_ref())
    }
}

impl DomainValidator<SyndicDomain> for AcquireThread {
    type Error = SyndicMutationError;
    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let AcquisitionTarget::Reuse(candidate, _) = &self.target else {
            return Err(SyndicMutationError::PristineThreadConflict);
        };
        candidate.validate(reader)?;
        for source in self.summary_sources() {
            let ThreadCatalogSummaryPreparation::ExactCurrent(exact) = source else {
                return Err(SyndicMutationError::ThreadCatalogSummaryConflict);
            };
            ValidateCurrentThreadCatalogSummary {
                exact: exact.clone(),
            }
            .validate(reader)?;
        }
        Ok(())
    }
}

fn qualify_predecessor(
    revision: beryl_model::DomainRevision,
    thread: beryl_model::SyndicThreadId,
    predecessor: Option<&ThreadCatalogSummaryPreparation>,
) -> Result<(), SyndicMutationError> {
    if predecessor
        .is_some_and(|source| source_revision(source) != revision || summary_id(source) == thread)
    {
        return Err(SyndicMutationError::ThreadCatalogSummaryConflict);
    }
    Ok(())
}

impl DomainMutation<SyndicDomain> for AcquireThread {
    type Error = SyndicMutationError;
    type Prepared = (
        Option<InitialThreadRecords>,
        Option<ThreadCatalogSummaryPreparation>,
        Option<ThreadCatalogSummaryPreparation>,
    );

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let (records, target_summary) = match self.target {
            AcquisitionTarget::Reuse(candidate, source) => {
                candidate.validate(reader)?;
                (None, Some(source))
            }
            AcquisitionTarget::Create(creation) => {
                let records = creation.records();
                records.ensure_absent(reader)?;
                if let Some(source) = &creation.source {
                    crate::mutation::validate_source_tail(reader, source, creation.created_at())?;
                }
                (Some(records), None)
            }
        };
        for source in target_summary
            .as_ref()
            .into_iter()
            .chain(self.predecessor.as_ref())
        {
            match source {
                ThreadCatalogSummaryPreparation::ExactCurrent(exact) => {
                    ValidateCurrentThreadCatalogSummary {
                        exact: exact.clone(),
                    }
                    .validate(reader)?
                }
                ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) => {
                    validate_prepared(reader, prepared)?
                }
            }
        }
        Ok((records, target_summary, self.predecessor))
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let summaries = self
            .summary_sources()
            .filter(|source| {
                matches!(
                    source,
                    ThreadCatalogSummaryPreparation::PreparedReplacement(_)
                )
            })
            .count();
        match &self.target {
            AcquisitionTarget::Create(creation) => creation
                .records()
                .reserve_with_catalog_summaries(reservation, 1 + summaries)?,
            AcquisitionTarget::Reuse(_, _) if summaries != 0 => {
                reservation.reserve_records::<ThreadCatalogSummariesCodec>(summaries)?
            }
            AcquisitionTarget::Reuse(_, _) => {}
        }
        Ok(())
    }

    fn contribute(
        (records, target_summary, predecessor): Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(records) = records {
            records.put(mutations)?;
        }
        for source in target_summary.into_iter().chain(predecessor) {
            if let ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) = source {
                mutations.put::<ThreadCatalogSummariesCodec>(
                    &prepared.replacement.thread_id(),
                    &prepared.replacement,
                )?;
            }
        }
        Ok(())
    }
}
