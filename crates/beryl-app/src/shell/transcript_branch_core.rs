use std::{fmt, time::Duration};

use beryl_backend::{
    ManagedBackendSession, ThreadForkResponse, ThreadInfo, ThreadRollbackResponse, ThreadSummary,
};
use beryl_model::{
    conversation::{
        ConversationThreadId, RegisteredConversationThread, WorkspaceConversationState,
    },
    workspace::WorkspaceId,
};

use super::transcript_branch_menu_state::{TranscriptBranchAction, TranscriptBranchRequest};

#[derive(Debug)]
pub(crate) enum TranscriptBranchOutcome {
    Branched {
        action: TranscriptBranchAction,
        source_thread_id: String,
        source_turn_id: String,
        title_seed: String,
        thread: ThreadInfo,
    },
    Failed {
        action: TranscriptBranchAction,
        source_thread_id: String,
        source_turn_id: String,
        message: String,
    },
}

pub(crate) trait TranscriptBranchBackend {
    type Error: fmt::Display;

    fn fork_thread(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadForkResponse, Self::Error>;

    fn rollback_thread(
        &mut self,
        thread_id: &str,
        num_turns: u32,
        timeout: Duration,
    ) -> Result<ThreadRollbackResponse, Self::Error>;
}

impl TranscriptBranchBackend for ManagedBackendSession {
    type Error = beryl_backend::ManagedBackendError;

