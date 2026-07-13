use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::conversation::{ConversationThreadId, ConversationTurnId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProvenanceError {
    EmptyActor,
    EmptyToolName,
    EmptyInvocationId,
    EmptyWorkspaceAction,
    InvalidConfidencePercent { confidence_percent: u8 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MutationSource {
    ConversationTurn {
        thread_id: ConversationThreadId,
        turn_id: ConversationTurnId,
    },
    DynamicToolCall {
        thread_id: ConversationThreadId,
        turn_id: ConversationTurnId,
        tool_name: String,
        call_id: String,
    },
    ToolAction {
        tool_name: String,
        invocation_id: String,
    },
    WorkspaceAction {
        action: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationProvenance {
    actor: String,
    recorded_at_millis: u64,
    source: MutationSource,
    confidence_percent: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementProvenance {
    created: MutationProvenance,
    last_updated: MutationProvenance,
}

impl MutationSource {
    pub fn conversation_turn(thread_id: ConversationThreadId, turn_id: ConversationTurnId) -> Self {
        Self::ConversationTurn { thread_id, turn_id }
    }

    pub fn dynamic_tool_call(
        thread_id: ConversationThreadId,
        turn_id: ConversationTurnId,
        tool_name: impl Into<String>,
        call_id: impl Into<String>,
    ) -> Result<Self, ProvenanceError> {
        let tool_name = tool_name.into();
        if tool_name.trim().is_empty() {
            return Err(ProvenanceError::EmptyToolName);
        }

        let call_id = call_id.into();
        if call_id.trim().is_empty() {
            return Err(ProvenanceError::EmptyInvocationId);
        }

        Ok(Self::DynamicToolCall {
            thread_id,
            turn_id,
            tool_name,
            call_id,
        })
    }

    pub fn tool_action(
        tool_name: impl Into<String>,
        invocation_id: impl Into<String>,
    ) -> Result<Self, ProvenanceError> {
        let tool_name = tool_name.into();
        if tool_name.trim().is_empty() {
            return Err(ProvenanceError::EmptyToolName);
        }

        let invocation_id = invocation_id.into();
        if invocation_id.trim().is_empty() {
            return Err(ProvenanceError::EmptyInvocationId);
        }

        Ok(Self::ToolAction {
            tool_name,
            invocation_id,
        })
    }

    pub fn workspace_action(action: impl Into<String>) -> Result<Self, ProvenanceError> {
        let action = action.into();
        if action.trim().is_empty() {
            return Err(ProvenanceError::EmptyWorkspaceAction);
        }

        Ok(Self::WorkspaceAction { action })
    }
}

impl MutationProvenance {
    pub fn new(
        actor: impl Into<String>,
        recorded_at_millis: u64,
        source: MutationSource,
        confidence_percent: Option<u8>,
    ) -> Result<Self, ProvenanceError> {
        let actor = actor.into();
        if actor.trim().is_empty() {
            return Err(ProvenanceError::EmptyActor);
        }

        if let Some(confidence_percent) = confidence_percent {
            if confidence_percent > 100 {
                return Err(ProvenanceError::InvalidConfidencePercent { confidence_percent });
            }
        }

        Ok(Self {
            actor,
            recorded_at_millis,
            source,
            confidence_percent,
        })
    }

    pub fn actor(&self) -> &str {
        &self.actor
    }

    pub fn recorded_at_millis(&self) -> u64 {
        self.recorded_at_millis
    }

    pub fn source(&self) -> &MutationSource {
        &self.source
    }

    pub fn confidence_percent(&self) -> Option<u8> {
        self.confidence_percent
    }
}

impl ElementProvenance {
    pub fn new(created: MutationProvenance) -> Self {
        Self {
            created: created.clone(),
            last_updated: created,
        }
    }

    pub fn created(&self) -> &MutationProvenance {
        &self.created
    }

    pub fn last_updated(&self) -> &MutationProvenance {
        &self.last_updated
    }

    pub fn touch(&mut self, provenance: MutationProvenance) {
        self.last_updated = provenance;
    }
}

impl fmt::Display for ProvenanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyActor => write!(f, "provenance actor must not be empty"),
            Self::EmptyToolName => write!(f, "tool action provenance must include a tool name"),
            Self::EmptyInvocationId => {
                write!(f, "tool action provenance must include an invocation id")
            }
            Self::EmptyWorkspaceAction => {
                write!(f, "workspace action provenance must include an action name")
            }
            Self::InvalidConfidencePercent { confidence_percent } => write!(
                f,
                "confidence percent {confidence_percent} is outside the supported range 0..=100"
            ),
        }
    }
}

impl Error for ProvenanceError {}
