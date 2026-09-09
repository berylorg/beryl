use std::sync::Arc;

use beryl_home_store::HomeGeneration;
use beryl_model::{BerylHomeId, CasThreadId};
use thiserror::Error;

use super::ProjectionServiceGeneration;

mod records;

pub use records::{
    ConnectionRequestWorkFact, ConnectionRequestWorkKind, ConnectionRequestWorkStage,
    ConnectionTargetWorkFact, ConnectionTargetWorkState, ConnectionWorkRecord,
    ConnectionWorkTargetIdentity,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ConnectionWorkStamp {
    pub(super) membership: u64,
    pub(super) retired: u64,
    pub(super) detached: u64,
    pub(super) routers: u64,
    pub(super) responses: u64,
}

impl ConnectionWorkStamp {
    pub(super) fn add(&mut self, other: Self) -> Result<(), ConnectionWorkError> {
        self.retired = self
            .retired
            .checked_add(other.retired)
            .ok_or(ConnectionWorkError::RevisionUnavailable)?;
        self.detached = self
            .detached
            .checked_add(other.detached)
            .ok_or(ConnectionWorkError::RevisionUnavailable)?;
        self.routers = self
            .routers
            .checked_add(other.routers)
            .ok_or(ConnectionWorkError::RevisionUnavailable)?;
        self.responses = self
            .responses
            .checked_add(other.responses)
            .ok_or(ConnectionWorkError::RevisionUnavailable)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ConnectionWorkRevision {
    pub(super) owner: Arc<()>,
    pub(super) home_id: BerylHomeId,
    pub(super) home_generation: HomeGeneration,
    pub(super) service_generation: ProjectionServiceGeneration,
    pub(super) stamp: ConnectionWorkStamp,
}

impl PartialEq for ConnectionWorkRevision {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner)
            && self.home_id == other.home_id
            && self.home_generation == other.home_generation
            && self.service_generation == other.service_generation
            && self.stamp == other.stamp
    }
}
impl Eq for ConnectionWorkRevision {}

impl ConnectionWorkRevision {
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

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum ConnectionWorkItemKey {
    Target(CasThreadId),
    Request(u64),
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct ConnectionWorkKey {
    pub(super) connection: u64,
    pub(super) item: ConnectionWorkItemKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionWorkCursor {
    pub(super) revision: ConnectionWorkRevision,
    pub(super) after: ConnectionWorkKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConnectionWorkPageLimits {
    pub(super) max_records: usize,
    pub(super) max_bytes: usize,
}

impl ConnectionWorkPageLimits {
    pub fn new(max_records: usize, max_bytes: usize) -> Result<Self, ConnectionWorkError> {
        if max_records == 0 || max_bytes == 0 {
            return Err(ConnectionWorkError::InvalidLimits);
        }
        Ok(Self {
            max_records: max_records.min(256),
            max_bytes: max_bytes.min(65_536),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionWorkPage {
    pub(super) revision: ConnectionWorkRevision,
    pub(super) records: Vec<ConnectionWorkRecord>,
    pub(super) bytes: usize,
    pub(super) next_cursor: Option<ConnectionWorkCursor>,
}

impl ConnectionWorkPage {
    pub const fn revision(&self) -> &ConnectionWorkRevision {
        &self.revision
    }
    pub fn records(&self) -> &[ConnectionWorkRecord] {
        &self.records
    }
    pub const fn bytes(&self) -> usize {
        self.bytes
    }
    pub const fn next_cursor(&self) -> Option<&ConnectionWorkCursor> {
        self.next_cursor.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ConnectionWorkError {
    #[error("the connection work owner is closed")]
    Closed,
    #[error("the connection work revision is stale")]
    StaleRevision,
    #[error("the connection work revision or cursor belongs to another owner")]
    ForeignRevision,
    #[error("connection work page limits must be nonzero")]
    InvalidLimits,
    #[error("one connection work record exceeds the page byte limit")]
    ByteLimit,
    #[error("the connection work revision is unavailable")]
    RevisionUnavailable,
    #[error("a connection work source is unavailable")]
    SourceUnavailable,
    #[error("the connection work owner lock is poisoned")]
    Poisoned,
}

pub(super) struct ConnectionWorkPageBuilder<'a> {
    pub(super) after: Option<&'a ConnectionWorkKey>,
    limits: ConnectionWorkPageLimits,
    records: Vec<ConnectionWorkRecord>,
    bytes: usize,
    last: Option<ConnectionWorkKey>,
    more: bool,
}

impl<'a> ConnectionWorkPageBuilder<'a> {
    pub(super) fn new(
        after: Option<&'a ConnectionWorkKey>,
        limits: ConnectionWorkPageLimits,
    ) -> Self {
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
        key: ConnectionWorkKey,
        record: ConnectionWorkRecord,
    ) -> Result<bool, ConnectionWorkError> {
        if self.after.is_some_and(|after| &key <= after) {
            return Ok(true);
        }
        let bytes = self
            .bytes
            .checked_add(record.bytes())
            .ok_or(ConnectionWorkError::ByteLimit)?;
        if self.records.len() >= self.limits.max_records || bytes > self.limits.max_bytes {
            if self.records.is_empty() {
                return Err(ConnectionWorkError::ByteLimit);
            }
            self.more = true;
            return Ok(false);
        }
        self.bytes = bytes;
        self.records.push(record);
        self.last = Some(key);
        Ok(true)
    }

    pub(super) fn finish(self, revision: ConnectionWorkRevision) -> ConnectionWorkPage {
        let next_cursor = if self.more {
            self.last.map(|after| ConnectionWorkCursor {
                revision: revision.clone(),
                after,
            })
        } else {
            None
        };
        ConnectionWorkPage {
            revision,
            records: self.records,
            bytes: self.bytes,
            next_cursor,
        }
    }
}
