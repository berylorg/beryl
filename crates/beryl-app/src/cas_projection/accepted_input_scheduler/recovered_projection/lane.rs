use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use beryl_model::SyndicThreadId;

use super::super::{AcceptedInputSchedulerSignal, SchedulerFailure};
use crate::cas_projection::{
    LoadedCasProjection,
    connection::{DormantRecoveredProjectionLeaseOwner, ProjectionConnection},
    persistent_failure::PendingProjectionWitness,
    service_startup::ServiceStartupGate,
};

pub(in crate::cas_projection) type RecoveredProjectionLaneParts = (
    PendingProjectionWitness,
    DormantRecoveredProjectionLeaseOwner,
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum RecoveredProjectionLaneStageReason {
    StartupGateNotClosed,
    AlreadyStaged,
    CapacityExceeded,
    QueueUnavailable,
}

#[must_use = "a failed recovered-projection stage still owns every dormant candidate"]
pub(in crate::cas_projection) struct RecoveredProjectionLaneStageError {
    reason: RecoveredProjectionLaneStageReason,
    entries: Vec<RecoveredProjectionLaneParts>,
}

#[derive(Clone)]
pub(in crate::cas_projection) struct RecoveredProjectionLane {
    inner: Arc<Mutex<RecoveredProjectionLaneState>>,
    startup: Arc<ServiceStartupGate>,
    signal: AcceptedInputSchedulerSignal,
}

struct RecoveredProjectionLaneState {
    capacity: usize,
    staged: bool,
    entries: VecDeque<RecoveredProjectionLaneEntry>,
}

pub(super) struct RecoveredProjectionLaneEntry {
    witness: PendingProjectionWitness,
    pub(super) owner: DormantRecoveredProjectionLeaseOwner,
    last_attempt: u64,
    last_scan: u64,
}

#[derive(Clone, Copy)]
pub(super) struct RecoveredProjectionLaneAttempt {
    pub(super) last_attempt: u64,
    pub(super) last_scan: u64,
}

impl RecoveredProjectionLane {
    pub(in crate::cas_projection) fn new(
        capacity: usize,
        startup: Arc<ServiceStartupGate>,
        signal: AcceptedInputSchedulerSignal,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(RecoveredProjectionLaneState {
                capacity,
                staged: false,
                entries: VecDeque::with_capacity(capacity),
            })),
            startup,
            signal,
        }
    }

    pub(in crate::cas_projection) fn stage(
        &self,
        entries: Vec<RecoveredProjectionLaneParts>,
    ) -> Result<usize, RecoveredProjectionLaneStageError> {
        let startup = match self.startup.lock_for_publication() {
            Ok(startup) => startup,
            Err(()) => {
                return Err(RecoveredProjectionLaneStageError::new(
                    RecoveredProjectionLaneStageReason::StartupGateNotClosed,
                    entries,
                ));
            }
        };
        let mut state = match self.inner.lock() {
            Ok(state) => state,
            Err(_) => {
                drop(startup);
                return Err(RecoveredProjectionLaneStageError::new(
                    RecoveredProjectionLaneStageReason::QueueUnavailable,
                    entries,
                ));
            }
        };
        if state.staged || !state.entries.is_empty() {
            return Err(RecoveredProjectionLaneStageError::new(
                RecoveredProjectionLaneStageReason::AlreadyStaged,
                entries,
            ));
        }
        if entries.len() > state.capacity {
            return Err(RecoveredProjectionLaneStageError::new(
                RecoveredProjectionLaneStageReason::CapacityExceeded,
                entries,
            ));
        }
        let staged = entries.len();
        state
            .entries
            .extend(
                entries
                    .into_iter()
                    .map(|(witness, owner)| RecoveredProjectionLaneEntry {
                        witness,
                        owner,
                        last_attempt: 0,
                        last_scan: 0,
                    }),
            );
        state.staged = true;
        drop(state);
        drop(startup);
        self.signal.record_recovered_projection_stage(staged);
        Ok(staged)
    }
}

impl RecoveredProjectionLane {
    pub(super) fn pop_eligible(
        &self,
        pass: u64,
        scan: u64,
        mut is_launchable: impl FnMut(SyndicThreadId) -> bool,
    ) -> Result<(Option<RecoveredProjectionLaneEntry>, bool), SchedulerFailure> {
        let mut state = self.inner.lock().map_err(|_| SchedulerFailure::Fatal)?;
        let count = state.entries.len();
        let mut eligible_waiting = false;
        for _ in 0..count {
            let mut entry = state
                .entries
                .pop_front()
                .expect("the bounded recovered queue count remains exact");
            let eligible = entry.last_attempt < pass && entry.last_scan < scan;
            if eligible && is_launchable(*entry.witness.syndic_thread_id()) {
                entry.last_scan = scan;
                drop(state);
                self.signal.record_recovered_projection_dequeued(1);
                return Ok((Some(entry), eligible_waiting));
            }
            eligible_waiting |= eligible;
            state.entries.push_back(entry);
        }
        Ok((None, eligible_waiting))
    }

