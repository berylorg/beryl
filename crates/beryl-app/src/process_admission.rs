use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct ProcessAdmissionGate {
    inner: Arc<Mutex<AdmissionState>>,
}

#[derive(Debug)]
struct AdmissionState {
    epoch: u64,
    fenced: bool,
    admissions: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProcessAdmissionError {
    #[error("process execution admission is fenced")]
    Fenced,
    #[error("process execution authority belongs to an earlier shutdown attempt")]
    Stale,
    #[error("admitted process work has not settled")]
    Unsettled,
    #[error("process admission authority is unavailable")]
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum ProcessExecutionAdmissionError {
    #[error(transparent)]
    Service(#[from] crate::cas_projection::LiveCommandAdmissionError),
    #[error(transparent)]
    Process(#[from] ProcessAdmissionError),
}

#[derive(Clone, Debug)]
pub(crate) struct ProcessExecutionPermit {
    gate: ProcessAdmissionGate,
    epoch: Option<u64>,
}

#[derive(Debug)]
pub(crate) struct ProcessAdmissionReservation {
    gate: ProcessAdmissionGate,
}

#[derive(Clone, Debug)]
pub(crate) struct ProcessAdmissionFence {
    gate: ProcessAdmissionGate,
    epoch: u64,
}

impl Default for ProcessAdmissionGate {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessAdmissionGate {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AdmissionState {
                epoch: 1,
                fenced: false,
                admissions: 0,
            })),
        }
    }

    pub(crate) fn execution_permit(&self) -> ProcessExecutionPermit {
        let epoch = self
            .inner
            .lock()
            .ok()
            .and_then(|state| (!state.fenced).then_some(state.epoch));
        ProcessExecutionPermit {
            gate: self.clone(),
            epoch,
        }
    }

    pub(crate) fn admit<T>(&self, admit: impl FnOnce() -> T) -> Result<T, ProcessAdmissionError> {
        let state = self
            .inner
            .lock()
            .map_err(|_| ProcessAdmissionError::Unavailable)?;
        if state.fenced {
            return Err(ProcessAdmissionError::Fenced);
        }
        Ok(admit())
    }

    pub(crate) fn fence(&self) -> Result<ProcessAdmissionFence, ProcessAdmissionError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ProcessAdmissionError::Unavailable)?;
        if !state.fenced {
            let epoch = state
                .epoch
                .checked_add(1)
                .ok_or(ProcessAdmissionError::Unavailable)?;
            state.epoch = epoch;
            state.fenced = true;
        }
        Ok(ProcessAdmissionFence {
            gate: self.clone(),
            epoch: state.epoch,
        })
    }

    pub(crate) fn settle<T>(&self, settle: impl FnOnce() -> T) -> T {
        let _state = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        settle()
    }
}

impl ProcessExecutionPermit {
    pub(crate) fn commit<T>(&self, commit: impl FnOnce() -> T) -> Result<T, ProcessAdmissionError> {
        let state = self
            .gate
            .inner
            .lock()
            .map_err(|_| ProcessAdmissionError::Unavailable)?;
        self.validate(&state)?;
        Ok(commit())
    }

    pub(crate) fn reserve(&self) -> Result<ProcessAdmissionReservation, ProcessAdmissionError> {
        let mut state = self
            .gate
            .inner
            .lock()
            .map_err(|_| ProcessAdmissionError::Unavailable)?;
        self.validate(&state)?;
        state.admissions = state
            .admissions
            .checked_add(1)
            .ok_or(ProcessAdmissionError::Unavailable)?;
        Ok(ProcessAdmissionReservation {
            gate: self.gate.clone(),
        })
    }

    fn validate(&self, state: &AdmissionState) -> Result<(), ProcessAdmissionError> {
        if state.fenced {
            return Err(ProcessAdmissionError::Fenced);
        }
        if self.epoch != Some(state.epoch) {
            return Err(ProcessAdmissionError::Stale);
        }
        Ok(())
    }

    pub(crate) fn reopen(
        &self,
        fence: &ProcessAdmissionFence,
        coherent: bool,
    ) -> Result<(), ProcessAdmissionError> {
        if !Arc::ptr_eq(&self.gate.inner, &fence.gate.inner) {
            return Err(ProcessAdmissionError::Stale);
        }
        fence.reopen_if(coherent)
    }
}

impl ProcessAdmissionFence {
    pub(crate) fn reopen_if(&self, coherent: bool) -> Result<(), ProcessAdmissionError> {
        let mut state = self
            .gate
            .inner
            .lock()
            .map_err(|_| ProcessAdmissionError::Unavailable)?;
        if !state.fenced || state.epoch != self.epoch {
            return Err(ProcessAdmissionError::Stale);
        }
        if state.admissions != 0 || !coherent {
            return Err(ProcessAdmissionError::Unsettled);
        }
        state.fenced = false;
        Ok(())
    }
}

impl Drop for ProcessAdmissionReservation {
    fn drop(&mut self) {
        let mut state = self
            .gate
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.admissions -= 1;
    }
}

#[cfg(test)]
#[path = "../tests/unit/process_admission.rs"]
mod tests;
