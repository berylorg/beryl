use std::sync::{atomic::AtomicBool, mpsc};

use beryl_backend::{
    GeneratedOutputError, GeneratedOutputPermit, GeneratedOutputSource, GeneratedReaderAllowance,
};
use beryl_model::RuntimeNativePath;

use super::*;
use crate::cas_projection::GeneratedSourceEligibility;

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../tests/unit/generated_runtime_source.rs"]
mod tests;

pub(in crate::cas_projection) struct GeneratedRuntimeOwner {
    shared: Arc<RuntimeInterestShared>,
    runtime_id: RuntimeId,
    attempt: u64,
    process: CasProcessGeneration,
}

pub(crate) struct GeneratedRuntimeSource {
    source: Option<GeneratedOutputSource>,
    lease: RuntimeSourceLease,
}

pub(crate) struct GeneratedSourceCloseFailure {
    error: GeneratedOutputError,
    source: GeneratedRuntimeSource,
}

pub(crate) enum GeneratedSourceOpenFailure {
    Eligibility(GeneratedSourceEligibility),
    Open { error: GeneratedOutputError, source: Option<GeneratedRuntimeSource> },
}

struct RuntimeSourceLease {
    shared: Arc<RuntimeInterestShared>,
    runtime_id: RuntimeId,
    attempt: u64,
    slot: Arc<GeneratedRuntimeSlot>,
}

pub(super) struct GeneratedRuntimeSlot {
    state: Mutex<SlotState>,
    response: Mutex<Option<mpsc::SyncSender<Result<GeneratedRuntimeSource, GeneratedSourceOpenFailure>>>>,
}

enum SlotState {
    Pending(OpenRequest),
    Opening,
    Active,
    Orphan(GeneratedOutputSource),
    Finished,
}

struct OpenRequest {
    permit: GeneratedOutputPermit,
    path: RuntimeNativePath,
    allowance: GeneratedReaderAllowance,
    cancel: Arc<AtomicBool>,
}

fn source_state(slot: &GeneratedRuntimeSlot) -> MutexGuard<'_, SlotState> {
    slot.state.lock().unwrap_or_else(|poison| poison.into_inner())
}

impl RuntimeInterestOwner {
    pub(in crate::cas_projection) fn install_generated_waker(&self, wake: std::task::Waker) {
        assert!(self.shared.generated_wake.set(wake).is_ok(), "one generated observer per runtime owner");
    }
    pub(in crate::cas_projection) fn generated_source_owner(
        &self,
        runtime_id: RuntimeId,
        process: CasProcessGeneration,
    ) -> Result<GeneratedRuntimeOwner, GeneratedSourceEligibility> {
        let state = self.shared.lock();
        if state.closed || !self.shared.commands.is_open() {
            return Err(GeneratedSourceEligibility::LostSourceAuthority);
        }
        let entry = state.runtimes.get(&runtime_id).ok_or(GeneratedSourceEligibility::LostSourceAuthority)?;
        check_entry(entry, process)?;
        Ok(GeneratedRuntimeOwner { shared: Arc::clone(&self.shared), runtime_id, attempt: entry.attempt, process })
    }
}

fn check_entry(entry: &RuntimeEntry, process: CasProcessGeneration) -> Result<(), GeneratedSourceEligibility> {
    match entry.status {
        RuntimeInterestStatus::Ready(ready) if ready.process_generation() == process => Ok(()),
        RuntimeInterestStatus::Starting => Err(GeneratedSourceEligibility::Busy),
        _ => Err(GeneratedSourceEligibility::LostSourceAuthority),
    }
}

impl GeneratedRuntimeOwner {
    pub(in crate::cas_projection) fn open(
        self,
        permit: GeneratedOutputPermit,
        path: RuntimeNativePath,
        allowance: GeneratedReaderAllowance,
        cancel: Arc<AtomicBool>,
    ) -> Result<GeneratedRuntimeSource, GeneratedSourceOpenFailure> {
        let (response, receiver) = mpsc::sync_channel(1);
        let slot = Arc::new(GeneratedRuntimeSlot {
            state: Mutex::new(SlotState::Pending(OpenRequest { permit, path, allowance, cancel })),
            response: Mutex::new(Some(response)),
        });
        self.admit_slot(slot).map_err(GeneratedSourceOpenFailure::Eligibility)?;
        self.shared.changed.notify_all();
        receiver.recv().unwrap_or(Err(GeneratedSourceOpenFailure::Eligibility(GeneratedSourceEligibility::LostSourceAuthority)))
    }

