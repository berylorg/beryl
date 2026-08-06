use std::{fmt, path::PathBuf, time::Duration};

use beryl_backend::{
    ThreadForkResponse, ThreadInfo, ThreadItem, ThreadReadResponse, ThreadRollbackResponse,
    ThreadSessionMetadata, ThreadStatus,
};
use beryl_model::{
    conversation::{
        ConversationThreadId, ConversationThreadMemberBinding, ConversationTurnId,
        RegisteredConversationThread,
    },
    workspace::{BerylWorkspaceId, WorkspaceId},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PhaseThreadPreparationRequest {
    request_generation: u64,
    workspace_id: BerylWorkspaceId,
    source_thread_id: ConversationThreadId,
    source_turn_id: ConversationTurnId,
    orchestration_root_thread_id: ConversationThreadId,
    source_selection_thread_id: ConversationThreadId,
    execution_target: WorkspaceId,
    canonical_cwd: PathBuf,
    member_binding: ConversationThreadMemberBinding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PhaseThreadPreparationRequestParts {
    pub(crate) request_generation: u64,
    pub(crate) workspace_id: BerylWorkspaceId,
    pub(crate) source_thread_id: ConversationThreadId,
    pub(crate) source_turn_id: ConversationTurnId,
    pub(crate) orchestration_root_thread_id: ConversationThreadId,
    pub(crate) source_selection_thread_id: ConversationThreadId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PhaseThreadPreparationRequestError {
    ZeroGeneration,
    BlankIdentity { field: &'static str },
    SourceSelectionMismatch,
    SourceRegistrationMismatch,
    RootRegistrationMismatch,
    SourceRootMismatch,
    RootNotSelfIdentified,
    SourceRebindRequired,
    RootRebindRequired,
    MissingMemberBinding,
    MemberBindingUnavailable,
    RootBindingMismatch,
    RootTargetMismatch,
    MemberBindingTargetMismatch,
}

impl fmt::Display for PhaseThreadPreparationRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid frozen phase-thread preparation request: {self:?}"
        )
    }
}

impl PhaseThreadPreparationRequest {
    #[allow(dead_code)]
    pub(crate) fn new(
        parts: PhaseThreadPreparationRequestParts,
        source: &RegisteredConversationThread,
        root: &RegisteredConversationThread,
    ) -> Result<Self, PhaseThreadPreparationRequestError> {
        Self::new_with_available_binding(parts, source, root, source.member_binding())
    }

    pub(crate) fn new_with_available_binding(
        parts: PhaseThreadPreparationRequestParts,
        source: &RegisteredConversationThread,
        root: &RegisteredConversationThread,
        available_member_binding: Option<&ConversationThreadMemberBinding>,
    ) -> Result<Self, PhaseThreadPreparationRequestError> {
        if parts.request_generation == 0 {
            return Err(PhaseThreadPreparationRequestError::ZeroGeneration);
        }
        for (field, value) in [
            ("workspace id", parts.workspace_id.as_str()),
            ("source thread id", parts.source_thread_id.as_str()),
            ("source turn id", parts.source_turn_id.as_str()),
            (
                "orchestration root thread id",
                parts.orchestration_root_thread_id.as_str(),
            ),
            (
                "source selection thread id",
                parts.source_selection_thread_id.as_str(),
            ),
        ] {
            if value.trim().is_empty() {
                return Err(PhaseThreadPreparationRequestError::BlankIdentity { field });
            }
        }
        if parts.source_selection_thread_id != parts.source_thread_id {
            return Err(PhaseThreadPreparationRequestError::SourceSelectionMismatch);
        }
        if source.thread_id() != &parts.source_thread_id {
            return Err(PhaseThreadPreparationRequestError::SourceRegistrationMismatch);
        }
        if root.thread_id() != &parts.orchestration_root_thread_id {
            return Err(PhaseThreadPreparationRequestError::RootRegistrationMismatch);
        }
        if source.orchestration_root_thread_id() != Some(&parts.orchestration_root_thread_id) {
            return Err(PhaseThreadPreparationRequestError::SourceRootMismatch);
        }
        if root.orchestration_root_thread_id() != Some(&parts.orchestration_root_thread_id) {
            return Err(PhaseThreadPreparationRequestError::RootNotSelfIdentified);
        }
        if source.rebind_required().is_some() {
            return Err(PhaseThreadPreparationRequestError::SourceRebindRequired);
        }
        if root.rebind_required().is_some() {
            return Err(PhaseThreadPreparationRequestError::RootRebindRequired);
        }

        let execution_target = source.execution_target().clone();
        if root.execution_target() != &execution_target {
            return Err(PhaseThreadPreparationRequestError::RootTargetMismatch);
        }
        let Some(member_binding) = source.member_binding().cloned() else {
            return Err(PhaseThreadPreparationRequestError::MissingMemberBinding);
        };
        if member_binding.execution_target() != &execution_target {
            return Err(PhaseThreadPreparationRequestError::MemberBindingTargetMismatch);
        }
        if root.member_binding() != Some(&member_binding) {
            return Err(PhaseThreadPreparationRequestError::RootBindingMismatch);
        }
        if available_member_binding != Some(&member_binding) {
            return Err(PhaseThreadPreparationRequestError::MemberBindingUnavailable);
        }

        Ok(Self {
            request_generation: parts.request_generation,
            workspace_id: parts.workspace_id,
            source_thread_id: parts.source_thread_id,
            source_turn_id: parts.source_turn_id,
            orchestration_root_thread_id: parts.orchestration_root_thread_id,
            source_selection_thread_id: parts.source_selection_thread_id,
            canonical_cwd: execution_target.canonical_path().to_path_buf(),
            execution_target,
            member_binding,
        })
    }

    pub(crate) fn request_generation(&self) -> u64 {
        self.request_generation
    }
    pub(crate) fn workspace_id(&self) -> &BerylWorkspaceId {
        &self.workspace_id
    }
    pub(crate) fn source_thread_id(&self) -> &ConversationThreadId {
        &self.source_thread_id
    }
    pub(crate) fn source_turn_id(&self) -> &ConversationTurnId {
        &self.source_turn_id
    }
    pub(crate) fn orchestration_root_thread_id(&self) -> &ConversationThreadId {
        &self.orchestration_root_thread_id
    }
    pub(crate) fn source_selection_thread_id(&self) -> &ConversationThreadId {
        &self.source_selection_thread_id
    }
    pub(crate) fn execution_target(&self) -> &WorkspaceId {
        &self.execution_target
    }
    pub(crate) fn canonical_cwd(&self) -> &std::path::Path {
        &self.canonical_cwd
    }
    pub(crate) fn member_binding(&self) -> &ConversationThreadMemberBinding {
        &self.member_binding
    }
}

pub(crate) trait PhaseThreadPreparationBackend {
    type Error: fmt::Display;

    fn fork_root(
        &mut self,
        root_id: &str,
        timeout: Duration,
    ) -> Result<ThreadForkResponse, PhaseThreadForkError<Self::Error>>;
    fn rollback_child(
        &mut self,
        child_id: &str,
        num_turns: u32,
        timeout: Duration,
    ) -> Result<ThreadRollbackResponse, Self::Error>;
    fn read_child(
        &mut self,
        child_id: &str,
        timeout: Duration,
    ) -> Result<ThreadReadResponse, Self::Error>;
    fn delete_child(
        &mut self,
        child_id: &str,
        timeout: Duration,
    ) -> Result<(), PhaseThreadCleanupError<Self::Error>>;
}

#[derive(Debug)]
pub(crate) enum PhaseThreadForkError<E> {
    NotCommitted(E),
    Indeterminate(E),
}

#[derive(Debug)]
pub(crate) enum PhaseThreadCleanupError<E> {
    ChildRemains(E),
    Indeterminate(E),
}

pub(crate) trait PhaseThreadPreparationCancellation {
    fn is_cancelled(&self) -> bool;
}

impl PhaseThreadPreparationCancellation for () {
    fn is_cancelled(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PhaseThreadPreparationOutcome {
    pub(crate) request: PhaseThreadPreparationRequest,
    pub(crate) result: PhaseThreadPreparationResult,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PhaseThreadPreparationResult {
    Prepared {
        child: ThreadInfo,
        session_metadata: ThreadSessionMetadata,
    },
    DefinitiveForkFailure {
        detail: String,
    },
    IndeterminateFork {
        detail: String,
    },
    CancelledBeforeFork,
    KnownChildFailure(PhaseThreadKnownChildFailure),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PhaseThreadKnownChildFailure {
    pub(crate) child_id: String,
    pub(crate) stage: PhaseThreadPreparationStage,
    pub(crate) detail: String,
    pub(crate) cleanup: PhaseThreadCleanupOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PhaseThreadPreparationStage {
    ForkResponseValidation,
    Rollback,
    RollbackResponseValidation,
    Read,
    ReadValidation,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PhaseThreadCleanupOutcome {
    Accepted,
    ChildRemains { detail: String },
    Indeterminate { detail: String },
}

pub(crate) fn inherited_user_turn_count(thread: &ThreadInfo) -> Result<u32, String> {
    let count = thread
        .turns
        .iter()
        .filter(|turn| {
            turn.items
                .iter()
                .any(|item| matches!(item, ThreadItem::UserMessage(_)))
        })
        .count();
    checked_rollback_turn_count(count)
}

pub(crate) fn checked_rollback_turn_count(count: usize) -> Result<u32, String> {
    u32::try_from(count)
        .map_err(|_| "inherited user-turn count exceeds backend rollback limit".to_string())
}

pub(crate) fn run_phase_thread_preparation<B, C>(
    backend: &mut B,
    request: PhaseThreadPreparationRequest,
    cancellation: &C,
    timeout: Duration,
) -> PhaseThreadPreparationOutcome
where
    B: PhaseThreadPreparationBackend,
    C: PhaseThreadPreparationCancellation,
{
    if cancellation.is_cancelled() {
        return outcome(request, PhaseThreadPreparationResult::CancelledBeforeFork);
    }

    let fork = match backend.fork_root(request.orchestration_root_thread_id().as_str(), timeout) {
        Ok(fork) => fork,
        Err(PhaseThreadForkError::NotCommitted(error)) => {
            return outcome(
                request,
                PhaseThreadPreparationResult::DefinitiveForkFailure {
                    detail: error.to_string(),
                },
            );
        }
        Err(PhaseThreadForkError::Indeterminate(error)) => {
            return outcome(
                request,
                PhaseThreadPreparationResult::IndeterminateFork {
                    detail: error.to_string(),
                },
            );
        }
    };
    let fork_metadata = fork.metadata();
    let child_id = match valid_child_id(&fork.thread, &request) {
        Ok(child_id) => child_id,
        Err(detail) => {
            return outcome(
                request,
                PhaseThreadPreparationResult::IndeterminateFork { detail },
            );
        }
    };
    if let Err(detail) = validate_child(&fork.thread, &request) {
        return known_child_failure(
            backend,
            request,
            child_id,
            PhaseThreadPreparationStage::ForkResponseValidation,
            detail,
            timeout,
        );
    }
    if cancellation.is_cancelled() {
        return known_child_failure(
            backend,
            request,
            child_id,
            PhaseThreadPreparationStage::Cancelled,
            "preparation cancelled after the child was created".to_string(),
            timeout,
        );
    }

    let user_turns = match inherited_user_turn_count(&fork.thread) {
        Ok(count) => count,
        Err(detail) => {
            return known_child_failure(
                backend,
                request,
                child_id,
                PhaseThreadPreparationStage::ForkResponseValidation,
                detail,
                timeout,
            );
        }
    };
    if user_turns > 0 {
        let rollback = match backend.rollback_child(&child_id, user_turns, timeout) {
            Ok(response) => response,
            Err(error) => {
                return known_child_failure(
                    backend,
                    request,
                    child_id,
                    PhaseThreadPreparationStage::Rollback,
                    error.to_string(),
                    timeout,
                );
            }
        };
        if rollback.thread.summary().id != child_id {
            return known_child_failure(
                backend,
                request,
                child_id,
                PhaseThreadPreparationStage::RollbackResponseValidation,
                "rollback response did not identify the exact forked child".to_string(),
                timeout,
            );
        }
        if cancellation.is_cancelled() {
            return known_child_failure(
                backend,
                request,
                child_id,
                PhaseThreadPreparationStage::Cancelled,
                "preparation cancelled after rollback".to_string(),
                timeout,
            );
        }
    }

    let read = match backend.read_child(&child_id, timeout) {
        Ok(read) => read,
        Err(error) => {
            return known_child_failure(
                backend,
                request,
                child_id,
                PhaseThreadPreparationStage::Read,
                error.to_string(),
                timeout,
            );
        }
    };
    if cancellation.is_cancelled() {
        return known_child_failure(
            backend,
            request,
            child_id,
            PhaseThreadPreparationStage::Cancelled,
            "preparation cancelled after child read".to_string(),
            timeout,
        );
    }
    let session_metadata = match validate_read(&read, &fork_metadata, &request, &child_id) {
        Ok(session_metadata) => session_metadata,
        Err(detail) => {
            return known_child_failure(
                backend,
                request,
                child_id,
                PhaseThreadPreparationStage::ReadValidation,
                detail,
                timeout,
            );
        }
    };
    outcome(
        request,
        PhaseThreadPreparationResult::Prepared {
            child: read.thread,
            session_metadata,
        },
    )
}

fn valid_child_id(
    thread: &ThreadInfo,
    request: &PhaseThreadPreparationRequest,
) -> Result<String, String> {
    let child_id = thread.summary().id;
    if child_id.trim().is_empty() {
        return Err("fork response did not identify a child thread".to_string());
    }
    if child_id == request.orchestration_root_thread_id().as_str()
        || child_id == request.source_thread_id().as_str()
    {
        return Err("fork response reused a source or orchestration-root identity instead of a distinct child".to_string());
    }
    Ok(child_id)
}

fn validate_child(
    thread: &ThreadInfo,
    request: &PhaseThreadPreparationRequest,
) -> Result<(), String> {
    let summary = thread.summary();
    if summary.ephemeral {
        return Err("fork response marked the child ephemeral".to_string());
    }
    if thread.status != ThreadStatus::Idle {
        return Err("fork response marked the child non-idle".to_string());
    }
    if summary.forked_from_id.as_deref() != Some(request.orchestration_root_thread_id().as_str()) {
        return Err("fork response did not retain direct orchestration-root lineage".to_string());
    }
    if summary.cwd.as_path() != request.canonical_cwd() {
        return Err(
            "fork response working directory differs from the frozen execution target".to_string(),
        );
    }
    if request.member_binding().execution_target() != request.execution_target() {
        return Err("frozen member binding is incompatible with the execution target".to_string());
    }
    Ok(())
}

fn validate_read(
    read: &ThreadReadResponse,
    fork_metadata: &ThreadSessionMetadata,
    request: &PhaseThreadPreparationRequest,
    child_id: &str,
) -> Result<ThreadSessionMetadata, String> {
    if read.thread.summary().id != child_id {
        return Err("child read did not identify the exact forked child".to_string());
    }
    validate_child(&read.thread, request)?;
    if !read.thread.turns.is_empty() {
        return Err("child read retained effective history after rollback".to_string());
    }
    validated_runtime_metadata(fork_metadata, &read.metadata())
}

fn validated_runtime_metadata(
    fork: &ThreadSessionMetadata,
    read: &ThreadSessionMetadata,
) -> Result<ThreadSessionMetadata, String> {
    for (field, fork_value, read_value) in [
        ("model", &fork.model, &read.model),
        ("model provider", &fork.model_provider, &read.model_provider),
        (
            "reasoning effort",
            &fork.reasoning_effort,
            &read.reasoning_effort,
        ),
    ] {
        if let (Some(fork_value), Some(read_value)) = (fork_value, read_value)
            && fork_value != read_value
        {
            return Err(format!("fork and read runtime {field} metadata conflict"));
        }
    }
    Ok(ThreadSessionMetadata {
        model: read.model.clone().or_else(|| fork.model.clone()),
        model_provider: read
            .model_provider
            .clone()
            .or_else(|| fork.model_provider.clone()),
        reasoning_effort: read
            .reasoning_effort
            .clone()
            .or_else(|| fork.reasoning_effort.clone()),
    })
}

fn known_child_failure<B>(
    backend: &mut B,
    request: PhaseThreadPreparationRequest,
    child_id: String,
    stage: PhaseThreadPreparationStage,
    detail: String,
    timeout: Duration,
) -> PhaseThreadPreparationOutcome
where
    B: PhaseThreadPreparationBackend,
{
    let cleanup = match backend.delete_child(&child_id, timeout) {
        Ok(()) => PhaseThreadCleanupOutcome::Accepted,
        Err(PhaseThreadCleanupError::ChildRemains(error)) => {
            PhaseThreadCleanupOutcome::ChildRemains {
                detail: error.to_string(),
            }
        }
        Err(PhaseThreadCleanupError::Indeterminate(error)) => {
            PhaseThreadCleanupOutcome::Indeterminate {
                detail: error.to_string(),
            }
        }
    };
    outcome(
        request,
        PhaseThreadPreparationResult::KnownChildFailure(PhaseThreadKnownChildFailure {
            child_id,
            stage,
            detail,
            cleanup,
        }),
    )
}

fn outcome(
    request: PhaseThreadPreparationRequest,
    result: PhaseThreadPreparationResult,
) -> PhaseThreadPreparationOutcome {
    PhaseThreadPreparationOutcome { request, result }
}
