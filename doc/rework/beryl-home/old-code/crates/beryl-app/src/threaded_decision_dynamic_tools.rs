use beryl_backend::{DynamicToolCallRequest, DynamicToolCallResponse, DynamicToolSpec};
use beryl_model::{
    semantic_graph::{SemanticGraphIdError, SemanticNodeId},
    threaded_decision::ThreadedDecisionOutcome,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    dynamic_tools::BERYL_DYNAMIC_TOOL_NAMESPACE,
    threaded_decision_branch_core::{
        MAX_TOPIC_DECISION_ITEM_SUMMARY_CHARS, MAX_TOPIC_DECISION_ITEM_TITLE_CHARS,
    },
};

mod schema;

use schema::{
    resolve_decision_branch_schema, start_decision_branch_schema, start_topic_decision_schema,
};

pub const START_DECISION_BRANCH_TOOL: &str = "start_decision_branch";
pub const START_TOPIC_DECISION_TOOL: &str = "start_topic_decision";
pub const RESOLVE_DECISION_BRANCH_TOOL: &str = "resolve_decision_branch";
const MAX_DECISION_BRANCH_TOOL_ITEMS: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThreadedDecisionDynamicToolRequest {
    StartDecisionBranch {
        checklist_item_ids: Vec<SemanticNodeId>,
    },
    StartTopicDecision {
        topic_node_id: SemanticNodeId,
        title: String,
        summary: String,
    },
    ResolveDecisionBranch {
        outcome: ThreadedDecisionOutcome,
        summary: String,
        handoff_message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionBranchToolItemResult {
    pub checklist_item_id: SemanticNodeId,
    pub status: DecisionBranchToolItemStatus,
    pub record_id: Option<String>,
    pub parent_thread_id: Option<String>,
    pub branch_point_turn_id: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionResolutionToolResult {
    pub record_id: String,
    pub checklist_item_id: String,
    pub parent_thread_id: String,
    pub child_thread_id: String,
    pub status: &'static str,
    pub message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopicDecisionToolResult {
    pub topic_node_id: SemanticNodeId,
    pub status: DecisionBranchToolItemStatus,
    pub checklist_item_id: Option<SemanticNodeId>,
    pub record_id: Option<String>,
    pub parent_thread_id: Option<String>,
    pub branch_point_turn_id: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionBranchToolItemStatus {
    Queued,
    Failed,
}

#[derive(Debug)]
pub struct ThreadedDecisionDynamicToolError {
    kind: &'static str,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartDecisionBranchArguments {
    checklist_item_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartTopicDecisionArguments {
    topic_node_id: String,
    title: String,
    #[serde(default)]
    summary: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResolveDecisionBranchArguments {
    outcome: String,
    summary: String,
    handoff_message: String,
}

pub fn beryl_threaded_decision_dynamic_tool_specs() -> Vec<DynamicToolSpec> {
    vec![
        DynamicToolSpec::new(
            START_DECISION_BRANCH_TOOL,
            "Queue decision-branch creation for explicit checklist-item ids after this parent turn reaches a terminal idle state.",
            start_decision_branch_schema(),
        )
        .with_namespace(BERYL_DYNAMIC_TOOL_NAMESPACE)
        .with_defer_loading(false),
        DynamicToolSpec::new(
            START_TOPIC_DECISION_TOOL,
            "Queue one topic-scoped decision by creating or reusing a decision checklist-item child under an exact topic node, then starting its decision branch after this parent turn reaches a terminal idle state.",
            start_topic_decision_schema(),
        )
        .with_namespace(BERYL_DYNAMIC_TOOL_NAMESPACE)
        .with_defer_loading(false),
        DynamicToolSpec::new(
            RESOLVE_DECISION_BRANCH_TOOL,
            "Resolve the active decision child thread with an accepted or rejected outcome and queue a parent-thread handoff.",
            resolve_decision_branch_schema(),
        )
        .with_namespace(BERYL_DYNAMIC_TOOL_NAMESPACE)
        .with_defer_loading(false),
    ]
}

pub fn is_beryl_threaded_decision_dynamic_tool(request: &DynamicToolCallRequest) -> bool {
    request
        .namespace()
        .is_none_or(|namespace| namespace == BERYL_DYNAMIC_TOOL_NAMESPACE)
        && matches!(
            request.tool(),
            START_DECISION_BRANCH_TOOL | START_TOPIC_DECISION_TOOL | RESOLVE_DECISION_BRANCH_TOOL
        )
}

pub fn parse_beryl_threaded_decision_dynamic_tool_request(
    request: &DynamicToolCallRequest,
) -> Result<ThreadedDecisionDynamicToolRequest, ThreadedDecisionDynamicToolError> {
    validate_namespace(request)?;
    match request.tool() {
        START_DECISION_BRANCH_TOOL => {
            let arguments: StartDecisionBranchArguments = parse_arguments(request.arguments())?;
            if arguments.checklist_item_ids.is_empty() {
                return Err(ThreadedDecisionDynamicToolError::invalid_field(
                    "checklistItemIds",
                    "must include at least one checklist item id",
                ));
            }
            if arguments.checklist_item_ids.len() > MAX_DECISION_BRANCH_TOOL_ITEMS {
                return Err(ThreadedDecisionDynamicToolError::invalid_field(
                    "checklistItemIds",
                    format!(
                        "contains {} ids, exceeding the supported limit {}",
                        arguments.checklist_item_ids.len(),
                        MAX_DECISION_BRANCH_TOOL_ITEMS
                    ),
                ));
            }
            let checklist_item_ids = arguments
                .checklist_item_ids
                .into_iter()
                .map(|value| {
                    SemanticNodeId::new(value)
                        .map_err(|source| invalid_graph_id("checklistItemIds", source))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ThreadedDecisionDynamicToolRequest::StartDecisionBranch { checklist_item_ids })
        }
        START_TOPIC_DECISION_TOOL => {
            let arguments: StartTopicDecisionArguments = parse_arguments(request.arguments())?;
            let topic_node_id = SemanticNodeId::new(arguments.topic_node_id)
                .map_err(|source| invalid_graph_id("topicNodeId", source))?;
            let title = normalize_limited_text(
                "title",
                arguments.title,
                1,
                MAX_TOPIC_DECISION_ITEM_TITLE_CHARS,
            )?;
            let summary = normalize_limited_text(
                "summary",
                arguments.summary,
                0,
                MAX_TOPIC_DECISION_ITEM_SUMMARY_CHARS,
            )?;
            Ok(ThreadedDecisionDynamicToolRequest::StartTopicDecision {
                topic_node_id,
                title,
                summary,
            })
        }
        RESOLVE_DECISION_BRANCH_TOOL => {
            let arguments: ResolveDecisionBranchArguments = parse_arguments(request.arguments())?;
            let outcome = parse_resolution_outcome(&arguments.outcome)?;
            let summary = normalize_required_text("summary", arguments.summary)?;
            let handoff_message =
                normalize_required_text("handoffMessage", arguments.handoff_message)?;
            Ok(ThreadedDecisionDynamicToolRequest::ResolveDecisionBranch {
                outcome,
                summary,
                handoff_message,
            })
        }
        tool => Err(ThreadedDecisionDynamicToolError::new(
            "unsupported_tool",
            format!("unsupported Beryl threaded-decision dynamic tool {tool:?}"),
        )),
    }
}

pub fn decision_resolution_tool_success_response(
    result: DecisionResolutionToolResult,
) -> DynamicToolCallResponse {
    DynamicToolCallResponse::success_text(compact_json(json!({
        "ok": true,
        "result": {
            "status": result.status,
            "recordId": result.record_id,
            "checklistItemId": result.checklist_item_id,
            "parentThreadId": result.parent_thread_id,
            "childThreadId": result.child_thread_id,
            "message": result.message,
        }
    })))
}

pub fn topic_decision_tool_success_response(
    result: TopicDecisionToolResult,
) -> DynamicToolCallResponse {
    DynamicToolCallResponse::success_text(compact_json(json!({
        "ok": true,
        "result": {
            "topicNodeId": result.topic_node_id.as_str(),
            "status": result.status.as_str(),
            "checklistItemId": result.checklist_item_id.as_ref().map(|id| id.as_str()),
            "recordId": result.record_id,
            "parentThreadId": result.parent_thread_id,
            "branchPointTurnId": result.branch_point_turn_id,
            "message": result.message,
        }
    })))
}

pub fn decision_branch_tool_success_response(
    results: Vec<DecisionBranchToolItemResult>,
) -> DynamicToolCallResponse {
    let branches = results
        .into_iter()
        .map(|result| {
            json!({
                "checklistItemId": result.checklist_item_id.as_str(),
                "status": result.status.as_str(),
                "recordId": result.record_id,
                "parentThreadId": result.parent_thread_id,
                "branchPointTurnId": result.branch_point_turn_id,
                "message": result.message,
            })
        })
        .collect::<Vec<_>>();
    DynamicToolCallResponse::success_text(compact_json(json!({
        "ok": true,
        "result": {
            "branches": branches
        }
    })))
}

pub fn threaded_decision_tool_failure_response(
    request: &DynamicToolCallRequest,
    error: ThreadedDecisionDynamicToolError,
) -> DynamicToolCallResponse {
    DynamicToolCallResponse::failure_text(compact_json(json!({
        "ok": false,
        "error": {
            "kind": error.kind(),
            "message": error.to_string(),
            "tool": request.tool(),
            "callId": request.call_id(),
        }
    })))
}

impl DecisionBranchToolItemResult {
    pub fn queued(
        checklist_item_id: SemanticNodeId,
        record_id: impl Into<String>,
        parent_thread_id: impl Into<String>,
        branch_point_turn_id: impl Into<String>,
    ) -> Self {
        Self {
            checklist_item_id,
            status: DecisionBranchToolItemStatus::Queued,
            record_id: Some(record_id.into()),
            parent_thread_id: Some(parent_thread_id.into()),
            branch_point_turn_id: Some(branch_point_turn_id.into()),
            message: None,
        }
    }

    pub fn failed(checklist_item_id: SemanticNodeId, message: impl Into<String>) -> Self {
        Self {
            checklist_item_id,
            status: DecisionBranchToolItemStatus::Failed,
            record_id: None,
            parent_thread_id: None,
            branch_point_turn_id: None,
            message: Some(message.into()),
        }
    }
}

impl TopicDecisionToolResult {
    pub fn queued(
        topic_node_id: SemanticNodeId,
        checklist_item_id: SemanticNodeId,
        record_id: impl Into<String>,
        parent_thread_id: impl Into<String>,
        branch_point_turn_id: impl Into<String>,
    ) -> Self {
        Self {
            topic_node_id,
            status: DecisionBranchToolItemStatus::Queued,
            checklist_item_id: Some(checklist_item_id),
            record_id: Some(record_id.into()),
            parent_thread_id: Some(parent_thread_id.into()),
            branch_point_turn_id: Some(branch_point_turn_id.into()),
            message: None,
        }
    }

    pub fn failed(topic_node_id: SemanticNodeId, message: impl Into<String>) -> Self {
        Self {
            topic_node_id,
            status: DecisionBranchToolItemStatus::Failed,
            checklist_item_id: None,
            record_id: None,
            parent_thread_id: None,
            branch_point_turn_id: None,
            message: Some(message.into()),
        }
    }
}

impl DecisionBranchToolItemStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Failed => "failed",
        }
    }
}

impl ThreadedDecisionDynamicToolError {
    pub fn kind(&self) -> &'static str {
        self.kind
    }

    pub(crate) fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    fn invalid_arguments(message: impl Into<String>) -> Self {
        Self::new("invalid_arguments", message)
    }

    fn invalid_field(field: &'static str, message: impl Into<String>) -> Self {
        Self::new("invalid_field", format!("{field}: {}", message.into()))
    }

    pub(crate) fn unavailable(message: impl Into<String>) -> Self {
        Self::new("unavailable", message)
    }
}

impl std::fmt::Display for ThreadedDecisionDynamicToolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ThreadedDecisionDynamicToolError {}

fn validate_namespace(
    request: &DynamicToolCallRequest,
) -> Result<(), ThreadedDecisionDynamicToolError> {
    if let Some(namespace) = request.namespace()
        && namespace != BERYL_DYNAMIC_TOOL_NAMESPACE
    {
        return Err(ThreadedDecisionDynamicToolError::new(
            "unsupported_namespace",
            format!("unsupported Beryl dynamic tool namespace {namespace:?}"),
        ));
    }
    Ok(())
}

fn parse_arguments<T>(arguments: &Value) -> Result<T, ThreadedDecisionDynamicToolError>
where
    T: for<'de> Deserialize<'de>,
{
    let arguments = if arguments.is_null() {
        json!({})
    } else {
        arguments.clone()
    };
    serde_json::from_value(arguments)
        .map_err(|source| ThreadedDecisionDynamicToolError::invalid_arguments(source.to_string()))
}

fn invalid_graph_id(
    field: &'static str,
    source: SemanticGraphIdError,
) -> ThreadedDecisionDynamicToolError {
    ThreadedDecisionDynamicToolError::invalid_field(field, source.to_string())
}

fn parse_resolution_outcome(
    value: &str,
) -> Result<ThreadedDecisionOutcome, ThreadedDecisionDynamicToolError> {
    match value.trim() {
        "accepted" => Ok(ThreadedDecisionOutcome::Accepted),
        "rejected" => Ok(ThreadedDecisionOutcome::Rejected),
        other => Err(ThreadedDecisionDynamicToolError::invalid_field(
            "outcome",
            format!("must be accepted or rejected, got {other:?}"),
        )),
    }
}

fn normalize_required_text(
    field: &'static str,
    value: String,
) -> Result<String, ThreadedDecisionDynamicToolError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(ThreadedDecisionDynamicToolError::invalid_field(
            field,
            "must not be empty",
        ));
    }
    Ok(value)
}

fn normalize_limited_text(
    field: &'static str,
    value: String,
    min_chars: usize,
    max_chars: usize,
) -> Result<String, ThreadedDecisionDynamicToolError> {
    let value = value.trim().to_string();
    let char_count = value.chars().count();
    if char_count < min_chars {
        return Err(ThreadedDecisionDynamicToolError::invalid_field(
            field,
            format!("must contain at least {min_chars} character(s)"),
        ));
    }
    if char_count > max_chars {
        return Err(ThreadedDecisionDynamicToolError::invalid_field(
            field,
            format!("length {char_count} exceeds the supported limit {max_chars}"),
        ));
    }
    Ok(value)
}

fn compact_json(value: Value) -> String {
    serde_json::to_string(&value).unwrap_or_else(|_| {
        "{\"ok\":false,\"error\":{\"kind\":\"internal\",\"message\":\"could not serialize dynamic tool response\"}}"
            .to_string()
    })
}
