use std::{error::Error, fmt};

use super::WorkspaceConversationStateError;

impl fmt::Display for WorkspaceConversationStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RuntimeEnvironmentLocked => write!(
                f,
                "the workspace runtime environment cannot change while explicit workspace members are attached"
            ),
            Self::RuntimeEnvironmentNotSelected => {
                write!(
                    f,
                    "select a workspace runtime environment before attaching members"
                )
            }
            Self::MissingWorkspaceMember { member_id } => {
                write!(f, "workspace member {} is not attached", member_id.as_str())
            }
            Self::UnavailableWorkspaceMember { member_id } => {
                write!(
                    f,
                    "workspace member {} is unavailable and cannot be primary",
                    member_id.as_str()
                )
            }
            Self::MissingThread { thread_id } => {
                write!(
                    f,
                    "conversation thread {} is not registered",
                    thread_id.as_str()
                )
            }
            Self::EmptyThreadTitle => write!(f, "conversation thread title must not be empty"),
            Self::EmptyRebindRequirement => write!(f, "thread rebind detail must not be empty"),
            Self::WorkspaceMemberOverlap {
                existing_member_id,
                existing_path,
                candidate_path,
            } => write!(
                f,
                "workspace member {candidate_path} overlaps attached member {} at {existing_path}",
                existing_member_id.as_str()
            ),
        }
    }
}

impl Error for WorkspaceConversationStateError {}
