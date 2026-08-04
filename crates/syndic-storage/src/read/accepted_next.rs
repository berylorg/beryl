use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeStore};
use beryl_model::{
    AcceptedInputRevision, DomainRevision, InputGateRevision, SyndicAcceptedInputId, SyndicThreadId,
};

use crate::{
    AcceptedInputLifecycle, AcceptedInputOrdinal, AcceptedInputRecord, AcceptedNextSourceRecord,
    AcceptedOrderIndexRecord, AcceptedRouteGeneration, AcceptedRouteGenerationHeadRecord,
    AcceptedRouteGenerationRecord, AcceptedRouteLeafRecord, AcceptedRouteLeafState,
    AcceptedRouteRevision, AcceptedRouteTarget, ActivityQueryHeadRecord, BindingHeadRecord,
    BindingRecord, DraftByThreadRecord, HistorySummaryRecord, InputGateRecord, InputGateState,
    NextTurnReason, ProjectionLifecycle, SyndicReadError, SyndicTimestamp,
    TranscriptViewHeadRecord, codec::*, domain::SyndicStorage,
};

use super::SyndicPointReadLimit;

mod candidate;
mod source;

/// Maximum accepted-order records scanned or sources returned by one accepted-next page.
pub const ACCEPTED_NEXT_PAGE_MAX_RECORDS: usize = 256;

/// Maximum stored or practical decoded bytes retained by one accepted-next page.
pub const ACCEPTED_NEXT_PAGE_MAX_BYTES: usize = 65_536;

const NEXT_POINT_MAX_BYTES: usize = 65_536;
// Accepted-order plus the largest V3 route leaf carrying both transition and promotion proofs.
const NEXT_CANDIDATE_MAX_ROW_BYTES: usize = 261;

/// Opaque revision-bound source of effective next-turn work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedNextSource {
    source_revision: DomainRevision,
    record: AcceptedNextSourceRecord,
}

impl AcceptedNextSource {
    /// Returns the exact domain revision at which this source was observed.
    #[must_use]
    pub const fn source_revision(self) -> DomainRevision {
        self.source_revision
    }

    /// Returns the source thread used for global scheduling order.
    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.record.thread_id()
    }

    /// Returns the source route generation used for global scheduling order.
    #[must_use]
    pub const fn generation(self) -> AcceptedRouteGeneration {
        self.record.generation()
    }

    pub(crate) const fn record(self) -> AcceptedNextSourceRecord {
        self.record
    }
}

/// Domain-revision-bound continuation for the global ordered next-source scan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedNextSourceCursor {
    source_revision: DomainRevision,
    after_thread_id: SyndicThreadId,
    after_generation: AcceptedRouteGeneration,
}

/// One bounded global page of compact durable next-turn source authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedNextSourcePage {
    source_revision: DomainRevision,
    records: Vec<AcceptedNextSource>,
    stored_bytes: usize,
    decoded_bytes: usize,
    next_cursor: Option<AcceptedNextSourceCursor>,
}

impl AcceptedNextSourcePage {
    /// Returns the exact domain revision fenced by this complete scan.
    #[must_use]
    pub const fn source_revision(&self) -> DomainRevision {
        self.source_revision
    }

    /// Returns compact sources in `(thread, route generation)` order.
    #[must_use]
    pub fn records(&self) -> &[AcceptedNextSource] {
        &self.records
    }

    /// Returns aggregate stored bytes retained by this page.
    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }

    /// Returns aggregate practical decoded bytes retained by this page.
    #[must_use]
    pub const fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }

    /// Returns the exact continuation when more sources exist at this revision.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<AcceptedNextSourceCursor> {
        self.next_cursor
    }
}

/// Exact source-bound continuation after the last accepted ordinal scanned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedNextCandidateCursor {
    source_revision: DomainRevision,
    thread_id: SyndicThreadId,
    gate_revision: InputGateRevision,
    generation: AcceptedRouteGeneration,
    generation_revision: AcceptedRouteRevision,
    scanned_after: AcceptedInputOrdinal,
}

/// Fixed current authority retained for one effective next-turn candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AcceptedNextCandidateBasis {
    source_revision: DomainRevision,
    source: AcceptedNextSourceRecord,
    gate: InputGateRecord,
    thread: crate::ThreadRecord,
    draft_by_thread: DraftByThreadRecord,
    route_head: Option<AcceptedRouteGenerationHeadRecord>,
    generation: AcceptedRouteGenerationRecord,
    leaf: AcceptedRouteLeafRecord,
    input: AcceptedInputRecord,
    order: AcceptedOrderIndexRecord,
    binding_head: BindingHeadRecord,
    binding: BindingRecord,
    transcript_head: TranscriptViewHeadRecord,
    summary: HistorySummaryRecord,
    activity_head: ActivityQueryHeadRecord,
}

