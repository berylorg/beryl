use std::cmp::Reverse;

use beryl_model::{ExecutionBinding, SyndicThreadId};
use syndic_storage::{SyndicReadError, SyndicTimestamp, ThreadCatalogTitle};
use thiserror::Error;

use crate::{
    cas_projection::*,
    lifecycle_attention::{
        LifecycleAttentionRecord, LifecycleAttentionWorkError, LifecycleAttentionWorkRevision,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessWorkRevision {
    pub(super) work: super::required::RequiredWorkRevision,
    pub(super) attention: LifecycleAttentionWorkRevision,
}

pub(super) type SortKey = (Reverse<SyndicTimestamp>, SyndicThreadId);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessWorkCursor {
    pub(super) revision: ProcessWorkRevision,
    pub(super) after: SortKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessWorkPageLimits {
    pub(super) max_records: usize,
    pub(super) max_bytes: usize,
}

impl ProcessWorkPageLimits {
    pub fn new(max_records: usize, max_bytes: usize) -> Result<Self, ProcessWorkError> {
        if max_records == 0 || max_bytes == 0 {
            return Err(ProcessWorkError::InvalidLimits);
        }
        Ok(Self {
            max_records: max_records.min(256),
            max_bytes: max_bytes.min(65_536),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProcessWorkFacts {
    pub pending: bool,
    pub queued: bool,
    pub preparing: bool,
    pub executing: bool,
    pub request_handling: bool,
    pub stopping: bool,
    pub compacting: bool,
    pub continuation: bool,
    pub terminal_settlement: bool,
    pub cleanup: bool,
}

impl ProcessWorkFacts {
    pub(super) fn merge(&mut self, other: Self) {
        self.pending |= other.pending;
        self.queued |= other.queued;
        self.preparing |= other.preparing;
        self.executing |= other.executing;
        self.request_handling |= other.request_handling;
        self.stopping |= other.stopping;
        self.compacting |= other.compacting;
        self.continuation |= other.continuation;
        self.terminal_settlement |= other.terminal_settlement;
        self.cleanup |= other.cleanup;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessWorkRecord {
    pub thread_id: SyndicThreadId,
    pub title: Option<ThreadCatalogTitle>,
    pub execution: ExecutionBinding,
    pub last_activity_at: SyndicTimestamp,
    pub facts: ProcessWorkFacts,
    pub attention: Vec<LifecycleAttentionRecord>,
}

impl ProcessWorkRecord {
    pub(super) fn key(&self) -> SortKey {
        (Reverse(self.last_activity_at), self.thread_id)
    }

    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.title.as_ref().map_or(0, |title| title.text().len())
            + self.execution.root_path().as_str().len()
            + self.attention.len() * std::mem::size_of::<LifecycleAttentionRecord>()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessWorkPage {
    pub(super) revision: ProcessWorkRevision,
    pub(super) records: Vec<ProcessWorkRecord>,
    pub(super) total_threads: u64,
    pub(super) bytes: usize,
    pub(super) next_cursor: Option<ProcessWorkCursor>,
}

impl ProcessWorkPage {
    pub fn revision(&self) -> &ProcessWorkRevision {
        &self.revision
    }
    pub fn records(&self) -> &[ProcessWorkRecord] {
        &self.records
    }
    pub const fn total_threads(&self) -> u64 {
        self.total_threads
    }
    pub const fn bytes(&self) -> usize {
        self.bytes
    }
    pub fn next_cursor(&self) -> Option<&ProcessWorkCursor> {
        self.next_cursor.as_ref()
    }
}

#[derive(Debug, Error)]
pub enum ProcessWorkError {
    #[error(transparent)]
    MutationObservation(#[from] beryl_home_store::HomeMutationObservationError),
    #[error("the process work service is closed")]
    Closed,
    #[error("the process work sources belong to different services")]
    ForeignSources,
    #[error("the process work cursor belongs to another revision")]
    ForeignCursor,
    #[error("the process work source revision is stale")]
    StaleRevision,
    #[error("the process work query was cancelled")]
    Cancelled,
    #[error("process work page limits must be nonzero")]
    InvalidLimits,
    #[error("the process work source exceeds the configured session capacity")]
    SourceBoundExceeded,
    #[error("one process work row exceeds the page byte limit")]
    ByteLimit,
    #[error("the logical process work count overflowed")]
    CountOverflow,
    #[error("a process work thread has no canonical metadata")]
    MissingMetadata,
    #[error(transparent)]
    Durable(#[from] SyndicReadError),
    #[error(transparent)]
    Sessions(#[from] ScheduledSessionWorkError),
    #[error(transparent)]
    Connections(#[from] ConnectionWorkError),
    #[error(transparent)]
    Controls(#[from] ControlWorkError),
    #[error(transparent)]
    Attention(#[from] LifecycleAttentionWorkError),
}
