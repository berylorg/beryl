use beryl_model::SyndicThreadId;

use crate::UnixMillis;

use super::{
    CatalogArchiveSummary, CatalogClaimSummary, CatalogExecutionSummary, CatalogFreshness,
    CatalogLineageSummary, CatalogRevision, CatalogSearchFields, CatalogSourceRevisions,
    CatalogTitleFacts, CatalogTitleSource, CatalogValueError,
};

/// Complete bounded facts copied into a compact catalog projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogFacts {
    titles: CatalogTitleFacts,
    execution: CatalogExecutionSummary,
    archive: CatalogArchiveSummary,
    last_activity_at: UnixMillis,
    claim: CatalogClaimSummary,
    lineage: CatalogLineageSummary,
    search: CatalogSearchFields,
}

impl CatalogFacts {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        titles: CatalogTitleFacts,
        execution: CatalogExecutionSummary,
        archive: CatalogArchiveSummary,
        last_activity_at: UnixMillis,
        claim: CatalogClaimSummary,
        lineage: CatalogLineageSummary,
        search: CatalogSearchFields,
    ) -> Self {
        Self {
            titles,
            execution,
            archive,
            last_activity_at,
            claim,
            lineage,
            search,
        }
    }

    #[must_use]
    pub const fn titles(&self) -> &CatalogTitleFacts {
        &self.titles
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
        for (kind, candidate) in [
            ("generated title", self.titles.generated()),
            ("Syndic title", self.titles.syndic()),
        ] {
            if candidate
                .is_some_and(|candidate| candidate.source_thread_revision() > sources.thread())
            {
                return Err(CatalogValueError::TitleSourceNewerThanRow { kind });
            }
        }
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
        top_level_thread_id,
        parent_thread_id,
        depth,
    } = lineage
    else {
        return Ok(());
    };
    if top_level_thread_id == thread_id || parent_thread_id == thread_id {
        return Err(CatalogValueError::InvalidLineage(
            "a lineage cannot name its own thread as an ancestor",
        ));
    }
    if (depth.get() == 1) != (top_level_thread_id == parent_thread_id) {
        return Err(CatalogValueError::InvalidLineage(
            "lineage depth one must name the top-level thread as its parent",
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
    pub fn display_title(&self) -> &str {
        self.facts.titles().display_title()
    }

    #[must_use]
    pub const fn display_title_source(&self) -> CatalogTitleSource {
        self.facts.titles().display_source()
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
