use std::collections::BTreeMap;

use beryl_model::{CasItemId, CasThreadId};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CollabAgentTool {
    SpawnAgent,
    SendInput,
    ResumeAgent,
    Wait,
    CloseAgent,
}

impl CollabAgentTool {
    pub(crate) const fn as_wire_str(self) -> &'static str {
        match self {
            Self::SpawnAgent => "spawnAgent",
            Self::SendInput => "sendInput",
            Self::ResumeAgent => "resumeAgent",
            Self::Wait => "wait",
            Self::CloseAgent => "closeAgent",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CollabAgentToolCallStatus {
    InProgress,
    Completed,
    Failed,
}

impl CollabAgentToolCallStatus {
    pub(crate) const fn as_wire_str(self) -> &'static str {
        match self {
            Self::InProgress => "inProgress",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CollabAgentStatus {
    PendingInit,
    Running,
    Interrupted,
    Completed,
    Errored,
    Shutdown,
    NotFound,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollabAgentState {
    pub status: CollabAgentStatus,
    pub message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollabAgentToolCallItem {
    pub id: CasItemId,
    pub tool: CollabAgentTool,
    pub status: CollabAgentToolCallStatus,
    pub sender_thread_id: CasThreadId,
    pub receiver_thread_ids: Vec<CasThreadId>,
    pub prompt: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub agents_states: BTreeMap<String, CollabAgentState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubAgentActivityKind {
    Started,
    Interacted,
    Interrupted,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubAgentActivityItem {
    pub id: CasItemId,
    pub kind: SubAgentActivityKind,
    pub agent_thread_id: CasThreadId,
    pub agent_path: String,
}
