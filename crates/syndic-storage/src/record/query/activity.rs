use beryl_model::{SyndicItemId, SyndicThreadId, SyndicTurnId};

use crate::{
    ActivityQueryRevision, ActivityWorkPeriod, CasItemSource, ProjectionLifecycle,
    ProjectionSourceRange, ProviderItemKind, ProviderItemLifecycle, SourceEventSequence,
    SyndicRecordError, SyndicTimestamp,
};

/// Maximum number of completed activity rows retained in one current work period.
pub(crate) const ACTIVITY_COMPLETED_RETAINED_ROWS: u64 = 256;
/// Maximum exact encoded bytes of completed activity rows retained in one current work period.
pub(crate) const ACTIVITY_COMPLETED_RETAINED_BYTES: u64 = 65_536;

/// One root source turn whose work period is selected by an activity query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivityQuerySource {
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
}

/// One exact source-turn membership in an owner work period.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityQuerySourceRecord {
    thread_id: SyndicThreadId,
    work_period: ActivityWorkPeriod,
    source: ActivityQuerySource,
    activity_start: Option<SourceEventSequence>,
    source_frontier: u64,
    active: bool,
    child_handoff: Option<ActivityChildHandoffMembership>,
}

/// Exact child final-answer item and range admitted into one source membership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivityChildHandoffMembership {
    item_id: SyndicItemId,
    final_answer_range: ProjectionSourceRange,
}

impl ActivityChildHandoffMembership {
    #[must_use]
    pub const fn new(item_id: SyndicItemId, final_answer_range: ProjectionSourceRange) -> Self {
        Self {
            item_id,
            final_answer_range,
        }
    }

    #[must_use]
    pub const fn item_id(self) -> SyndicItemId {
        self.item_id
    }

    #[must_use]
    pub const fn final_answer_range(self) -> ProjectionSourceRange {
        self.final_answer_range
    }
}

impl ActivityQuerySourceRecord {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        work_period: ActivityWorkPeriod,
        source: ActivityQuerySource,
        activity_start: Option<SourceEventSequence>,
        source_frontier: u64,
        active: bool,
        child_handoff: Option<ActivityChildHandoffMembership>,
    ) -> Self {
        Self {
            thread_id,
            work_period,
            source,
            activity_start,
            source_frontier,
            active,
            child_handoff,
        }
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn work_period(&self) -> ActivityWorkPeriod {
        self.work_period
    }
    #[must_use]
    pub const fn source(&self) -> ActivityQuerySource {
        self.source
    }
    #[must_use]
    pub const fn activity_start(&self) -> Option<SourceEventSequence> {
        self.activity_start
    }
    #[must_use]
    pub const fn source_frontier(&self) -> u64 {
        self.source_frontier
    }
    #[must_use]
    pub const fn active(&self) -> bool {
        self.active
    }
    #[must_use]
    pub const fn child_handoff(&self) -> Option<ActivityChildHandoffMembership> {
        self.child_handoff
    }
}

impl ActivityQuerySource {
    #[must_use]
    pub const fn new(thread_id: SyndicThreadId, turn_id: SyndicTurnId) -> Self {
        Self { thread_id, turn_id }
    }

    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn turn_id(self) -> SyndicTurnId {
        self.turn_id
    }
}

/// Exact Syndic and CAS identity of one activity row's source item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityItemSource {
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    item_id: SyndicItemId,
    cas_item: CasItemSource,
}

impl ActivityItemSource {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        item_id: SyndicItemId,
        cas_item: CasItemSource,
    ) -> Self {
        Self {
            thread_id,
            turn_id,
            item_id,
            cas_item,
        }
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }

    #[must_use]
    pub const fn item_id(&self) -> SyndicItemId {
        self.item_id
    }

    #[must_use]
    pub const fn cas_item(&self) -> &CasItemSource {
        &self.cas_item
    }
}

/// Exact source-bound fact for a completed child-thread handoff row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityChildHandoffFact {
    observed_child_thread_id: SyndicThreadId,
    final_answer_range: ProjectionSourceRange,
}

