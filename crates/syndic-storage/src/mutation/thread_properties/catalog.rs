use beryl_home_store::{
    DomainMutation, DomainReader, DomainValidator, MutationBuilder, MutationContribution,
    ReconciliationReservation, ValidationContribution,
};

use crate::{
    ExactThreadCatalogSummary, PreparedThreadCatalogSummaryReplacement, SyndicMutationError,
    ThreadCatalogSummaryRecord, ThreadCatalogTitleSource,
    codec::{
        HistorySummariesFamily, ThreadAttributesFamily, ThreadCatalogSummariesCodec,
        ThreadCatalogSummariesFamily, ThreadExecutionsFamily, ThreadsFamily,
    },
    domain::{SyndicDomain, SyndicStorage},
};

use super::super::required;

struct RebuildThreadCatalogSummary {
    prepared: PreparedThreadCatalogSummaryReplacement,
}

struct ValidateCurrentThreadCatalogSummary {
    exact: ExactThreadCatalogSummary,
}

impl SyndicStorage {
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

impl DomainMutation<SyndicDomain> for RebuildThreadCatalogSummary {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        validate_prepared(reader, &self.prepared)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ThreadCatalogSummariesCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        validate_prepared(reader, &self.prepared)?;
        mutations.put::<ThreadCatalogSummariesCodec>(
            &self.prepared.replacement.thread_id(),
            &self.prepared.replacement,
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
    let thread_id = prepared.expected.thread_id();
    if prepared.replacement.thread_id() != thread_id
        || required::<ThreadCatalogSummariesFamily>(reader, &thread_id)? != prepared.expected
        || required::<ThreadsFamily>(reader, &thread_id)? != prepared.sources.thread
        || required::<ThreadExecutionsFamily>(reader, &thread_id)? != prepared.sources.execution
        || required::<ThreadAttributesFamily>(reader, &thread_id)? != prepared.sources.attributes
        || required::<HistorySummariesFamily>(reader, &thread_id)? != prepared.sources.history
        || prepared.expected.revision().checked_next().ok() != Some(prepared.replacement.revision())
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
        || prepared.expected == prepared.replacement
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
