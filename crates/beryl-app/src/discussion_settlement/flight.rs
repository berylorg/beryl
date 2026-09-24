use std::{
    collections::HashSet,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use super::DiscussionSettlementError;
use crate::process_admission::{ProcessAdmissionGate, ProcessExecutionPermit};
use beryl_model::JobId;

#[derive(Clone)]
pub struct DiscussionSettlementOperations {
    process: ProcessAdmissionGate,
    flights: Arc<Flights>,
}

struct Flights {
    maximum: NonZeroUsize,
    active: Mutex<HashSet<JobId>>,
}

impl DiscussionSettlementOperations {
    pub(super) fn configured_slots(&self) -> NonZeroUsize {
        self.flights.maximum
    }
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

    pub(super) fn acquire(&self, job_id: JobId) -> Result<Flight, DiscussionSettlementError> {
        let mut active = self
            .flights
            .active
            .lock()
            .map_err(|_| DiscussionSettlementError::CustodyUnavailable)?;
        if active.contains(&job_id) {
            return Err(DiscussionSettlementError::DuplicateIdentity);
        }
        if active.len() >= self.flights.maximum.get() {
            return Err(DiscussionSettlementError::Capacity);
        }
        active.insert(job_id);
        Ok(Flight {
            flights: Arc::clone(&self.flights),
            job_id,
        })
    }
}

pub(super) struct Flight {
    flights: Arc<Flights>,
    job_id: JobId,
}

impl Drop for Flight {
    fn drop(&mut self) {
        self.flights
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.job_id);
    }
}