impl ActivityChildHandoffFact {
    #[must_use]
    pub const fn new(
        observed_child_thread_id: SyndicThreadId,
        final_answer_range: ProjectionSourceRange,
    ) -> Self {
        Self {
            observed_child_thread_id,
            final_answer_range,
        }
    }

    #[must_use]
    pub const fn observed_child_thread_id(&self) -> SyndicThreadId {
        self.observed_child_thread_id
    }

    #[must_use]
    pub const fn final_answer_range(&self) -> ProjectionSourceRange {
        self.final_answer_range
    }

    /// Returns the handoff byte count derived from the exact narrative range.
    #[must_use]
    pub const fn byte_count(&self) -> u64 {
        self.final_answer_range.len()
    }
}

/// One compact GUI-derived activity fact that carries no source payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActivityCompactFact {
    ChildHandoff(ActivityChildHandoffFact),
}

/// Durable running-first/recent ordering key for one activity row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivityQueryOrder {
    running: bool,
    updated_at: SyndicTimestamp,
    item_id: SyndicItemId,
}

impl Ord for ActivityQueryOrder {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .running
            .cmp(&self.running)
            .then_with(|| other.updated_at.cmp(&self.updated_at))
            .then_with(|| self.item_id.cmp(&other.item_id))
    }
}

impl PartialOrd for ActivityQueryOrder {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl ActivityQueryOrder {
    #[must_use]
    pub const fn new(running: bool, updated_at: SyndicTimestamp, item_id: SyndicItemId) -> Self {
        Self {
            running,
            updated_at,
            item_id,
        }
    }

    #[must_use]
    pub const fn running(self) -> bool {
        self.running
    }

    #[must_use]
    pub const fn updated_at(self) -> SyndicTimestamp {
        self.updated_at
    }

    #[must_use]
    pub const fn item_id(self) -> SyndicItemId {
        self.item_id
    }
}

/// Selected work period, source membership, and exact bounded activity counters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityQueryHeadRecord {
    thread_id: SyndicThreadId,
    work_period: ActivityWorkPeriod,
    source: Option<ActivityQuerySource>,
    source_active: bool,
    source_frontier: u64,
    revision: ActivityQueryRevision,
    source_count: u64,
    logical_row_count: u64,
    running_row_count: u64,
    completed_row_count: u64,
    completed_stored_bytes: u64,
    completed_retention_cutoff: Option<ActivityQueryOrder>,
    lifecycle: ProjectionLifecycle,
}

