use std::{
    collections::HashSet,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use super::DiscussionCreationError;
use crate::process_admission::{ProcessAdmissionGate, ProcessExecutionPermit};
use beryl_model::SyndicThreadId;

#[derive(Clone)]
pub struct DiscussionCreationOperations {
    process: ProcessAdmissionGate,
    flights: Arc<Flights>,
}

struct Flights {
    maximum: NonZeroUsize,
    active: Mutex<HashSet<SyndicThreadId>>,
}

impl DiscussionCreationOperations {
    pub fn new(process: ProcessAdmissionGate, maximum: NonZeroUsize) -> Self {
        Self {
            process,
            flights: Arc::new(Flights {
                maximum,
                active: Mutex::new(HashSet::new()),
            }),
        }
    }

    pub(super) fn permit(&self) -> ProcessExecutionPermit {
        self.process.execution_permit()
    }

    pub(super) fn acquire(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<Flight, DiscussionCreationError> {
        let mut active = self
            .flights
            .active
            .lock()
            .map_err(|_| DiscussionCreationError::CustodyUnavailable)?;
        if active.contains(&thread_id) {
            return Err(DiscussionCreationError::DuplicateIdentity);
        }
        if active.len() >= self.flights.maximum.get() {
            return Err(DiscussionCreationError::Capacity);
        }
        active.insert(thread_id);
        Ok(Flight {
            flights: Arc::clone(&self.flights),
            thread_id,
        })
    }
}

pub(super) struct Flight {
    flights: Arc<Flights>,
    thread_id: SyndicThreadId,
}

impl Drop for Flight {
    fn drop(&mut self) {
        self.flights
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.thread_id);
    }
}