    fn admit_slot(&self, slot: Arc<GeneratedRuntimeSlot>) -> Result<(), GeneratedSourceEligibility> {
            let mut state = self.shared.lock();
            if state.closed || !self.shared.commands.is_open() {
                return Err(GeneratedSourceEligibility::LostSourceAuthority);
            }
            let entry = state.runtimes.get_mut(&self.runtime_id)
                .filter(|entry| entry.attempt == self.attempt)
                .ok_or(GeneratedSourceEligibility::LostSourceAuthority)?;
            check_entry(entry, self.process)?;
            if entry.interests.is_empty() {
                return Err(GeneratedSourceEligibility::LostSourceAuthority);
            }
            if entry.generated_source.is_some() {
                return Err(GeneratedSourceEligibility::Busy);
            }
            entry.generated_source = Some(slot);
            Ok(())
    }
}

impl GeneratedSourceOpenFailure {
    pub(crate) fn error(&self) -> GeneratedOutputError {
        match self { Self::Eligibility(error) => error.error(), Self::Open { error, .. } => *error }
    }

    pub(crate) fn into_parts(self) -> (GeneratedOutputError, Option<GeneratedRuntimeSource>) {
        match self { Self::Eligibility(error) => (error.error(), None), Self::Open { error, source } => (error, source) }
    }
}

impl GeneratedSourceCloseFailure {
    pub(crate) fn error(&self) -> GeneratedOutputError { self.error }
    pub(crate) fn into_parts(self) -> (GeneratedOutputError, GeneratedRuntimeSource) { (self.error, self.source) }
}

impl GeneratedRuntimeSource {
    pub(crate) fn length(&self) -> u64 { self.source.as_ref().expect("owned source").length() }
    pub(crate) fn cursor(&self) -> beryl_backend::GeneratedOutputCursor<'_> { self.source.as_ref().expect("owned source").cursor() }

    pub(crate) fn close(mut self) -> Result<(), GeneratedSourceCloseFailure> {
        match self.source.take().expect("owned source").close() {
            Ok(()) => { self.lease.finish(); Ok(()) }
            Err(failure) => {
                let (error, source) = failure.into_parts();
                self.source = Some(source);
                Err(GeneratedSourceCloseFailure { error, source: self })
            }
        }
    }
}

impl Drop for GeneratedRuntimeSource {
    fn drop(&mut self) {
        let Some(source) = self.source.take() else { return; };
        match source.close() {
            Ok(()) => self.lease.finish(),
            Err(failure) => {
                let (_, source) = failure.into_parts();
                *source_state(&self.lease.slot) = SlotState::Orphan(source);
                self.lease.shared.changed.notify_all();
            }
        }
    }
}

impl RuntimeSourceLease {
    fn finish(&self) {
        *source_state(&self.slot) = SlotState::Finished;
        let mut state = self.shared.lock();
        if let Some(entry) = state.runtimes.get_mut(&self.runtime_id)
            && entry.attempt == self.attempt
            && entry.generated_source.as_ref().is_some_and(|slot| Arc::ptr_eq(slot, &self.slot))
        {
            entry.generated_source = None;
        }
        drop(state);
        self.shared.changed.notify_all();
        self.shared.wake_generated();
    }
}

