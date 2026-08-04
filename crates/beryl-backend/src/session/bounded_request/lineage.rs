use std::{path::Path, time::Duration};

use beryl_model::{CasThreadId, CasTurnId};

use super::{
    ManagedBackendError, ManagedBackendSession,
    wire::{ThreadForkParams, ThreadResumeParams, ThreadStartParams},
};
use crate::{
    BoundedResponseResult, FreshLoadedThreadSession, LoadedThreadSession, ThreadLineageResponse,
    ThreadLoadOptions, ThreadStartOptions, incoming_json::ResponseFamily,
};

impl ManagedBackendSession {
    pub fn start_thread(
        &mut self,
        cwd: &Path,
        timeout: Duration,
    ) -> Result<FreshLoadedThreadSession, ManagedBackendError> {
        self.start_thread_with_options(cwd, ThreadStartOptions::persistent(), timeout)
    }

    pub fn start_thread_with_options(
        &mut self,
        cwd: &Path,
        options: ThreadStartOptions,
        timeout: Duration,
    ) -> Result<FreshLoadedThreadSession, ManagedBackendError> {
        self.require_foreground_request(ResponseFamily::ThreadStart.method())?;
        let params = ThreadStartParams::new(cwd, &options);
        self.lineage_response(&params, ResponseFamily::ThreadStart, timeout)
            .map(ThreadLineageResponse::into_fresh)
    }

    pub fn resume_thread(
        &mut self,
        thread_id: &CasThreadId,
        options: &ThreadLoadOptions,
        timeout: Duration,
    ) -> Result<LoadedThreadSession, ManagedBackendError> {
        self.require_foreground_request(ResponseFamily::ThreadResume.method())?;
        let params = ThreadResumeParams::new(thread_id, options);
        let loaded = self
            .lineage_response(&params, ResponseFamily::ThreadResume, timeout)?
            .into_loaded();
        if loaded.thread_id() != thread_id {
            let actual = loaded.thread_id().clone();
            self.retire_connection();
            return Err(ManagedBackendError::ThreadResponseIdentityMismatch {
                method: ResponseFamily::ThreadResume.method().to_owned(),
                expected: thread_id.clone(),
                actual,
            });
        }
        Ok(loaded)
    }

    pub fn fork_thread(
        &mut self,
        thread_id: &CasThreadId,
        options: &ThreadLoadOptions,
        timeout: Duration,
    ) -> Result<FreshLoadedThreadSession, ManagedBackendError> {
        let params = ThreadForkParams::full(thread_id, options);
        self.fork_with_params(thread_id, &params, timeout)
    }

    pub fn fork_thread_through_turn(
        &mut self,
        thread_id: &CasThreadId,
        last_turn_id: &CasTurnId,
        options: &ThreadLoadOptions,
        timeout: Duration,
    ) -> Result<FreshLoadedThreadSession, ManagedBackendError> {
        let params = ThreadForkParams::through_turn(thread_id, last_turn_id, options);
        self.fork_with_params(thread_id, &params, timeout)
    }

    fn fork_with_params(
        &mut self,
        source_thread_id: &CasThreadId,
        params: &ThreadForkParams<'_>,
        timeout: Duration,
    ) -> Result<FreshLoadedThreadSession, ManagedBackendError> {
        self.require_foreground_request(ResponseFamily::ThreadFork.method())?;
        let response = self.lineage_response(params, ResponseFamily::ThreadFork, timeout)?;
        let fresh = response.into_fresh();
        if fresh.thread_id() == source_thread_id {
            self.retire_connection();
            return Err(ManagedBackendError::ForkResponseReusedSource {
                method: ResponseFamily::ThreadFork.method().to_owned(),
                source_thread: source_thread_id.clone(),
            });
        }
        Ok(fresh)
    }

    fn lineage_response<P: super::wire::RequestSpec>(
        &mut self,
        params: &P,
        family: ResponseFamily,
        timeout: Duration,
    ) -> Result<ThreadLineageResponse, ManagedBackendError> {
        let completion = self.dispatch_request(params, timeout)?;
        let exact = self.exact_response(completion, family.method())?;
        match (family, exact.result) {
            (ResponseFamily::ThreadStart, BoundedResponseResult::ThreadStart(response))
            | (ResponseFamily::ThreadResume, BoundedResponseResult::ThreadResume(response))
            | (ResponseFamily::ThreadFork, BoundedResponseResult::ThreadFork(response)) => {
                Ok(response)
            }
            _ => self.fail_unexpected_response(family.method()),
        }
    }
}
