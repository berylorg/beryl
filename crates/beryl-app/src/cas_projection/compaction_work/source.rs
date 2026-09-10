use std::{
    collections::BTreeMap,
    ops::Bound::{Excluded, Unbounded},
    sync::{Arc, Mutex, Weak},
};

use beryl_model::{SyndicThreadId, SyndicTurnId};

use super::*;

mod handles;
pub(in crate::cas_projection) use handles::{
    CompactionCommandObservation, CompactionLocalObservation, CompactionWorkHandle,
    ContinuationObservation,
};

pub(in crate::cas_projection) struct CompactionWorkSource {
    state: Mutex<SourceState>,
    capacity: usize,
    scheduler_signal: crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal,
}

struct SourceState {
    revision: Option<u64>,
    next_serial: Option<u64>,
    records: BTreeMap<u64, CompactionWorkRecord>,
}

pub(in crate::cas_projection) struct CompactionWorkSourcePage {
    records: Vec<CompactionWorkRecord>,
    bytes: usize,
    after: Option<u64>,
}

impl CompactionWorkSourcePage {
    pub(in crate::cas_projection) fn finish(
        self,
        revision: CompactionWorkRevision,
    ) -> CompactionWorkPage {
        let next_cursor = self.after.map(|after| CompactionWorkCursor {
            revision: revision.clone(),
            after,
        });
        CompactionWorkPage {
            revision,
            records: self.records,
            bytes: self.bytes,
            next_cursor,
        }
    }
}

impl SourceState {
    fn invalidate(&mut self) {
        self.revision = None;
        self.records.clear();
    }

    fn changed(&mut self) {
        self.revision = self.revision.and_then(|revision| revision.checked_add(1));
        if self.revision.is_none() {
            self.records.clear();
        }
    }
}

impl CompactionWorkSource {
    pub(in crate::cas_projection) fn new(
        capacity: usize,
        scheduler_signal: crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal,
    ) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(SourceState {
                revision: Some(0),
                next_serial: Some(1),
                records: BTreeMap::new(),
            }),
            capacity,
            scheduler_signal,
        })
    }

    pub(in crate::cas_projection) fn invalidate(&self) {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .invalidate();
    }

    pub(in crate::cas_projection) fn begin(
        self: &Arc<Self>,
        thread_id: SyndicThreadId,
        continuation: Option<SyndicTurnId>,
    ) -> CompactionWorkHandle {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poison) => {
                poison.into_inner().invalidate();
                return CompactionWorkHandle::unavailable();
            }
        };
        if state.revision.is_none() {
            return CompactionWorkHandle::unavailable();
        }
        let Some(serial) = state.next_serial else {
            state.invalidate();
            return CompactionWorkHandle::unavailable();
        };
        if state.records.len() >= self.capacity {
            state.invalidate();
            return CompactionWorkHandle::unavailable();
        }
        state.next_serial = serial.checked_add(1);
        let record = CompactionWorkRecord {
            serial,
            thread_id,
            continuation: continuation.map(|yielding_turn_id| ContinuationWorkFact {
                yielding_turn_id,
                pending: true,
                compaction_turn_id: None,
                stage: ContinuationWorkStage::Accepting,
            }),
            compaction: continuation
                .is_none()
                .then(|| CompactionWorkFact::preparing(None)),
        };
        state.records.insert(serial, record);
        state.changed();
        CompactionWorkHandle {
            source: Arc::downgrade(self),
            serial,
        }
    }

    fn update(&self, serial: u64, update: impl FnOnce(&mut CompactionWorkRecord)) {
        self.update_checked(serial, |record| {
            update(record);
            true
        });
    }

    fn replace_local(&self, serial: u64, replaced: Option<&CompactionWorkHandle>) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poison) => {
                poison.into_inner().invalidate();
                return;
            }
        };
        if state.revision.is_none() {
            return;
        }
        let valid_new = state
            .records
            .get(&serial)
            .and_then(|record| record.compaction.as_ref())
            .is_some_and(|fact| !fact.local_registered && fact.operation.is_some());
        let valid_old = replaced.is_none_or(|old| {
            old.source
                .upgrade()
                .is_some_and(|source| std::ptr::eq(source.as_ref(), self))
                && old.serial != serial
                && state
                    .records
                    .get(&old.serial)
                    .and_then(|record| record.compaction.as_ref())
                    .is_some_and(|fact| fact.local_registered)
        });
        if !valid_new || !valid_old {
            state.invalidate();
            return;
        }
        if let Some(old) = replaced {
            let record = state.records.get_mut(&old.serial).unwrap();
            record.compaction.as_mut().unwrap().local_registered = false;
            record.prune();
            if record.is_empty() {
                state.records.remove(&old.serial);
            }
        }
        state
            .records
            .get_mut(&serial)
            .unwrap()
            .compaction
            .as_mut()
            .unwrap()
            .local_registered = true;
        state.changed();
        drop(state);
        self.scheduler_signal.wake(
            crate::cas_projection::accepted_input_scheduler::AcceptedInputWakeReason::IdleRecheck,
        );
    }

    fn update_checked(&self, serial: u64, update: impl FnOnce(&mut CompactionWorkRecord) -> bool) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poison) => {
                poison.into_inner().invalidate();
                return;
            }
        };
        if state.revision.is_none() {
            return;
        }
        let Some(record) = state.records.get_mut(&serial) else {
            return;
        };
        let before = record.clone();
        if !update(record) {
            state.invalidate();
            return;
        }
        record.prune();
        let changed = *record != before;
        if record.is_empty() {
            state.records.remove(&serial);
        }
        if changed {
            state.changed();
        }
        drop(state);
        if changed {
            self.scheduler_signal.wake(crate::cas_projection::accepted_input_scheduler::AcceptedInputWakeReason::IdleRecheck);
        }
    }

    pub(in crate::cas_projection) fn revision(&self) -> Result<u64, CompactionWorkError> {
        self.state
            .lock()
            .map_err(|_| CompactionWorkError::Poisoned)?
            .revision
            .ok_or(CompactionWorkError::RevisionUnavailable)
    }

    pub(in crate::cas_projection) fn page(
        &self,
        revision: u64,
        after: Option<u64>,
        limits: CompactionWorkPageLimits,
    ) -> Result<CompactionWorkSourcePage, CompactionWorkError> {
        let state = self
            .state
            .lock()
            .map_err(|_| CompactionWorkError::Poisoned)?;
        if state
            .revision
            .ok_or(CompactionWorkError::RevisionUnavailable)?
            != revision
        {
            return Err(CompactionWorkError::StaleRevision);
        }
        let mut records = Vec::new();
        let mut bytes = 0usize;
        let mut next_cursor = None;
        for (serial, record) in state
            .records
            .range((after.map_or(Unbounded, Excluded), Unbounded))
        {
            let next_bytes = bytes
                .checked_add(record.bytes())
                .ok_or(CompactionWorkError::ByteLimit)?;
            if records.len() >= limits.max_records || next_bytes > limits.max_bytes {
                let Some(last) = records.last() else {
                    return Err(CompactionWorkError::ByteLimit);
                };
                let last: &CompactionWorkRecord = last;
                next_cursor = Some(last.serial);
                break;
            }
            debug_assert_eq!(*serial, record.serial);
            bytes = next_bytes;
            records.push(record.clone());
        }
        Ok(CompactionWorkSourcePage {
            records,
            bytes,
            after: next_cursor,
        })
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/compaction_work_source.rs"
    ));
}
