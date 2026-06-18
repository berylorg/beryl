use std::path::PathBuf;

use serde::Deserialize;

use crate::workspace::{
    RuntimeMode, WorkspaceMember, WorkspaceMemberAvailability, WorkspaceMemberId,
};

use super::{ConversationThreadId, RegisteredConversationThread, WorkspaceConversationState};

#[derive(Deserialize)]
struct WorkspaceConversationStateWire {
    #[serde(default, alias = "selected_runtime")]
    default_runtime: Option<RuntimeMode>,
    #[serde(default)]
    explicit_members: Vec<WorkspaceMemberWire>,
    #[serde(default)]
    primary_explicit_member_id: Option<WorkspaceMemberId>,
    #[serde(default)]
    next_member_number: u64,
    #[serde(default)]
    threads: Vec<RegisteredConversationThread>,
    #[serde(default)]
    active_thread: Option<ConversationThreadId>,
}

#[derive(Deserialize)]
struct WorkspaceMemberWire {
    id: WorkspaceMemberId,
    #[serde(default)]
    runtime_mode: Option<RuntimeMode>,
    canonical_path: PathBuf,
    #[serde(default)]
    availability: Option<WorkspaceMemberAvailability>,
    #[serde(default)]
    available: Option<bool>,
}

impl<'de> Deserialize<'de> for WorkspaceConversationState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = WorkspaceConversationStateWire::deserialize(deserializer)?;
        let default_runtime = wire.default_runtime;
        let explicit_members = wire
            .explicit_members
            .into_iter()
            .map(|member| {
                let runtime_mode = member
                    .runtime_mode
                    .or_else(|| default_runtime.clone())
                    .ok_or_else(|| {
                        serde::de::Error::custom(format!(
                            "workspace member {} at {} is missing a runtime mode",
                            member.id.as_str(),
                            member.canonical_path.display()
                        ))
                    })?;
                let availability = member.availability.unwrap_or_else(|| {
                    if member.available == Some(false) {
                        WorkspaceMemberAvailability::PathNotFound
                    } else {
                        WorkspaceMemberAvailability::Available
                    }
                });

                Ok(WorkspaceMember::new_with_availability(
                    member.id,
                    runtime_mode,
                    member.canonical_path,
                    availability,
                ))
            })
            .collect::<Result<Vec<_>, D::Error>>()?;

        let mut state = WorkspaceConversationState {
            default_runtime,
            explicit_members,
            primary_explicit_member_id: wire.primary_explicit_member_id,
            next_member_number: wire.next_member_number,
            threads: wire.threads,
            active_thread: wire.active_thread,
        };
        state.normalize_unavailable_primary_after_deserialize();
        state.normalize_active_thread_after_deserialize();
        Ok(state)
    }
}

impl WorkspaceConversationState {
    fn normalize_unavailable_primary_after_deserialize(&mut self) {
        let Some(primary_id) = self.primary_explicit_member_id.as_ref() else {
            return;
        };
        let primary_available = self
            .explicit_members
            .iter()
            .any(|member| member.id() == primary_id && member.is_available());
        if primary_available {
            return;
        }

        self.primary_explicit_member_id = self
            .explicit_members
            .iter()
            .find(|member| member.is_available())
            .map(|member| member.id().clone());
    }
}
