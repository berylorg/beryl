use serde::{Deserialize, Serialize};

use crate::workspace::{RuntimeMode, WorkspaceId, WorkspaceMemberId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConversationThreadTitleSource {
    BackendMetadata,
    FirstCompletedTurn,
    Manual,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationThreadTitle {
    text: String,
    source: ConversationThreadTitleSource,
    recorded_at_millis: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConversationThreadMemberBinding {
    Explicit {
        member_id: WorkspaceMemberId,
        execution_target: WorkspaceId,
    },
    ImplicitHome {
        execution_target: WorkspaceId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationThreadRebindRequirement {
    detail: String,
}

impl ConversationThreadTitle {
    pub fn new(
        text: impl Into<String>,
        source: ConversationThreadTitleSource,
        recorded_at_millis: u64,
    ) -> Option<Self> {
        let text = text.into().trim().to_string();
        if text.is_empty() {
            return None;
        }

        Some(Self {
            text,
            source,
            recorded_at_millis,
        })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn source(&self) -> ConversationThreadTitleSource {
        self.source
    }

    pub fn recorded_at_millis(&self) -> u64 {
        self.recorded_at_millis
    }
}

impl ConversationThreadMemberBinding {
    pub fn explicit(member_id: WorkspaceMemberId, execution_target: WorkspaceId) -> Self {
        Self::Explicit {
            member_id,
            execution_target,
        }
    }

    pub fn implicit_home(execution_target: WorkspaceId) -> Self {
        Self::ImplicitHome { execution_target }
    }

    pub fn execution_target(&self) -> &WorkspaceId {
        match self {
            Self::Explicit {
                execution_target, ..
            }
            | Self::ImplicitHome { execution_target } => execution_target,
        }
    }

    pub fn runtime_mode(&self) -> &RuntimeMode {
        self.execution_target().runtime_mode()
    }

    pub fn explicit_member_id(&self) -> Option<&WorkspaceMemberId> {
        match self {
            Self::Explicit { member_id, .. } => Some(member_id),
            Self::ImplicitHome { .. } => None,
        }
    }

    pub fn is_implicit_home(&self) -> bool {
        matches!(self, Self::ImplicitHome { .. })
    }
}

impl ConversationThreadRebindRequirement {
    pub fn new(detail: impl Into<String>) -> Option<Self> {
        let detail = detail.into().trim().to_string();
        if detail.is_empty() {
            return None;
        }

        Some(Self { detail })
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }
}
