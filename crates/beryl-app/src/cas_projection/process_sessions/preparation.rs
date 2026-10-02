use beryl_home_store::HomeServiceReference;
use std::{path::Path, thread, time::Duration};

use beryl_backend::ManagedBackendLaunchSpec;

use beryl_model::RuntimeId;
use beryl_state::RuntimeRootState;
use syndic_storage::SyndicStorage;

use super::*;
mod handoff;
mod run;
#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../tests/unit/shutdown_preparation_capture.rs"]
mod shutdown_capture_tests;
mod target;
mod token_directory;
pub use token_directory::RuntimeTokenDirectory;
use crate::cas_projection::{
    ProcessOrdinaryDynamicToolAuthority, RuntimeInterestError, RuntimeInterestStatus,
    ScheduledOrdinaryAdmission, ScheduledOrdinaryAdmissionResult,
    ScheduledOrdinaryExecutionUnavailable, runtime_interest::RuntimeInterestOwner,
    service::ProjectionAdmissionContext, service_config::ProjectionWorkerPool,
};

pub struct RuntimeSessionPreparationConfig {
    pub runtime_roots: RuntimeRootState,
    pub assets: AssetState,
    pub policy: ScheduledOrdinaryRequestPolicy,
    pub token_directory: RuntimeTokenDirectory,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RuntimeSessionPreparationError {
    #[error("runtime session preparation requires an open configured service")]
    ServiceUnavailable,
    #[error("runtime session preparation is already configured")]
    AlreadyConfigured,
    #[error("runtime session preparation configuration is invalid")]
    InvalidConfiguration,
    #[error("the execution session provider belongs to another service")]
    OwnerMismatch,
    #[error("the exact failed runtime target is no longer eligible for retry")]
    RetryUnavailable,
}

pub(in crate::cas_projection) struct PreparationContext {
    pub(in crate::cas_projection) config: RuntimeSessionPreparationConfig,
    pub(in crate::cas_projection) home: Arc<HomeServiceReference>,
    pub(in crate::cas_projection) storage: SyndicStorage,
    pub(in crate::cas_projection) owner: Arc<RuntimeInterestOwner>,
    pub(in crate::cas_projection) work_sources: crate::cas_projection::service::ProcessWorkSources,
    pub(in crate::cas_projection) admission: ProjectionAdmissionContext,
    pub(in crate::cas_projection) workers: ProjectionWorkerPool,
    pub(in crate::cas_projection) commands: LiveCommandAuthorizer,
    pub(in crate::cas_projection) tools: ProcessOrdinaryDynamicToolAuthority,
    pub(in crate::cas_projection) timeout: Duration,
}

pub(super) struct PreparationWorker {
    handle: thread::JoinHandle<()>,
    pub(super) complete: bool,
    pub(super) binding: ExecutionBinding,
}

impl ScheduledExecutionSessions {
    pub fn runtime_failure(
        &self,
        runtime_id: RuntimeId,
    ) -> Option<crate::cas_projection::RuntimeFailureSnapshot> {
        let context = self.lock().preparation.as_ref().cloned()?;
        context.revoke_obsolete_retry(runtime_id);
        context.owner.failure_snapshot(runtime_id)
    }

    pub fn retry_runtime_session(
        &self,
        snapshot: crate::cas_projection::RuntimeFailureSnapshot,
        thread_id: SyndicThreadId,
        binding: ExecutionBinding,
    ) -> Result<(), RuntimeSessionPreparationError> {
        let (context, ready) = {
            let state = self.lock();
            if state.closed {
                return Err(RuntimeSessionPreparationError::ServiceUnavailable);
            }
            let context = state
                .preparation
                .as_ref()
                .cloned()
                .ok_or(RuntimeSessionPreparationError::ServiceUnavailable)?;
            let ready = state
                .context
                .as_ref()
                .ok_or(RuntimeSessionPreparationError::OwnerMismatch)?
                .ready
                .clone();
            (context, ready)
        };
        context.revoke_obsolete_retry(binding.runtime_id());
        context
            .launch_spec_for(thread_id, &binding)
            .map_err(|_| RuntimeSessionPreparationError::RetryUnavailable)?;
        context
            .owner
            .authorize_retry(snapshot, thread_id, binding)
            .map_err(|_| RuntimeSessionPreparationError::RetryUnavailable)?;
        ready.notify();
        Ok(())
    }

