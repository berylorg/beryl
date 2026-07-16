use beryl_model::CasThreadId;

use super::super::ProviderTextV1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderCollabToolV1 {
    SpawnAgent,
    SendInput,
    ResumeAgent,
    Wait,
    CloseAgent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderCollabToolStatusV1 {
    InProgress,
    Completed,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderCollabAgentStatusV1 {
    PendingInit,
    Running,
    Interrupted,
    Completed,
    Errored,
    Shutdown,
    NotFound,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderCollabAgentStateV1 {
    pub status: ProviderCollabAgentStatusV1,
    pub message: Option<ProviderTextV1>,
}

/// One entry from the normalized ordered agent-state map.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderCollabAgentStateEntryV1 {
    pub agent: ProviderTextV1,
    pub state: ProviderCollabAgentStateV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderCollabAgentToolCallV1 {
    pub tool: ProviderCollabToolV1,
    pub status: ProviderCollabToolStatusV1,
    pub sender_thread_id: CasThreadId,
    pub receiver_thread_ids: Vec<CasThreadId>,
    pub prompt: Option<ProviderTextV1>,
    pub model: Option<ProviderTextV1>,
    pub reasoning_effort: Option<ProviderTextV1>,
    pub agents_states: Vec<ProviderCollabAgentStateEntryV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderSubAgentActivityKindV1 {
    Started,
    Interacted,
    Interrupted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderSubAgentActivityV1 {
    pub kind: ProviderSubAgentActivityKindV1,
    pub agent_thread_id: CasThreadId,
    pub agent_path: ProviderTextV1,
}
