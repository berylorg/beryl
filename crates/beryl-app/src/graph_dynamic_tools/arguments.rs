use beryl_model::{
    provenance::MutationProvenance,
    semantic_graph::{
        ChecklistItemStatus, SemanticGraphIdError, SemanticGraphPatch, SemanticGraphPatchOp,
        SemanticNodeDraft, SemanticNodeFacets, SemanticNodeId, SoftLinkDraft, SoftLinkId,
        SoftLinkKind,
    },
    workspace::BerylWorkspaceId,
};
use serde::Deserialize;

use crate::graph_tools::{ChecklistReadRequest, GraphNeighborhoodRequest};

use super::{DynamicGraphToolError, MAX_DYNAMIC_NODE_SUMMARY_CHARS, MAX_DYNAMIC_NODE_TITLE_CHARS};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EmptyArguments {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct GraphNeighborhoodArguments {
    #[serde(default)]
    anchor_node_id: Option<String>,
    #[serde(default = "default_parent_depth")]
    parent_depth: usize,
    #[serde(default = "default_child_depth")]
    child_depth: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ChecklistReadArguments {
    checklist_node_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct UpsertGraphNodeArguments {
    node_id: String,
    parent_id: NullableGraphIdArgument,
    title: String,
    summary: String,
    topic: bool,
    checklist: bool,
    checklist_item: bool,
    #[serde(default)]
    checklist_item_status: Option<DynamicChecklistItemStatus>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SetGraphNodeParentArguments {
    child_id: String,
    parent_id: NullableGraphIdArgument,
    #[serde(default)]
    index: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct UpsertGraphSoftLinkArguments {
    link_id: String,
    source_id: String,
    target_id: String,
    kind: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SetChecklistItemStatusArguments {
    node_id: String,
    status: DynamicChecklistItemStatus,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DynamicChecklistItemStatus {
    Todo,
    InProgress,
    Done,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum NullableGraphIdArgument {
    Id(String),
    Null(()),
}

impl GraphNeighborhoodArguments {
    pub(super) fn into_request(
        self,
        workspace_id: &BerylWorkspaceId,
    ) -> Result<GraphNeighborhoodRequest, DynamicGraphToolError> {
        Ok(GraphNeighborhoodRequest {
            workspace_id: workspace_id.clone(),
            anchor_node_id: self
                .anchor_node_id
                .map(|node_id| semantic_node_id("anchorNodeId", node_id))
                .transpose()?,
            parent_depth: self.parent_depth,
            child_depth: self.child_depth,
        })
    }
}

impl ChecklistReadArguments {
    pub(super) fn into_request(
        self,
        workspace_id: &BerylWorkspaceId,
    ) -> Result<ChecklistReadRequest, DynamicGraphToolError> {
        Ok(ChecklistReadRequest {
            workspace_id: workspace_id.clone(),
            checklist_node_id: semantic_node_id("checklistNodeId", self.checklist_node_id)?,
        })
    }
}

impl UpsertGraphNodeArguments {
    pub(super) fn into_patch(
        self,
        provenance: MutationProvenance,
    ) -> Result<SemanticGraphPatch, DynamicGraphToolError> {
        let node_id = semantic_node_id("nodeId", self.node_id)?;
        let parent_id = self
            .parent_id
            .into_option()
            .map(|node_id| semantic_node_id("parentId", node_id))
            .transpose()?;
        let node_operation = SemanticGraphPatchOp::UpsertNode {
            node: SemanticNodeDraft::new(
                node_id.clone(),
                validated_text("title", self.title, 1, MAX_DYNAMIC_NODE_TITLE_CHARS)?,
                validated_text("summary", self.summary, 0, MAX_DYNAMIC_NODE_SUMMARY_CHARS)?,
                semantic_node_facets("facets", self.topic, self.checklist, self.checklist_item)?,
                checklist_item_status_for_node(self.checklist_item, self.checklist_item_status)?,
            ),
            provenance: provenance.clone(),
        };
        let parent_operation = SemanticGraphPatchOp::SetHardParent {
            child_id: node_id,
            parent_id,
            index: None,
            provenance,
        };
        Ok(SemanticGraphPatch::new(vec![
            node_operation,
            parent_operation,
        ]))
    }
}

impl SetGraphNodeParentArguments {
    pub(super) fn into_patch(
        self,
        provenance: MutationProvenance,
    ) -> Result<SemanticGraphPatch, DynamicGraphToolError> {
        let operation = SemanticGraphPatchOp::SetHardParent {
            child_id: semantic_node_id("childId", self.child_id)?,
            parent_id: self
                .parent_id
                .into_option()
                .map(|node_id| semantic_node_id("parentId", node_id))
                .transpose()?,
            index: self.index,
            provenance,
        };
        Ok(SemanticGraphPatch::from_operation(operation))
    }
}

impl UpsertGraphSoftLinkArguments {
    pub(super) fn into_patch(
        self,
        provenance: MutationProvenance,
    ) -> Result<SemanticGraphPatch, DynamicGraphToolError> {
        let operation = SemanticGraphPatchOp::UpsertSoftLink {
            link: SoftLinkDraft::new(
                soft_link_id("linkId", self.link_id)?,
                semantic_node_id("sourceId", self.source_id)?,
                semantic_node_id("targetId", self.target_id)?,
                soft_link_kind("kind", self.kind)?,
            ),
            provenance,
        };
        Ok(SemanticGraphPatch::from_operation(operation))
    }
}

impl SetChecklistItemStatusArguments {
    pub(super) fn into_patch(
        self,
        provenance: MutationProvenance,
    ) -> Result<SemanticGraphPatch, DynamicGraphToolError> {
        let operation = SemanticGraphPatchOp::SetChecklistItemStatus {
            node_id: semantic_node_id("nodeId", self.node_id)?,
            status: self.status.into_checklist_item_status(),
            provenance,
        };
        Ok(SemanticGraphPatch::from_operation(operation))
    }
}

impl DynamicChecklistItemStatus {
    fn into_checklist_item_status(self) -> ChecklistItemStatus {
        match self {
            Self::Todo => ChecklistItemStatus::Todo,
            Self::InProgress => ChecklistItemStatus::InProgress,
            Self::Done => ChecklistItemStatus::Done,
        }
    }
}

impl NullableGraphIdArgument {
    fn into_option(self) -> Option<String> {
        match self {
            Self::Id(id) => Some(id),
            Self::Null(()) => None,
        }
    }
}

fn semantic_node_id(
    field: &'static str,
    value: String,
) -> Result<SemanticNodeId, DynamicGraphToolError> {
    SemanticNodeId::new(value).map_err(|source| invalid_id(field, source))
}

fn soft_link_id(field: &'static str, value: String) -> Result<SoftLinkId, DynamicGraphToolError> {
    SoftLinkId::new(value).map_err(|source| invalid_id(field, source))
}

fn soft_link_kind(
    field: &'static str,
    value: String,
) -> Result<SoftLinkKind, DynamicGraphToolError> {
    SoftLinkKind::new(value).map_err(|source| invalid_id(field, source))
}

fn invalid_id(field: &'static str, source: SemanticGraphIdError) -> DynamicGraphToolError {
    DynamicGraphToolError::InvalidField {
        field,
        detail: source.to_string(),
    }
}

fn semantic_node_facets(
    field: &'static str,
    topic: bool,
    checklist: bool,
    checklist_item: bool,
) -> Result<SemanticNodeFacets, DynamicGraphToolError> {
    SemanticNodeFacets::new(topic, checklist, checklist_item)
        .map_err(|detail| DynamicGraphToolError::InvalidField { field, detail })
}

fn checklist_item_status_for_node(
    checklist_item: bool,
    status: Option<DynamicChecklistItemStatus>,
) -> Result<Option<ChecklistItemStatus>, DynamicGraphToolError> {
    match (checklist_item, status) {
        (true, Some(status)) => Ok(Some(status.into_checklist_item_status())),
        (true, None) => Err(DynamicGraphToolError::InvalidField {
            field: "checklistItemStatus",
            detail: "is required when checklistItem is true".to_string(),
        }),
        (false, Some(_)) => Err(DynamicGraphToolError::InvalidField {
            field: "checklistItemStatus",
            detail: "must be omitted unless checklistItem is true".to_string(),
        }),
        (false, None) => Ok(None),
    }
}

fn validated_text(
    field: &'static str,
    value: String,
    min_chars: usize,
    max_chars: usize,
) -> Result<String, DynamicGraphToolError> {
    let char_count = value.chars().count();
    if char_count < min_chars {
        return Err(DynamicGraphToolError::InvalidField {
            field,
            detail: format!("must contain at least {min_chars} character(s)"),
        });
    }
    if char_count > max_chars {
        return Err(DynamicGraphToolError::InvalidField {
            field,
            detail: format!("length {char_count} exceeds the supported limit {max_chars}"),
        });
    }
    Ok(value)
}

pub(super) fn default_parent_depth() -> usize {
    1
}

pub(super) fn default_child_depth() -> usize {
    1
}
