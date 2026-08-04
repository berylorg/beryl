use beryl_model::{
    ExecutionBinding, ProjectionRevision, SyndicPathDigest, SyndicThreadId, ThreadRevision,
};

use crate::{
    HistorySummaryRecord, SyndicTimestamp, SyndicValueError, ThreadAttributesRecord,
    ThreadAttributesRevision, ThreadExecutionRecord, ThreadLineageDepth, ThreadRecord,
};

use super::{ThreadArchiveState, attributes::validate_thread_title};

/// Maximum logical UTF-8 source bytes examined for one history-derived title.
pub const HISTORY_DERIVED_TITLE_SCAN_MAX_BYTES: usize = 4_096;

/// Maximum Unicode scalar values retained by one history-derived title.
pub const HISTORY_DERIVED_TITLE_MAX_SCALARS: usize = 80;

/// Canonical source of one resolved compact catalog title.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadCatalogTitleSource {
    Generated,
    HistoryDerived,
}

/// One resolved title copied into the compact Syndic catalog projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadCatalogTitle {
    text: Box<str>,
    source: ThreadCatalogTitleSource,
}

impl ThreadCatalogTitle {
    pub fn new(
        text: impl AsRef<str>,
        source: ThreadCatalogTitleSource,
    ) -> Result<Self, SyndicValueError> {
        let text = match source {
            ThreadCatalogTitleSource::Generated => validate_thread_title(text.as_ref())?,
            ThreadCatalogTitleSource::HistoryDerived => {
                validate_history_derived_title(text.as_ref())?
            }
        };
        Ok(Self { text, source })
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
    #[must_use]
    pub const fn source(&self) -> ThreadCatalogTitleSource {
        self.source
    }
}

/// Exact canonical-source fence retained by one compact catalog projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThreadCatalogSourceWitnesses {
    attributes_revision: ThreadAttributesRevision,
    history_summary_revision: ProjectionRevision,
    history_thread_revision: ThreadRevision,
    history_selected_path_digest: SyndicPathDigest,
    thread_revision: ThreadRevision,
}

impl ThreadCatalogSourceWitnesses {
    #[must_use]
    pub const fn new(
        attributes_revision: ThreadAttributesRevision,
        history_summary_revision: ProjectionRevision,
        history_thread_revision: ThreadRevision,
        history_selected_path_digest: SyndicPathDigest,
        thread_revision: ThreadRevision,
    ) -> Self {
        Self {
            attributes_revision,
            history_summary_revision,
            history_thread_revision,
            history_selected_path_digest,
            thread_revision,
        }
    }

    #[must_use]
    pub const fn attributes_revision(self) -> ThreadAttributesRevision {
        self.attributes_revision
    }
    #[must_use]
    pub const fn history_summary_revision(self) -> ProjectionRevision {
        self.history_summary_revision
    }
    #[must_use]
    pub const fn history_thread_revision(self) -> ThreadRevision {
        self.history_thread_revision
    }
    #[must_use]
    pub const fn history_selected_path_digest(self) -> SyndicPathDigest {
        self.history_selected_path_digest
    }
    #[must_use]
    pub const fn thread_revision(self) -> ThreadRevision {
        self.thread_revision
    }
}

/// Rebuildable compact thread catalog projection owned by Syndic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadCatalogSummaryRecord {
    thread_id: SyndicThreadId,
    revision: ProjectionRevision,
    title: Option<ThreadCatalogTitle>,
    execution: ExecutionBinding,
    archive: ThreadArchiveState,
    last_activity_at: SyndicTimestamp,
    complete: bool,
    parent_thread_id: Option<SyndicThreadId>,
    lineage_depth: ThreadLineageDepth,
    lineage_digest: SyndicPathDigest,
    sources: ThreadCatalogSourceWitnesses,
}

impl ThreadCatalogSummaryRecord {
    #[must_use]
    pub fn initial(
        thread: &ThreadRecord,
        execution: &ThreadExecutionRecord,
        attributes: &ThreadAttributesRecord,
        history: &HistorySummaryRecord,
    ) -> Self {
        Self::new(
            thread.id(),
            ProjectionRevision::new(1).expect("initial projection revision is nonzero"),
            None,
            execution.execution().clone(),
            attributes.archive(),
            history.last_activity_at(),
            history.complete(),
            thread.parent_thread_id(),
            thread.lineage_depth(),
            thread.lineage_digest(),
            ThreadCatalogSourceWitnesses::new(
                attributes.revision(),
                history.revision(),
                history.thread_revision(),
                history.selected_path_digest(),
                thread.revision(),
            ),
        )
    }

