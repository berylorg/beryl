use std::path::Path;

use beryl_model::{CasThreadId, CasTurnId};
use serde::{Deserialize, Serialize};

use crate::{
    DynamicToolSpec,
    thread_lineage::{ThreadApprovalPolicy, ThreadSandboxMode},
};

use super::UserInput;

/// Exact accepted CAS turn identity without response-item materialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartedTurn {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    status: TurnStatus,
}

/// Exact active CAS turn identity returned after steering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SteeredTurn {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
}

#[derive(Deserialize)]
pub(crate) struct TurnStartResponseWire {
    turn: TurnControlWire,
}

#[derive(Deserialize)]
struct TurnControlWire {
    id: CasTurnId,
    status: TurnStatus,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TurnSteerResponseWire {
    turn_id: CasTurnId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurnStatus {
    Completed,
    Interrupted,
    Failed,
    InProgress,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadStartParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub ephemeral: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub developer_instructions: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dynamic_tools: Vec<DynamicToolSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_policy: Option<ThreadApprovalPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<ThreadSandboxMode>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TurnStartParams<'a> {
    pub thread_id: &'a CasThreadId,
    pub input: Vec<UserInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collaboration_mode: Option<TurnStartCollaborationMode>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TurnStartCollaborationMode {
    mode: TurnStartCollaborationModeKind,
    settings: TurnStartCollaborationModeSettings,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum TurnStartCollaborationModeKind {
    Default,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct TurnStartCollaborationModeSettings {
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    developer_instructions: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TurnSteerParams<'a> {
    pub thread_id: &'a CasThreadId,
    pub expected_turn_id: &'a CasTurnId,
    pub input: Vec<UserInput>,
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

impl SteeredTurn {
    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    #[must_use]
    pub const fn turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }
}

impl TurnStartResponseWire {
    pub(crate) fn into_started(self, thread_id: CasThreadId) -> StartedTurn {
        StartedTurn {
            thread_id,
            turn_id: self.turn.id,
            status: self.turn.status,
        }
    }
}

impl TurnSteerResponseWire {
    pub(crate) fn into_steered(self, thread_id: CasThreadId) -> SteeredTurn {
        SteeredTurn {
            thread_id,
            turn_id: self.turn_id,
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

impl ThreadStartParams {
    pub(crate) fn for_root(path: &Path, options: ThreadStartOptions) -> Self {
        Self {
            cwd: Some(path.display().to_string()),
            ephemeral: options.ephemeral,
            developer_instructions: options.developer_instructions,
            dynamic_tools: options.dynamic_tools,
            model: options.model,
            model_provider: options.model_provider,
            approval_policy: options.approval_policy,
            sandbox: options.sandbox,
        }
    }
}

impl<'a> TurnStartParams<'a> {
    pub(crate) fn text(
        thread_id: &'a CasThreadId,
        text: impl Into<String>,
        options: TurnStartOptions,
    ) -> Self {
        Self::input(thread_id, vec![UserInput::text(text)], options)
    }

    pub(crate) fn input(
        thread_id: &'a CasThreadId,
        input: Vec<UserInput>,
        options: TurnStartOptions,
    ) -> Self {
        Self {
            thread_id,
            input,
            model: options.model,
            effort: options.reasoning_effort,
            collaboration_mode: options
                .developer_instructions_context
                .map(TurnStartCollaborationMode::developer_instructions_context),
        }
    }
}

impl TurnStartCollaborationMode {
    fn developer_instructions_context(context: TurnDeveloperInstructionsContext) -> Self {
        Self {
            mode: TurnStartCollaborationModeKind::Default,
            settings: TurnStartCollaborationModeSettings {
                model: context.model,
                reasoning_effort: context.reasoning_effort,
                developer_instructions: context.developer_instructions,
            },
        }
    }
}

impl<'a> TurnSteerParams<'a> {
    pub(crate) fn input(
        thread_id: &'a CasThreadId,
        expected_turn_id: &'a CasTurnId,
        input: Vec<UserInput>,
    ) -> Self {
        Self {
            thread_id,
            expected_turn_id,
            input,
        }
    }
}

fn non_empty_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| (!value.is_empty()).then_some(value))
}
