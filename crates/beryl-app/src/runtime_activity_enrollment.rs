use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use beryl_home_store::{
    CommandCancellation, CommandError, CommandOutcome, HomeCandidateRecoveryAccess, HomeCommand,
    HomeGeneration, HomeStore, ReconciliationFailure, ReconciliationHandle,
    ReconciliationResolution,
};
use beryl_model::{BerylHomeId, RuntimeId};
use syndic_storage::{
    ActivityEnrollmentStatus, ActivityEnrollmentWitness, PreparedActivityEnrollment,
    SyndicReadError, SyndicStorage,
};
use thiserror::Error;

#[derive(Clone)]
pub struct RuntimeActivityEnrollmentOperations(Arc<Slots>);

struct Slots {
    home: BerylHomeId,
    capacity: NonZeroUsize,
    entries: Mutex<HashMap<RuntimeId, Arc<Entry>>>,
}

struct Entry {
    generation: HomeGeneration,
    state: Mutex<Attempt>,
}

struct Attempt {
    witness: Option<ActivityEnrollmentWitness>,
    reconciliation: Option<ReconciliationHandle>,
}

pub(crate) struct ActivityEnrollmentAttempt {
    runtime: RuntimeId,
    entry: Arc<Entry>,
}

pub struct ReservedActivityEnrollment<'a> {
    owner: RuntimeActivityEnrollmentOperations,
    runtime: RuntimeId,
    entry: Arc<Entry>,
    home: &'a HomeStore,
    command: Option<HomeCommand>,
}

#[must_use]
pub enum ActivityEnrollmentCommandOutcome {
    Definitive {
        outcome: CommandOutcome,
        witness: ActivityEnrollmentWitness,
    },
    Pending {
        failure: CommandError,
    },
}

#[derive(Debug, Error)]
pub enum ActivityEnrollmentCustodyError {
    #[error("Activity enrollment belongs to another home or generation")]
    Identity,
    #[error("this runtime already owns an enrollment reservation")]
    Occupied,
    #[error("Activity enrollment custody capacity is full")]
    Capacity,
    #[error("Activity enrollment submission has not reached an outcome")]
    SubmissionPending,
    #[error("Activity enrollment recovery was cancelled")]
    Cancelled,
    #[error("Activity enrollment witness and reconciliation do not agree")]
    Conflict,
    #[error("Activity enrollment witness could not be read: {0}")]
    Read(#[from] SyndicReadError),
    #[error("Activity enrollment reconciliation failed: {0}")]
    Reconciliation(#[from] ReconciliationFailure),
}

impl RuntimeActivityEnrollmentOperations {
    pub(crate) fn matches(&self, home: BerylHomeId, capacity: usize) -> bool {
        self.0.home == home && self.0.capacity.get() == capacity
    }

    pub(crate) fn settle_attempt(
        &self,
        home: &HomeStore,
        syndic: &SyndicStorage,
        original: &ActivityEnrollmentAttempt,
    ) -> Result<ActivityEnrollmentStatus, ActivityEnrollmentCustodyError> {
        if home.home_id() != self.0.home
            || home.health().generation() != Some(original.entry.generation)
            || !self
                .0
                .entries
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&original.runtime)
                .is_some_and(|entry| Arc::ptr_eq(entry, &original.entry))
        {
            return Err(ActivityEnrollmentCustodyError::Identity);
        }
        let attempt = original
            .entry
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let handle = attempt
            .reconciliation
            .as_ref()
            .ok_or(ActivityEnrollmentCustodyError::SubmissionPending)?;
        let witness = attempt
            .witness
            .as_ref()
            .ok_or(ActivityEnrollmentCustodyError::SubmissionPending)?;
        let result = home.retry_reconciliation(handle)?;
        let natural = syndic.activity_enrollment_status(home, witness)?;
        if !matches!(
            (&result, &natural),
            (
                ReconciliationResolution::ExactOld,
                ActivityEnrollmentStatus::NotCommitted
            ) | (
                ReconciliationResolution::ExactNew { .. },
                ActivityEnrollmentStatus::Committed { .. }
            )
        ) {
            return Err(ActivityEnrollmentCustodyError::Conflict);
        }
        self.release(original.runtime, &original.entry);
        Ok(natural)
    }

    pub(crate) fn settle_retired_runtime(
        &self,
        home: &HomeStore,
        syndic: &SyndicStorage,
        runtime: RuntimeId,
    ) -> Result<(), ActivityEnrollmentCustodyError> {
        let entry = self
            .0
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&runtime)
            .cloned();
        if let Some(entry) = entry {
            self.settle_attempt(home, syndic, &ActivityEnrollmentAttempt { runtime, entry })?;
        }
        Ok(())
    }

