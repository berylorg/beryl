use beryl_home_store::{FrozenHomeRead, HomeStore};
use beryl_model::SyndicThreadId;

use super::{
    Family, HistorySummariesFamily, SyndicReadError, SyndicStorage, ThreadAttributesFamily,
    ThreadCatalogSummariesFamily, ThreadCatalogSummaryRecord, ThreadCatalogTitleSource,
    ThreadExecutionsFamily, ThreadsFamily, family_point_limit, required,
    validate_canonical_sources, validate_source_identities,
};
use crate::{codec::ExactCodec, domain::SyndicDomain};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadCatalogSummaryRebuildReason {
    Missing,
    Outdated,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FrozenThreadCatalogSummaryAuthentication {
    ThreadMissing,
    RebuildRequired(ThreadCatalogSummaryRebuildReason),
    Current(ThreadCatalogSummaryRecord),
}

impl SyndicStorage {
    pub fn authenticate_frozen_thread_catalog_summary(
        &self,
        store: &HomeStore,
        frozen: &FrozenHomeRead,
        thread_id: SyndicThreadId,
    ) -> Result<FrozenThreadCatalogSummaryAuthentication, SyndicReadError> {
        let Some(thread) = self.frozen_catalog_point::<ThreadsFamily>(store, frozen, thread_id)?
        else {
            return Ok(FrozenThreadCatalogSummaryAuthentication::ThreadMissing);
        };
        if thread.id() != thread_id {
            return Err(SyndicReadError::Invariant(
                "thread-catalog thread key and identity disagree",
            ));
        }
        let execution = required(
            self.frozen_catalog_point::<ThreadExecutionsFamily>(store, frozen, thread_id)?,
            "thread-catalog execution source is missing",
        )?;
        let attributes = required(
            self.frozen_catalog_point::<ThreadAttributesFamily>(store, frozen, thread_id)?,
            "thread-catalog attributes source is missing",
        )?;
        let history = required(
            self.frozen_catalog_point::<HistorySummariesFamily>(store, frozen, thread_id)?,
            "thread-catalog history source is missing",
        )?;
        authenticate_sources(
            thread,
            execution,
            attributes,
            history,
            self.frozen_catalog_point::<ThreadCatalogSummariesFamily>(store, frozen, thread_id)?,
        )
    }

    pub fn authenticate_thread_catalog_summary(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
    ) -> Result<FrozenThreadCatalogSummaryAuthentication, SyndicReadError> {
        let Some(thread) =
            self.point::<ThreadsFamily>(store, thread_id, live_point_limit::<ThreadsFamily>())?
        else {
            return Ok(FrozenThreadCatalogSummaryAuthentication::ThreadMissing);
        };
        if thread.id() != thread_id {
            return Err(SyndicReadError::Invariant(
                "thread-catalog thread key and identity disagree",
            ));
        }
        let execution = required(
            self.point::<ThreadExecutionsFamily>(
                store,
                thread_id,
                live_point_limit::<ThreadExecutionsFamily>(),
            )?,
            "thread-catalog execution source is missing",
        )?;
        let attributes = required(
            self.point::<ThreadAttributesFamily>(
                store,
                thread_id,
                live_point_limit::<ThreadAttributesFamily>(),
            )?,
            "thread-catalog attributes source is missing",
        )?;
        let history = required(
            self.point::<HistorySummariesFamily>(
                store,
                thread_id,
                live_point_limit::<HistorySummariesFamily>(),
            )?,
            "thread-catalog history source is missing",
        )?;
        authenticate_sources(
            thread,
            execution,
            attributes,
            history,
            self.point::<ThreadCatalogSummariesFamily>(
                store,
                thread_id,
                live_point_limit::<ThreadCatalogSummariesFamily>(),
            )?,
        )
    }

    fn frozen_catalog_point<F: Family<Key = SyndicThreadId>>(
        &self,
        store: &HomeStore,
        frozen: &FrozenHomeRead,
        thread_id: SyndicThreadId,
    ) -> Result<Option<F::Value>, SyndicReadError> {
        Ok(store.read_frozen_point::<SyndicDomain, ExactCodec<F>>(
            frozen,
            &self.handle,
            &thread_id,
            family_point_limit::<F>(),
        )?)
    }
}

fn live_point_limit<F: Family>() -> crate::SyndicPointReadLimit {
    let bytes = F::MAX_VALUE_BYTES
        .checked_add(beryl_home_store::RECORD_VERSION_BYTES)
        .expect("catalog witness family bound fits usize");
    crate::SyndicPointReadLimit::new(bytes)
        .expect("catalog witness family has a nonzero schema bound")
}

fn authenticate_sources(
    thread: crate::ThreadRecord,
    execution: crate::ThreadExecutionRecord,
    attributes: crate::ThreadAttributesRecord,
    history: crate::HistorySummaryRecord,
    current: Option<ThreadCatalogSummaryRecord>,
) -> Result<FrozenThreadCatalogSummaryAuthentication, SyndicReadError> {
    validate_canonical_sources(&thread, &execution, &attributes, &history)?;
    let Some(current) = current else {
        return Ok(FrozenThreadCatalogSummaryAuthentication::RebuildRequired(
            ThreadCatalogSummaryRebuildReason::Missing,
        ));
    };
    validate_source_identities(&thread, &execution, &attributes, &history, &current)?;
    let desired = ThreadCatalogSummaryRecord::from_sources(
        current.revision(),
        current.title().cloned(),
        &thread,
        &execution,
        &attributes,
        &history,
    );
    let title_agrees = match (attributes.generated_title(), current.title()) {
        (Some(generated), Some(title)) => {
            title.source() == ThreadCatalogTitleSource::Generated
                && title.text() == generated.text()
        }
        (Some(_), None) => false,
        (None, Some(title)) => title.source() == ThreadCatalogTitleSource::HistoryDerived,
        (None, None) => true,
    };
    Ok(if current == desired && title_agrees {
        FrozenThreadCatalogSummaryAuthentication::Current(current)
    } else {
        FrozenThreadCatalogSummaryAuthentication::RebuildRequired(
            ThreadCatalogSummaryRebuildReason::Outdated,
        )
    })
}
