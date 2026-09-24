use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
    sync::{Arc, Mutex, Weak},
};

use super::{DiscussionSettlementAudit, DiscussionSettlementError};
use crate::process_admission::{ProcessAdmissionGate, ProcessExecutionPermit};
use beryl_model::JobId;

#[derive(Clone)]
pub struct DiscussionSettlementOperations {
    process: ProcessAdmissionGate,
    flights: Arc<Flights>,
    retained: Arc<RetainedAudits>,
}

pub(super) struct RetainedAudits(Mutex<HashMap<JobId, DiscussionSettlementAudit>>);
impl RetainedAudits {
    pub(super) fn retain(&self, job: JobId, audit: DiscussionSettlementAudit) {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(job, audit);
    }
    fn release(&self, job: JobId) {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&job);
    }
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
            retained: Arc::new(RetainedAudits(Mutex::new(HashMap::new()))),
            flights: Arc::new(Flights {
                maximum,
                active: Mutex::new(HashSet::new()),
            }),
        }
    }

    pub fn retained_audit(&self, job: JobId) -> Option<DiscussionSettlementAudit> {
        self.retained
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(&job)
            .cloned()
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
            retained: Arc::downgrade(&self.retained),
            job_id,
        })
    }
}

pub(super) struct Flight {
    flights: Arc<Flights>,
    retained: Weak<RetainedAudits>,
    job_id: JobId,
}

impl Flight {
    pub(super) fn custody(&self) -> Result<Arc<RetainedAudits>, DiscussionSettlementError> {
        self.retained
            .upgrade()
            .ok_or(DiscussionSettlementError::CustodyUnavailable)
    }
    pub(super) fn retain(&self, custody: &RetainedAudits, audit: DiscussionSettlementAudit) {
        custody.retain(self.job_id, audit);
    }
    pub(super) fn release_retained(&self) {
        if let Some(custody) = self.retained.upgrade() {
            custody.release(self.job_id);
        }
    }
    pub(super) fn retarget(&mut self, job_id: JobId) -> Result<(), DiscussionSettlementError> {
        if self.job_id == job_id {
            return Ok(());
        }
        let mut active = self
            .flights
            .active
            .lock()
            .map_err(|_| DiscussionSettlementError::CustodyUnavailable)?;
        if active.contains(&job_id) {
            return Err(DiscussionSettlementError::DuplicateIdentity);
        }
        active.remove(&self.job_id);
        active.insert(job_id);
        self.job_id = job_id;
        Ok(())
    }
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
