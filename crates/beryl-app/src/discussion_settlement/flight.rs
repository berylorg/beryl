use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
    sync::{Arc, Mutex, Weak},
    task::Waker,
};

use super::{
    DiscussionSettlementAudit, DiscussionSettlementError, reservation::RetainedNondispatch,
};
use crate::process_admission::{ProcessAdmissionGate, ProcessExecutionPermit};
use beryl_model::JobId;

#[derive(Clone)]
pub struct DiscussionSettlementOperations {
    process: ProcessAdmissionGate,
    flights: Arc<Flights>,
    retained: Arc<RetainedAudits>,
}

pub(super) struct RetainedAudits {
    audits: Mutex<HashMap<JobId, DiscussionSettlementAudit>>,
    nondispatch: Mutex<HashMap<JobId, Arc<RetainedNondispatch>>>,
}
impl RetainedAudits {
    pub(super) fn retain(&self, job: JobId, audit: DiscussionSettlementAudit) {
        self.audits
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(job, audit);
    }
    fn release(&self, job: JobId) {
        self.audits
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&job);
    }
    pub(super) fn retain_nondispatch(&self, job: JobId, proof: Arc<RetainedNondispatch>) {
        self.nondispatch
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(job, proof);
    }
    pub(super) fn release_nondispatch(&self, job: JobId) {
        let removed = self
            .nondispatch
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&job);
        drop(removed);
    }
}

struct Flights {
    maximum: NonZeroUsize,
    active: Mutex<ActiveFlights>,
}
struct ActiveFlights {
    jobs: HashSet<JobId>,
    dispatch_wake: Option<Waker>,
    dispatch_waiting: bool,
}

impl DiscussionSettlementOperations {
    pub(super) fn configured_slots(&self) -> NonZeroUsize {
        self.flights.maximum
    }
    pub fn new(process: ProcessAdmissionGate, maximum: NonZeroUsize) -> Self {
        Self {
            process,
            retained: Arc::new(RetainedAudits {
                audits: Mutex::new(HashMap::new()),
                nondispatch: Mutex::new(HashMap::new()),
            }),
            flights: Arc::new(Flights {
                maximum,
                active: Mutex::new(ActiveFlights {
                    jobs: HashSet::new(),
                    dispatch_wake: None,
                    dispatch_waiting: false,
                }),
            }),
        }
    }

    pub fn retained_audit(&self, job: JobId) -> Option<DiscussionSettlementAudit> {
        self.retained
            .audits
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(&job)
            .cloned()
    }

    pub fn pending_nondispatch_count(&self) -> usize {
        self.retained
            .nondispatch
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    pub(super) fn next_nondispatch(&self) -> Option<Arc<RetainedNondispatch>> {
        self.retained
            .nondispatch
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .next()
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
        if active.jobs.contains(&job_id) {
            active.dispatch_waiting = true;
            return Err(DiscussionSettlementError::DuplicateIdentity);
        }
        if active.jobs.len() >= self.flights.maximum.get() {
            active.dispatch_waiting = true;
            return Err(DiscussionSettlementError::Capacity);
        }
        active.jobs.insert(job_id);
        Ok(Flight {
            flights: Arc::clone(&self.flights),
            retained: Arc::downgrade(&self.retained),
            job_id,
        })
    }

    pub(crate) fn set_dispatch_capacity_waker(&self, wake: Waker) {
        let notify = {
            let mut active = self
                .flights
                .active
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            active.dispatch_wake = Some(wake.clone());
            if active.dispatch_waiting && active.jobs.len() < self.flights.maximum.get() {
                active.dispatch_waiting = false;
                true
            } else {
                false
            }
        };
        if notify {
            wake.wake();
        }
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
    pub(super) fn release_nondispatch(&self) {
        if let Some(custody) = self.retained.upgrade() {
            custody.release_nondispatch(self.job_id);
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
        if active.jobs.contains(&job_id) {
            return Err(DiscussionSettlementError::DuplicateIdentity);
        }
        active.jobs.remove(&self.job_id);
        active.jobs.insert(job_id);
        self.job_id = job_id;
        Ok(())
    }
}

impl Drop for Flight {
    fn drop(&mut self) {
        let wake = {
            let mut active = self
                .flights
                .active
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            active.jobs.remove(&self.job_id);
            if active.dispatch_waiting && active.dispatch_wake.is_some() {
                active.dispatch_waiting = false;
                active.dispatch_wake.clone()
            } else {
                None
            }
        };
        if let Some(wake) = wake {
            wake.wake();
        }
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/discussion_settlement_capacity.rs"
    ));
}