    pub(super) fn queued_len(&self) -> Result<usize, SchedulerFailure> {
        self.inner
            .lock()
            .map(|state| state.entries.len())
            .map_err(|_| SchedulerFailure::Fatal)
    }

    pub(super) fn requeue(
        &self,
        entry: RecoveredProjectionLaneEntry,
    ) -> Result<(), SchedulerFailure> {
        let result = match self.inner.lock() {
            Ok(mut state) => {
                debug_assert!(state.entries.len() < state.capacity);
                state.entries.push_back(entry);
                Ok(())
            }
            Err(poison) => {
                let mut state = poison.into_inner();
                debug_assert!(state.entries.len() < state.capacity);
                state.entries.push_back(entry);
                Err(SchedulerFailure::Fatal)
            }
        };
        self.signal.record_recovered_projection_requeued();
        result
    }

    pub(super) fn take_all(&self) -> (VecDeque<RecoveredProjectionLaneEntry>, bool) {
        let (mut state, poisoned) = match self.inner.lock() {
            Ok(state) => (state, false),
            Err(poison) => (poison.into_inner(), true),
        };
        let entries = std::mem::take(&mut state.entries);
        let count = entries.len();
        drop(state);
        self.signal.record_recovered_projection_dequeued(count);
        (entries, poisoned)
    }

    #[cfg(test)]
    pub(super) fn poison_for_test(&self) {
        let inner = Arc::clone(&self.inner);
        let result = std::thread::spawn(move || {
            let _state = inner.lock().expect("the lane starts usable");
            panic!("poison recovered-projection lane for test");
        })
        .join();
        assert!(result.is_err());
    }
}

impl RecoveredProjectionLaneStageError {
    fn new(
        reason: RecoveredProjectionLaneStageReason,
        entries: Vec<RecoveredProjectionLaneParts>,
    ) -> Self {
        Self { reason, entries }
    }

    pub(in crate::cas_projection) fn into_parts(
        self,
    ) -> (
        RecoveredProjectionLaneStageReason,
        Vec<RecoveredProjectionLaneParts>,
    ) {
        (self.reason, self.entries)
    }
}

impl RecoveredProjectionLaneEntry {
    pub(super) fn thread_id(&self) -> SyndicThreadId {
        *self.witness.syndic_thread_id()
    }

    pub(super) fn expected_connection(&self) -> Arc<ProjectionConnection> {
        Arc::clone(self.owner.stable_connection_observation().connection())
    }

    pub(super) fn mark_attempted(&mut self, pass: u64) {
        self.last_attempt = pass;
    }

    pub(super) fn materialize(
        self,
        retainer: crate::cas_projection::persistent_failure::PersistentFailureProjectionRetainer,
    ) -> (
        LoadedCasProjection,
        crate::cas_projection::service_config::ProjectionWorkerPermit,
        RecoveredProjectionLaneAttempt,
    ) {
        let attempt = RecoveredProjectionLaneAttempt {
            last_attempt: self.last_attempt,
            last_scan: self.last_scan,
        };
        let (projection, worker) =
            LoadedCasProjection::from_dormant_recovered(self.witness, self.owner, retainer);
        (projection, worker, attempt)
    }

    pub(super) fn from_materialized(
        projection: LoadedCasProjection,
        attempt: RecoveredProjectionLaneAttempt,
    ) -> Self {
        let (witness, owner) = projection.into_dormant_recovered();
        Self {
            witness,
            owner,
            last_attempt: attempt.last_attempt,
            last_scan: attempt.last_scan,
        }
    }

    pub(super) fn from_materialized_with_worker(
        projection: LoadedCasProjection,
        worker: crate::cas_projection::service_config::ProjectionWorkerPermit,
        attempt: RecoveredProjectionLaneAttempt,
    ) -> Self {
        let (witness, owner) = projection.into_dormant_recovered_with_worker(worker);
        Self {
            witness,
            owner,
            last_attempt: attempt.last_attempt,
            last_scan: attempt.last_scan,
        }
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn retain_for_persistent_failure(
    context: &super::super::AcceptedInputSchedulerContext,
) -> Result<(), SchedulerFailure> {
    let (mut entries, poisoned) = context.recovered_projection_lane.take_all();
    while let Some(entry) = entries.pop_front() {
        let (projection, worker, _) = entry.materialize(context.projection_retainer.clone());
        drop(worker);
        context.projection_retainer.retain(projection);
    }
    if poisoned {
        Err(SchedulerFailure::Fatal)
    } else {
        Ok(())
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn dispose_retained(
    context: &super::super::AcceptedInputSchedulerContext,
) -> Result<(), SchedulerFailure> {
    let (mut entries, poisoned) = context.recovered_projection_lane.take_all();
    while let Some(entry) = entries.pop_front() {
        drop(entry);
    }
    if poisoned {
        Err(SchedulerFailure::Fatal)
    } else {
        Ok(())
    }
}
