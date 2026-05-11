use std::path::PathBuf;

use beryl_model::conversation::{
    ConversationThreadId, ConversationThreadMemberBinding, ConversationThreadRebindRequirement,
    ConversationThreadTitle, PrimaryWorkspaceMember, RegisteredConversationThread,
    WorkspaceConversationState,
};
use beryl_model::workspace::{
    BerylWorkspaceId, BerylWorkspaceManifest, ExecutionTargetId, RuntimeMode, WorkspaceMemberId,
};
use serde::{Deserialize, Serialize};

use super::{WorkspaceGraphToolError, WorkspaceGraphToolService};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStateReadRequest {
    pub workspace_id: BerylWorkspaceId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStateSnapshot {
    pub manifest: BerylWorkspaceManifest,
    #[serde(default)]
    pub selected_runtime: Option<RuntimeMode>,
    pub primary_member: WorkspacePrimaryMemberSnapshot,
    #[serde(default)]
    pub available_members: Vec<WorkspaceMemberSnapshot>,
    #[serde(default)]
    pub threads: Vec<WorkspaceThreadMetadataSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum WorkspacePrimaryMemberSnapshot {
    None,
    Explicit {
        member_id: WorkspaceMemberId,
        runtime: RuntimeMode,
        canonical_path: PathBuf,
    },
    ImplicitHome {
        runtime: RuntimeMode,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceMemberSnapshotKind {
    Explicit,
    ImplicitHome,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMemberSnapshot {
    pub kind: WorkspaceMemberSnapshotKind,
    #[serde(default)]
    pub member_id: Option<WorkspaceMemberId>,
    pub runtime: RuntimeMode,
    #[serde(default)]
    pub canonical_path: Option<PathBuf>,
    pub primary: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceThreadMetadataSnapshot {
    pub thread_id: ConversationThreadId,
    pub execution_target: ExecutionTargetId,
    pub preview: String,
    #[serde(default)]
    pub backend_name: Option<String>,
    #[serde(default)]
    pub title: Option<ConversationThreadTitle>,
    #[serde(default)]
    pub member_binding: Option<ConversationThreadMemberBinding>,
    #[serde(default)]
    pub rebind_required: Option<ConversationThreadRebindRequirement>,
    pub created_at_millis: i64,
    pub updated_at_millis: i64,
    pub active: bool,
}

impl WorkspaceGraphToolService {
    pub fn read_workspace_state(
        &self,
        request: &WorkspaceStateReadRequest,
    ) -> Result<WorkspaceStateSnapshot, WorkspaceGraphToolError> {
        let manifest = self.load_workspace_manifest(&request.workspace_id)?;
        let state = self
            .persistence
            .load_workspace_state(&request.workspace_id)?;
        Ok(workspace_state_snapshot(manifest, &state))
    }
}

impl WorkspaceThreadMetadataSnapshot {
    fn from_registered_thread(
        thread: &RegisteredConversationThread,
        active_thread: Option<&ConversationThreadId>,
    ) -> Self {
        Self {
            thread_id: thread.thread_id().clone(),
            execution_target: thread.execution_target().clone(),
            preview: thread.preview().to_string(),
            backend_name: thread.backend_name().map(str::to_string),
            title: thread.gui_title().cloned(),
            member_binding: thread.member_binding().cloned(),
            rebind_required: thread.rebind_required().cloned(),
            created_at_millis: thread.created_at_millis(),
            updated_at_millis: thread.updated_at_millis(),
            active: active_thread == Some(thread.thread_id()),
        }
    }
}

fn workspace_state_snapshot(
    manifest: BerylWorkspaceManifest,
    state: &WorkspaceConversationState,
) -> WorkspaceStateSnapshot {
    WorkspaceStateSnapshot {
        manifest,
        selected_runtime: state.selected_runtime().cloned(),
        primary_member: workspace_primary_member_snapshot(state),
        available_members: workspace_available_member_snapshots(state),
        threads: state
            .threads()
            .iter()
            .map(|thread| {
                WorkspaceThreadMetadataSnapshot::from_registered_thread(
                    thread,
                    state.active_thread(),
                )
            })
            .collect(),
    }
}

fn workspace_primary_member_snapshot(
    state: &WorkspaceConversationState,
) -> WorkspacePrimaryMemberSnapshot {
    match state.primary_member() {
        Some(PrimaryWorkspaceMember::Explicit(member)) => {
            WorkspacePrimaryMemberSnapshot::Explicit {
                member_id: member.id().clone(),
                runtime: member.runtime_mode().clone(),
                canonical_path: member.canonical_path().to_path_buf(),
            }
        }
        Some(PrimaryWorkspaceMember::ImplicitHome(runtime)) => {
            WorkspacePrimaryMemberSnapshot::ImplicitHome {
                runtime: runtime.clone(),
            }
        }
        None => WorkspacePrimaryMemberSnapshot::None,
    }
}

fn workspace_available_member_snapshots(
    state: &WorkspaceConversationState,
) -> Vec<WorkspaceMemberSnapshot> {
    let Some(runtime) = state.selected_runtime().cloned() else {
        return Vec::new();
    };

    if !state.has_available_explicit_members() {
        return vec![WorkspaceMemberSnapshot {
            kind: WorkspaceMemberSnapshotKind::ImplicitHome,
            member_id: None,
            runtime,
            canonical_path: None,
            primary: true,
        }];
    }

    let primary_member_id = state.primary_explicit_member_id().cloned();
    state
        .available_explicit_members()
        .map(|member| WorkspaceMemberSnapshot {
            kind: WorkspaceMemberSnapshotKind::Explicit,
            member_id: Some(member.id().clone()),
            runtime: member.runtime_mode().clone(),
            canonical_path: Some(member.canonical_path().to_path_buf()),
            primary: primary_member_id.as_ref() == Some(member.id()),
        })
        .collect()
}