    pub fn new(home: BerylHomeId, capacity: NonZeroUsize) -> Self {
        Self(Arc::new(Slots {
            home,
            capacity,
            entries: Mutex::new(HashMap::new()),
        }))
    }

    pub fn pending_count(&self) -> usize {
        self.0
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    pub fn reserve<'a>(
        &self,
        home: &'a HomeStore,
        prepared: PreparedActivityEnrollment,
    ) -> Result<ReservedActivityEnrollment<'a>, ActivityEnrollmentCustodyError> {
        if home.home_id() != self.0.home || prepared.home_id() != self.0.home {
            return Err(ActivityEnrollmentCustodyError::Identity);
        }
        let generation = home
            .health()
            .generation()
            .ok_or(ActivityEnrollmentCustodyError::Identity)?;
        let runtime = prepared.runtime_id();
        let mut entries = self.0.entries.lock().unwrap_or_else(|e| e.into_inner());
        if entries.contains_key(&runtime) {
            return Err(ActivityEnrollmentCustodyError::Occupied);
        }
        if entries.len() == self.0.capacity.get() {
            return Err(ActivityEnrollmentCustodyError::Capacity);
        }
        let (command, witness) = prepared.into_command();
        let entry = Arc::new(Entry {
            generation,
            state: Mutex::new(Attempt {
                witness: Some(witness),
                reconciliation: None,
            }),
        });
        entries.insert(runtime, Arc::clone(&entry));
        Ok(ReservedActivityEnrollment {
            owner: self.clone(),
            runtime,
            entry,
            home,
            command: Some(command),
        })
    }

    pub fn settle_retired_candidate(
        &self,
        candidate: &HomeCandidateRecoveryAccess<'_>,
        syndic: &SyndicStorage,
        cancellation: &CommandCancellation,
    ) -> Result<usize, ActivityEnrollmentCustodyError> {
        if candidate.home_id() != self.0.home {
            return Err(ActivityEnrollmentCustodyError::Identity);
        }
        let mut settled = 0;
        loop {
            if cancellation.is_cancelled() {
                return Err(ActivityEnrollmentCustodyError::Cancelled);
            }
            let next = self
                .0
                .entries
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .next()
                .map(|(runtime, entry)| (*runtime, Arc::clone(entry)));
            let Some((runtime, entry)) = next else {
                return Ok(settled);
            };
            if candidate.generation() == entry.generation {
                return Err(ActivityEnrollmentCustodyError::Identity);
            }
            let attempt = entry.state.lock().unwrap_or_else(|e| e.into_inner());
            let handle = attempt
                .reconciliation
                .as_ref()
                .ok_or(ActivityEnrollmentCustodyError::SubmissionPending)?;
            let witness = attempt
                .witness
                .as_ref()
                .ok_or(ActivityEnrollmentCustodyError::SubmissionPending)?;
            let result = candidate.retry_reconciliation(handle)?;
            let natural = syndic.activity_enrollment_status_candidate(candidate, witness)?;
            if !matches!(
                (result, natural),
                (
                    ReconciliationResolution::ExactOld,
                    ActivityEnrollmentStatus::NotCommitted
                ) | (
                    ReconciliationResolution::ExactNew { .. },
                    ActivityEnrollmentStatus::Committed { .. }
                )
            ) {
                return Err(ActivityEnrollmentCustodyError::Conflict);
            }
            self.release(runtime, &entry);
            settled += 1;
        }
    }

    fn release(&self, runtime: RuntimeId, entry: &Arc<Entry>) {
        let mut entries = self.0.entries.lock().unwrap_or_else(|e| e.into_inner());
        if entries
            .get(&runtime)
            .is_some_and(|current| Arc::ptr_eq(current, entry))
        {
            entries.remove(&runtime);
        }
    }
}

impl ReservedActivityEnrollment<'_> {
    pub(crate) fn attempt(&self) -> ActivityEnrollmentAttempt {
        ActivityEnrollmentAttempt {
            runtime: self.runtime,
            entry: Arc::clone(&self.entry),
        }
    }

    pub fn execute(mut self) -> ActivityEnrollmentCommandOutcome {
        let mut attempt = self.entry.state.lock().unwrap_or_else(|e| e.into_inner());
        let outcome = self
            .home
            .execute(self.command.take().expect("reserved command"));
        match outcome {
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => {
                attempt.reconciliation = Some(reconciliation.install_and_handle());
                ActivityEnrollmentCommandOutcome::Pending { failure }
            }
            outcome => {
                let witness = attempt.witness.take().expect("reserved witness");
                self.owner.release(self.runtime, &self.entry);
                ActivityEnrollmentCommandOutcome::Definitive { outcome, witness }
            }
        }
    }
}

impl Drop for ReservedActivityEnrollment<'_> {
    fn drop(&mut self) {
        if self.command.is_some() {
            self.owner.release(self.runtime, &self.entry);
        }
    }
}
