use std::{fmt, path::Path, time::Duration};

use beryl_backend::{
    ManagedBackendSession, ThreadSessionMetadata, ThreadSessionResponse, ThreadStartOptions,
    ThreadSummary,
};
use beryl_model::workspace::WorkspaceId;

use crate::beryl_user_thread_start_options;

pub(crate) struct ActivatedThread {
    pub(crate) thread_id: String,
    pub(crate) summary: ThreadSummary,
    pub(crate) session_metadata: ThreadSessionMetadata,
}

pub(crate) trait ThreadActivationBackend {
    type Error: fmt::Display;

    fn start_thread_with_options(
        &mut self,
        cwd: &Path,
        options: ThreadStartOptions,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error>;

    fn resume_thread_metadata(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error>;
}

impl ThreadActivationBackend for ManagedBackendSession {
    type Error = beryl_backend::ManagedBackendError;

    fn start_thread_with_options(
        &mut self,
        cwd: &Path,
        options: ThreadStartOptions,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error> {
        ManagedBackendSession::start_thread_with_options(self, cwd, options, timeout)
    }

    fn resume_thread_metadata(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error> {
        ManagedBackendSession::resume_thread_metadata(self, thread_id, timeout)
    }
}

pub(crate) fn activate_thread<B>(
    session: &mut B,
    workspace: &WorkspaceId,
    selected_thread_id: Option<&str>,
    timeout: Duration,
) -> Result<ActivatedThread, String>
where
    B: ThreadActivationBackend,
{
    let response: ThreadSessionResponse = match selected_thread_id {
        Some(thread_id) => session
            .resume_thread_metadata(thread_id, timeout)
            .map_err(|error| {
                format!("Beryl could not activate the selected conversation thread: {error}")
            })?,
        None => session
            .start_thread_with_options(
                workspace.canonical_path(),
                beryl_user_thread_start_options(),
                timeout,
            )
            .map_err(|error| {
                format!("Beryl could not create a new conversation thread: {error}")
            })?,
    };
    let summary = response.thread.summary();
    let session_metadata = response.metadata();
    Ok(ActivatedThread {
        thread_id: summary.id.clone(),
        summary,
        session_metadata,
    })
}