    pub(in crate::cas_projection) fn configure_preparation(
        &self,
        context: PreparationContext,
    ) -> Result<(), RuntimeSessionPreparationError> {
        let ready = {
            let mut state = self.lock();
            let attached = state
                .context
                .as_ref()
                .ok_or(RuntimeSessionPreparationError::OwnerMismatch)?;
            if attached.service_generation != context.commands.service_generation() {
                return Err(RuntimeSessionPreparationError::OwnerMismatch);
            }
            if state.closed || !attached.commands.is_open() {
                return Err(RuntimeSessionPreparationError::ServiceUnavailable);
            }
            if state.preparation.is_some() {
                return Err(RuntimeSessionPreparationError::AlreadyConfigured);
            }
            let ready = attached.ready.clone();
            state.preparation = Some(Arc::new(context));
            ready
        };
        ready.notify();
        Ok(())
    }

    pub(super) fn prepare(
        &self,
        admission: ScheduledOrdinaryAdmission,
        handoff: Option<crate::discussion_settlement::DiscussionSettlementService>,
    ) -> ScheduledOrdinaryAdmissionResult {
        let mut state = self.lock();
        let Some(context) = state.preparation.as_ref().cloned() else {
            return admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady);
        };
        let thread_id = admission.thread_id();
        let capacity = state.context.as_ref().map_or(0, |context| context.capacity);
        if state.closed
            || !context.commands.is_open()
            || !admission
                .acquisition()
                .is_some_and(|acquisition| acquisition.belongs_to(&context.commands))
        {
            return admission.decline(ScheduledOrdinaryExecutionUnavailable::ShuttingDown);
        }
        if state.preparing.contains_key(&thread_id) || state.preparing.len() >= capacity {
            return admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady);
        }
        let sessions = self.clone();
        let binding = admission.execution_binding().clone();
        let worker = thread::Builder::new()
            .name("beryl-session-preparation".to_owned())
            .spawn(move || {
                context.run(&sessions, &admission, handoff.as_ref());
                drop(context);
                let mut state = sessions.lock();
                drop(admission);
                if let Some(worker) = state.preparing.get_mut(&thread_id) {
                    worker.complete = true;
                    state.work_changed();
                }
                let ready = state.context.as_ref().map(|context| context.ready.clone());
                let has_session = state.slots.contains_key(&thread_id);
                drop(state);
                if let Some(ready) = ready {
                    if has_session {
                        ready.notify();
                    } else {
                        ready.signal.wake(AcceptedInputWakeReason::IdleRecheck);
                    }
                }
            });
        if let Ok(worker) = worker {
            state.preparing.insert(
                thread_id,
                PreparationWorker {
                    handle: worker,
                    complete: false,
                    binding,
                },
            );
            state.work_changed();
        }
        ScheduledOrdinaryAdmissionResult::Unavailable(
            ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady,
        )
    }

    pub(super) fn reap_preparation(&self) {
        let finished: Vec<_> = {
            let mut state = self.lock();
            let threads: Vec<_> = state
                .preparing
                .iter()
                .filter_map(|(id, worker)| {
                    (worker.complete || worker.handle.is_finished()).then_some(*id)
                })
                .collect();
            if !threads.is_empty() {
                state.work_changed();
            }
            threads
                .into_iter()
                .filter_map(|id| state.preparing.remove(&id))
                .collect()
        };
        for worker in finished {
            let _ = worker.handle.join();
        }
    }

    pub(super) fn close_preparation(&self) {
        let (context, workers) = {
            let mut state = self.lock();
            if !state.closed || !state.preparing.is_empty() {
                state.work_changed();
            }
            state.closed = true;
            (
                state.preparation.take(),
                std::mem::take(&mut state.preparing),
            )
        };
        for worker in workers.into_values() {
            let _ = worker.handle.join();
        }
        drop(context);
    }
}
