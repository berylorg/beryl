use beryl_home_store::HomeStore;
use beryl_model::{DomainRevision, ProjectionRevision};

mod frozen;
pub use frozen::{FrozenThreadCatalogSummaryAuthentication, ThreadCatalogSummaryRebuildReason};

use crate::{
    HistorySummaryRecord, SyndicReadError, ThreadAttributesRecord, ThreadCatalogSummaryRecord,
    ThreadCatalogTitle, ThreadCatalogTitleSource, ThreadExecutionRecord, ThreadRecord,
    catalog_title::{
        HistoryTitlePath, HistoryTitleReadError, StoreTitleSnapshot, derive_history_title,
    },
    codec::{
        Family, HistorySummariesFamily, ThreadAttributesFamily, ThreadCatalogSummariesFamily,
        ThreadExecutionsFamily, ThreadsFamily, family_point_limit,
    },
    domain::SyndicStorage,
};

const PREPARATION_OPERATION: &str = "thread-catalog summary preparation";
const INITIAL_SUMMARY_REVISION: ProjectionRevision =
    ProjectionRevision::from_nonzero(std::num::NonZeroU64::MIN);

#[derive(Clone, Debug)]
pub(crate) struct CurrentCatalogSources {
    pub(crate) thread: ThreadRecord,
    pub(crate) execution: ThreadExecutionRecord,
    pub(crate) attributes: ThreadAttributesRecord,
    pub(crate) history: HistorySummaryRecord,
}

/// One exact source-current compact summary captured under a stable Syndic revision.
#[derive(Clone, Debug)]
pub struct ExactThreadCatalogSummary {
    pub(crate) source_revision: DomainRevision,
    pub(crate) summary: ThreadCatalogSummaryRecord,
    pub(crate) sources: CurrentCatalogSources,
}

impl ExactThreadCatalogSummary {
    /// Returns the exact source-current summary suitable for a Beryl catalog row.
    #[must_use]
    pub const fn summary(&self) -> &ThreadCatalogSummaryRecord {
        &self.summary
    }
}

/// Opaque exact replacement prepared from one stable canonical Syndic snapshot.
#[derive(Clone, Debug)]
pub struct PreparedThreadCatalogSummaryReplacement {
    pub(crate) source_revision: DomainRevision,
    pub(crate) expected: Option<ThreadCatalogSummaryRecord>,
    pub(crate) replacement: ThreadCatalogSummaryRecord,
    pub(crate) sources: CurrentCatalogSources,
}

impl PreparedThreadCatalogSummaryReplacement {
    /// Returns the exact summary that this opaque mutation authority will publish atomically.
    #[must_use]
    pub const fn replacement(&self) -> &ThreadCatalogSummaryRecord {
        &self.replacement
    }
}

/// Stable result of preparing one compact Syndic catalog summary.
#[derive(Clone, Debug)]
pub enum ThreadCatalogSummaryPreparation {
    /// The stored summary is already the exact semantic projection of current sources.
    ExactCurrent(ExactThreadCatalogSummary),
    /// A checked semantic successor is ready for exact revision-checked publication.
    PreparedReplacement(PreparedThreadCatalogSummaryReplacement),
}

