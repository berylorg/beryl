use std::sync::Arc;

use beryl_home_store::HomeGeneration;
use beryl_model::BerylHomeId;
use thiserror::Error;

use super::ProjectionServiceGeneration;

mod records;
pub(super) mod source;

pub use records::{
    CompactionCommandWorkStage, CompactionOperationWorkFact, CompactionWorkFact,
    CompactionWorkRecord, ContinuationWorkFact, ContinuationWorkStage,
};

#[derive(Clone, Debug)]
pub struct CompactionWorkRevision {
    pub(super) owner: Arc<()>,
    pub(super) home_id: BerylHomeId,
    pub(super) home_generation: HomeGeneration,
    pub(super) service_generation: ProjectionServiceGeneration,
    pub(super) stamp: u64,
}

impl PartialEq for CompactionWorkRevision {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner)
            && self.home_id == other.home_id
            && self.home_generation == other.home_generation
            && self.service_generation == other.service_generation
            && self.stamp == other.stamp
    }
}
impl Eq for CompactionWorkRevision {}

impl CompactionWorkRevision {
    pub const fn home_id(&self) -> BerylHomeId {
        self.home_id
    }
    pub const fn home_generation(&self) -> HomeGeneration {
        self.home_generation
    }
    pub const fn service_generation(&self) -> ProjectionServiceGeneration {
        self.service_generation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactionWorkCursor {
    pub(super) revision: CompactionWorkRevision,
    pub(super) after: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactionWorkPageLimits {
    pub(super) max_records: usize,
    pub(super) max_bytes: usize,
}

impl CompactionWorkPageLimits {
    pub fn new(max_records: usize, max_bytes: usize) -> Result<Self, CompactionWorkError> {
        if max_records == 0 || max_bytes == 0 {
            return Err(CompactionWorkError::InvalidLimits);
        }
        Ok(Self {
            max_records: max_records.min(256),
            max_bytes: max_bytes.min(65_536),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactionWorkPage {
    pub(super) revision: CompactionWorkRevision,
    pub(super) records: Vec<CompactionWorkRecord>,
    pub(super) bytes: usize,
    pub(super) next_cursor: Option<CompactionWorkCursor>,
}

impl CompactionWorkPage {
    pub const fn revision(&self) -> &CompactionWorkRevision {
        &self.revision
    }
    pub fn records(&self) -> &[CompactionWorkRecord] {
        &self.records
    }
    pub const fn bytes(&self) -> usize {
        self.bytes
    }
    pub const fn next_cursor(&self) -> Option<&CompactionWorkCursor> {
        self.next_cursor.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum CompactionWorkError {
    #[error("the compaction work owner is closed")]
    Closed,
    #[error("the compaction work revision is stale")]
    StaleRevision,
    #[error("the compaction work revision or cursor belongs to another owner")]
    ForeignRevision,
    #[error("compaction work page limits must be nonzero")]
    InvalidLimits,
    #[error("one compaction work record exceeds the page byte limit")]
    ByteLimit,
    #[error("the compaction work revision is unavailable")]
    RevisionUnavailable,
    #[error("a compaction work source is unavailable")]
    SourceUnavailable,
    #[error("the compaction work source lock is poisoned")]
    Poisoned,
}
