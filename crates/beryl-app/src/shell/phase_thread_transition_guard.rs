use std::path::PathBuf;

use beryl_model::{
    conversation::{
        ConversationThreadId, ConversationThreadMemberBinding, ConversationTurnId,
        WorkspaceConversationState,
    },
    workspace::{BerylWorkspaceId, WorkspaceId},
};

use super::phase_thread_preparation_core::PhaseThreadPreparationRequest;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PhaseThreadCurrentIdentity {
    pub(crate) request_generation: u64,
    pub(crate) workspace_id: BerylWorkspaceId,
    pub(crate) source_thread_id: ConversationThreadId,
    pub(crate) source_turn_id: ConversationTurnId,
    pub(crate) orchestration_root_thread_id: ConversationThreadId,
    pub(crate) source_selection_thread_id: ConversationThreadId,
    pub(crate) execution_target: WorkspaceId,
    pub(crate) canonical_cwd: PathBuf,
    pub(crate) member_binding: ConversationThreadMemberBinding,
    pub(crate) available_member_binding: Option<ConversationThreadMemberBinding>,
    pub(crate) source_rebind_required: bool,
    pub(crate) root_rebind_required: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PhaseThreadResultGuardFailure {
    UnexpectedRequest,
    RequestGeneration,
    Workspace,
    SourceThread,
    SourceTurn,
    OrchestrationRoot,
    SourceSelection,
    ExecutionTarget,
    CanonicalWorkingDirectory,
    MemberBinding,
    MemberBindingUnavailable,
    SourceRebindRequired,
    RootRebindRequired,
}

pub(crate) fn guard_phase_thread_preparation_result(
    expected_request: &PhaseThreadPreparationRequest,
    returned_request: &PhaseThreadPreparationRequest,
    current: &PhaseThreadCurrentIdentity,
) -> Result<(), PhaseThreadResultGuardFailure> {
    if returned_request != expected_request {
        return Err(PhaseThreadResultGuardFailure::UnexpectedRequest);
    }
    if current.request_generation != expected_request.request_generation() {
        return Err(PhaseThreadResultGuardFailure::RequestGeneration);
    }
    if &current.workspace_id != expected_request.workspace_id() {
        return Err(PhaseThreadResultGuardFailure::Workspace);
    }
    if &current.source_thread_id != expected_request.source_thread_id() {
        return Err(PhaseThreadResultGuardFailure::SourceThread);
    }
    if &current.source_turn_id != expected_request.source_turn_id() {
        return Err(PhaseThreadResultGuardFailure::SourceTurn);
    }
    if &current.orchestration_root_thread_id != expected_request.orchestration_root_thread_id() {
        return Err(PhaseThreadResultGuardFailure::OrchestrationRoot);
    }
    if &current.source_selection_thread_id != expected_request.source_selection_thread_id() {
        return Err(PhaseThreadResultGuardFailure::SourceSelection);
    }
    if &current.execution_target != expected_request.execution_target() {
        return Err(PhaseThreadResultGuardFailure::ExecutionTarget);
    }
    if current.canonical_cwd.as_path() != expected_request.canonical_cwd() {
        return Err(PhaseThreadResultGuardFailure::CanonicalWorkingDirectory);
    }
    if &current.member_binding != expected_request.member_binding() {
        return Err(PhaseThreadResultGuardFailure::MemberBinding);
    }
    if current.source_rebind_required {
        return Err(PhaseThreadResultGuardFailure::SourceRebindRequired);
    }
    if current.root_rebind_required {
        return Err(PhaseThreadResultGuardFailure::RootRebindRequired);
    }
    if current.available_member_binding.as_ref() != Some(expected_request.member_binding()) {
        return Err(PhaseThreadResultGuardFailure::MemberBindingUnavailable);
    }
    Ok(())
}

pub(crate) fn phase_thread_request_registrations_are_available(
    request: &PhaseThreadPreparationRequest,
    state: &WorkspaceConversationState,
    implicit_home_execution_target: Option<&WorkspaceId>,
) -> bool {
    let Some(source) = state.thread_registration(request.source_thread_id()) else {
        return false;
    };
    let Some(root) = state.thread_registration(request.orchestration_root_thread_id()) else {
        return false;
    };
    source.rebind_required().is_none()
        && root.rebind_required().is_none()
        && source.execution_target() == request.execution_target()
        && root.execution_target() == request.execution_target()
        && source.member_binding() == Some(request.member_binding())
        && root.member_binding() == Some(request.member_binding())
        && source.orchestration_root_thread_id() == Some(request.orchestration_root_thread_id())
        && root.orchestration_root_thread_id() == Some(request.orchestration_root_thread_id())
        && state
            .binding_for_available_execution_target(
                request.execution_target(),
                implicit_home_execution_target,
            )
            .as_ref()
            == Some(request.member_binding())
}
