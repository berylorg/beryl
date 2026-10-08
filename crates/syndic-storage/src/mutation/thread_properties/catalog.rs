use beryl_home_store::{
    DomainMutation, DomainReader, DomainValidator, MutationBuilder, MutationContribution,
    ReconciliationReservation, ValidationContribution,
};

use crate::{
    ExactThreadCatalogSummary, PreparedThreadCatalogSummaryReplacement, SyndicMutationError,
    ThreadCatalogSummaryPreparation, ThreadCatalogSummaryRecord, ThreadCatalogTitleSource,
    codec::{
        HistorySummariesFamily, ThreadAttributesFamily, ThreadCatalogSummariesCodec,
        ThreadCatalogSummariesFamily, ThreadExecutionsFamily, ThreadsFamily,
    },
    domain::{SyndicDomain, SyndicStorage},
};

use super::super::required;
mod acquisition;
pub use acquisition::ThreadAcquisitionContribution;

struct RebuildThreadCatalogSummary {
    prepared: PreparedThreadCatalogSummaryReplacement,
}

struct ValidateCurrentThreadCatalogSummary {
    exact: ExactThreadCatalogSummary,
}

impl SyndicStorage {
    pub fn validate_thread_catalog_summary_pair(
        &self,
        first: ExactThreadCatalogSummary,
        second: Option<ExactThreadCatalogSummary>,
    ) -> Result<ValidationContribution, SyndicMutationError> {
        if second.as_ref().is_some_and(|other| {
            other.source_revision != first.source_revision
                || other.summary.thread_id() == first.summary.thread_id()
        }) {
            return Err(SyndicMutationError::ThreadCatalogSummaryConflict);
        }
        Ok(self.handle.validation(
            first.source_revision,
            ValidateThreadCatalogSummaryPair { first, second },
        ))
    }

    pub fn publish_thread_catalog_summary_pair(
        &self,
        first: ThreadCatalogSummaryPreparation,
        second: Option<ThreadCatalogSummaryPreparation>,
    ) -> Result<MutationContribution, SyndicMutationError> {
        let revision = source_revision(&first);
        if second.as_ref().is_some_and(|other| {
            source_revision(other) != revision || summary_id(other) == summary_id(&first)
        }) {
            return Err(SyndicMutationError::ThreadCatalogSummaryConflict);
        }
        if std::iter::once(&first)
            .chain(second.as_ref())
            .all(|source| matches!(source, ThreadCatalogSummaryPreparation::ExactCurrent(_)))
        {
            return Err(SyndicMutationError::ThreadCatalogSummaryConflict);
        }
        Ok(self
            .handle
            .contribution(revision, PublishThreadCatalogSummaryPair { first, second }))
    }

    /// Seals the opaque prepared semantic successor under its exact stable source revision.
    #[must_use]
    pub fn rebuild_thread_catalog_summary(
        &self,
        prepared: PreparedThreadCatalogSummaryReplacement,
    ) -> MutationContribution {
        self.handle.contribution(
            prepared.source_revision,
            RebuildThreadCatalogSummary { prepared },
        )
    }

    /// Seals an exact source-current summary assertion for a heterogeneous home command.
    #[must_use]
    pub fn validate_current_thread_catalog_summary(
        &self,
        exact: ExactThreadCatalogSummary,
    ) -> ValidationContribution {
        self.handle.validation(
            exact.source_revision,
            ValidateCurrentThreadCatalogSummary { exact },
        )
    }
}

struct PublishThreadCatalogSummaryPair {
    first: ThreadCatalogSummaryPreparation,
    second: Option<ThreadCatalogSummaryPreparation>,
}

struct ValidateThreadCatalogSummaryPair {
    first: ExactThreadCatalogSummary,
    second: Option<ExactThreadCatalogSummary>,
}

impl DomainValidator<SyndicDomain> for ValidateThreadCatalogSummaryPair {
    type Error = SyndicMutationError;
    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        for exact in std::iter::once(&self.first).chain(self.second.as_ref()) {
            ValidateCurrentThreadCatalogSummary {
                exact: exact.clone(),
            }
            .validate(reader)?;
        }
        Ok(())
    }
}

fn source_revision(source: &ThreadCatalogSummaryPreparation) -> beryl_model::DomainRevision {
    match source {
        ThreadCatalogSummaryPreparation::ExactCurrent(exact) => exact.source_revision,
        ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) => prepared.source_revision,
    }
}

fn summary_id(source: &ThreadCatalogSummaryPreparation) -> beryl_model::SyndicThreadId {
    match source {
        ThreadCatalogSummaryPreparation::ExactCurrent(exact) => exact.summary.thread_id(),
        ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) => {
            prepared.replacement.thread_id()
        }
    }
}

