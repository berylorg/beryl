use std::time::Duration;

use beryl_backend::{
    ManagedBackendSession, ThreadForkResponse, ThreadInfo, ThreadRollbackResponse,
    ThreadSessionMetadata, ThreadSummary, TurnInfo,
};
use beryl_model::{
    conversation::{
        ConversationThreadId, ConversationTurnId, RegisteredConversationThread,
        WorkspaceConversationState,
    },
    workspace::WorkspaceId,
};

use crate::branch_bootstrap_core::{
    BranchBootstrapBackend, BranchBootstrapMessageInput, branch_bootstrap_message,
    start_branch_bootstrap_turn,
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
        durable_summary: ThreadSummary,
        bootstrap_turn_id: Option<ConversationTurnId>,
    },
    Failed {
        action: TranscriptBranchAction,
        source_thread_id: String,
        source_turn_id: String,
        message: String,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedTranscriptBranch {
    action: TranscriptBranchAction,
    source_thread_id: String,
    source_turn_id: String,
    title_seed: String,
    branch_thread_id: ConversationThreadId,
    thread: ThreadInfo,
    session_metadata: ThreadSessionMetadata,
    bootstrap_message: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ForegroundTranscriptBranchStart {
    action: TranscriptBranchAction,
    source_thread_id: String,
    source_turn_id: String,
    branch_thread_id: ConversationThreadId,
    thread: ThreadInfo,
    session_metadata: ThreadSessionMetadata,
    bootstrap_message: String,
    bootstrap_turn: TurnInfo,
    bootstrap_turn_id: ConversationTurnId,
}

#[derive(Clone, Debug)]
pub(crate) enum ForegroundTranscriptBranchPublication {
    Published {
        source_thread_id: String,
        source_turn_id: String,
        title_seed: String,
        durable_summary: ThreadSummary,
        bootstrap_turn_id: ConversationTurnId,
    },
    Failed {
        source_thread_id: String,
        source_turn_id: String,
        message: String,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct ForegroundTranscriptBranchState {
    action: TranscriptBranchAction,
    source_thread_id: String,
    source_turn_id: String,
    branch_thread_id: Option<ConversationThreadId>,
    bootstrap_turn_id: Option<ConversationTurnId>,
}

pub(crate) trait TranscriptBranchBackend: BranchBootstrapBackend {
    fn fork_thread(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadForkResponse, <Self as BranchBootstrapBackend>::Error>;

    fn rollback_thread(
        &mut self,
        thread_id: &str,
        num_turns: u32,
        timeout: Duration,
    ) -> Result<ThreadRollbackResponse, <Self as BranchBootstrapBackend>::Error>;
}

impl TranscriptBranchBackend for ManagedBackendSession {
    fn fork_thread(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadForkResponse, <Self as BranchBootstrapBackend>::Error> {
        ManagedBackendSession::fork_thread(self, thread_id, timeout)
    }

    fn rollback_thread(
        &mut self,
        thread_id: &str,
        num_turns: u32,
        timeout: Duration,
    ) -> Result<ThreadRollbackResponse, <Self as BranchBootstrapBackend>::Error> {
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
    let prepared = match prepare_transcript_branch(backend, request, timeout) {
        Ok(prepared) => prepared,
        Err(outcome) => return outcome,
    };

    let branch_thread_id = prepared.branch_thread_id.clone();
    let bootstrap = match start_branch_bootstrap_turn(
        backend,
        &branch_thread_id,
        &prepared.bootstrap_message,
        timeout,
    ) {
        Ok(bootstrap) => bootstrap,
        Err(error) => {
            return branch_failed(
                prepared.action,
                prepared.source_thread_id,
                prepared.source_turn_id,
                error.to_string(),
            );
        }
    };

    TranscriptBranchOutcome::Branched {
        action: prepared.action,
        source_thread_id: prepared.source_thread_id,
        source_turn_id: prepared.source_turn_id,
        title_seed: prepared.title_seed,
        thread: prepared.thread,
        durable_summary: bootstrap.thread().clone(),
        bootstrap_turn_id: bootstrap.bootstrap_turn_id().cloned(),
    }
}

pub(crate) fn prepare_transcript_branch<B>(
    backend: &mut B,
    request: TranscriptBranchRequest,
    timeout: Duration,
) -> Result<PreparedTranscriptBranch, TranscriptBranchOutcome>
where
    B: TranscriptBranchBackend,
{
    let action = request.action();
    let source_thread_id = request.target().source_thread_id().to_string();
    let source_turn_id = request.target().source_turn_id().to_string();
    let title_seed = request.target().title_seed_text();
    let parent_thread_id = ConversationThreadId::new(source_thread_id.clone());

    let fork = match backend.fork_thread(&source_thread_id, timeout) {
        Ok(fork) => fork,
        Err(error) => {
            return Err(branch_failed(
                action,
                source_thread_id,
                source_turn_id,
                format!("Beryl could not fork the source conversation thread: {error}"),
            ));
        }
    };

    let session_metadata = fork.metadata();
    let branch_thread_id = fork.thread.summary().id;
    let trailing_turns = match trailing_turn_count_after(&fork.thread, &source_turn_id) {
        Ok(count) => count,
        Err(message) => {
            return Err(branch_failed(
                action,
                source_thread_id,
                source_turn_id,
                message,
            ));
        }
    };

    let thread = if trailing_turns == 0 {
        fork.thread
    } else {
        let num_turns = match u32::try_from(trailing_turns) {
            Ok(num_turns) => num_turns,
            Err(_) => {
                return Err(branch_failed(
                    action,
                    source_thread_id,
                    source_turn_id,
                    format!(
                        "Beryl forked thread {branch_thread_id} but the rollback turn count exceeded the backend limit."
                    ),
                ));
            }
        };
        match backend.rollback_thread(&branch_thread_id, num_turns, timeout) {
            Ok(response) => response.thread,
            Err(error) => {
                return Err(branch_failed(
                    action,
                    source_thread_id,
                    source_turn_id,
                    format!(
                        "Beryl forked thread {branch_thread_id} but could not prune later turns from the branch: {error}"
                    ),
                ));
            }
        }
    };

    if thread.summary().ephemeral {
        return Err(branch_failed(
            action,
            source_thread_id,
            source_turn_id,
            format!("Beryl forked thread {branch_thread_id}, but the backend marked it ephemeral."),
        ));
    }

    match trailing_turn_count_after(&thread, &source_turn_id) {
        Ok(0) => {
            let branch_thread_id = ConversationThreadId::new(thread.summary().id);
            let bootstrap_message = branch_bootstrap_message(BranchBootstrapMessageInput {
                parent_thread_id: &parent_thread_id,
                parent_thread_title: request.parent_thread_title(),
                branch_context: None,
            });

            Ok(PreparedTranscriptBranch {
                action,
                source_thread_id,
                source_turn_id,
                title_seed,
                branch_thread_id,
                thread,
                session_metadata,
                bootstrap_message,
            })
        }
        Ok(count) => Err(branch_failed(
            action,
            source_thread_id,
            source_turn_id,
            format!(
                "Beryl forked thread {branch_thread_id}, but {count} later turn(s) remained after rollback."
            ),
        )),
        Err(message) => Err(branch_failed(
            action,
            source_thread_id,
            source_turn_id,
            message,
        )),
    }
}

impl PreparedTranscriptBranch {
    pub(crate) fn title_seed(&self) -> &str {
        &self.title_seed
    }

    pub(crate) fn branch_thread_id(&self) -> &ConversationThreadId {
        &self.branch_thread_id
    }

    pub(crate) fn bootstrap_message(&self) -> &str {
        &self.bootstrap_message
    }

    pub(crate) fn into_foreground_start(
        self,
        bootstrap_turn: TurnInfo,
        bootstrap_turn_id: ConversationTurnId,
    ) -> ForegroundTranscriptBranchStart {
        ForegroundTranscriptBranchStart {
            action: self.action,
            source_thread_id: self.source_thread_id,
            source_turn_id: self.source_turn_id,
            branch_thread_id: self.branch_thread_id,
            thread: self.thread,
            session_metadata: self.session_metadata,
            bootstrap_message: self.bootstrap_message,
            bootstrap_turn,
            bootstrap_turn_id,
        }
    }
}

impl ForegroundTranscriptBranchStart {
    pub(crate) fn action(&self) -> TranscriptBranchAction {
        self.action
    }

    pub(crate) fn source_thread_id(&self) -> &str {
        &self.source_thread_id
    }

    pub(crate) fn source_turn_id(&self) -> &str {
        &self.source_turn_id
    }

    pub(crate) fn branch_thread_id(&self) -> &ConversationThreadId {
        &self.branch_thread_id
    }

    pub(crate) fn thread(&self) -> &ThreadInfo {
        &self.thread
    }

    pub(crate) fn session_metadata(&self) -> &ThreadSessionMetadata {
        &self.session_metadata
    }

    pub(crate) fn bootstrap_message(&self) -> &str {
        &self.bootstrap_message
    }

    pub(crate) fn bootstrap_turn(&self) -> &TurnInfo {
        &self.bootstrap_turn
    }

    pub(crate) fn bootstrap_turn_id(&self) -> &ConversationTurnId {
        &self.bootstrap_turn_id
    }
}

impl ForegroundTranscriptBranchState {
    pub(crate) fn starting(request: &TranscriptBranchRequest) -> Self {
        Self {
            action: request.action(),
            source_thread_id: request.target().source_thread_id().to_string(),
            source_turn_id: request.target().source_turn_id().to_string(),
            branch_thread_id: None,
            bootstrap_turn_id: None,
        }
    }

    pub(crate) fn action(&self) -> TranscriptBranchAction {
        self.action
    }

    pub(crate) fn source_thread_id(&self) -> &str {
        &self.source_thread_id
    }

    pub(crate) fn source_turn_id(&self) -> &str {
        &self.source_turn_id
    }

    pub(crate) fn branch_thread_id(&self) -> Option<&ConversationThreadId> {
        self.branch_thread_id.as_ref()
    }

    pub(crate) fn activate(
        &mut self,
        branch_thread_id: ConversationThreadId,
        bootstrap_turn_id: ConversationTurnId,
    ) {
        self.branch_thread_id = Some(branch_thread_id);
        self.bootstrap_turn_id = Some(bootstrap_turn_id);
    }

    pub(crate) fn bootstrap_turn_matches(&self, thread_id: &str, turn_id: &str) -> bool {
        self.branch_thread_id.as_ref().map(|id| id.as_str()) == Some(thread_id)
            && self.bootstrap_turn_id.as_ref().map(|id| id.as_str()) == Some(turn_id)
    }
}

pub(crate) fn register_transcript_branch_thread(
    workspace_state: &mut WorkspaceConversationState,
    source_thread_id: &ConversationThreadId,
    source_turn_id: &ConversationTurnId,
    branch_summary: &ThreadSummary,
    bootstrap_turn_id: Option<ConversationTurnId>,
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
    .with_beryl_created()
    .with_branch_parent_thread_id(source_thread_id.clone())
    .with_transcript_branch_bootstrap(source_turn_id.clone(), bootstrap_turn_id);
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
