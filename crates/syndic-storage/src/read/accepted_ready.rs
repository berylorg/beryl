use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeStore};
use beryl_model::{
    AcceptedInputRevision, DomainRevision, InputGateRevision, SyndicAcceptedInputId, SyndicThreadId,
};

use crate::{
    AcceptedInputLifecycle, AcceptedInputOrdinal, AcceptedReadySourceRecord,
    AcceptedRouteGeneration, AcceptedRouteGenerationHeadRecord, AcceptedRouteGenerationRecord,
    AcceptedRouteHeadProof, AcceptedRouteLeafRecord, AcceptedRouteLeafState, AcceptedRouteRevision,
    AcceptedRouteTarget, InputGateRecord, InputGateState, SyndicReadError, codec::*,
    domain::SyndicStorage,
};

use super::SyndicPointReadLimit;

mod candidate;
mod source;

/// Maximum records scanned or returned by one accepted-ready page.
pub const ACCEPTED_READY_PAGE_MAX_RECORDS: usize = 256;

/// Maximum stored or practical decoded bytes retained by one accepted-ready page.
pub const ACCEPTED_READY_PAGE_MAX_BYTES: usize = 65_536;

const READY_POINT_MAX_BYTES: usize = 65_536;
const READY_CANDIDATE_MAX_ROW_BYTES: usize = 189;

/// Domain-revision-bound continuation for the global ordered ready-source scan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedReadySourceCursor {
    source_revision: DomainRevision,
    after_thread_id: SyndicThreadId,
    after_generation: AcceptedRouteGeneration,
}

/// One bounded global page of compact durable ready-source authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedReadySourcePage {
    source_revision: DomainRevision,
    records: Vec<AcceptedReadySourceRecord>,
    stored_bytes: usize,
    decoded_bytes: usize,
    next_cursor: Option<AcceptedReadySourceCursor>,
}

impl AcceptedReadySourcePage {
    /// Returns the exact domain revision fenced by this complete scan.
    #[must_use]
    pub const fn source_revision(&self) -> DomainRevision {
        self.source_revision
    }

    /// Returns compact sources in `(thread, route generation)` order.
    #[must_use]
    pub fn records(&self) -> &[AcceptedReadySourceRecord] {
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
    pub const fn next_cursor(&self) -> Option<AcceptedReadySourceCursor> {
        self.next_cursor
    }
}

/// Exact source-bound continuation after the last accepted ordinal scanned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedReadyCandidateCursor {
    thread_id: SyndicThreadId,
    gate_revision: InputGateRevision,
    generation: AcceptedRouteGeneration,
    generation_revision: AcceptedRouteRevision,
    scanned_after: AcceptedInputOrdinal,
}

/// Compact ready or retryable accepted-input scheduling fact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedReadyCandidate {
    input_id: SyndicAcceptedInputId,
    ordinal: AcceptedInputOrdinal,
    lifecycle: AcceptedInputLifecycle,
    leaf_revision: AcceptedInputRevision,
}

impl AcceptedReadyCandidate {
    #[must_use]
    pub const fn input_id(self) -> SyndicAcceptedInputId {
        self.input_id
    }

    #[must_use]
    pub const fn ordinal(self) -> AcceptedInputOrdinal {
        self.ordinal
    }

    #[must_use]
    pub const fn lifecycle(self) -> AcceptedInputLifecycle {
        self.lifecycle
    }

    #[must_use]
    pub const fn leaf_revision(self) -> AcceptedInputRevision {
        self.leaf_revision
    }
}

/// One bounded source-local scan page containing only ready or retryable candidates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedReadyCandidatePage {
    records: Vec<AcceptedReadyCandidate>,
    stored_bytes: usize,
    decoded_bytes: usize,
    next_cursor: Option<AcceptedReadyCandidateCursor>,
}

impl AcceptedReadyCandidatePage {
    #[must_use]
    pub fn records(&self) -> &[AcceptedReadyCandidate] {
        &self.records
    }

    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }

    #[must_use]
    pub const fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }

    #[must_use]
    pub const fn next_cursor(&self) -> Option<AcceptedReadyCandidateCursor> {
        self.next_cursor
    }
}