    fn fork_thread(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadForkResponse, Self::Error> {
        ManagedBackendSession::fork_thread(self, thread_id, timeout)
    }

    fn rollback_thread(
        &mut self,
        thread_id: &str,
        num_turns: u32,
        timeout: Duration,
    ) -> Result<ThreadRollbackResponse, Self::Error> {
        ManagedBackendSession::rollback_thread(self, thread_id, num_turns, timeout)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptBranchActivationGate {
    pub(crate) activation_in_progress: bool,
    pub(crate) workspace_ready: bool,
    pub(crate) execution_target_matches_branch: bool,
    pub(crate) backend_available: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptBranchActivationBlocker {
    ActivationInProgress,
    WorkspaceNotReady,
    ExecutionTargetChanged,
    BackendUnavailable,
}

impl TranscriptBranchActivationBlocker {
    pub(crate) fn notice_detail(self) -> &'static str {
        match self {
            Self::ActivationInProgress => {
                "Beryl created the branch, but another thread activation is already running."
            }
            Self::WorkspaceNotReady => {
                "Beryl created the branch, but the workspace is no longer ready to activate it."
            }
            Self::ExecutionTargetChanged => {
                "Beryl created the branch, but the active backend target changed before it could be opened."
            }
            Self::BackendUnavailable => {
                "Beryl created the branch, but no managed backend is available to activate it."
            }
        }
    }
}

pub(crate) fn transcript_branch_activation_blocker(
    gate: TranscriptBranchActivationGate,
) -> Option<TranscriptBranchActivationBlocker> {
    if gate.activation_in_progress {
        return Some(TranscriptBranchActivationBlocker::ActivationInProgress);
    }
    if !gate.workspace_ready {
        return Some(TranscriptBranchActivationBlocker::WorkspaceNotReady);
    }
    if !gate.execution_target_matches_branch {
        return Some(TranscriptBranchActivationBlocker::ExecutionTargetChanged);
    }
    if !gate.backend_available {
        return Some(TranscriptBranchActivationBlocker::BackendUnavailable);
    }
    None
}

pub(crate) fn run_transcript_branch<B>(
    backend: &mut B,
    request: TranscriptBranchRequest,
    timeout: Duration,
) -> TranscriptBranchOutcome
where
    B: TranscriptBranchBackend,
{
    let action = request.action();
    let source_thread_id = request.target().source_thread_id().to_string();
    let source_turn_id = request.target().source_turn_id().to_string();
    let title_seed = request.target().title_seed_text();

    let fork = match backend.fork_thread(&source_thread_id, timeout) {
        Ok(fork) => fork,
        Err(error) => {
            return branch_failed(
                action,
                source_thread_id,
                source_turn_id,
                format!("Beryl could not fork the source conversation thread: {error}"),
            );
        }
    };

    let branch_thread_id = fork.thread.summary().id;
    let trailing_turns = match trailing_turn_count_after(&fork.thread, &source_turn_id) {
        Ok(count) => count,
        Err(message) => {
            return branch_failed(action, source_thread_id, source_turn_id, message);
        }
    };

    let thread = if trailing_turns == 0 {
        fork.thread
    } else {
        let num_turns = match u32::try_from(trailing_turns) {
            Ok(num_turns) => num_turns,
            Err(_) => {
                return branch_failed(
                    action,
                    source_thread_id,
                    source_turn_id,
                    format!(
                        "Beryl forked thread {branch_thread_id} but the rollback turn count exceeded the backend limit."
                    ),
                );
            }
        };
        match backend.rollback_thread(&branch_thread_id, num_turns, timeout) {
            Ok(response) => response.thread,
            Err(error) => {
                return branch_failed(
                    action,
                    source_thread_id,
                    source_turn_id,
                    format!(
                        "Beryl forked thread {branch_thread_id} but could not prune later turns from the branch: {error}"
                    ),
                );
            }
        }
    };

    if thread.summary().ephemeral {
        return branch_failed(
            action,
            source_thread_id,
            source_turn_id,
            format!("Beryl forked thread {branch_thread_id}, but the backend marked it ephemeral."),
        );
    }

    match trailing_turn_count_after(&thread, &source_turn_id) {
        Ok(0) => TranscriptBranchOutcome::Branched {
            action,
            source_thread_id,
            source_turn_id,
            title_seed,
            thread,
        },
        Ok(count) => branch_failed(
            action,
            source_thread_id,
            source_turn_id,
            format!(
                "Beryl forked thread {branch_thread_id}, but {count} later turn(s) remained after rollback."
            ),
        ),
        Err(message) => branch_failed(action, source_thread_id, source_turn_id, message),
    }
}

pub(crate) fn register_transcript_branch_thread(
    workspace_state: &mut WorkspaceConversationState,
    source_thread_id: &ConversationThreadId,
    branch_summary: &ThreadSummary,
) -> Result<(WorkspaceId, bool), String> {
    let source_thread = workspace_state
        .thread_registration(source_thread_id)
        .ok_or_else(|| {
            format!(
                "Beryl could not register the branch because source thread {} is no longer registered in this workspace.",
                source_thread_id.as_str()
            )
        })?;
    let execution_target = source_thread.execution_target().clone();
    let member_binding = source_thread.member_binding().cloned();
    let copied_source_name =
        copied_source_backend_name(source_thread.backend_name(), branch_summary);

    if branch_summary.cwd.as_path() != execution_target.canonical_path() {
        return Err(format!(
            "Beryl forked thread {}, but it records working directory {} instead of the source thread workspace member {}.",
            branch_summary.id,
            branch_summary.cwd.display(),
            execution_target.canonical_path().display()
        ));
    }

    let mut registered_thread = RegisteredConversationThread::new(
        ConversationThreadId::new(branch_summary.id.clone()),
        execution_target.clone(),
        branch_summary.preview.clone(),
        if copied_source_name.is_some() {
            None
        } else {
            branch_summary.name.clone()
        },
        branch_summary.created_at,
        branch_summary.updated_at,
    )
    .with_beryl_created();
    if copied_source_name.is_some() {
        registered_thread =
            registered_thread.with_ignored_backend_name_for_automatic_title(copied_source_name);
    }
    if let Some(binding) = member_binding {
        registered_thread = registered_thread.with_member_binding(binding);
    }

    let changed = workspace_state.remember_thread(registered_thread);
    Ok((execution_target, changed))
}

fn copied_source_backend_name(
    source_backend_name: Option<&str>,
    branch_summary: &ThreadSummary,
) -> Option<String> {
    let source_backend_name = normalized_backend_name(source_backend_name)?;
    let branch_backend_name = normalized_backend_name(branch_summary.name.as_deref())?;
    (branch_backend_name == source_backend_name).then(|| branch_backend_name.to_string())
}

fn normalized_backend_name(name: Option<&str>) -> Option<&str> {
    let name = name?.trim();
    (!name.is_empty()).then_some(name)
}

fn trailing_turn_count_after(thread: &ThreadInfo, selected_turn_id: &str) -> Result<usize, String> {
    let branch_thread_id = thread.summary().id;
    let Some(position) = thread
        .turns
        .iter()
        .position(|turn| turn.id == selected_turn_id)
    else {
        return Err(format!(
            "Beryl forked thread {branch_thread_id}, but the backend did not return selected turn {selected_turn_id} in the forked history."
        ));
    };

    Ok(thread.turns.len().saturating_sub(position + 1))
}

fn branch_failed(
    action: TranscriptBranchAction,
    source_thread_id: String,
    source_turn_id: String,
    message: String,
) -> TranscriptBranchOutcome {
    TranscriptBranchOutcome::Failed {
        action,
        source_thread_id,
        source_turn_id,
        message,
    }
}
