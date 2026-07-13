use std::{fmt, path::Path, time::Duration};

use beryl_backend::{ManagedBackendSession, ThreadInfo, ThreadSessionResponse, ThreadStartOptions};
use beryl_model::workspace::WorkspaceId;

use crate::beryl_user_thread_start_options;

pub(crate) trait DecisionChildThreadStartBackend {
    type Error: fmt::Display;

    fn start_thread_with_options(
        &mut self,
        cwd: &Path,
        options: ThreadStartOptions,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error>;
}

impl DecisionChildThreadStartBackend for ManagedBackendSession {
    type Error = beryl_backend::ManagedBackendError;

    fn start_thread_with_options(
        &mut self,
        cwd: &Path,
        options: ThreadStartOptions,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error> {
        ManagedBackendSession::start_thread_with_options(self, cwd, options, timeout)
    }
}

pub(crate) fn start_empty_decision_child_thread<B>(
    backend: &mut B,
    execution_target: &WorkspaceId,
    timeout: Duration,
) -> Result<ThreadInfo, String>
where
    B: DecisionChildThreadStartBackend,
{
    let response = backend
        .start_thread_with_options(
            execution_target.canonical_path(),
            beryl_user_thread_start_options(),
            timeout,
        )
        .map_err(|error| {
            format!("Beryl could not create an empty decision child thread: {error}")
        })?;
    let thread = response.thread;
    let summary = thread.summary();
    if summary.ephemeral {
        return Err(format!(
            "Beryl created decision child thread {}, but the backend marked it ephemeral.",
            summary.id
        ));
    }
    if !thread.turns.is_empty() {
        return Err(format!(
            "Beryl created decision child thread {}, but it already contained {} turn(s); decision branches must start empty.",
            summary.id,
            thread.turns.len()
        ));
    }
    Ok(thread)
}