impl ActivityQueryHeadRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        thread_id: SyndicThreadId,
        work_period: ActivityWorkPeriod,
        source: Option<ActivityQuerySource>,
        source_active: bool,
        source_frontier: u64,
        revision: ActivityQueryRevision,
        source_count: u64,
        logical_row_count: u64,
        running_row_count: u64,
        completed_row_count: u64,
        completed_stored_bytes: u64,
        completed_retention_cutoff: Option<ActivityQueryOrder>,
        lifecycle: ProjectionLifecycle,
    ) -> Result<Self, SyndicRecordError> {
        let counted = running_row_count
            .checked_add(completed_row_count)
            .ok_or(SyndicRecordError::InvalidActivityQueryFrontier)?;
        let completed_shape = if completed_row_count == 0 {
            completed_stored_bytes == 0 && completed_retention_cutoff.is_none()
        } else {
            completed_stored_bytes != 0 && completed_retention_cutoff.is_some()
        };
        let source_shape = match source {
            None => !source_active && source_frontier == 0 && source_count == 0 && counted == 0,
            Some(source) => {
                source.thread_id() == thread_id
                    && source_count != 0
                    && (!source_active || lifecycle == ProjectionLifecycle::Current)
            }
        };
        if logical_row_count != counted
            || !completed_shape
            || completed_retention_cutoff.is_some_and(ActivityQueryOrder::running)
            || !source_shape
            || (!source_active && running_row_count != 0)
        {
            return Err(SyndicRecordError::InvalidActivityQueryFrontier);
        }
        Ok(Self {
            thread_id,
            work_period,
            source,
            source_active,
            source_frontier,
            revision,
            source_count,
            logical_row_count,
            running_row_count,
            completed_row_count,
            completed_stored_bytes,
            completed_retention_cutoff,
            lifecycle,
        })
    }

    #[must_use]
    pub fn empty(thread_id: SyndicThreadId) -> Self {
        Self::new(
            thread_id,
            ActivityWorkPeriod::FIRST,
            None,
            false,
            0,
            ActivityQueryRevision::FIRST,
            0,
            0,
            0,
            0,
            0,
            None,
            ProjectionLifecycle::Current,
        )
        .expect("the canonical empty activity head is valid")
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn work_period(&self) -> ActivityWorkPeriod {
        self.work_period
    }
    #[must_use]
    pub const fn source(&self) -> Option<ActivityQuerySource> {
        self.source
    }
    #[must_use]
    pub const fn source_active(&self) -> bool {
        self.source_active
    }
    #[must_use]
    pub const fn source_frontier(&self) -> u64 {
        self.source_frontier
    }
    #[must_use]
    pub const fn revision(&self) -> ActivityQueryRevision {
        self.revision
    }
    #[must_use]
    pub const fn source_count(&self) -> u64 {
        self.source_count
    }
    #[must_use]
    pub const fn logical_row_count(&self) -> u64 {
        self.logical_row_count
    }
    #[must_use]
    pub const fn running_row_count(&self) -> u64 {
        self.running_row_count
    }
    #[must_use]
    pub const fn completed_row_count(&self) -> u64 {
        self.completed_row_count
    }
    #[must_use]
    pub const fn completed_stored_bytes(&self) -> u64 {
        self.completed_stored_bytes
    }
    #[must_use]
    pub const fn completed_retention_cutoff(&self) -> Option<ActivityQueryOrder> {
        self.completed_retention_cutoff
    }
    #[must_use]
    pub const fn lifecycle(&self) -> ProjectionLifecycle {
        self.lifecycle
    }
}

/// One ordered activity row backed by exact Syndic and external item authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityQueryEntryRecord {
    thread_id: SyndicThreadId,
    work_period: ActivityWorkPeriod,
    order: ActivityQueryOrder,
    source: ActivityItemSource,
    source_event: SourceEventSequence,
    provider_kind: ProviderItemKind,
    provider_lifecycle: ProviderItemLifecycle,
    compact_fact: Option<ActivityCompactFact>,
}

impl ActivityQueryEntryRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        thread_id: SyndicThreadId,
        work_period: ActivityWorkPeriod,
        order: ActivityQueryOrder,
        source: ActivityItemSource,
        source_event: SourceEventSequence,
        provider_kind: ProviderItemKind,
        provider_lifecycle: ProviderItemLifecycle,
        compact_fact: Option<ActivityCompactFact>,
    ) -> Result<Self, SyndicRecordError> {
        if order.item_id() != source.item_id()
            || order.running() != (provider_lifecycle == ProviderItemLifecycle::Started)
            || (order.running() && compact_fact.is_some())
        {
            return Err(SyndicRecordError::InvalidActivityQueryLifecycle);
        }
        Ok(Self {
            thread_id,
            work_period,
            order,
            source,
            source_event,
            provider_kind,
            provider_lifecycle,
            compact_fact,
        })
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn work_period(&self) -> ActivityWorkPeriod {
        self.work_period
    }
    #[must_use]
    pub const fn order(&self) -> ActivityQueryOrder {
        self.order
    }
    #[must_use]
    pub const fn source(&self) -> &ActivityItemSource {
        &self.source
    }
    #[must_use]
    pub const fn item_id(&self) -> SyndicItemId {
        self.order.item_id()
    }
    #[must_use]
    pub const fn source_event(&self) -> SourceEventSequence {
        self.source_event
    }
    #[must_use]
    pub const fn provider_kind(&self) -> ProviderItemKind {
        self.provider_kind
    }
    #[must_use]
    pub const fn provider_lifecycle(&self) -> ProviderItemLifecycle {
        self.provider_lifecycle
    }
    #[must_use]
    pub const fn compact_fact(&self) -> Option<&ActivityCompactFact> {
        self.compact_fact.as_ref()
    }
}
