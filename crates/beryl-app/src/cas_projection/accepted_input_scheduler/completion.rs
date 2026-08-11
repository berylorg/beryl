use std::sync::{Arc, Mutex};

use super::WorkerDisposition;

pub(super) struct WorkerCompletion {
    pub(super) thread_id: std::thread::ThreadId,
}

#[derive(Clone)]
pub(super) struct WorkerCompletions {
    inner: Arc<Mutex<WorkerCompletionState>>,
}

struct WorkerCompletionState {
    capacity: usize,
    entries: Vec<WorkerCompletion>,
}

impl WorkerCompletions {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(WorkerCompletionState {
                capacity,
                entries: Vec::with_capacity(capacity),
            })),
        }
    }

    pub(super) fn publish(&self, completion: WorkerCompletion) {
        let mut state = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.entries.len() < state.capacity {
            state.entries.push(completion);
        } else {
            debug_assert!(
                false,
                "worker completion capacity is fixed by worker permits"
            );
        }
    }

    pub(super) fn drain(&self) -> Vec<WorkerCompletion> {
        let mut state = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let capacity = state.capacity;
        std::mem::replace(&mut state.entries, Vec::with_capacity(capacity))
    }
}
