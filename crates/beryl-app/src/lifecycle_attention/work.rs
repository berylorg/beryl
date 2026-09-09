use super::*;
use thiserror::Error;

#[derive(Clone)]
pub struct LifecycleAttentionWorkRevision {
    owner: Arc<Owner>,
    stamp: u64,
}

impl std::fmt::Debug for LifecycleAttentionWorkRevision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LifecycleAttentionWorkRevision")
            .field("stamp", &self.stamp)
            .finish_non_exhaustive()
    }
}

impl PartialEq for LifecycleAttentionWorkRevision {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner) && self.stamp == other.stamp
    }
}
impl Eq for LifecycleAttentionWorkRevision {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleAttentionWorkSnapshot {
    revision: LifecycleAttentionWorkRevision,
    records: Vec<LifecycleAttentionRecord>,
}

impl LifecycleAttentionWorkSnapshot {
    pub const fn revision(&self) -> &LifecycleAttentionWorkRevision {
        &self.revision
    }
    pub fn records(&self) -> &[LifecycleAttentionRecord] {
        &self.records
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum LifecycleAttentionWorkError {
    #[error("the attention pool is closed")]
    Closed,
    #[error("the attention work revision belongs to another pool")]
    ForeignRevision,
    #[error("the attention work revision is stale")]
    StaleRevision,
    #[error("the attention work revision is unavailable")]
    RevisionUnavailable,
    #[error("the attention work source is poisoned")]
    Poisoned,
}

impl ProcessLifecycleAttentionPool {
    fn current_work_stamp(&self, state: &State) -> Result<u64, LifecycleAttentionWorkError> {
        if self.owner.closed.load(Ordering::Acquire) {
            return Err(LifecycleAttentionWorkError::Closed);
        }
        state
            .revision
            .ok_or(LifecycleAttentionWorkError::RevisionUnavailable)
    }

    pub fn work_revision(
        &self,
    ) -> Result<LifecycleAttentionWorkRevision, LifecycleAttentionWorkError> {
        let state = self
            .state
            .lock()
            .map_err(|_| LifecycleAttentionWorkError::Poisoned)?;
        Ok(LifecycleAttentionWorkRevision {
            owner: Arc::clone(&self.owner),
            stamp: self.current_work_stamp(&state)?,
        })
    }

    pub fn validate_work_revision(
        &self,
        revision: &LifecycleAttentionWorkRevision,
    ) -> Result<(), LifecycleAttentionWorkError> {
        if !Arc::ptr_eq(&self.owner, &revision.owner) {
            return Err(LifecycleAttentionWorkError::ForeignRevision);
        }
        let state = self
            .state
            .lock()
            .map_err(|_| LifecycleAttentionWorkError::Poisoned)?;
        if self.current_work_stamp(&state)? != revision.stamp {
            return Err(LifecycleAttentionWorkError::StaleRevision);
        }
        Ok(())
    }

    pub fn work_snapshot(
        &self,
        revision: &LifecycleAttentionWorkRevision,
    ) -> Result<LifecycleAttentionWorkSnapshot, LifecycleAttentionWorkError> {
        if !Arc::ptr_eq(&self.owner, &revision.owner) {
            return Err(LifecycleAttentionWorkError::ForeignRevision);
        }
        let state = self
            .state
            .lock()
            .map_err(|_| LifecycleAttentionWorkError::Poisoned)?;
        if self.current_work_stamp(&state)? != revision.stamp {
            return Err(LifecycleAttentionWorkError::StaleRevision);
        }
        Ok(LifecycleAttentionWorkSnapshot {
            revision: revision.clone(),
            records: state.records.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/lifecycle_attention_work.rs"
    ));
}
