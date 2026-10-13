use super::request::DraftReplacementErrorV1;
use beryl_home_store::{CommandOutcome, HomeStore, ReconciliationHandle, ReconciliationResolution};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(crate) struct CommandObservation(Arc<Mutex<State>>);
#[derive(Clone)]
enum State {
    Unobserved,
    Committed,
    Noncommit,
    Pending(ReconciliationHandle),
    Collision,
}
impl Default for CommandObservation {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(State::Unobserved)))
    }
}
impl CommandObservation {
    pub(crate) fn retain(&self, outcome: CommandOutcome) -> Result<(), DraftReplacementErrorV1> {
        let incoming = match outcome {
            CommandOutcome::Committed { .. } => State::Committed,
            CommandOutcome::NotCommitted { .. } => State::Noncommit,
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                State::Pending(reconciliation.install_and_handle())
            }
        };
        {
            let mut current = self
                .0
                .lock()
                .map_err(|_| DraftReplacementErrorV1::Unavailable)?;
            if matches!(*current, State::Unobserved) {
                *current = incoming;
            }
        }
        Ok(())
    }
    pub(crate) fn resume(&self, store: &HomeStore) -> Result<bool, DraftReplacementErrorV1> {
        let current = self
            .0
            .lock()
            .map_err(|_| DraftReplacementErrorV1::Unavailable)?
            .clone();
        match current {
            State::Unobserved => Err(DraftReplacementErrorV1::Unavailable),
            State::Committed => Ok(true),
            State::Noncommit => Ok(false),
            State::Collision => Err(DraftReplacementErrorV1::Conflict),
            State::Pending(handle) => {
                let resolution = store.retry_reconciliation(&handle)?;
                let next = match resolution {
                    ReconciliationResolution::ExactNew { .. } => State::Committed,
                    ReconciliationResolution::ExactOld => State::Noncommit,
                    ReconciliationResolution::Collision
                    | ReconciliationResolution::ExactSuccessor { .. } => State::Collision,
                };
                let mut current = self
                    .0
                    .lock()
                    .map_err(|_| DraftReplacementErrorV1::Unavailable)?;
                if matches!(*current, State::Pending(_)) {
                    *current = next;
                }
                match *current {
                    State::Committed => Ok(true),
                    State::Noncommit => Ok(false),
                    _ => Err(DraftReplacementErrorV1::Conflict),
                }
            }
        }
    }
    pub(crate) fn known(&self) -> Result<bool, DraftReplacementErrorV1> {
        match *self
            .0
            .lock()
            .map_err(|_| DraftReplacementErrorV1::Unavailable)?
        {
            State::Committed => Ok(true),
            State::Noncommit => Ok(false),
            State::Collision => Err(DraftReplacementErrorV1::Conflict),
            State::Unobserved | State::Pending(_) => Err(DraftReplacementErrorV1::Unavailable),
        }
    }
}