pub(super) fn process_request(
    shared: &Arc<RuntimeInterestShared>,
    runtime_id: RuntimeId,
    attempt: u64,
    runtime: &mut dyn RunningRuntime,
) {
    let Some(slot) = current_slot(shared, runtime_id, attempt) else { return; };
    let request = {
        let mut state = source_state(&slot);
        match std::mem::replace(&mut *state, SlotState::Opening) {
            SlotState::Pending(request) => request,
            other => { *state = other; return; }
        }
    };
    let lease = RuntimeSourceLease { shared: Arc::clone(shared), runtime_id, attempt, slot: Arc::clone(&slot) };
    let result = runtime.open_generated_source(request.permit, &request.path, request.allowance, request.cancel);
    let result = match result {
        Ok(source) => {
            *source_state(&slot) = SlotState::Active;
            Ok(GeneratedRuntimeSource { source: Some(source), lease })
        }
        Err(failure) => {
            let (error, source) = failure.into_parts();
            let source = source.map(|source| GeneratedRuntimeSource { source: Some(source), lease: RuntimeSourceLease {
                shared: Arc::clone(shared), runtime_id, attempt, slot: Arc::clone(&slot),
            }});
            if source.is_some() { *source_state(&slot) = SlotState::Active; } else { lease.finish(); }
            Err(GeneratedSourceOpenFailure::Open { error, source })
        }
    };
    let response = slot.response.lock().unwrap_or_else(|poison| poison.into_inner()).take();
    if let Some(response) = response { let _ = response.send(result); }
}

fn current_slot(shared: &RuntimeInterestShared, runtime_id: RuntimeId, attempt: u64) -> Option<Arc<GeneratedRuntimeSlot>> {
    shared.lock().runtimes.get(&runtime_id).filter(|entry| entry.attempt == attempt)?.generated_source.clone()
}

pub(super) fn refuse_pending(shared: &RuntimeInterestShared, runtime_id: RuntimeId, attempt: u64) {
    let Some(slot) = current_slot(shared, runtime_id, attempt) else { return; };
    let pending = {
        let mut state = source_state(&slot);
        if matches!(&*state, SlotState::Pending(_)) { *state = SlotState::Finished; true } else { false }
    };
    if pending {
        if let Some(response) = slot.response.lock().unwrap_or_else(|poison| poison.into_inner()).take() {
            let _ = response.send(Err(GeneratedSourceOpenFailure::Eligibility(GeneratedSourceEligibility::LostSourceAuthority)));
        }
    }
}

pub(super) fn fail_response(shared: &RuntimeInterestShared, runtime_id: RuntimeId, attempt: u64) {
    let Some(slot) = current_slot(shared, runtime_id, attempt) else { return; };
    if let Some(response) = slot.response.lock().unwrap_or_else(|poison| poison.into_inner()).take() {
        let _ = response.send(Err(GeneratedSourceOpenFailure::Open { error: GeneratedOutputError::CleanupPending, source: None }));
    }
}

pub(super) fn source_drained(shared: &RuntimeInterestShared, runtime_id: RuntimeId, attempt: u64) -> bool {
    let Some(slot) = current_slot(shared, runtime_id, attempt) else { return true; };
    let orphan = {
        let mut state = source_state(&slot);
        match std::mem::replace(&mut *state, SlotState::Opening) {
            SlotState::Orphan(source) => Some(source),
            other => { *state = other; None }
        }
    };
    if let Some(source) = orphan {
        *source_state(&slot) = match source.close() {
            Ok(()) => SlotState::Finished,
            Err(failure) => SlotState::Orphan(failure.into_parts().1),
        };
    }
    let finished = matches!(&*source_state(&slot), SlotState::Finished);
    if finished {
        let mut state = shared.lock();
        if let Some(entry) = state.runtimes.get_mut(&runtime_id)
            && entry.attempt == attempt
            && entry.generated_source.as_ref().is_some_and(|current| Arc::ptr_eq(current, &slot))
        { entry.generated_source = None; }
        drop(state);
        shared.wake_generated();
    }
    finished
}

pub(super) fn source_drained_after_retirement(shared: &RuntimeInterestShared, runtime_id: RuntimeId, attempt: u64) -> bool {
    if let Some(slot) = current_slot(shared, runtime_id, attempt) {
        let mut state = source_state(&slot);
        if matches!(&*state, SlotState::Pending(_) | SlotState::Opening) {
            *state = SlotState::Finished;
        }
    }
    source_drained(shared, runtime_id, attempt)
}
