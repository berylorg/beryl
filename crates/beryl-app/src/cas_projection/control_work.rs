use beryl_home_store::HomeGeneration;
use beryl_model::BerylHomeId;
use thiserror::Error;

use super::{
    CompactionWorkCursor, CompactionWorkError, CompactionWorkRecord, CompactionWorkRevision,
    ProjectionServiceGeneration, StopWorkCursor, StopWorkError, StopWorkRecord, StopWorkRevision,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlWorkRevision {
    pub(super) stop: StopWorkRevision,
    pub(super) compaction: CompactionWorkRevision,
}

impl ControlWorkRevision {
    pub const fn home_id(&self) -> BerylHomeId {
        self.stop.home_id()
    }
    pub const fn home_generation(&self) -> HomeGeneration {
        self.stop.home_generation()
    }
    pub const fn service_generation(&self) -> ProjectionServiceGeneration {
        self.stop.service_generation()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ControlWorkPosition {
    Stop(StopWorkCursor),
    Compaction(Option<CompactionWorkCursor>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlWorkCursor {
    pub(super) revision: ControlWorkRevision,
    pub(super) position: ControlWorkPosition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlWorkPageLimits {
    pub(super) max_records: usize,
    pub(super) max_bytes: usize,
}

impl ControlWorkPageLimits {
    pub fn new(max_records: usize, max_bytes: usize) -> Result<Self, ControlWorkError> {
        if max_records == 0 || max_bytes == 0 {
            return Err(ControlWorkError::InvalidLimits);
        }
        Ok(Self {
            max_records: max_records.min(256),
            max_bytes: max_bytes.min(65_536),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlWorkPage {
    pub(super) revision: ControlWorkRevision,
    pub(super) stop_records: Vec<StopWorkRecord>,
    pub(super) compaction_records: Vec<CompactionWorkRecord>,
    pub(super) bytes: usize,
    pub(super) next_cursor: Option<ControlWorkCursor>,
}

impl ControlWorkPage {
    pub const fn revision(&self) -> &ControlWorkRevision {
        &self.revision
    }
    pub fn stop_records(&self) -> &[StopWorkRecord] {
        &self.stop_records
    }
    pub fn compaction_records(&self) -> &[CompactionWorkRecord] {
        &self.compaction_records
    }
    pub fn len(&self) -> usize {
        self.stop_records.len() + self.compaction_records.len()
    }
    pub fn is_empty(&self) -> bool {
        self.stop_records.is_empty() && self.compaction_records.is_empty()
    }
    pub const fn bytes(&self) -> usize {
        self.bytes
    }
    pub const fn next_cursor(&self) -> Option<&ControlWorkCursor> {
        self.next_cursor.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ControlWorkError {
    #[error("the control work cursor belongs to another revision")]
    ForeignCursor,
    #[error("control work page limits must be nonzero")]
    InvalidLimits,
    #[error(transparent)]
    Stop(#[from] StopWorkError),
    #[error(transparent)]
    Compaction(#[from] CompactionWorkError),
}
