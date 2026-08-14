mod arguments;
mod schema;

use std::time::{SystemTime, UNIX_EPOCH};

use beryl_backend::{DynamicToolCallRequest, DynamicToolCallResponse, DynamicToolSpec};
use beryl_model::{
    conversation::{ConversationThreadId, ConversationTurnId},
    provenance::{MutationProvenance, MutationSource},
    workspace::BerylWorkspaceId,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::warn;

use crate::WorkspaceGraphMutationCommit;
use crate::dynamic_tools::BERYL_DYNAMIC_TOOL_NAMESPACE;
use crate::graph_tools::{
    GraphPatchWriteRequest, READ_CHECKLIST_TOOL, READ_GRAPH_NEIGHBORHOOD_TOOL,
    READ_WORKSPACE_GRAPH_SUMMARY_TOOL, SET_CHECKLIST_ITEM_STATUS_TOOL, SET_GRAPH_NODE_PARENT_TOOL,
    UPSERT_GRAPH_NODE_TOOL, UPSERT_GRAPH_SOFT_LINK_TOOL, WorkspaceGraphSummaryRequest,
    WorkspaceGraphToolService,
};
use arguments::{
    ChecklistReadArguments, EmptyArguments, GraphNeighborhoodArguments,
    SetChecklistItemStatusArguments, SetGraphNodeParentArguments, UpsertGraphNodeArguments,
    UpsertGraphSoftLinkArguments,
};
use schema::{
    checklist_read_schema, empty_object_schema, graph_neighborhood_schema,
    set_checklist_item_status_schema, set_graph_node_parent_schema, upsert_graph_node_schema,
    upsert_graph_soft_link_schema,
};

pub const BERYL_GRAPH_DYNAMIC_TOOL_NAMESPACE: &str = BERYL_DYNAMIC_TOOL_NAMESPACE;
pub const MAX_DYNAMIC_NODE_TITLE_CHARS: usize = 160;
pub const MAX_DYNAMIC_NODE_SUMMARY_CHARS: usize = 1200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BerylGraphDynamicToolDispatch {
    response: DynamicToolCallResponse,
    graph_write: Option<BerylGraphDynamicWrite>,
    graph_failure: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BerylGraphDynamicWrite {
    commit: WorkspaceGraphMutationCommit,
}

pub fn beryl_graph_dynamic_tool_specs() -> Vec<DynamicToolSpec> {
    vec![
        graph_tool_spec(
            READ_WORKSPACE_GRAPH_SUMMARY_TOOL,
            "Read a compact summary of this Beryl workspace semantic graph.",
            empty_object_schema(),
        ),
        graph_tool_spec(
            READ_GRAPH_NEIGHBORHOOD_TOOL,
            "Read a bounded semantic graph neighborhood around a node, or bounded root-level information when no anchor is provided, in this Beryl workspace.",
            graph_neighborhood_schema(),
        ),
        graph_tool_spec(
            READ_CHECKLIST_TOOL,
            "Read a bounded checklist slice from this Beryl workspace semantic graph.",
            checklist_read_schema(),
        ),
        graph_tool_spec(
            UPSERT_GRAPH_NODE_TOOL,
            r#"Create or update one semantic graph node in this Beryl workspace and assign its hard-forest parent or root-level placement atomically. Use parentId=null for a root-level node. Use topic=true for ordinary work topics, checklist=true for checklist containers, and checklistItem=true plus checklistItemStatus for checklist rows. Example arguments: {"nodeId":"root","parentId":null,"title":"Root","summary":"Workspace root topic.","topic":true,"checklist":false,"checklistItem":false}"#,
            upsert_graph_node_schema(),
        ),
        graph_tool_spec(
            SET_GRAPH_NODE_PARENT_TOOL,
            r#"Move one semantic graph node under a parent node, or make it root-level by using parentId=null. Example arguments: {"childId":"root","parentId":null}"#,
            set_graph_node_parent_schema(),
        ),
        graph_tool_spec(
            UPSERT_GRAPH_SOFT_LINK_TOOL,
            r#"Create or update one typed soft link between two semantic graph nodes. Example arguments: {"linkId":"release_depends_on_docs","sourceId":"release","targetId":"docs","kind":"depends_on"}"#,
            upsert_graph_soft_link_schema(),
        ),
        graph_tool_spec(
            SET_CHECKLIST_ITEM_STATUS_TOOL,
            r#"Set the status of one checklist-item semantic node. Example arguments: {"nodeId":"draft_release_notes","status":"done"}"#,
            set_checklist_item_status_schema(),
        ),
    ]
}

pub fn dispatch_beryl_graph_dynamic_tool_call(
    service: &WorkspaceGraphToolService,
    workspace_id: &BerylWorkspaceId,
    request: &DynamicToolCallRequest,
) -> DynamicToolCallResponse {
    dispatch_beryl_graph_dynamic_tool_call_with_metadata(service, workspace_id, request)
        .into_response()
}

pub fn dispatch_beryl_graph_dynamic_tool_call_with_metadata(
    service: &WorkspaceGraphToolService,
    workspace_id: &BerylWorkspaceId,
    request: &DynamicToolCallRequest,
) -> BerylGraphDynamicToolDispatch {
    match dispatch_beryl_graph_dynamic_tool_call_result(service, workspace_id, request) {
        Ok(result) => BerylGraphDynamicToolDispatch {
            response: dynamic_tool_success(result.value),
            graph_write: result.graph_write,
            graph_failure: None,
        },
        Err(error) => {
            let graph_failure = dynamic_graph_write_tool(request.tool()).then(|| error.to_string());
            warn!(
                tool_call = %request.summary(),
                error = %error,
                "Beryl graph dynamic tool call failed"
            );
            BerylGraphDynamicToolDispatch {
                response: dynamic_tool_failure(request, error),
                graph_write: None,
                graph_failure,
            }
        }
    }
}

fn dispatch_beryl_graph_dynamic_tool_call_result(
    service: &WorkspaceGraphToolService,
    workspace_id: &BerylWorkspaceId,
    request: &DynamicToolCallRequest,
) -> Result<DynamicGraphToolResult, DynamicGraphToolError> {
    if let Some(namespace) = request.namespace()
        && namespace != BERYL_GRAPH_DYNAMIC_TOOL_NAMESPACE
    {
        return Err(DynamicGraphToolError::UnsupportedNamespace {
            namespace: namespace.to_string(),
        });
    }

    match request.tool() {
        READ_WORKSPACE_GRAPH_SUMMARY_TOOL => {
            let _: EmptyArguments = parse_arguments(request.arguments())?;
            let response = service
                .read_workspace_summary(&WorkspaceGraphSummaryRequest {
                    workspace_id: workspace_id.clone(),
                })
                .map_err(DynamicGraphToolError::graph_tool)?;
            serialize_result(response).map(DynamicGraphToolResult::read)
        }
        READ_GRAPH_NEIGHBORHOOD_TOOL => {
            let arguments: GraphNeighborhoodArguments = parse_arguments(request.arguments())?;
            let response = service
                .read_graph_neighborhood(&arguments.into_request(workspace_id)?)
                .map_err(DynamicGraphToolError::graph_tool)?;
            serialize_result(response).map(DynamicGraphToolResult::read)
        }
        READ_CHECKLIST_TOOL => {
            let arguments: ChecklistReadArguments = parse_arguments(request.arguments())?;
            let response = service
                .read_checklist(&arguments.into_request(workspace_id)?)
                .map_err(DynamicGraphToolError::graph_tool)?;
            serialize_result(response).map(DynamicGraphToolResult::read)
        }
        UPSERT_GRAPH_NODE_TOOL => {
            let arguments: UpsertGraphNodeArguments = parse_arguments(request.arguments())?;
            let patch = arguments.into_patch(dynamic_tool_provenance(request)?)?;
            apply_dynamic_graph_patch(service, workspace_id, patch)
        }
        SET_GRAPH_NODE_PARENT_TOOL => {
            let arguments: SetGraphNodeParentArguments = parse_arguments(request.arguments())?;
            let patch = arguments.into_patch(dynamic_tool_provenance(request)?)?;
            apply_dynamic_graph_patch(service, workspace_id, patch)
        }
        UPSERT_GRAPH_SOFT_LINK_TOOL => {
            let arguments: UpsertGraphSoftLinkArguments = parse_arguments(request.arguments())?;
            let patch = arguments.into_patch(dynamic_tool_provenance(request)?)?;
            apply_dynamic_graph_patch(service, workspace_id, patch)
        }
        SET_CHECKLIST_ITEM_STATUS_TOOL => {
            let arguments: SetChecklistItemStatusArguments = parse_arguments(request.arguments())?;
            let patch = arguments.into_patch(dynamic_tool_provenance(request)?)?;
            apply_dynamic_graph_patch(service, workspace_id, patch)
        }
        tool => Err(DynamicGraphToolError::UnsupportedTool {
            tool: tool.to_string(),
        }),
    }
}

fn apply_dynamic_graph_patch(
    service: &WorkspaceGraphToolService,
    workspace_id: &BerylWorkspaceId,
    patch: beryl_model::semantic_graph::SemanticGraphPatch,
) -> Result<DynamicGraphToolResult, DynamicGraphToolError> {
    let response = service
        .apply_graph_patch(&GraphPatchWriteRequest {
            workspace_id: workspace_id.clone(),
            patch,
            expected_base_revision: None,
        })
        .map_err(DynamicGraphToolError::graph_tool)?;
    let commit = response.commit.clone();
    serialize_result(response).map(|value| DynamicGraphToolResult {
        value,
        graph_write: Some(BerylGraphDynamicWrite { commit }),
    })
}

fn graph_tool_spec(name: &str, description: &str, input_schema: Value) -> DynamicToolSpec {
    DynamicToolSpec::new(name, description, input_schema)
        .with_namespace(BERYL_GRAPH_DYNAMIC_TOOL_NAMESPACE)
        .with_defer_loading(false)
}

fn dynamic_graph_write_tool(tool: &str) -> bool {
    matches!(
        tool,
        UPSERT_GRAPH_NODE_TOOL
            | SET_GRAPH_NODE_PARENT_TOOL
            | UPSERT_GRAPH_SOFT_LINK_TOOL
            | SET_CHECKLIST_ITEM_STATUS_TOOL
    )
}

impl BerylGraphDynamicToolDispatch {
    pub fn response(&self) -> &DynamicToolCallResponse {
        &self.response
    }

    pub fn graph_write(&self) -> Option<BerylGraphDynamicWrite> {
        self.graph_write.clone()
    }

    pub fn graph_failure(&self) -> Option<String> {
        self.graph_failure.clone()
    }

    pub fn into_response(self) -> DynamicToolCallResponse {
        self.response
    }
}

impl BerylGraphDynamicWrite {
    pub fn commit(&self) -> &WorkspaceGraphMutationCommit {
        &self.commit
    }

    pub fn into_commit(self) -> WorkspaceGraphMutationCommit {
        self.commit
    }
}

struct DynamicGraphToolResult {
    value: Value,
    graph_write: Option<BerylGraphDynamicWrite>,
}

impl DynamicGraphToolResult {
    fn read(value: Value) -> Self {
        Self {
            value,
            graph_write: None,
        }
    }
}

fn parse_arguments<T>(arguments: &Value) -> Result<T, DynamicGraphToolError>
where
    T: for<'de> Deserialize<'de>,
{
    let arguments = if arguments.is_null() {
        json!({})
    } else {
        arguments.clone()
    };
    serde_json::from_value(arguments).map_err(|source| DynamicGraphToolError::InvalidArguments {
        detail: source.to_string(),
    })
}

fn serialize_result<T>(value: T) -> Result<Value, DynamicGraphToolError>
where
    T: Serialize,
{
    serde_json::to_value(value).map_err(|source| DynamicGraphToolError::Internal {
        detail: source.to_string(),
    })
}

fn dynamic_tool_success(result: Value) -> DynamicToolCallResponse {
    DynamicToolCallResponse::success_text(compact_json(json!({
        "ok": true,
        "result": result,
    })))
}

fn dynamic_tool_failure(
    request: &DynamicToolCallRequest,
    error: DynamicGraphToolError,
) -> DynamicToolCallResponse {
    DynamicToolCallResponse::failure_text(compact_json(json!({
        "ok": false,
        "error": {
            "kind": error.kind(),
            "message": error.to_string(),
            "tool": request.tool(),
            "callId": request.call_id(),
        },
    })))
}

fn compact_json(value: Value) -> String {
    serde_json::to_string(&value).unwrap_or_else(|_| {
        "{\"ok\":false,\"error\":{\"kind\":\"internal\",\"message\":\"could not serialize dynamic tool response\"}}"
            .to_string()
    })
}

fn dynamic_tool_provenance(
    request: &DynamicToolCallRequest,
) -> Result<MutationProvenance, DynamicGraphToolError> {
    let source = MutationSource::dynamic_tool_call(
        ConversationThreadId::new(request.thread_id().to_string()),
        ConversationTurnId::new(request.turn_id().to_string()),
        request.tool().to_string(),
        request.call_id().to_string(),
    )
    .map_err(|source| DynamicGraphToolError::InvalidProvenance {
        detail: source.to_string(),
    })?;

    MutationProvenance::new("codex", current_unix_millis(), source, None).map_err(|source| {
        DynamicGraphToolError::InvalidProvenance {
            detail: source.to_string(),
        }
    })
}

fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug)]
enum DynamicGraphToolError {
    UnsupportedNamespace { namespace: String },
    UnsupportedTool { tool: String },
    InvalidArguments { detail: String },
    InvalidField { field: &'static str, detail: String },
    GraphTool { detail: String },
    InvalidProvenance { detail: String },
    Internal { detail: String },
}

impl DynamicGraphToolError {
    fn graph_tool(error: impl std::error::Error) -> Self {
        Self::GraphTool {
            detail: error.to_string(),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::UnsupportedNamespace { .. } => "unsupported_namespace",
            Self::UnsupportedTool { .. } => "unsupported_tool",
            Self::InvalidArguments { .. } => "invalid_arguments",
            Self::InvalidField { .. } => "invalid_field",
            Self::GraphTool { .. } => "graph_tool_error",
            Self::InvalidProvenance { .. } => "invalid_provenance",
            Self::Internal { .. } => "internal",
        }
    }
}

impl std::fmt::Display for DynamicGraphToolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedNamespace { namespace } => {
                write!(
                    formatter,
                    "unsupported Beryl dynamic tool namespace {namespace:?}"
                )
            }
            Self::UnsupportedTool { tool } => {
                write!(formatter, "unsupported Beryl graph dynamic tool {tool:?}")
            }
            Self::InvalidArguments { detail } => {
                write!(formatter, "invalid dynamic tool arguments: {detail}")
            }
            Self::InvalidField { field, detail } => {
                write!(formatter, "invalid dynamic tool field {field}: {detail}")
            }
            Self::GraphTool { detail } => write!(formatter, "{detail}"),
            Self::InvalidProvenance { detail } => {
                write!(
                    formatter,
                    "could not build dynamic tool provenance: {detail}"
                )
            }
            Self::Internal { detail } => {
                write!(formatter, "internal dynamic tool error: {detail}")
            }
        }
    }
}

impl std::error::Error for DynamicGraphToolError {}
