use beryl_model::{CasThreadId, CasTurnId};
use serde::{Deserialize, Serialize};

use crate::{
    DynamicToolSpec,
    thread_lineage::{ThreadApprovalPolicy, ThreadSandboxMode},
};

/// Exact accepted CAS turn identity without response-item materialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartedTurn {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    status: TurnStatus,
}

/// Compact bounded `turn/start` result retained by the incremental response decoder.
#[doc(hidden)]
#[derive(Debug, PartialEq, Eq)]
pub struct TurnStartResponseWire {
    turn: TurnControlWire,
}

#[derive(Debug, PartialEq, Eq)]
struct TurnControlWire {
    id: CasTurnId,
    status: TurnStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurnStatus {
    Completed,
    Interrupted,
    Failed,
    InProgress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TerminalNonSteerableTurnKind {
    Review,
    Compact,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CodexErrorInfo {
    ContextWindowExceeded,
    SessionBudgetExceeded,
    UsageLimitExceeded,
    ServerOverloaded,
    CyberPolicy,
    HttpConnectionFailed {
        http_status_code: Option<u16>,
    },
    ResponseStreamConnectionFailed {
        http_status_code: Option<u16>,
    },
    InternalServerError,
    Unauthorized,
    BadRequest,
    ThreadRollbackFailed,
    SandboxError,
    ResponseStreamDisconnected {
        http_status_code: Option<u16>,
    },
    ResponseTooManyFailedAttempts {
        http_status_code: Option<u16>,
    },
    ActiveTurnNotSteerable {
        turn_kind: TerminalNonSteerableTurnKind,
    },
    Other,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadStartOptions {
    ephemeral: bool,
    developer_instructions: Option<String>,
    dynamic_tools: Vec<DynamicToolSpec>,
    model: Option<String>,
    model_provider: Option<String>,
    approval_policy: Option<ThreadApprovalPolicy>,
    sandbox: Option<ThreadSandboxMode>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TurnStartOptions {
    model: Option<String>,
    reasoning_effort: Option<String>,
    developer_instructions_context: Option<TurnDeveloperInstructionsContext>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnDeveloperInstructionsContext {
    developer_instructions: Option<String>,
    model: String,
    reasoning_effort: Option<String>,
}

impl StartedTurn {
    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    #[must_use]
    pub const fn turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }

    #[must_use]
    pub const fn status(&self) -> &TurnStatus {
        &self.status
    }
}

impl TurnStartResponseWire {
    pub(crate) fn try_new(turn_id: &str, status: TurnStatus) -> Option<Self> {
        Some(Self {
            turn: TurnControlWire {
                id: CasTurnId::new(turn_id).ok()?,
                status,
            },
        })
    }

    #[doc(hidden)]
    #[must_use]
    pub const fn turn_id(&self) -> &CasTurnId {
        &self.turn.id
    }

    #[doc(hidden)]
    #[must_use]
    pub const fn status(&self) -> &TurnStatus {
        &self.turn.status
    }

    pub(crate) fn into_started(self, thread_id: CasThreadId) -> StartedTurn {
        StartedTurn {
            thread_id,
            turn_id: self.turn.id,
            status: self.turn.status,
        }
    }
}

impl ThreadStartOptions {
    #[must_use]
    pub fn persistent() -> Self {
        Self {
            ephemeral: false,
            developer_instructions: None,
            dynamic_tools: Vec::new(),
            model: None,
            model_provider: None,
            approval_policy: None,
            sandbox: None,
        }
    }

    #[must_use]
    pub fn ephemeral() -> Self {
        Self {
            ephemeral: true,
            developer_instructions: None,
            dynamic_tools: Vec::new(),
            model: None,
            model_provider: None,
            approval_policy: None,
            sandbox: None,
        }
    }

    #[must_use]
    pub fn with_developer_instructions(
        mut self,
        developer_instructions: impl Into<String>,
    ) -> Self {
        self.developer_instructions = Some(developer_instructions.into());
        self
    }

    #[must_use]
    pub fn with_dynamic_tool(mut self, tool: DynamicToolSpec) -> Self {
        self.dynamic_tools.push(tool);
        self
    }

    #[must_use]
    pub fn with_dynamic_tools(mut self, tools: Vec<DynamicToolSpec>) -> Self {
        self.dynamic_tools = tools;
        self
    }

    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = non_empty_string(Some(model.into()));
        self
    }

    #[must_use]
    pub fn with_model_provider(mut self, provider: impl Into<String>) -> Self {
        self.model_provider = non_empty_string(Some(provider.into()));
        self
    }

    #[must_use]
    pub fn with_approval_policy(mut self, policy: ThreadApprovalPolicy) -> Self {
        self.approval_policy = Some(policy);
        self
    }

    #[must_use]
    pub fn with_sandbox(mut self, sandbox: ThreadSandboxMode) -> Self {
        self.sandbox = Some(sandbox);
        self
    }

    #[must_use]
    pub const fn is_ephemeral(&self) -> bool {
        self.ephemeral
    }

    #[must_use]
    pub fn dynamic_tools(&self) -> &[DynamicToolSpec] {
        &self.dynamic_tools
    }

    #[must_use]
    pub fn developer_instructions(&self) -> Option<&str> {
        self.developer_instructions.as_deref()
    }

    #[must_use]
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    #[must_use]
    pub fn model_provider(&self) -> Option<&str> {
        self.model_provider.as_deref()
    }

    #[must_use]
    pub const fn approval_policy(&self) -> Option<ThreadApprovalPolicy> {
        self.approval_policy
    }

    #[must_use]
    pub const fn sandbox(&self) -> Option<ThreadSandboxMode> {
        self.sandbox
    }
}

impl TurnStartOptions {
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = non_empty_string(Some(model.into()));
        self
    }

    #[must_use]
    pub fn with_reasoning_effort(mut self, reasoning_effort: impl Into<String>) -> Self {
        self.reasoning_effort = non_empty_string(Some(reasoning_effort.into()));
        self
    }

    #[must_use]
    pub fn with_developer_instructions_context(
        mut self,
        developer_instructions: Option<String>,
        model: impl Into<String>,
        reasoning_effort: Option<String>,
    ) -> Self {
        self.developer_instructions_context =
            TurnDeveloperInstructionsContext::new(developer_instructions, model, reasoning_effort);
        self
    }

    #[must_use]
    pub fn without_developer_instructions_context(mut self) -> Self {
        self.developer_instructions_context = None;
        self
    }

    #[must_use]
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    #[must_use]
    pub fn reasoning_effort(&self) -> Option<&str> {
        self.reasoning_effort.as_deref()
    }

    #[must_use]
    pub const fn developer_instructions_context(
        &self,
    ) -> Option<&TurnDeveloperInstructionsContext> {
        self.developer_instructions_context.as_ref()
    }
}

impl TurnDeveloperInstructionsContext {
    fn new(
        developer_instructions: Option<String>,
        model: impl Into<String>,
        reasoning_effort: Option<String>,
    ) -> Option<Self> {
        let model = non_empty_string(Some(model.into()))?;
        Some(Self {
            developer_instructions: developer_instructions
                .and_then(|value| (!value.trim().is_empty()).then_some(value)),
            model,
            reasoning_effort: non_empty_string(reasoning_effort),
        })
    }

    #[must_use]
    pub fn developer_instructions(&self) -> Option<&str> {
        self.developer_instructions.as_deref()
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub fn reasoning_effort(&self) -> Option<&str> {
        self.reasoning_effort.as_deref()
    }
}

fn non_empty_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| (!value.is_empty()).then_some(value))
}
