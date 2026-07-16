use beryl_model::{CasItemId, CasThreadId, CasTurnId};

use super::{FileUpdateChange, ThreadItemKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemDeltaPayload {
    AgentMessage { delta: String },
    Plan { delta: String },
    ReasoningSummaryPartAdded { summary_index: usize },
    ReasoningSummaryText { summary_index: usize, delta: String },
    ReasoningTextObserved { content_index: usize },
    CommandExecutionOutput { delta: String },
    FileChangeOutput { delta: String },
    FileChangePatchUpdated { changes: Vec<FileUpdateChange> },
    McpToolCallProgress { message: String },
}

impl ItemDeltaPayload {
    #[must_use]
    pub const fn expected_item_kind(&self) -> ThreadItemKind {
        match self {
            Self::AgentMessage { .. } => ThreadItemKind::AgentMessage,
            Self::Plan { .. } => ThreadItemKind::Plan,
            Self::ReasoningSummaryPartAdded { .. }
            | Self::ReasoningSummaryText { .. }
            | Self::ReasoningTextObserved { .. } => ThreadItemKind::Reasoning,
            Self::CommandExecutionOutput { .. } => ThreadItemKind::CommandExecution,
            Self::FileChangeOutput { .. } | Self::FileChangePatchUpdated { .. } => {
                ThreadItemKind::FileChange
            }
            Self::McpToolCallProgress { .. } => ThreadItemKind::McpToolCall,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemDelta {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    item_id: CasItemId,
    expected_item_kind: ThreadItemKind,
    payload: ItemDeltaPayload,
}

impl ItemDelta {
    pub(crate) fn new(
        thread_id: CasThreadId,
        turn_id: CasTurnId,
        item_id: CasItemId,
        payload: ItemDeltaPayload,
    ) -> Self {
        let expected_item_kind = payload.expected_item_kind();
        Self {
            thread_id,
            turn_id,
            item_id,
            expected_item_kind,
            payload,
        }
    }

    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    #[must_use]
    pub const fn turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }

    #[must_use]
    pub const fn item_id(&self) -> &CasItemId {
        &self.item_id
    }

    #[must_use]
    pub const fn expected_item_kind(&self) -> ThreadItemKind {
        self.expected_item_kind
    }

    #[must_use]
    pub const fn payload(&self) -> &ItemDeltaPayload {
        &self.payload
    }

    #[must_use]
    pub fn into_payload(self) -> ItemDeltaPayload {
        self.payload
    }
}