impl DomainMutation<SyndicDomain> for PublishThreadCatalogSummaryPair {
    type Error = SyndicMutationError;
    type Prepared = Self;

    fn prepare(self, reader: &DomainReader<'_, SyndicDomain>) -> Result<Self, Self::Error> {
        for source in std::iter::once(&self.first).chain(self.second.as_ref()) {
            match source {
                ThreadCatalogSummaryPreparation::ExactCurrent(exact) => {
                    ValidateCurrentThreadCatalogSummary {
                        exact: exact.clone(),
                    }
                    .validate(reader)?;
                }
                ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) => {
                    validate_prepared(reader, prepared)?
                }
            }
        }
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let count = std::iter::once(&self.first)
            .chain(self.second.as_ref())
            .filter(|source| {
                matches!(
                    source,
                    ThreadCatalogSummaryPreparation::PreparedReplacement(_)
                )
            })
            .count();
        if count > 0 {
            reservation.reserve_records::<ThreadCatalogSummariesCodec>(count)?;
        }
        Ok(())
    }

    fn contribute(
        prepared: Self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        for source in std::iter::once(prepared.first).chain(prepared.second) {
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

impl DomainMutation<SyndicDomain> for RebuildThreadCatalogSummary {
    type Error = SyndicMutationError;
    type Prepared = Self;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        validate_prepared(reader, &self.prepared)?;
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ThreadCatalogSummariesCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<ThreadCatalogSummariesCodec>(
            &prepared.prepared.replacement.thread_id(),
            &prepared.prepared.replacement,
        )?;
        Ok(())
    }
}

impl DomainValidator<SyndicDomain> for ValidateCurrentThreadCatalogSummary {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let exact = &self.exact;
        let thread_id = exact.summary.thread_id();
        if required::<ThreadCatalogSummariesFamily>(reader, &thread_id)? != exact.summary
            || required::<ThreadsFamily>(reader, &thread_id)? != exact.sources.thread
            || required::<ThreadExecutionsFamily>(reader, &thread_id)? != exact.sources.execution
            || required::<ThreadAttributesFamily>(reader, &thread_id)? != exact.sources.attributes
            || required::<HistorySummariesFamily>(reader, &thread_id)? != exact.sources.history
        {
            return Err(SyndicMutationError::ThreadCatalogSummaryConflict);
        }
        Ok(())
    }
}

fn validate_prepared(
    reader: &DomainReader<'_, SyndicDomain>,
    prepared: &PreparedThreadCatalogSummaryReplacement,
) -> Result<(), SyndicMutationError> {
    let thread_id = prepared.replacement.thread_id();
    let next_revision = match &prepared.expected {
        Some(expected) if expected.thread_id() == thread_id => {
            expected.revision().checked_next().ok()
        }
        Some(_) => None,
        None => Some(beryl_model::ProjectionRevision::from_nonzero(
            std::num::NonZeroU64::MIN,
        )),
    };
    if super::super::point::<ThreadCatalogSummariesFamily>(reader, &thread_id)? != prepared.expected
        || required::<ThreadsFamily>(reader, &thread_id)? != prepared.sources.thread
        || required::<ThreadExecutionsFamily>(reader, &thread_id)? != prepared.sources.execution
        || required::<ThreadAttributesFamily>(reader, &thread_id)? != prepared.sources.attributes
        || required::<HistorySummariesFamily>(reader, &thread_id)? != prepared.sources.history
        || next_revision != Some(prepared.replacement.revision())
    {
        return Err(SyndicMutationError::ThreadCatalogSummaryConflict);
    }
    let expected_replacement = ThreadCatalogSummaryRecord::from_sources(
        prepared.replacement.revision(),
        prepared.replacement.title().cloned(),
        &prepared.sources.thread,
        &prepared.sources.execution,
        &prepared.sources.attributes,
        &prepared.sources.history,
    );
    if expected_replacement != prepared.replacement
        || prepared.expected.as_ref() == Some(&prepared.replacement)
        || !title_precedence_agrees(prepared)
    {
        return Err(SyndicMutationError::ThreadCatalogSummaryConflict);
    }
    Ok(())
}

fn title_precedence_agrees(prepared: &PreparedThreadCatalogSummaryReplacement) -> bool {
    match (
        prepared.sources.attributes.generated_title(),
        prepared.replacement.title(),
    ) {
        (Some(generated), Some(title)) => {
            title.source() == ThreadCatalogTitleSource::Generated
                && title.text() == generated.text()
        }
        (Some(_), None) => false,
        (None, Some(title)) => title.source() == ThreadCatalogTitleSource::HistoryDerived,
        (None, None) => true,
    }
}