impl SyndicStorage {
    /// Prepares one exact current compact summary with fixed-memory selected-path and content reads.
    ///
    /// Absence is returned only when the named thread remains absent across the stable read. A
    /// generated title avoids the fallback scan; otherwise the exact history-derived algorithm is
    /// evaluated before the source revision is stabilized.
    pub fn prepare_thread_catalog_summary(
        &self,
        store: &HomeStore,
        thread_id: beryl_model::SyndicThreadId,
    ) -> Result<Option<ThreadCatalogSummaryPreparation>, SyndicReadError> {
        let source_revision = self.revision(store)?;
        let Some(thread) =
            self.point::<ThreadsFamily>(store, thread_id, limit::<ThreadsFamily>())?
        else {
            return if self.revision(store)? == source_revision {
                Ok(None)
            } else {
                Err(concurrent())
            };
        };
        let execution = required(
            self.point::<ThreadExecutionsFamily>(
                store,
                thread_id,
                limit::<ThreadExecutionsFamily>(),
            )?,
            "thread-catalog execution source is missing",
        )?;
        let attributes = required(
            self.point::<ThreadAttributesFamily>(
                store,
                thread_id,
                limit::<ThreadAttributesFamily>(),
            )?,
            "thread-catalog attributes source is missing",
        )?;
        let history = required(
            self.point::<HistorySummariesFamily>(
                store,
                thread_id,
                limit::<HistorySummariesFamily>(),
            )?,
            "thread-catalog history source is missing",
        )?;
        if thread.id() != thread_id {
            return Err(SyndicReadError::Invariant(
                "thread-catalog thread key and identity disagree",
            ));
        }
        let current = self.point::<ThreadCatalogSummariesFamily>(
            store,
            thread_id,
            limit::<ThreadCatalogSummariesFamily>(),
        )?;
        validate_canonical_sources(&thread, &execution, &attributes, &history)?;
        if let Some(current) = &current {
            validate_source_identities(&thread, &execution, &attributes, &history, current)?;
        }
        let title = match attributes.generated_title() {
            Some(generated) => Some(
                ThreadCatalogTitle::new(generated.text(), ThreadCatalogTitleSource::Generated)
                    .map_err(|_| {
                        SyndicReadError::Invariant(
                            "canonical generated title cannot enter a compact summary",
                        )
                    })?,
            ),
            None => derive_history_title(
                &StoreTitleSnapshot {
                    storage: self,
                    store,
                },
                &thread,
                HistoryTitlePath::ThreadRules,
            )
            .map_err(map_title_error)?,
        };
        if self.revision(store)? != source_revision {
            return Err(concurrent());
        }

        let sources = CurrentCatalogSources {
            thread,
            execution,
            attributes,
            history,
        };
        let desired = ThreadCatalogSummaryRecord::from_sources(
            current.as_ref().map_or(
                INITIAL_SUMMARY_REVISION,
                ThreadCatalogSummaryRecord::revision,
            ),
            title,
            &sources.thread,
            &sources.execution,
            &sources.attributes,
            &sources.history,
        );
        if current.as_ref() == Some(&desired) {
            return Ok(Some(ThreadCatalogSummaryPreparation::ExactCurrent(
                ExactThreadCatalogSummary {
                    source_revision,
                    summary: desired,
                    sources,
                },
            )));
        }
        let next_revision = match &current {
            Some(current) => current
                .revision()
                .checked_next()
                .map_err(|_| SyndicReadError::CatalogSummaryRevisionExhausted)?,
            None => INITIAL_SUMMARY_REVISION,
        };
        let replacement = ThreadCatalogSummaryRecord::from_sources(
            next_revision,
            desired.title().cloned(),
            &sources.thread,
            &sources.execution,
            &sources.attributes,
            &sources.history,
        );
        Ok(Some(ThreadCatalogSummaryPreparation::PreparedReplacement(
            PreparedThreadCatalogSummaryReplacement {
                source_revision,
                expected: current,
                replacement,
                sources,
            },
        )))
    }
}

fn validate_source_identities(
    thread: &ThreadRecord,
    execution: &ThreadExecutionRecord,
    attributes: &ThreadAttributesRecord,
    history: &HistorySummaryRecord,
    current: &ThreadCatalogSummaryRecord,
) -> Result<(), SyndicReadError> {
    validate_canonical_sources(thread, execution, attributes, history)?;
    if current.thread_id() != thread.id() {
        return Err(SyndicReadError::Invariant(
            "thread-catalog canonical source identities disagree",
        ));
    }
    if current.execution() != execution.execution()
        || current.parent_thread_id() != thread.parent_thread_id()
        || current.lineage_depth() != thread.lineage_depth()
        || current.lineage_digest() != thread.lineage_digest()
    {
        return Err(SyndicReadError::Invariant(
            "thread-catalog immutable payload disagrees with canonical sources",
        ));
    }
    let witnesses = current.sources();
    if witnesses.attributes_revision() > attributes.revision()
        || witnesses.history_summary_revision() > history.revision()
        || witnesses.history_thread_revision() > history.thread_revision()
        || witnesses.thread_revision() > thread.revision()
    {
        return Err(SyndicReadError::Invariant(
            "thread-catalog source witness is in the future",
        ));
    }
    Ok(())
}

fn validate_canonical_sources(
    thread: &ThreadRecord,
    execution: &ThreadExecutionRecord,
    attributes: &ThreadAttributesRecord,
    history: &HistorySummaryRecord,
) -> Result<(), SyndicReadError> {
    if execution.thread_id() != thread.id()
        || attributes.thread_id() != thread.id()
        || history.thread_id() != thread.id()
    {
        return Err(SyndicReadError::Invariant(
            "thread-catalog canonical source identities disagree",
        ));
    }
    if history.thread_revision() != thread.revision()
        || history.committed_tail() != thread.committed_tail()
        || history.selected_path_digest() != thread.selected_path_digest()
    {
        return Err(SyndicReadError::Invariant(
            "thread-catalog history source is not current for its thread",
        ));
    }
    Ok(())
}

fn limit<F: Family>() -> crate::SyndicPointReadLimit {
    crate::SyndicPointReadLimit::new(family_point_limit::<F>().max_bytes())
        .expect("catalog source point-read limit is nonzero")
}

fn required<T>(value: Option<T>, message: &'static str) -> Result<T, SyndicReadError> {
    value.ok_or(SyndicReadError::Invariant(message))
}

fn map_title_error(error: HistoryTitleReadError) -> SyndicReadError {
    match error {
        HistoryTitleReadError::Read(source) => SyndicReadError::Read(source),
        HistoryTitleReadError::Invariant(message) => SyndicReadError::Invariant(message),
    }
}

fn concurrent() -> SyndicReadError {
    SyndicReadError::ConcurrentChange {
        operation: PREPARATION_OPERATION,
    }
}
