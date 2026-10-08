use std::{
    collections::BTreeMap,
    sync::{Arc, Condvar, Mutex},
};

use fjall::Snapshot;

use super::{FrozenReadAccessError, ReadError};

const MAX_ACTIVE_READ_REQUESTS: usize = 512;

pub(crate) struct FrozenReadRegistry {
    state: Mutex<RegistryState>,
}

struct RegistryState {
    slots: BTreeMap<u64, Arc<ReadSlot>>,
    next_id: u64,
    maximum: usize,
    closed: bool,
}

struct ReadSlot {
    state: Mutex<SlotState>,
    drained: Condvar,
}

struct SlotState {
    snapshot: Option<Arc<Snapshot>>,
    active: usize,
    closed: bool,
}

pub(crate) struct FrozenReadReservation {
    registry: Arc<FrozenReadRegistry>,
    slot: Arc<ReadSlot>,
    id: u64,
    committed: bool,
}

pub(crate) struct FrozenReadRequest {
    slot: Arc<ReadSlot>,
    snapshot: Option<Arc<Snapshot>>,
}

impl FrozenReadRegistry {
    pub(crate) fn new(maximum: usize) -> Self {
        Self {
            state: Mutex::new(RegistryState {
                slots: BTreeMap::new(),
                next_id: 1,
                maximum,
                closed: false,
            }),
        }
    }

    pub(crate) fn reserve(self: &Arc<Self>) -> Result<FrozenReadReservation, ReadError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ReadError::GenerationPoisoned)?;
        if state.closed {
            return Err(FrozenReadAccessError::Released.into());
        }
        if state.slots.len() >= state.maximum {
            return Err(FrozenReadAccessError::Saturated {
                maximum: state.maximum,
            }
            .into());
        }
        let id = state.next_id;
        state.next_id = id
            .checked_add(1)
            .ok_or(FrozenReadAccessError::IdentityExhausted)?;
        let slot = Arc::new(ReadSlot {
            state: Mutex::new(SlotState {
                snapshot: None,
                active: 0,
                closed: false,
            }),
            drained: Condvar::new(),
        });
        state.slots.insert(id, Arc::clone(&slot));
        Ok(FrozenReadReservation {
            registry: Arc::clone(self),
            slot,
            id,
            committed: false,
        })
    }

    pub(crate) fn request(&self, id: u64) -> Result<FrozenReadRequest, ReadError> {
        let slot = {
            let state = self
                .state
                .lock()
                .map_err(|_| ReadError::GenerationPoisoned)?;
            if state.closed {
                return Err(FrozenReadAccessError::Released.into());
            }
            state
                .slots
                .get(&id)
                .cloned()
                .ok_or(FrozenReadAccessError::Released)?
        };
        let snapshot = {
            let mut state = slot
                .state
                .lock()
                .map_err(|_| ReadError::GenerationPoisoned)?;
            if state.closed {
                return Err(FrozenReadAccessError::Released.into());
            }
            if state.active >= MAX_ACTIVE_READ_REQUESTS {
                return Err(FrozenReadAccessError::RequestsSaturated.into());
            }
            let snapshot = state
                .snapshot
                .as_ref()
                .cloned()
                .ok_or(FrozenReadAccessError::Released)?;
            state.active += 1;
            snapshot
        };
        Ok(FrozenReadRequest {
            slot,
            snapshot: Some(snapshot),
        })
    }

    pub(crate) fn release(&self, id: u64) -> Result<(), ReadError> {
        let slot = self
            .state
            .lock()
            .map_err(|_| ReadError::GenerationPoisoned)?
            .slots
            .get(&id)
            .cloned()
            .ok_or(FrozenReadAccessError::Released)?;
        slot.close();
        self.state
            .lock()
            .map_err(|_| ReadError::GenerationPoisoned)?
            .slots
            .remove(&id)
            .ok_or(FrozenReadAccessError::Released)?;
        Ok(())
    }

    pub(crate) fn close(&self) {
        let slots = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.closed = true;
            std::mem::take(&mut state.slots)
        };
        for slot in slots.into_values() {
            slot.close();
        }
    }

    #[cfg(feature = "test-faults")]
    pub(crate) fn set_limits(&self, maximum: usize, next_id: u64) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.maximum = maximum;
        state.next_id = next_id;
    }

    pub(crate) fn count(&self) -> Result<usize, ReadError> {
        self.state
            .lock()
            .map(|state| state.slots.len())
            .map_err(|_| ReadError::GenerationPoisoned)
    }
}

impl ReadSlot {
    fn close(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.closed = true;
        while state.active != 0 {
            state = self
                .drained
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
        state.snapshot.take();
    }
}

impl FrozenReadReservation {
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn install(&self, snapshot: Arc<Snapshot>) -> Result<(), ReadError> {
        let mut state = self
            .slot
            .state
            .lock()
            .map_err(|_| ReadError::GenerationPoisoned)?;
        if state.closed {
            return Err(FrozenReadAccessError::Released.into());
        }
        state.snapshot = Some(snapshot);
        Ok(())
    }

    pub(crate) fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for FrozenReadReservation {
    fn drop(&mut self) {
        if !self.committed {
            let slot = self
                .registry
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .slots
                .get(&self.id)
                .cloned();
            if let Some(slot) = slot {
                slot.close();
                self.registry
                    .state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .slots
                    .remove(&self.id);
            }
        }
    }
}

impl FrozenReadRequest {
    pub(crate) fn snapshot(&self) -> &Snapshot {
        self.snapshot.as_deref().expect("admitted frozen snapshot")
    }

    pub(crate) fn shared_snapshot(&self) -> Arc<Snapshot> {
        Arc::clone(self.snapshot.as_ref().expect("admitted frozen snapshot"))
    }
}

impl Drop for FrozenReadRequest {
    fn drop(&mut self) {
        self.snapshot.take();
        let mut state = self
            .slot
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.active -= 1;
        if state.active == 0 {
            self.slot.drained.notify_all();
        }
    }
}