    #[must_use]
    pub(crate) fn initial_with_history_title(
        thread: &ThreadRecord,
        execution: &ThreadExecutionRecord,
        attributes: &ThreadAttributesRecord,
        history: &HistorySummaryRecord,
        title: Option<ThreadCatalogTitle>,
    ) -> Self {
        Self::from_sources(
            ProjectionRevision::new(1).expect("initial projection revision is nonzero"),
            title,
            thread,
            execution,
            attributes,
            history,
        )
    }

    #[must_use]
    pub(crate) fn from_sources(
        revision: ProjectionRevision,
        title: Option<ThreadCatalogTitle>,
        thread: &ThreadRecord,
        execution: &ThreadExecutionRecord,
        attributes: &ThreadAttributesRecord,
        history: &HistorySummaryRecord,
    ) -> Self {
        Self::new(
            thread.id(),
            revision,
            title,
            execution.execution().clone(),
            attributes.archive(),
            history.last_activity_at(),
            history.complete(),
            thread.parent_thread_id(),
            thread.lineage_depth(),
            thread.lineage_digest(),
            ThreadCatalogSourceWitnesses::new(
                attributes.revision(),
                history.revision(),
                history.thread_revision(),
                history.selected_path_digest(),
                thread.revision(),
            ),
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        revision: ProjectionRevision,
        title: Option<ThreadCatalogTitle>,
        execution: ExecutionBinding,
        archive: ThreadArchiveState,
        last_activity_at: SyndicTimestamp,
        complete: bool,
        parent_thread_id: Option<SyndicThreadId>,
        lineage_depth: ThreadLineageDepth,
        lineage_digest: SyndicPathDigest,
        sources: ThreadCatalogSourceWitnesses,
    ) -> Self {
        Self {
            thread_id,
            revision,
            title,
            execution,
            archive,
            last_activity_at,
            complete,
            parent_thread_id,
            lineage_depth,
            lineage_digest,
            sources,
        }
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn revision(&self) -> ProjectionRevision {
        self.revision
    }
    #[must_use]
    pub const fn title(&self) -> Option<&ThreadCatalogTitle> {
        self.title.as_ref()
    }
    #[must_use]
    pub const fn execution(&self) -> &ExecutionBinding {
        &self.execution
    }
    #[must_use]
    pub const fn archive(&self) -> ThreadArchiveState {
        self.archive
    }
    #[must_use]
    pub const fn last_activity_at(&self) -> SyndicTimestamp {
        self.last_activity_at
    }
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.complete
    }
    #[must_use]
    pub const fn parent_thread_id(&self) -> Option<SyndicThreadId> {
        self.parent_thread_id
    }
    #[must_use]
    pub const fn lineage_depth(&self) -> ThreadLineageDepth {
        self.lineage_depth
    }
    #[must_use]
    pub const fn lineage_digest(&self) -> SyndicPathDigest {
        self.lineage_digest
    }
    #[must_use]
    pub const fn sources(&self) -> ThreadCatalogSourceWitnesses {
        self.sources
    }
}

fn validate_history_derived_title(value: &str) -> Result<Box<str>, SyndicValueError> {
    const KIND: &str = "history-derived thread title";
    if value.is_empty() {
        return Err(SyndicValueError::EmptyText { kind: KIND });
    }
    if value.len() > super::attributes::THREAD_TITLE_MAX_BYTES {
        return Err(SyndicValueError::TextTooLong {
            kind: KIND,
            maximum: super::attributes::THREAD_TITLE_MAX_BYTES,
            actual: value.len(),
        });
    }
    let scalar_count = value.chars().count();
    if scalar_count > HISTORY_DERIVED_TITLE_MAX_SCALARS {
        return Err(SyndicValueError::TooManyScalars {
            kind: KIND,
            maximum: HISTORY_DERIVED_TITLE_MAX_SCALARS,
            actual: scalar_count,
        });
    }
    if value.chars().next_back().is_some_and(char::is_whitespace) {
        return Err(SyndicValueError::SurroundingWhitespace { kind: KIND });
    }
    if let Some((index, _)) = value.char_indices().find(|(_, value)| value.is_control()) {
        return Err(SyndicValueError::ControlCharacter { kind: KIND, index });
    }
    Ok(value.into())
}
