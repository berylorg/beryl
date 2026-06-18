use std::{
    fmt,
    time::{Duration, Instant},
};

use beryl_backend::{
    ManagedBackendSession, ThreadInfo, ThreadSessionMetadata, ThreadSessionResponse, ThreadSummary,
};
use beryl_model::workspace::WorkspaceId;
use tracing::debug;

use crate::memory_diagnostics::MemoryMilestone;
use crate::shell::thread_selection::thread_rebind_detail;

#[derive(Debug)]
pub(crate) struct ExistingThreadActivation {
    pub thread: ThreadInfo,
    pub session_metadata: ThreadSessionMetadata,
}

#[derive(Debug)]
pub(crate) enum ExistingThreadActivationError {
    RequiresRebind { detail: String },
    Failed { message: String },
}

pub(crate) trait ExistingThreadActivationBackend {
    type Error: fmt::Display;

    fn resume_thread_metadata(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error>;
}

impl ExistingThreadActivationBackend for ManagedBackendSession {
    type Error = beryl_backend::ManagedBackendError;

    fn resume_thread_metadata(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error> {
        ManagedBackendSession::resume_thread_metadata(self, thread_id, timeout)
    }
}

pub(crate) struct ThreadActivationLoader;

impl ThreadActivationLoader {
    pub(crate) fn load_existing_thread<B>(
        backend: &mut B,
        execution_target: &WorkspaceId,
        thread_id: &str,
        label: &str,
        timeout: Duration,
    ) -> Result<ExistingThreadActivation, ExistingThreadActivationError>
    where
        B: ExistingThreadActivationBackend,
    {
        let activation_started = Instant::now();
        let resume_started = Instant::now();
        let response = backend
            .resume_thread_metadata(thread_id, timeout)
            .map_err(|error| ExistingThreadActivationError::RequiresRebind {
                detail: thread_rebind_detail(
                    label,
                    execution_target,
                    &format!("Beryl could not reopen the recorded thread: {error}."),
                ),
            })?;
        debug!(
            thread_id,
            resume_metadata_ms = elapsed_ms(resume_started.elapsed()),
            "resumed existing thread metadata"
        );
        MemoryMilestone::new("thread_activation_metadata_resumed")
            .runtime(execution_target.runtime_mode().display_name())
            .thread_id(thread_id)
            .log();
        let session_metadata = response.metadata();
        let thread = response.thread;
        let summary = thread.summary();
        validate_thread_execution_target(&summary, execution_target, label)?;

        debug!(
            thread_id,
            worker_activation_total_ms = elapsed_ms(activation_started.elapsed()),
            "validated existing-thread activation metadata"
        );

        Ok(ExistingThreadActivation {
            thread,
            session_metadata,
        })
    }
}

pub(crate) fn validate_thread_execution_target(
    summary: &ThreadSummary,
    execution_target: &WorkspaceId,
    label: &str,
) -> Result<(), ExistingThreadActivationError> {
    let expected = execution_target.canonical_path();
    if summary.cwd == expected {
        return Ok(());
    }

    Err(ExistingThreadActivationError::RequiresRebind {
        detail: thread_rebind_detail(
            label,
            execution_target,
            &format!(
                "The reopened thread records working directory {}, but the expected workspace member is {}.",
                summary.cwd.display(),
                expected.display()
            ),
        ),
    })
}

fn elapsed_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}