impl AcceptedNextCandidateBasis {
    pub(crate) const fn source(&self) -> &AcceptedNextSourceRecord {
        &self.source
    }

    pub(crate) const fn gate(&self) -> &InputGateRecord {
        &self.gate
    }

    pub(crate) const fn thread(&self) -> &crate::ThreadRecord {
        &self.thread
    }

    pub(crate) const fn draft_by_thread(&self) -> &DraftByThreadRecord {
        &self.draft_by_thread
    }

    pub(crate) const fn route_head(&self) -> Option<&AcceptedRouteGenerationHeadRecord> {
        self.route_head.as_ref()
    }

    pub(crate) const fn generation(&self) -> &AcceptedRouteGenerationRecord {
        &self.generation
    }

    pub(crate) const fn leaf(&self) -> &AcceptedRouteLeafRecord {
        &self.leaf
    }

    pub(crate) const fn input(&self) -> &AcceptedInputRecord {
        &self.input
    }

    pub(crate) const fn order(&self) -> &AcceptedOrderIndexRecord {
        &self.order
    }

    pub(crate) const fn binding_head(&self) -> &BindingHeadRecord {
        &self.binding_head
    }

    pub(crate) const fn binding(&self) -> &BindingRecord {
        &self.binding
    }

    pub(crate) const fn transcript_head(&self) -> &TranscriptViewHeadRecord {
        &self.transcript_head
    }

    pub(crate) const fn summary(&self) -> &HistorySummaryRecord {
        &self.summary
    }

    pub(crate) const fn activity_head(&self) -> &ActivityQueryHeadRecord {
        &self.activity_head
    }
}

/// The earliest effective next-turn input found for one source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedNextCandidate {
    reason: NextTurnReason,
    basis: AcceptedNextCandidateBasis,
}

impl AcceptedNextCandidate {
    /// Returns the owning thread.
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.basis.source.thread_id()
    }

    /// Returns the permanent accepted-input identity.
    #[must_use]
    pub const fn input_id(&self) -> SyndicAcceptedInputId {
        self.basis.input.id()
    }

    /// Returns the permanent accepted-input ordinal.
    #[must_use]
    pub const fn ordinal(&self) -> AcceptedInputOrdinal {
        self.basis.order.ordinal()
    }

    /// Returns the exact current route-leaf revision.
    #[must_use]
    pub const fn leaf_revision(&self) -> AcceptedInputRevision {
        self.basis.leaf.revision()
    }

    /// Returns why this input is effective next-turn work.
    #[must_use]
    pub const fn next_turn_reason(&self) -> NextTurnReason {
        self.reason
    }

    /// Returns the domain revision that fences this candidate's complete basis.
    #[must_use]
    pub const fn source_revision(&self) -> DomainRevision {
        self.basis.source_revision
    }

    /// Returns the earliest timestamp accepted by this candidate's promotion witness.
    #[must_use]
    pub const fn minimum_promotion_timestamp(&self) -> SyndicTimestamp {
        let admitted_at = self.basis.input.admitted_at();
        let last_activity_at = self.basis.summary.last_activity_at();
        if admitted_at.unix_millis() >= last_activity_at.unix_millis() {
            admitted_at
        } else {
            last_activity_at
        }
    }

    pub(crate) const fn basis(&self) -> &AcceptedNextCandidateBasis {
        &self.basis
    }
}

/// One bounded source-local scan page containing at most the first effective candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedNextCandidatePage {
    candidate: Option<AcceptedNextCandidate>,
    stored_bytes: usize,
    decoded_bytes: usize,
    next_cursor: Option<AcceptedNextCandidateCursor>,
}

impl AcceptedNextCandidatePage {
    /// Borrows the earliest effective candidate when this page found one.
    #[must_use]
    pub const fn candidate(&self) -> Option<&AcceptedNextCandidate> {
        self.candidate.as_ref()
    }

    /// Returns aggregate stored bytes scanned by this page.
    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }

    /// Returns aggregate practical decoded bytes scanned by this page.
    #[must_use]
    pub const fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }

    /// Returns the exact continuation after a terminal-only bounded scan.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<AcceptedNextCandidateCursor> {
        self.next_cursor
    }

    /// Consumes the page and returns its earliest effective candidate, if present.
    #[must_use]
    pub fn into_candidate(self) -> Option<AcceptedNextCandidate> {
        self.candidate
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/accepted_next_row_bound.rs"
    ));
}
