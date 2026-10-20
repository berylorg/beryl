use std::{
    collections::HashMap,
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
    pub(super) flights: Arc<Flights>,
    pub(super) retained: Arc<RetainedAudits>,
}

pub(super) struct RetainedAudits {
    pub(super) audits: Mutex<HashMap<JobId, DiscussionSettlementAudit>>,
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

pub(super) struct Flights {
    maximum: NonZeroUsize,
    pub(super) active: Mutex<ActiveFlights>,
    pub(super) presentation: Arc<super::presentation::LocalBoundary>,
}
pub(super) struct ActiveFlights {
    pub(super) jobs: HashMap<JobId, ActiveAttempt>,
    dispatch_wake: Option<Waker>,
    coordinator_wake: Option<Waker>,
    dispatch_waiting: bool,
}

pub(super) struct ActiveAttempt {
    pub(super) child: Option<beryl_model::SyndicThreadId>,
    pub(super) audit: Option<Weak<super::Attempt>>,
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
                presentation: Arc::new(super::presentation::LocalBoundary::new()),
                active: Mutex::new(ActiveFlights {
                    jobs: HashMap::new(),
                    dispatch_wake: None,
                    coordinator_wake: None,
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
        self.acquire_selected(job_id, None)
    }

    pub(super) fn acquire_selected(
        &self,
        job_id: JobId,
        child: Option<beryl_model::SyndicThreadId>,
    ) -> Result<Flight, DiscussionSettlementError> {
        let revision = if let Some(child) = child {
            let revision = self.flights.presentation.revision()?;
            if self.presentation_fact(child)?.is_some_and(|fact| {
                matches!(
                    fact.disposition,
                    super::presentation::LocalDisposition::Collision
                        | super::presentation::LocalDisposition::Pending
                )
            }) {
                return Err(DiscussionSettlementError::OriginalAttemptUnavailable);
            }
            Some(revision)
        } else {
            None
        };
        let mut active = self
            .flights
            .active
            .lock()
            .map_err(|_| DiscussionSettlementError::CustodyUnavailable)?;
        if active.jobs.contains_key(&job_id) {
            active.dispatch_waiting = true;
            return Err(DiscussionSettlementError::DuplicateIdentity);
        }
        if active.jobs.len() >= self.flights.maximum.get() {
            active.dispatch_waiting = true;
            return Err(DiscussionSettlementError::Capacity);
        }
        let insert = || {
            active
                .jobs
                .insert(job_id, ActiveAttempt { child, audit: None })
        };
        if let Some(revision) = revision {
            self.flights.presentation.mutate_at(revision, insert)?;
        } else {
            self.flights.presentation.mutate(insert);
        }
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

    pub(super) fn set_coordinator_waker(&self, wake: Option<Waker>) -> bool {
        let mut active = self
            .flights
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if wake.is_some() && active.coordinator_wake.is_some() {
            return false;
        }
        active.coordinator_wake = wake;
        true
    }

    pub(super) fn capacity_ready(&self) -> bool {
        let mut active = self
            .flights
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if active.jobs.len() < self.flights.maximum.get() {
            true
        } else {
            active.dispatch_waiting = true;
            false
        }
    }
}

pub(super) struct Flight {
    pub(super) flights: Arc<Flights>,
    retained: Weak<RetainedAudits>,
    job_id: JobId,
}

impl Flight {
    pub(super) fn wake_dispatch(&self) {
        let wake = self
            .flights
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .dispatch_wake
            .clone();
        if let Some(wake) = wake {
            wake.wake();
        }
    }
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
        if active.jobs.contains_key(&job_id) {
            return Err(DiscussionSettlementError::DuplicateIdentity);
        }
        self.flights.presentation.mutate(|| {
            let slot = active
                .jobs
                .remove(&self.job_id)
                .expect("owned active flight");
            active.jobs.insert(job_id, slot);
        });
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
            self.flights
                .presentation
                .mutate(|| active.jobs.remove(&self.job_id));
            if active.dispatch_waiting
                && (active.dispatch_wake.is_some() || active.coordinator_wake.is_some())
            {
                active.dispatch_waiting = false;
                (
                    active.dispatch_wake.clone(),
                    active.coordinator_wake.clone(),
                )
            } else {
                (None, None)
            }
        };
        for wake in [wake.0, wake.1].into_iter().flatten() {
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
