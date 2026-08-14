mod mutation;
mod workspace_state;

use beryl_model::conversation::ConversationThreadId;
use beryl_model::semantic_graph::{
    ChecklistItemStatus, SemanticGraph, SemanticNode, SemanticNodeFacets, SemanticNodeId,
    SoftLinkId, SoftLinkKind, ThreadRef, ThreadRefId,
};
use beryl_model::workspace::{BerylWorkspaceId, BerylWorkspaceManifest, ExecutionTargetId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{BerylWorkspacePersistence, WorkspacePersistenceError};

pub use mutation::{
    GraphPatchWriteRequest, GraphPatchWriteResponse, NodeLeafDeleteRequest, NodeLeafDeleteResponse,
    NodeSubtreeDeleteRequest, NodeSubtreeDeleteResponse, ThreadRefUpsertRequest,
    ThreadRefUpsertResponse, node_leaf_delete_patch, node_subtree_delete_patch,
    thread_ref_upsert_patch,
};
pub use workspace_state::{
    WorkspaceMemberSnapshot, WorkspaceMemberSnapshotKind, WorkspacePrimaryMemberSnapshot,
    WorkspaceStateReadRequest, WorkspaceStateSnapshot, WorkspaceThreadMetadataSnapshot,
};

pub const READ_WORKSPACE_GRAPH_SUMMARY_TOOL: &str = "read_workspace_graph_summary";
pub const READ_WORKSPACE_STATE_TOOL: &str = "beryl_workspace_state";
pub const READ_GRAPH_NEIGHBORHOOD_TOOL: &str = "read_graph_neighborhood";
pub const READ_CHECKLIST_TOOL: &str = "read_checklist";
pub const UPSERT_GRAPH_NODE_TOOL: &str = "upsert_graph_node";
pub const SET_GRAPH_NODE_PARENT_TOOL: &str = "set_graph_node_parent";
pub const UPSERT_GRAPH_SOFT_LINK_TOOL: &str = "upsert_graph_soft_link";
pub const SET_CHECKLIST_ITEM_STATUS_TOOL: &str = "set_checklist_item_status";
pub const UPSERT_THREAD_REF_TOOL: &str = "beryl_workspace_thread_ref_upsert";

pub const MAX_GRAPH_NEIGHBORHOOD_PARENT_DEPTH: usize = 4;
pub const MAX_GRAPH_NEIGHBORHOOD_CHILD_DEPTH: usize = 3;
pub const MAX_GRAPH_NEIGHBORHOOD_NODE_COUNT: usize = 24;
pub const MAX_GRAPH_SUMMARY_ROOT_COUNT: usize = 24;
pub const MAX_CHECKLIST_ITEM_COUNT: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceGraphToolService {
    persistence: BerylWorkspacePersistence,
}

#[derive(Debug, Error)]
pub enum WorkspaceGraphToolError {
    #[error(transparent)]
    Persistence(#[from] WorkspacePersistenceError),
    #[error("workspace {workspace_id} does not exist")]
    MissingWorkspace { workspace_id: String },
    #[error("semantic graph node {node_id} does not exist in workspace {workspace_id}")]
    MissingNode {
        workspace_id: String,
        node_id: String,
    },
    #[error("semantic graph node {node_id} is not checklist-capable in workspace {workspace_id}")]
    NodeNotChecklist {
        workspace_id: String,
        node_id: String,
    },
    #[error("graph neighborhood parent depth {requested} exceeds the supported limit {maximum}")]
    ParentDepthTooLarge { requested: usize, maximum: usize },
    #[error("graph neighborhood child depth {requested} exceeds the supported limit {maximum}")]
    ChildDepthTooLarge { requested: usize, maximum: usize },
    #[error(
        "thread ref {thread_ref_id} was not present after a successful upsert in workspace {workspace_id}"
    )]
    MissingThreadRefAfterWrite {
        workspace_id: String,
        thread_ref_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGraphSummaryRequest {
    pub workspace_id: BerylWorkspaceId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNeighborhoodRequest {
    pub workspace_id: BerylWorkspaceId,
    #[serde(default)]
    pub anchor_node_id: Option<SemanticNodeId>,
    pub parent_depth: usize,
    pub child_depth: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecklistReadRequest {
    pub workspace_id: BerylWorkspaceId,
    pub checklist_node_id: SemanticNodeId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGraphSummary {
    pub manifest: BerylWorkspaceManifest,
    pub root_node_count: usize,
    pub root_nodes_truncated: bool,
    #[serde(default)]
    pub root_nodes: Vec<GraphNodeSnapshot>,
    pub node_count: usize,
    pub soft_link_count: usize,
    pub thread_ref_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNeighborhoodResponse {
    pub summary: WorkspaceGraphSummary,
    pub anchor_node_id: Option<SemanticNodeId>,
    pub truncated: bool,
    #[serde(default)]
    pub lineage: Vec<GraphNodeSnapshot>,
    #[serde(default)]
    pub anchor: Option<GraphNeighborhoodNode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNeighborhoodNode {
    pub node: GraphNodeSnapshot,
    #[serde(default)]
    pub soft_links: Vec<GraphSoftLinkSnapshot>,
    #[serde(default)]
    pub thread_refs: Vec<GraphThreadRefSnapshot>,
    #[serde(default)]
    pub children: Vec<GraphNeighborhoodNode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecklistReadResponse {
    pub summary: WorkspaceGraphSummary,
    pub checklist: GraphNodeSnapshot,
    pub truncated: bool,
    #[serde(default)]
    pub items: Vec<ChecklistItemSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecklistItemSnapshot {
    pub node: GraphNodeSnapshot,
    #[serde(default)]
    pub thread_refs: Vec<GraphThreadRefSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNodeSnapshot {
    pub id: SemanticNodeId,
    pub title: String,
    pub summary: String,
    pub facets: SemanticNodeFacets,
    #[serde(default)]
    pub checklist_item_status: Option<ChecklistItemStatus>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphSoftLinkSnapshot {
    pub id: SoftLinkId,
    pub kind: SoftLinkKind,
    pub target: GraphNodeSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphThreadRefSnapshot {
    pub id: ThreadRefId,
    pub thread_id: ConversationThreadId,
    pub execution_target: ExecutionTargetId,
    pub label: String,
}

struct WorkspaceGraphBundle {
    manifest: BerylWorkspaceManifest,
    graph: SemanticGraph,
}

impl WorkspaceGraphToolService {
    pub fn new(persistence: BerylWorkspacePersistence) -> Self {
        Self { persistence }
    }

    pub fn read_workspace_summary(
        &self,
        request: &WorkspaceGraphSummaryRequest,
    ) -> Result<WorkspaceGraphSummary, WorkspaceGraphToolError> {
        let bundle = self.load_workspace_bundle(&request.workspace_id)?;
        Ok(workspace_graph_summary(bundle.manifest, &bundle.graph))
    }

    pub fn read_graph_neighborhood(
        &self,
        request: &GraphNeighborhoodRequest,
    ) -> Result<GraphNeighborhoodResponse, WorkspaceGraphToolError> {
        self.validate_neighborhood_request(request)?;

        let bundle = self.load_workspace_bundle(&request.workspace_id)?;
        let summary = workspace_graph_summary(bundle.manifest.clone(), &bundle.graph);
        let Some(anchor_id) = request.anchor_node_id.as_ref() else {
            return Ok(GraphNeighborhoodResponse {
                summary,
                anchor_node_id: None,
                truncated: false,
                lineage: Vec::new(),
                anchor: None,
            });
        };

        let path = bundle
            .graph
            .path_to_root(anchor_id)
            .ok_or_else(|| self.missing_node(&request.workspace_id, anchor_id))?;
        let mut truncated = false;
        let path_without_anchor = &path[..path.len().saturating_sub(1)];
        let lineage_start = path_without_anchor
            .len()
            .saturating_sub(request.parent_depth);
        let mut lineage: Vec<_> = path_without_anchor[lineage_start..]
            .iter()
            .map(|node| GraphNodeSnapshot::from_node(node))
            .collect();
        let lineage_budget = MAX_GRAPH_NEIGHBORHOOD_NODE_COUNT.saturating_sub(1);
        if lineage.len() > lineage_budget {
            let overflow = lineage.len() - lineage_budget;
            lineage.drain(0..overflow);
            truncated = true;
        }

        let mut remaining_nodes =
            MAX_GRAPH_NEIGHBORHOOD_NODE_COUNT.saturating_sub(lineage.len() + 1);
        let anchor = Some(build_neighborhood_node(
            &bundle.graph,
            &request.workspace_id,
            anchor_id,
            request.child_depth,
            &mut remaining_nodes,
            &mut truncated,
        )?);

        Ok(GraphNeighborhoodResponse {
            summary,
            anchor_node_id: Some(anchor_id.clone()),
            truncated,
            lineage,
            anchor,
        })
    }

    pub fn read_checklist(
        &self,
        request: &ChecklistReadRequest,
    ) -> Result<ChecklistReadResponse, WorkspaceGraphToolError> {
        let bundle = self.load_workspace_bundle(&request.workspace_id)?;
        let checklist = bundle
            .graph
            .node(&request.checklist_node_id)
            .ok_or_else(|| self.missing_node(&request.workspace_id, &request.checklist_node_id))?;
        if !checklist.facets().has_checklist() {
            return Err(WorkspaceGraphToolError::NodeNotChecklist {
                workspace_id: request.workspace_id.as_str().to_string(),
                node_id: request.checklist_node_id.as_str().to_string(),
            });
        }

        let items = bundle.graph.checklist_items(&request.checklist_node_id);
        let truncated = items.len() > MAX_CHECKLIST_ITEM_COUNT;
        let items = items
            .into_iter()
            .take(MAX_CHECKLIST_ITEM_COUNT)
            .map(|item| ChecklistItemSnapshot {
                node: GraphNodeSnapshot::from_node(item),
                thread_refs: bundle
                    .graph
                    .thread_refs_for_node(item.id())
                    .map(GraphThreadRefSnapshot::from_thread_ref)
                    .collect(),
            })
            .collect();

        Ok(ChecklistReadResponse {
            summary: workspace_graph_summary(bundle.manifest, &bundle.graph),
            checklist: GraphNodeSnapshot::from_node(checklist),
            truncated,
            items,
        })
    }

    fn validate_neighborhood_request(
        &self,
        request: &GraphNeighborhoodRequest,
    ) -> Result<(), WorkspaceGraphToolError> {
        if request.parent_depth > MAX_GRAPH_NEIGHBORHOOD_PARENT_DEPTH {
            return Err(WorkspaceGraphToolError::ParentDepthTooLarge {
                requested: request.parent_depth,
                maximum: MAX_GRAPH_NEIGHBORHOOD_PARENT_DEPTH,
            });
        }
        if request.child_depth > MAX_GRAPH_NEIGHBORHOOD_CHILD_DEPTH {
            return Err(WorkspaceGraphToolError::ChildDepthTooLarge {
                requested: request.child_depth,
                maximum: MAX_GRAPH_NEIGHBORHOOD_CHILD_DEPTH,
            });
        }
        Ok(())
    }

    fn ensure_workspace_exists(
        &self,
        workspace_id: &BerylWorkspaceId,
    ) -> Result<(), WorkspaceGraphToolError> {
        let _ = self.load_workspace_manifest(workspace_id)?;
        Ok(())
    }

    fn load_workspace_bundle(
        &self,
        workspace_id: &BerylWorkspaceId,
    ) -> Result<WorkspaceGraphBundle, WorkspaceGraphToolError> {
        let manifest = self.load_workspace_manifest(workspace_id)?;
        let graph = self.persistence.load_workspace_graph_state(workspace_id)?;
        Ok(WorkspaceGraphBundle { manifest, graph })
    }

    fn load_workspace_manifest(
        &self,
        workspace_id: &BerylWorkspaceId,
    ) -> Result<BerylWorkspaceManifest, WorkspaceGraphToolError> {
        self.persistence
            .load_workspace_manifest(workspace_id)?
            .ok_or_else(|| WorkspaceGraphToolError::MissingWorkspace {
                workspace_id: workspace_id.as_str().to_string(),
            })
    }

    fn missing_node(
        &self,
        workspace_id: &BerylWorkspaceId,
        node_id: &SemanticNodeId,
    ) -> WorkspaceGraphToolError {
        WorkspaceGraphToolError::MissingNode {
            workspace_id: workspace_id.as_str().to_string(),
            node_id: node_id.as_str().to_string(),
        }
    }
}

impl GraphNodeSnapshot {
    fn from_node(node: &SemanticNode) -> Self {
        Self {
            id: node.id().clone(),
            title: node.title().to_string(),
            summary: node.summary().to_string(),
            facets: node.facets().clone(),
            checklist_item_status: node.checklist_item_status(),
        }
    }
}

impl GraphThreadRefSnapshot {
    fn from_thread_ref(thread_ref: &ThreadRef) -> Self {
        Self {
            id: thread_ref.id().clone(),
            thread_id: thread_ref.thread_id().clone(),
            execution_target: thread_ref.execution_target().clone(),
            label: thread_ref.label().to_string(),
        }
    }
}

fn workspace_graph_summary(
    manifest: BerylWorkspaceManifest,
    graph: &SemanticGraph,
) -> WorkspaceGraphSummary {
    let root_node_count = graph.root_node_ids().len();
    let root_nodes = graph
        .root_nodes()
        .take(MAX_GRAPH_SUMMARY_ROOT_COUNT)
        .map(GraphNodeSnapshot::from_node)
        .collect();

    WorkspaceGraphSummary {
        manifest,
        root_node_count,
        root_nodes_truncated: root_node_count > MAX_GRAPH_SUMMARY_ROOT_COUNT,
        root_nodes,
        node_count: graph.node_count(),
        soft_link_count: graph.soft_link_count(),
        thread_ref_count: graph.thread_ref_count(),
    }
}

fn build_neighborhood_node(
    graph: &SemanticGraph,
    workspace_id: &BerylWorkspaceId,
    node_id: &SemanticNodeId,
    child_depth: usize,
    remaining_nodes: &mut usize,
    truncated: &mut bool,
) -> Result<GraphNeighborhoodNode, WorkspaceGraphToolError> {
    let node = graph
        .node(node_id)
        .ok_or_else(|| WorkspaceGraphToolError::MissingNode {
            workspace_id: workspace_id.as_str().to_string(),
            node_id: node_id.as_str().to_string(),
        })?;
    let soft_links = graph
        .soft_links_from(node_id)
        .map(|link| {
            let target = graph.node(link.target_id()).ok_or_else(|| {
                WorkspaceGraphToolError::MissingNode {
                    workspace_id: workspace_id.as_str().to_string(),
                    node_id: link.target_id().as_str().to_string(),
                }
            })?;

            Ok::<GraphSoftLinkSnapshot, WorkspaceGraphToolError>(GraphSoftLinkSnapshot {
                id: link.id().clone(),
                kind: link.kind().clone(),
                target: GraphNodeSnapshot::from_node(target),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let thread_refs = graph
        .thread_refs_for_node(node_id)
        .map(GraphThreadRefSnapshot::from_thread_ref)
        .collect();
    let mut children = Vec::new();

    if child_depth > 0 {
        for child in graph.child_nodes_of(node_id) {
            if *remaining_nodes == 0 {
                *truncated = true;
                break;
            }

            *remaining_nodes -= 1;
            children.push(build_neighborhood_node(
                graph,
                workspace_id,
                child.id(),
                child_depth - 1,
                remaining_nodes,
                truncated,
            )?);
        }
    }

    Ok(GraphNeighborhoodNode {
        node: GraphNodeSnapshot::from_node(node),
        soft_links,
        thread_refs,
        children,
    })
}
