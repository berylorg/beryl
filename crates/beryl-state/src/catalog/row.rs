use beryl_model::SyndicThreadId;

use crate::UnixMillis;

use super::{
    CatalogArchiveSummary, CatalogClaimSummary, CatalogExecutionSummary, CatalogFreshness,
    CatalogLineageSummary, CatalogResolvedTitle, CatalogRevision, CatalogSearchFields,
    CatalogSourceRevisions, CatalogTitleSource, CatalogValueError,
};

/// Complete bounded facts copied into a compact catalog projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogFacts {
    title: CatalogResolvedTitle,
    execution: CatalogExecutionSummary,
    archive: CatalogArchiveSummary,
    last_activity_at: UnixMillis,
    complete: bool,
    claim: CatalogClaimSummary,
    lineage: CatalogLineageSummary,
    search: CatalogSearchFields,
}

impl CatalogFacts {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        title: CatalogResolvedTitle,
        execution: CatalogExecutionSummary,
        archive: CatalogArchiveSummary,
        last_activity_at: UnixMillis,
        complete: bool,
        claim: CatalogClaimSummary,
        lineage: CatalogLineageSummary,
    ) -> Result<Self, CatalogValueError> {
        let search = CatalogSearchFields::from_visible(&title, &execution)?;
        Ok(Self::from_parts(
            title,
            execution,
            archive,
            last_activity_at,
            complete,
            claim,
            lineage,
            search,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) const fn from_parts(
        title: CatalogResolvedTitle,
        execution: CatalogExecutionSummary,
        archive: CatalogArchiveSummary,
        last_activity_at: UnixMillis,
        complete: bool,
        claim: CatalogClaimSummary,
        lineage: CatalogLineageSummary,
        search: CatalogSearchFields,
    ) -> Self {
        Self {
            title,
            execution,
            archive,
            last_activity_at,
            complete,
            claim,
            lineage,
            search,
        }
    }

    #[must_use]
    pub const fn title(&self) -> &CatalogResolvedTitle {
        &self.title
    }

    #[must_use]
    pub const fn execution(&self) -> &CatalogExecutionSummary {
        &self.execution
    }

    #[must_use]
    pub const fn archive(&self) -> CatalogArchiveSummary {
        self.archive
    }

    #[must_use]
    pub const fn last_activity_at(&self) -> UnixMillis {
        self.last_activity_at
    }

    #[must_use]
    pub const fn complete(&self) -> bool {
        self.complete
    }

    #[must_use]
    pub const fn claim(&self) -> CatalogClaimSummary {
        self.claim
    }

    #[must_use]
    pub const fn lineage(&self) -> CatalogLineageSummary {
        self.lineage
    }

    #[must_use]
    pub const fn search(&self) -> &CatalogSearchFields {
        &self.search
    }

    pub(super) fn validate_for(
        &self,
        thread_id: SyndicThreadId,
        sources: CatalogSourceRevisions,
    ) -> Result<(), CatalogValueError> {
        if self.claim.window_id().is_some() != sources.claim().is_some() {
            return Err(CatalogValueError::ClaimSourceMismatch);
        }
        validate_lineage(self.lineage, thread_id)
    }
}

fn validate_lineage(
    lineage: CatalogLineageSummary,
    thread_id: SyndicThreadId,
) -> Result<(), CatalogValueError> {
    let CatalogLineageSummary::Descendant {
        parent_thread_id, ..
    } = lineage
    else {
        return Ok(());
    };
    if parent_thread_id == thread_id {
        return Err(CatalogValueError::InvalidLineage(
            "a lineage cannot name its own thread as its parent",
        ));
    }
    Ok(())
}

/// One durable compact projection and its package-local revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogRow {
    thread_id: SyndicThreadId,
    sources: CatalogSourceRevisions,
    freshness: CatalogFreshness,
    facts: CatalogFacts,
    revision: CatalogRevision,
}

impl CatalogRow {
    pub(super) fn current(
        thread_id: SyndicThreadId,
        sources: CatalogSourceRevisions,
        facts: CatalogFacts,
        revision: CatalogRevision,
    ) -> Result<Self, CatalogValueError> {
        Self::from_parts(
            thread_id,
            sources,
            CatalogFreshness::Current,
            facts,
            revision,
        )
    }

    pub(super) fn from_parts(
        thread_id: SyndicThreadId,
        sources: CatalogSourceRevisions,
        freshness: CatalogFreshness,
        facts: CatalogFacts,
        revision: CatalogRevision,
    ) -> Result<Self, CatalogValueError> {
        facts.validate_for(thread_id, sources)?;
        Ok(Self {
            thread_id,
            sources,
            freshness,
            facts,
            revision,
        })
    }

    pub(super) fn mark_stale(&self, revision: CatalogRevision) -> Result<Self, CatalogValueError> {
        Self::from_parts(
            self.thread_id,
            self.sources,
            CatalogFreshness::Stale,
            self.facts.clone(),
            revision,
        )
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn sources(&self) -> CatalogSourceRevisions {
        self.sources
    }

    #[must_use]
    pub const fn freshness(&self) -> CatalogFreshness {
        self.freshness
    }

    #[must_use]
    pub const fn facts(&self) -> &CatalogFacts {
        &self.facts
    }

    #[must_use]
    pub const fn revision(&self) -> CatalogRevision {
        self.revision
    }

    #[must_use]
    pub fn title(&self) -> &CatalogResolvedTitle {
        self.facts.title()
    }

    #[must_use]
    pub const fn title_source(&self) -> CatalogTitleSource {
        self.facts.title().source()
    }

    #[must_use]
    pub const fn recency_cursor(&self) -> CatalogRecencyCursor {
        CatalogRecencyCursor::new(self.facts.last_activity_at(), self.thread_id)
    }
}

/// Stable pagination position in recent-first order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CatalogRecencyCursor {
    last_activity_at: UnixMillis,
    thread_id: SyndicThreadId,
}

impl CatalogRecencyCursor {
    #[must_use]
    pub const fn new(last_activity_at: UnixMillis, thread_id: SyndicThreadId) -> Self {
        Self {
            last_activity_at,
            thread_id,
        }
    }

    #[must_use]
    pub const fn last_activity_at(self) -> UnixMillis {
        self.last_activity_at
    }

    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    pub(super) const fn first() -> Self {
        Self::new(
            UnixMillis::new(u64::MAX),
            SyndicThreadId::from_bytes([0; 16]),
        )
    }

    pub(super) const fn last() -> Self {
        Self::new(
            UnixMillis::new(0),
            SyndicThreadId::from_bytes([u8::MAX; 16]),
        )
    }
}
