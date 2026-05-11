use std::{collections::BTreeSet, fmt};

use beryl_backend::{DynamicToolCallRequest, DynamicToolCallResponse, DynamicToolSpec};
use beryl_model::workspace::BerylWorkspaceId;

use crate::{
    WorkspaceGraphToolService,
    graph_dynamic_tools::{
        BerylGraphDynamicToolDispatch, BerylGraphDynamicWrite, beryl_graph_dynamic_tool_specs,
        dispatch_beryl_graph_dynamic_tool_call_with_metadata,
    },
    graph_tools::{
        READ_CHECKLIST_TOOL, READ_GRAPH_NEIGHBORHOOD_TOOL, READ_WORKSPACE_GRAPH_SUMMARY_TOOL,
        SET_CHECKLIST_ITEM_STATUS_TOOL, SET_GRAPH_NODE_PARENT_TOOL, UPSERT_GRAPH_NODE_TOOL,
        UPSERT_GRAPH_SOFT_LINK_TOOL,
    },
    lifecycle_dynamic_tools::{
        BerylLifecycleDynamicToolDispatch, LifecycleYieldOutcome, YIELD_TOOL,
        beryl_lifecycle_dynamic_tool_specs,
        dispatch_beryl_lifecycle_dynamic_tool_call_with_metadata,
    },
};

pub const BERYL_DYNAMIC_TOOL_NAMESPACE: &str = "beryl";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BerylDynamicToolDispatch {
    response: DynamicToolCallResponse,
    graph_write: Option<BerylGraphDynamicWrite>,
    graph_failure: Option<String>,
    lifecycle_yield: Option<LifecycleYieldOutcome>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynamicToolRegistryError {
    namespace: Option<String>,
    name: String,
}

pub fn beryl_dynamic_tool_specs() -> Vec<DynamicToolSpec> {
    let mut tools = Vec::new();
    tools.extend(beryl_graph_dynamic_tool_specs());
    tools.extend(beryl_lifecycle_dynamic_tool_specs());
    validate_unique_dynamic_tool_names(&tools)
        .expect("Beryl dynamic tool names must be unique within each namespace");
    tools
}

pub fn dispatch_beryl_dynamic_tool_call_with_metadata(
    graph_service: &WorkspaceGraphToolService,
    workspace_id: &BerylWorkspaceId,
    request: &DynamicToolCallRequest,
) -> BerylDynamicToolDispatch {
    match request.tool() {
        READ_WORKSPACE_GRAPH_SUMMARY_TOOL
        | READ_GRAPH_NEIGHBORHOOD_TOOL
        | READ_CHECKLIST_TOOL
        | UPSERT_GRAPH_NODE_TOOL
        | SET_GRAPH_NODE_PARENT_TOOL
        | UPSERT_GRAPH_SOFT_LINK_TOOL
        | SET_CHECKLIST_ITEM_STATUS_TOOL => {
            let dispatch = dispatch_beryl_graph_dynamic_tool_call_with_metadata(
                graph_service,
                workspace_id,
                request,
            );
            BerylDynamicToolDispatch::from_graph(dispatch)
        }
        YIELD_TOOL => {
            let dispatch = dispatch_beryl_lifecycle_dynamic_tool_call_with_metadata(request);
            BerylDynamicToolDispatch::from_lifecycle(dispatch)
        }
        _ => BerylDynamicToolDispatch::unsupported_tool(request),
    }
}

impl BerylDynamicToolDispatch {
    pub fn response(&self) -> &DynamicToolCallResponse {
        &self.response
    }

    pub fn graph_write(&self) -> Option<BerylGraphDynamicWrite> {
        self.graph_write.clone()
    }

    pub fn graph_failure(&self) -> Option<String> {
        self.graph_failure.clone()
    }

    pub fn lifecycle_yield(&self) -> Option<LifecycleYieldOutcome> {
        self.lifecycle_yield
    }

    pub fn into_response(self) -> DynamicToolCallResponse {
        self.response
    }

    fn from_graph(dispatch: BerylGraphDynamicToolDispatch) -> Self {
        let graph_write = dispatch.graph_write();
        let graph_failure = dispatch.graph_failure();
        Self {
            response: dispatch.into_response(),
            graph_write,
            graph_failure,
            lifecycle_yield: None,
        }
    }

    fn from_lifecycle(dispatch: BerylLifecycleDynamicToolDispatch) -> Self {
        let lifecycle_yield = dispatch.outcome();
        Self {
            response: dispatch.into_response(),
            graph_write: None,
            graph_failure: None,
            lifecycle_yield,
        }
    }

    fn unsupported_tool(request: &DynamicToolCallRequest) -> Self {
        Self {
            response: DynamicToolCallResponse::failure_text(unsupported_tool_json(request)),
            graph_write: None,
            graph_failure: None,
            lifecycle_yield: None,
        }
    }
}

pub fn validate_unique_dynamic_tool_names(
    tools: &[DynamicToolSpec],
) -> Result<(), DynamicToolRegistryError> {
    let mut names = BTreeSet::new();
    for tool in tools {
        let namespace = tool.namespace.clone();
        let name = tool.name.clone();
        let key = (namespace.clone(), name.clone());
        if !names.insert(key) {
            return Err(DynamicToolRegistryError { namespace, name });
        }
    }
    Ok(())
}

impl DynamicToolRegistryError {
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for DynamicToolRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "duplicate dynamic tool registration for namespace {:?} and name {:?}",
            self.namespace.as_deref().unwrap_or("<none>"),
            self.name
        )
    }
}

impl std::error::Error for DynamicToolRegistryError {}

fn unsupported_tool_json(request: &DynamicToolCallRequest) -> String {
    serde_json::to_string(&serde_json::json!({
        "ok": false,
        "error": {
            "kind": "unsupported_tool",
            "message": format!("unsupported Beryl dynamic tool {:?}", request.tool()),
            "tool": request.tool(),
            "callId": request.call_id(),
        }
    }))
    .unwrap_or_else(|_| {
        "{\"ok\":false,\"error\":{\"kind\":\"internal\",\"message\":\"could not serialize dynamic tool response\"}}"
            .to_string()
    })
}
