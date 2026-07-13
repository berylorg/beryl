use beryl_model::provenance::MutationProvenance;
use beryl_model::semantic_graph::{
    SemanticGraphPatch, SemanticGraphPatchOp, SemanticNodeId, ThreadRefDraft,
};
use beryl_model::workspace::BerylWorkspaceId;
use serde::{Deserialize, Serialize};

use crate::{WorkspaceGraphMutationCommit, WorkspaceGraphRevision};

use super::{
    GraphThreadRefSnapshot, WorkspaceGraphSummary, WorkspaceGraphToolError,
    WorkspaceGraphToolService, workspace_graph_summary,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphPatchWriteRequest {
    pub workspace_id: BerylWorkspaceId,
    pub patch: SemanticGraphPatch,
    #[serde(default)]
    pub expected_base_revision: Option<WorkspaceGraphRevision>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeSubtreeDeleteRequest {
    pub workspace_id: BerylWorkspaceId,
    pub node_id: SemanticNodeId,
    pub provenance: MutationProvenance,
    #[serde(default)]
    pub expected_base_revision: Option<WorkspaceGraphRevision>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeLeafDeleteRequest {
    pub workspace_id: BerylWorkspaceId,
    pub node_id: SemanticNodeId,
    pub provenance: MutationProvenance,
    #[serde(default)]
    pub expected_base_revision: Option<WorkspaceGraphRevision>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRefUpsertRequest {
    pub workspace_id: BerylWorkspaceId,
    pub thread_ref: ThreadRefDraft,
    pub provenance: MutationProvenance,
    #[serde(default)]
    pub expected_base_revision: Option<WorkspaceGraphRevision>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphPatchWriteResponse {
    pub summary: WorkspaceGraphSummary,
    pub commit: WorkspaceGraphMutationCommit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeSubtreeDeleteResponse {
    pub summary: WorkspaceGraphSummary,
    pub commit: WorkspaceGraphMutationCommit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeLeafDeleteResponse {
    pub summary: WorkspaceGraphSummary,
    pub commit: WorkspaceGraphMutationCommit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRefUpsertResponse {
    pub summary: WorkspaceGraphSummary,
    pub commit: WorkspaceGraphMutationCommit,
    pub thread_ref: GraphThreadRefSnapshot,
}

pub fn node_subtree_delete_patch(
    node_id: &SemanticNodeId,
    provenance: &MutationProvenance,
) -> SemanticGraphPatch {
    SemanticGraphPatch::from_operation(SemanticGraphPatchOp::DeleteNodeSubtree {
        node_id: node_id.clone(),
        provenance: provenance.clone(),
    })
}

pub fn node_leaf_delete_patch(
    node_id: &SemanticNodeId,
    provenance: &MutationProvenance,
) -> SemanticGraphPatch {
    SemanticGraphPatch::from_operation(SemanticGraphPatchOp::DeleteNodeLeaf {
        node_id: node_id.clone(),
        provenance: provenance.clone(),
    })
}

pub fn thread_ref_upsert_patch(
    thread_ref: &ThreadRefDraft,
    provenance: &MutationProvenance,
) -> SemanticGraphPatch {
    SemanticGraphPatch::from_operation(SemanticGraphPatchOp::UpsertThreadRef {
        thread_ref: thread_ref.clone(),
        provenance: provenance.clone(),
    })
}

impl WorkspaceGraphToolService {
    pub fn apply_graph_patch(
        &self,
        request: &GraphPatchWriteRequest,
    ) -> Result<GraphPatchWriteResponse, WorkspaceGraphToolError> {
        self.ensure_workspace_exists(&request.workspace_id)?;
        self.ensure_patch_respects_threaded_decision_protection(
            &request.workspace_id,
            &request.patch,
        )?;
        self.apply_graph_patch_unchecked(request)
    }

    pub(crate) fn apply_threaded_decision_graph_patch(
        &self,
        request: &GraphPatchWriteRequest,
    ) -> Result<GraphPatchWriteResponse, WorkspaceGraphToolError> {
        self.ensure_workspace_exists(&request.workspace_id)?;
        self.apply_graph_patch_unchecked(request)
    }

    fn apply_graph_patch_unchecked(
        &self,
        request: &GraphPatchWriteRequest,
    ) -> Result<GraphPatchWriteResponse, WorkspaceGraphToolError> {
        let commit = self.persistence.apply_workspace_graph_patch(
            &request.workspace_id,
            &request.patch,
            request.expected_base_revision,
        )?;
        let bundle = self.load_workspace_bundle(&request.workspace_id)?;

        Ok(GraphPatchWriteResponse {
            summary: workspace_graph_summary(bundle.manifest, &bundle.graph),
            commit,
        })
    }

    fn ensure_patch_respects_threaded_decision_protection(
        &self,
        workspace_id: &BerylWorkspaceId,
        patch: &SemanticGraphPatch,
    ) -> Result<(), WorkspaceGraphToolError> {
        let decisions = self
            .persistence
            .load_workspace_threaded_decision_state(workspace_id)?;
        let protected_item_ids = decisions
            .protected_resolved_checklist_item_ids()
            .collect::<Vec<_>>();
        if protected_item_ids.is_empty() {
            return Ok(());
        }

        let graph = self.persistence.load_workspace_graph_state(workspace_id)?;
        graph
            .ensure_patch_respects_protected_decision_items(patch, protected_item_ids)
            .map_err(
                |error| WorkspaceGraphToolError::ProtectedDecisionItemMutation {
                    workspace_id: workspace_id.as_str().to_string(),
                    detail: error.to_string(),
                },
            )
    }

    pub fn delete_node_subtree(
        &self,
        request: &NodeSubtreeDeleteRequest,
    ) -> Result<NodeSubtreeDeleteResponse, WorkspaceGraphToolError> {
        let bundle = self.load_workspace_bundle(&request.workspace_id)?;
        if bundle.graph.node(&request.node_id).is_none() {
            return Err(self.missing_node(&request.workspace_id, &request.node_id));
        }

        let patch = node_subtree_delete_patch(&request.node_id, &request.provenance);
        let commit = self.persistence.apply_workspace_graph_patch(
            &request.workspace_id,
            &patch,
            request.expected_base_revision,
        )?;
        let bundle = self.load_workspace_bundle(&request.workspace_id)?;

        Ok(NodeSubtreeDeleteResponse {
            summary: workspace_graph_summary(bundle.manifest, &bundle.graph),
            commit,
        })
    }

    pub fn delete_node_leaf(
        &self,
        request: &NodeLeafDeleteRequest,
    ) -> Result<NodeLeafDeleteResponse, WorkspaceGraphToolError> {
        let bundle = self.load_workspace_bundle(&request.workspace_id)?;
        if bundle.graph.node(&request.node_id).is_none() {
            return Err(self.missing_node(&request.workspace_id, &request.node_id));
        }

        let patch = node_leaf_delete_patch(&request.node_id, &request.provenance);
        let commit = self.persistence.apply_workspace_graph_patch(
            &request.workspace_id,
            &patch,
            request.expected_base_revision,
        )?;
        let bundle = self.load_workspace_bundle(&request.workspace_id)?;

        Ok(NodeLeafDeleteResponse {
            summary: workspace_graph_summary(bundle.manifest, &bundle.graph),
            commit,
        })
    }

    pub fn upsert_thread_ref(
        &self,
        request: &ThreadRefUpsertRequest,
    ) -> Result<ThreadRefUpsertResponse, WorkspaceGraphToolError> {
        self.ensure_workspace_exists(&request.workspace_id)?;
        let patch = thread_ref_upsert_patch(&request.thread_ref, &request.provenance);
        let commit = self.persistence.apply_workspace_graph_patch(
            &request.workspace_id,
            &patch,
            request.expected_base_revision,
        )?;
        let bundle = self.load_workspace_bundle(&request.workspace_id)?;
        let thread_ref = bundle
            .graph
            .thread_ref(request.thread_ref.id())
            .map(GraphThreadRefSnapshot::from_thread_ref)
            .ok_or_else(|| WorkspaceGraphToolError::MissingThreadRefAfterWrite {
                workspace_id: request.workspace_id.as_str().to_string(),
                thread_ref_id: request.thread_ref.id().as_str().to_string(),
            })?;

        Ok(ThreadRefUpsertResponse {
            summary: workspace_graph_summary(bundle.manifest, &bundle.graph),
            commit,
            thread_ref,
        })
    }
}
