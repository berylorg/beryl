use std::{path::Path, thread, time::Duration};

use beryl_backend::ManagedBackendLaunchSpec;
use beryl_home_store::HomeStore;
use beryl_model::{AdmittedHostPath, RuntimeId, RuntimeNativePath};
use beryl_state::RuntimeRootState;
use syndic_storage::SyndicStorage;

use super::*;
use crate::cas_projection::{
    ProcessOrdinaryDynamicToolAuthority, RuntimeInterestError, RuntimeInterestStatus,
    ScheduledOrdinaryAdmission, ScheduledOrdinaryAdmissionResult,
    ScheduledOrdinaryExecutionUnavailable, runtime_interest::RuntimeInterestOwner,
    service::ProjectionAdmissionContext, service_config::ProjectionWorkerPool,
};

#[derive(Clone)]
pub struct RuntimeTokenDirectories {
    pub runtime_id: RuntimeId,
    pub host: AdmittedHostPath,
    pub runtime: RuntimeNativePath,
}

pub struct RuntimeSessionPreparationConfig {
    pub runtime_roots: RuntimeRootState,
    pub assets: AssetState,
    pub policy: ScheduledOrdinaryRequestPolicy,
    pub token_directories: Vec<RuntimeTokenDirectories>,
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
    pub(in crate::cas_projection) home: Arc<HomeStore>,
    pub(in crate::cas_projection) storage: SyndicStorage,
    pub(in crate::cas_projection) owner: Arc<RuntimeInterestOwner>,
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
            .ok_or(RuntimeSessionPreparationError::RetryUnavailable)?;
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
    ) -> ScheduledOrdinaryAdmissionResult {
        let mut state = self.lock();
        let Some(context) = state.preparation.as_ref().cloned() else {
            return admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady);
        };
        let thread_id = admission.thread_id();
        let capacity = state.context.as_ref().map_or(0, |context| context.capacity);
        if state.closed || !context.commands.is_open() {
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
                context.run(&sessions, &admission);
                drop(context);
                let mut state = sessions.lock();
                drop(admission);
                if let Some(worker) = state.preparing.get_mut(&thread_id) {
                    worker.complete = true;
                    state.work_changed();
                }
                if state.slots.contains_key(&thread_id)
                    && let Some(context) = state.context.as_ref()
                {
                    context.ready.notify();
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

impl PreparationContext {
    fn revoke_obsolete_retry(&self, runtime_id: RuntimeId) {
        if let Some((thread_id, binding)) = self.owner.pending_retry(runtime_id)
            && self.launch_spec_for(thread_id, &binding).is_none()
        {
            self.owner.revoke_retry(thread_id, &binding);
        }
    }

    fn launch_spec(
        &self,
        admission: &ScheduledOrdinaryAdmission,
    ) -> Option<ManagedBackendLaunchSpec> {
        self.launch_spec_for(admission.thread_id(), admission.execution_binding())
    }

    fn launch_spec_for(
        &self,
        thread_id: SyndicThreadId,
        binding: &ExecutionBinding,
    ) -> Option<ManagedBackendLaunchSpec> {
        if !self.commands.is_open() {
            return None;
        }
        let execution = self
            .storage
            .thread_execution(
                &self.home,
                thread_id,
                crate::cas_projection::input_replay::point_limit(),
            )
            .ok()??;
        if execution.execution() != binding {
            return None;
        }
        let runtime = self
            .config
            .runtime_roots
            .runtime(&self.home, binding.runtime_id())
            .ok()??;
        let root = self
            .config
            .runtime_roots
            .root(&self.home, binding.root_id())
            .ok()??;
        if root.runtime_id() != runtime.runtime_id() || root.canonical_path() != binding.root_path()
        {
            return None;
        }
        let tokens = self
            .config
            .token_directories
            .iter()
            .find(|entry| entry.runtime_id == binding.runtime_id())?;
        ManagedBackendLaunchSpec::new(
            runtime.runtime_id(),
            runtime.canonical_executable().clone(),
            runtime.mode().clone(),
            runtime.runtime_native_executable().clone(),
            root.canonical_path().clone(),
            tokens.host.clone(),
            tokens.runtime.clone(),
        )
        .ok()
    }

    fn run(&self, sessions: &ScheduledExecutionSessions, admission: &ScheduledOrdinaryAdmission) {
        let Some(spec) = self.launch_spec(admission) else {
            self.owner
                .revoke_retry(admission.thread_id(), admission.execution_binding());
            return;
        };
        let mut execution_workers = None;
        let interest = self.owner.acquire_prepared_managed(
            spec.clone(),
            admission.execution_binding().clone(),
            admission.thread_id(),
            || {
                let (readiness, execution) = self
                    .workers
                    .try_acquire_cold_preparation_or_arm()
                    .map_err(|_| RuntimeInterestError::WorkerCapacity)?;
                execution_workers = Some(execution);
                Ok((self.admission.clone(), readiness))
            },
        );
        let Ok(interest) = interest else { return };
        let execution_workers = match execution_workers {
            Some(workers) => workers,
            None => match self.workers.try_acquire_warm_preparation_or_arm() {
                Ok(workers) => workers,
                Err(_) => return,
            },
        };
        let mut status = interest.status();
        while status == RuntimeInterestStatus::Starting {
            if !self.commands.is_open() || sessions.lock().closed {
                return;
            }
            status = interest.wait_for_change(status, self.timeout);
        }
        if !matches!(status, RuntimeInterestStatus::Ready(_))
            || self.launch_spec(admission).as_ref() != Some(&spec)
            || sessions.lock().closed
        {
            return;
        }
        let Ok((connector, ready)) = self.owner.session_connector(&interest) else {
            return;
        };
        let Some(identity) = connector.launch_identity() else {
            return;
        };
        let session = match self.admission.admit_with_reserved_workers(
            &connector,
            identity.runtime_id(),
            identity.process_generation(),
            Path::new(identity.working_directory().as_str()),
            self.timeout,
            execution_workers,
        ) {
            Ok(session) => session,
            Err(error) => {
                if matches!(error, crate::cas_projection::ProjectionSessionAdmissionError::CandidateConnection { .. }
                    | crate::cas_projection::ProjectionSessionAdmissionError::Initialization { .. }
                    | crate::cas_projection::ProjectionSessionAdmissionError::ReleaseAdmission { .. }) {
                    interest.invalidate_configuration(ready);
                }
                return;
            }
        };
        let Ok(session) = interest.publish_session(ready, session) else {
            return;
        };
        if self.launch_spec(admission).as_ref() != Some(&spec) || sessions.lock().closed {
            return;
        }
        let _ = sessions.register(
            admission.thread_id(),
            admission.execution_binding().clone(),
            session,
            self.config.policy.clone(),
            self.config.assets.clone(),
            Box::new(self.tools.clone()),
        );
    }
}
