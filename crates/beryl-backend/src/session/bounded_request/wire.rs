use std::path::Path;

use beryl_model::{CasThreadId, CasTurnId};
use serde::Serialize;

use crate::{
    DynamicToolSpec, ModelListOptions, ThreadApprovalPolicy, ThreadLoadOptions, ThreadSandboxMode,
    ThreadStartOptions, incoming_json::ResponseFamily,
};

pub(super) trait RequestSpec: Serialize {
    fn response_family(&self) -> ResponseFamily;
}

#[derive(Serialize)]
pub(super) struct JsonRpcRequest<'a, P: RequestSpec> {
    jsonrpc: &'static str,
    id: u64,
    method: &'static str,
    params: &'a P,
}

impl<'a, P: RequestSpec> JsonRpcRequest<'a, P> {
    pub(super) fn new(id: u64, params: &'a P) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: params.response_family().method(),
            params,
        }
    }
}

#[derive(Serialize)]
pub(super) struct InitializedNotification {
    jsonrpc: &'static str,
    method: &'static str,
}

impl InitializedNotification {
    pub(super) const fn new() -> Self {
        Self {
            jsonrpc: "2.0",
            method: "initialized",
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InitializeParams {
    client_info: ClientInfo,
    capabilities: InitializeCapabilities,
}

#[derive(Serialize)]
struct ClientInfo {
    name: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InitializeCapabilities {
    experimental_api: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    opt_out_notification_methods: Option<&'static [&'static str]>,
}

impl InitializeParams {
    pub(super) const fn foreground() -> Self {
        Self::new(None)
    }

    pub(super) const fn request_only() -> Self {
        Self::new(Some(REQUEST_ONLY_NOTIFICATION_OPT_OUTS))
    }

    const fn new(opt_out_notification_methods: Option<&'static [&'static str]>) -> Self {
        Self {
            client_info: ClientInfo {
                name: "beryl",
                version: env!("CARGO_PKG_VERSION"),
            },
            capabilities: InitializeCapabilities {
                experimental_api: true,
                opt_out_notification_methods,
            },
        }
    }
}

impl RequestSpec for InitializeParams {
    fn response_family(&self) -> ResponseFamily {
        ResponseFamily::Initialize
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ConfigReadParams<'a> {
    cwd: &'a Path,
    include_layers: bool,
}

impl<'a> ConfigReadParams<'a> {
    pub(super) const fn new(cwd: &'a Path) -> Self {
        Self {
            cwd,
            include_layers: false,
        }
    }
}

impl RequestSpec for ConfigReadParams<'_> {
    fn response_family(&self) -> ResponseFamily {
        ResponseFamily::ConfigRead
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ModelListParams<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<&'a str>,
    limit: u8,
    include_hidden: bool,
}

impl<'a> ModelListParams<'a> {
    pub(super) fn new(options: &'a ModelListOptions) -> Self {
        Self {
            cursor: options.cursor(),
            limit: options.limit().get(),
            include_hidden: options.includes_hidden(),
        }
    }
}

impl RequestSpec for ModelListParams<'_> {
    fn response_family(&self) -> ResponseFamily {
        ResponseFamily::ModelList
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ThreadStartParams<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_provider: Option<&'a str>,
    cwd: &'a Path,
    #[serde(skip_serializing_if = "Option::is_none")]
    approval_policy: Option<ThreadApprovalPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sandbox: Option<ThreadSandboxMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    developer_instructions: Option<&'a str>,
    ephemeral: bool,
    #[serde(skip_serializing_if = "slice_is_empty")]
    dynamic_tools: &'a [DynamicToolSpec],
}

impl<'a> ThreadStartParams<'a> {
    pub(super) fn new(cwd: &'a Path, options: &'a ThreadStartOptions) -> Self {
        Self {
            model: options.model(),
            model_provider: options.model_provider(),
            cwd,
            approval_policy: options.approval_policy(),
            sandbox: options.sandbox(),
            developer_instructions: options.developer_instructions(),
            ephemeral: options.is_ephemeral(),
            dynamic_tools: options.dynamic_tools(),
        }
    }
}

impl RequestSpec for ThreadStartParams<'_> {
    fn response_family(&self) -> ResponseFamily {
        ResponseFamily::ThreadStart
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ThreadReadParams<'a> {
    thread_id: &'a CasThreadId,
    include_turns: bool,
}

impl<'a> ThreadReadParams<'a> {
    pub(super) const fn new(thread_id: &'a CasThreadId) -> Self {
        Self {
            thread_id,
            include_turns: false,
        }
    }
}

impl RequestSpec for ThreadReadParams<'_> {
    fn response_family(&self) -> ResponseFamily {
        ResponseFamily::ThreadRead
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ThreadUnsubscribeParams<'a> {
    thread_id: &'a CasThreadId,
}

impl<'a> ThreadUnsubscribeParams<'a> {
    pub(super) const fn new(thread_id: &'a CasThreadId) -> Self {
        Self { thread_id }
    }
}

impl RequestSpec for ThreadUnsubscribeParams<'_> {
    fn response_family(&self) -> ResponseFamily {
        ResponseFamily::ThreadUnsubscribe
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ThreadResumeParams<'a> {
    thread_id: &'a CasThreadId,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_provider: Option<&'a str>,
    cwd: &'a Path,
    #[serde(skip_serializing_if = "Option::is_none")]
    approval_policy: Option<ThreadApprovalPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sandbox: Option<ThreadSandboxMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    developer_instructions: Option<&'a str>,
    exclude_turns: bool,
}

impl<'a> ThreadResumeParams<'a> {
    pub(super) fn new(thread_id: &'a CasThreadId, options: &'a ThreadLoadOptions) -> Self {
        Self {
            thread_id,
            model: options.model(),
            model_provider: options.model_provider(),
            cwd: options.cwd(),
            approval_policy: options.approval_policy(),
            sandbox: options.sandbox(),
            developer_instructions: options.developer_instructions(),
            exclude_turns: true,
        }
    }
}

impl RequestSpec for ThreadResumeParams<'_> {
    fn response_family(&self) -> ResponseFamily {
        ResponseFamily::ThreadResume
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ThreadForkParams<'a> {
    thread_id: &'a CasThreadId,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_turn_id: Option<&'a CasTurnId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_provider: Option<&'a str>,
    cwd: &'a Path,
    #[serde(skip_serializing_if = "Option::is_none")]
    approval_policy: Option<ThreadApprovalPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sandbox: Option<ThreadSandboxMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    developer_instructions: Option<&'a str>,
    ephemeral: bool,
    exclude_turns: bool,
}

impl<'a> ThreadForkParams<'a> {
    pub(super) fn full(thread_id: &'a CasThreadId, options: &'a ThreadLoadOptions) -> Self {
        Self::new(thread_id, None, options)
    }

    pub(super) fn through_turn(
        thread_id: &'a CasThreadId,
        last_turn_id: &'a CasTurnId,
        options: &'a ThreadLoadOptions,
    ) -> Self {
        Self::new(thread_id, Some(last_turn_id), options)
    }

    fn new(
        thread_id: &'a CasThreadId,
        last_turn_id: Option<&'a CasTurnId>,
        options: &'a ThreadLoadOptions,
    ) -> Self {
        Self {
            thread_id,
            last_turn_id,
            model: options.model(),
            model_provider: options.model_provider(),
            cwd: options.cwd(),
            approval_policy: options.approval_policy(),
            sandbox: options.sandbox(),
            developer_instructions: options.developer_instructions(),
            ephemeral: false,
            exclude_turns: true,
        }
    }
}

impl RequestSpec for ThreadForkParams<'_> {
    fn response_family(&self) -> ResponseFamily {
        ResponseFamily::ThreadFork
    }
}

const REQUEST_ONLY_NOTIFICATION_OPT_OUTS: &[&str] = &[
    "thread/started",
    "thread/status/changed",
    "thread/closed",
    "thread/name/updated",
    "thread/tokenUsage/updated",
    "account/rateLimits/updated",
    "turn/started",
    "turn/completed",
    "turn/diff/updated",
    "item/started",
    "item/completed",
    "item/agentMessage/delta",
    "item/plan/delta",
    "item/reasoning/summaryPartAdded",
    "item/reasoning/summaryTextDelta",
    "item/reasoning/textDelta",
    "item/commandExecution/outputDelta",
    "item/fileChange/outputDelta",
    "item/fileChange/patchUpdated",
    "item/mcpToolCall/progress",
    "codex/event/collab_agent_spawn_end",
];

fn slice_is_empty<T>(values: &&[T]) -> bool {
    values.is_empty()
}
