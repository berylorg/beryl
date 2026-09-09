use std::sync::Arc;

use beryl_home_store::HomeGeneration;
use beryl_model::BerylHomeId;
use syndic_storage::StopOperationId;
use thiserror::Error;

use super::ProjectionServiceGeneration;

mod records;

pub use records::{
    PermissionInterruptionWorkFact, PermissionInterruptionWorkStage, StopDispatchWorkState,
    StopWorkFact, StopWorkRecord,
};

#[derive(Clone, Debug)]
pub struct StopWorkRevision {
    pub(super) owner: Arc<()>,
    pub(super) home_id: BerylHomeId,
    pub(super) home_generation: HomeGeneration,
    pub(super) service_generation: ProjectionServiceGeneration,
    pub(super) stamp: u64,
}

impl PartialEq for StopWorkRevision {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner)
            && self.home_id == other.home_id
            && self.home_generation == other.home_generation
            && self.service_generation == other.service_generation
            && self.stamp == other.stamp
    }
}
impl Eq for StopWorkRevision {}

impl StopWorkRevision {
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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum StopWorkKey {
    Stop(StopOperationId),
    Permission(u64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StopWorkCursor {
    pub(super) revision: StopWorkRevision,
    pub(super) after: StopWorkKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StopWorkPageLimits {
    pub(super) max_records: usize,
    pub(super) max_bytes: usize,
}

impl StopWorkPageLimits {
    pub fn new(max_records: usize, max_bytes: usize) -> Result<Self, StopWorkError> {
        if max_records == 0 || max_bytes == 0 {
            return Err(StopWorkError::InvalidLimits);
        }
        Ok(Self {
            max_records: max_records.min(256),
            max_bytes: max_bytes.min(65_536),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StopWorkPage {
    pub(super) revision: StopWorkRevision,
    pub(super) records: Vec<StopWorkRecord>,
    pub(super) bytes: usize,
    pub(super) next_cursor: Option<StopWorkCursor>,
}

impl StopWorkPage {
    pub const fn revision(&self) -> &StopWorkRevision {
        &self.revision
    }
    pub fn records(&self) -> &[StopWorkRecord] {
        &self.records
    }
    pub const fn bytes(&self) -> usize {
        self.bytes
    }
    pub const fn next_cursor(&self) -> Option<&StopWorkCursor> {
        self.next_cursor.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum StopWorkError {
    #[error("the stop work owner is closed")]
    Closed,
    #[error("the stop work revision is stale")]
    StaleRevision,
    #[error("the stop work revision or cursor belongs to another owner")]
    ForeignRevision,
    #[error("stop work page limits must be nonzero")]
    InvalidLimits,
    #[error("one stop work record exceeds the page byte limit")]
    ByteLimit,
    #[error("the stop work revision is unavailable")]
    RevisionUnavailable,
    #[error("a stop work source is unavailable")]
    SourceUnavailable,
    #[error("the stop work owner lock is poisoned")]
    Poisoned,
}

pub(super) struct StopWorkPageBuilder<'a> {
    pub(super) after: Option<&'a StopWorkKey>,
    limits: StopWorkPageLimits,
    records: Vec<StopWorkRecord>,
    bytes: usize,
    last: Option<StopWorkKey>,
    more: bool,
}

impl<'a> StopWorkPageBuilder<'a> {
    pub(super) fn new(after: Option<&'a StopWorkKey>, limits: StopWorkPageLimits) -> Self {
        Self {
            after,
            limits,
            records: Vec::new(),
            bytes: 0,
            last: None,
            more: false,
        }
    }

    pub(super) fn push(
        &mut self,
        key: StopWorkKey,
        record: StopWorkRecord,
    ) -> Result<bool, StopWorkError> {
        if self.after.is_some_and(|after| &key <= after) {
            return Ok(true);
        }
        let bytes = self
            .bytes
            .checked_add(record.bytes())
            .ok_or(StopWorkError::ByteLimit)?;
        if self.records.len() >= self.limits.max_records || bytes > self.limits.max_bytes {
            if self.records.is_empty() {
                return Err(StopWorkError::ByteLimit);
            }
            self.more = true;
            return Ok(false);
        }
        self.bytes = bytes;
        self.records.push(record);
        self.last = Some(key);
        Ok(true)
    }

    pub(super) fn finish(self, revision: StopWorkRevision) -> StopWorkPage {
        let next_cursor = if self.more {
            self.last.map(|after| StopWorkCursor {
                revision: revision.clone(),
                after,
            })
        } else {
            None
        };
        StopWorkPage {
            revision,
            records: self.records,
            bytes: self.bytes,
            next_cursor,
        }
    }
}
