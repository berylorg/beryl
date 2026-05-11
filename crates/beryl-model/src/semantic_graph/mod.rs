mod delete;
mod ids;
mod mutation;
mod patch;
mod query;
mod validate;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::conversation::ConversationThreadId;
use crate::provenance::{ElementProvenance, MutationProvenance};
use crate::workspace::ExecutionTargetId;
use validate::{validate_node, validate_thread_ref};

pub use ids::{SemanticGraphIdError, SemanticNodeId, SoftLinkId, SoftLinkKind, ThreadRefId};
pub use patch::{SemanticGraphError, SemanticGraphPatch, SemanticGraphPatchOp};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChecklistItemStatus {
    Todo,
    InProgress,
    Done,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticNodeFacets {
    topic: bool,
    checklist: bool,
    checklist_item: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticNodeDraft {
    id: SemanticNodeId,
    title: String,
    summary: String,
    facets: SemanticNodeFacets,
    checklist_item_status: Option<ChecklistItemStatus>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticNode {
    id: SemanticNodeId,
    title: String,
    summary: String,
    facets: SemanticNodeFacets,
    checklist_item_status: Option<ChecklistItemStatus>,
    provenance: ElementProvenance,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoftLinkDraft {
    id: SoftLinkId,
    source_id: SemanticNodeId,
    target_id: SemanticNodeId,
    kind: SoftLinkKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoftLink {
    id: SoftLinkId,
    source_id: SemanticNodeId,
    target_id: SemanticNodeId,
    kind: SoftLinkKind,
    provenance: ElementProvenance,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadRefDraft {
    id: ThreadRefId,
    node_id: SemanticNodeId,
    thread_id: ConversationThreadId,
    execution_target: ExecutionTargetId,
    label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadRef {
    id: ThreadRefId,
    node_id: SemanticNodeId,
    thread_id: ConversationThreadId,
    execution_target: ExecutionTargetId,
    label: String,
    provenance: ElementProvenance,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardLink {
    child_id: SemanticNodeId,
    parent_id: SemanticNodeId,
    provenance: ElementProvenance,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderedChildren {
    parent_id: SemanticNodeId,
    child_ids: Vec<SemanticNodeId>,
    provenance: ElementProvenance,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderedRootNodes {
    node_ids: Vec<SemanticNodeId>,
    provenance: ElementProvenance,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticGraph {
    #[serde(default)]
    nodes: BTreeMap<SemanticNodeId, SemanticNode>,
    #[serde(default)]
    ordered_roots: Option<OrderedRootNodes>,
    #[serde(default)]
    hard_links: BTreeMap<SemanticNodeId, HardLink>,
    #[serde(default)]
    ordered_children: BTreeMap<SemanticNodeId, OrderedChildren>,
    #[serde(default)]
    soft_links: BTreeMap<SoftLinkId, SoftLink>,
    #[serde(default)]
    thread_refs: BTreeMap<ThreadRefId, ThreadRef>,
}

impl SemanticNodeFacets {
    pub fn new(topic: bool, checklist: bool, checklist_item: bool) -> Result<Self, String> {
        if !topic && !checklist && !checklist_item {
            return Err("at least one semantic facet is required".to_string());
        }
        if checklist_item && !topic {
            return Err("ChecklistItem requires Topic".to_string());
        }
        if checklist && checklist_item {
            return Err("Checklist and ChecklistItem do not coexist in V1".to_string());
        }

        Ok(Self {
            topic,
            checklist,
            checklist_item,
        })
    }

    pub fn topic() -> Self {
        Self::new(true, false, false).expect("topic facets are valid")
    }

    pub fn checklist() -> Self {
        Self::new(false, true, false).expect("checklist facets are valid")
    }

    pub fn topic_and_checklist() -> Self {
        Self::new(true, true, false).expect("topic + checklist facets are valid")
    }

    pub fn topic_and_checklist_item() -> Self {
        Self::new(true, false, true).expect("topic + checklist-item facets are valid")
    }

    pub fn has_topic(&self) -> bool {
        self.topic
    }

    pub fn has_checklist(&self) -> bool {
        self.checklist
    }

    pub fn has_checklist_item(&self) -> bool {
        self.checklist_item
    }
}

impl SemanticNodeDraft {
    pub fn new(
        id: SemanticNodeId,
        title: impl Into<String>,
        summary: impl Into<String>,
        facets: SemanticNodeFacets,
        checklist_item_status: Option<ChecklistItemStatus>,
    ) -> Self {
        Self {
            id,
            title: title.into(),
            summary: summary.into(),
            facets,
            checklist_item_status,
        }
    }
}

impl SemanticNode {
    fn from_draft(
        draft: SemanticNodeDraft,
        provenance: MutationProvenance,
    ) -> Result<Self, SemanticGraphError> {
        let node = Self {
            id: draft.id,
            title: draft.title,
            summary: draft.summary,
            facets: draft.facets,
            checklist_item_status: draft.checklist_item_status,
            provenance: ElementProvenance::new(provenance),
        };
        validate_node(&node)?;
        Ok(node)
    }

    fn update_from_draft(
        &mut self,
        draft: SemanticNodeDraft,
        provenance: MutationProvenance,
    ) -> Result<(), SemanticGraphError> {
        if self.title == draft.title
            && self.summary == draft.summary
            && self.facets == draft.facets
            && self.checklist_item_status == draft.checklist_item_status
        {
            return Ok(());
        }

        self.title = draft.title;
        self.summary = draft.summary;
        self.facets = draft.facets;
        self.checklist_item_status = draft.checklist_item_status;
        self.provenance.touch(provenance);
        validate_node(self)
    }

    pub fn id(&self) -> &SemanticNodeId {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }

    pub fn facets(&self) -> &SemanticNodeFacets {
        &self.facets
    }

    pub fn checklist_item_status(&self) -> Option<ChecklistItemStatus> {
        self.checklist_item_status
    }

    pub fn provenance(&self) -> &ElementProvenance {
        &self.provenance
    }
}

impl SoftLinkDraft {
    pub fn new(
        id: SoftLinkId,
        source_id: SemanticNodeId,
        target_id: SemanticNodeId,
        kind: SoftLinkKind,
    ) -> Self {
        Self {
            id,
            source_id,
            target_id,
            kind,
        }
    }
}

impl SoftLink {
    fn from_draft(draft: SoftLinkDraft, provenance: MutationProvenance) -> Self {
        Self {
            id: draft.id,
            source_id: draft.source_id,
            target_id: draft.target_id,
            kind: draft.kind,
            provenance: ElementProvenance::new(provenance),
        }
    }

    fn update_from_draft(&mut self, draft: SoftLinkDraft, provenance: MutationProvenance) {
        if self.source_id == draft.source_id
            && self.target_id == draft.target_id
            && self.kind == draft.kind
        {
            return;
        }

        self.source_id = draft.source_id;
        self.target_id = draft.target_id;
        self.kind = draft.kind;
        self.provenance.touch(provenance);
    }

    pub fn id(&self) -> &SoftLinkId {
        &self.id
    }

    pub fn source_id(&self) -> &SemanticNodeId {
        &self.source_id
    }

    pub fn target_id(&self) -> &SemanticNodeId {
        &self.target_id
    }

    pub fn kind(&self) -> &SoftLinkKind {
        &self.kind
    }

    pub fn provenance(&self) -> &ElementProvenance {
        &self.provenance
    }
}

impl ThreadRefDraft {
    pub fn new(
        id: ThreadRefId,
        node_id: SemanticNodeId,
        thread_id: ConversationThreadId,
        execution_target: ExecutionTargetId,
        label: impl Into<String>,
    ) -> Self {
        Self {
            id,
            node_id,
            thread_id,
            execution_target,
            label: label.into(),
        }
    }
}

impl ThreadRef {
    fn from_draft(
        draft: ThreadRefDraft,
        provenance: MutationProvenance,
    ) -> Result<Self, SemanticGraphError> {
        let thread_ref = Self {
            id: draft.id,
            node_id: draft.node_id,
            thread_id: draft.thread_id,
            execution_target: draft.execution_target,
            label: draft.label,
            provenance: ElementProvenance::new(provenance),
        };
        validate_thread_ref(&thread_ref)?;
        Ok(thread_ref)
    }

    fn update_from_draft(
        &mut self,
        draft: ThreadRefDraft,
        provenance: MutationProvenance,
    ) -> Result<(), SemanticGraphError> {
        if self.node_id == draft.node_id
            && self.thread_id == draft.thread_id
            && self.execution_target == draft.execution_target
            && self.label == draft.label
        {
            return Ok(());
        }

        self.node_id = draft.node_id;
        self.thread_id = draft.thread_id;
        self.execution_target = draft.execution_target;
        self.label = draft.label;
        self.provenance.touch(provenance);
        validate_thread_ref(self)
    }

    pub fn id(&self) -> &ThreadRefId {
        &self.id
    }

    pub fn node_id(&self) -> &SemanticNodeId {
        &self.node_id
    }

    pub fn thread_id(&self) -> &ConversationThreadId {
        &self.thread_id
    }

    pub fn execution_target(&self) -> &ExecutionTargetId {
        &self.execution_target
    }

    pub fn matches_thread_target(
        &self,
        thread_id: &ConversationThreadId,
        execution_target: &ExecutionTargetId,
    ) -> bool {
        self.thread_id == *thread_id && self.execution_target == *execution_target
    }

    pub fn execution_target_in_scope<'a>(
        &self,
        execution_targets: impl IntoIterator<Item = &'a ExecutionTargetId>,
    ) -> bool {
        execution_targets
            .into_iter()
            .any(|execution_target| execution_target == &self.execution_target)
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn provenance(&self) -> &ElementProvenance {
        &self.provenance
    }
}

impl SemanticGraph {
    pub fn node(&self, node_id: &SemanticNodeId) -> Option<&SemanticNode> {
        self.nodes.get(node_id)
    }

    pub fn root_node_ids(&self) -> &[SemanticNodeId] {
        self.ordered_roots
            .as_ref()
            .map_or(&[], |roots| roots.node_ids.as_slice())
    }

    pub fn root_nodes(&self) -> impl Iterator<Item = &SemanticNode> {
        self.root_node_ids()
            .iter()
            .filter_map(|node_id| self.node(node_id))
    }

    pub fn root_order_provenance(&self) -> Option<&ElementProvenance> {
        self.ordered_roots.as_ref().map(|roots| &roots.provenance)
    }

    pub fn parent_id_of(&self, node_id: &SemanticNodeId) -> Option<&SemanticNodeId> {
        self.hard_links.get(node_id).map(|link| &link.parent_id)
    }

    pub fn child_ids_of(&self, parent_id: &SemanticNodeId) -> Option<&[SemanticNodeId]> {
        self.ordered_children
            .get(parent_id)
            .map(|ordered| ordered.child_ids.as_slice())
    }

    pub fn soft_link(&self, link_id: &SoftLinkId) -> Option<&SoftLink> {
        self.soft_links.get(link_id)
    }

    pub fn thread_ref(&self, thread_ref_id: &ThreadRefId) -> Option<&ThreadRef> {
        self.thread_refs.get(thread_ref_id)
    }
}
