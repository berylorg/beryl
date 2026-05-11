use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::conversation::ConversationThreadId;
use crate::provenance::MutationProvenance;

use super::{
    ChecklistItemStatus, SemanticNodeDraft, SemanticNodeId, SoftLinkDraft, SoftLinkId,
    SoftLinkKind, ThreadRefDraft, ThreadRefId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticGraphError {
    MissingNode {
        node_id: SemanticNodeId,
    },
    NonLeafNode {
        node_id: SemanticNodeId,
    },
    EmptyNodeTitle {
        node_id: SemanticNodeId,
    },
    InvalidNodeFacets {
        node_id: SemanticNodeId,
        detail: String,
    },
    InvalidChecklistItemStatus {
        node_id: SemanticNodeId,
    },
    InvalidChildIndex {
        parent_id: SemanticNodeId,
        index: usize,
        child_count: usize,
    },
    InvalidRootIndex {
        index: usize,
        root_count: usize,
    },
    InvalidHardForest {
        detail: String,
    },
    DuplicateSoftLinkRelation {
        source_id: SemanticNodeId,
        target_id: SemanticNodeId,
        kind: SoftLinkKind,
        existing_link_id: SoftLinkId,
        conflicting_link_id: SoftLinkId,
    },
    EmptyThreadRefLabel {
        thread_ref_id: ThreadRefId,
    },
    DuplicateThreadRefBinding {
        node_id: SemanticNodeId,
        thread_id: ConversationThreadId,
        existing_thread_ref_id: ThreadRefId,
        conflicting_thread_ref_id: ThreadRefId,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticGraphPatch {
    #[serde(default)]
    operations: Vec<SemanticGraphPatchOp>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticGraphPatchOp {
    UpsertNode {
        node: SemanticNodeDraft,
        provenance: MutationProvenance,
    },
    SetHardParent {
        child_id: SemanticNodeId,
        parent_id: Option<SemanticNodeId>,
        index: Option<usize>,
        provenance: MutationProvenance,
    },
    UpsertSoftLink {
        link: SoftLinkDraft,
        provenance: MutationProvenance,
    },
    UpsertThreadRef {
        thread_ref: ThreadRefDraft,
        provenance: MutationProvenance,
    },
    DeleteNodeSubtree {
        node_id: SemanticNodeId,
        provenance: MutationProvenance,
    },
    DeleteNodeLeaf {
        node_id: SemanticNodeId,
        provenance: MutationProvenance,
    },
    SetChecklistItemStatus {
        node_id: SemanticNodeId,
        status: ChecklistItemStatus,
        provenance: MutationProvenance,
    },
}

impl SemanticGraphPatch {
    pub fn new(operations: Vec<SemanticGraphPatchOp>) -> Self {
        Self { operations }
    }

    pub fn from_operation(operation: SemanticGraphPatchOp) -> Self {
        Self::new(vec![operation])
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    pub fn operations(&self) -> &[SemanticGraphPatchOp] {
        &self.operations
    }
}

impl fmt::Display for SemanticGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingNode { node_id } => {
                write!(f, "semantic graph node {} does not exist", node_id.as_str())
            }
            Self::NonLeafNode { node_id } => write!(
                f,
                "semantic graph node {} has hard children and is not a leaf",
                node_id.as_str()
            ),
            Self::EmptyNodeTitle { node_id } => write!(
                f,
                "semantic graph node {} must have a non-empty title",
                node_id.as_str()
            ),
            Self::InvalidNodeFacets { node_id, detail } => write!(
                f,
                "semantic graph node {} has invalid facets: {detail}",
                node_id.as_str()
            ),
            Self::InvalidChecklistItemStatus { node_id } => write!(
                f,
                "semantic graph node {} has an invalid checklist-item status",
                node_id.as_str()
            ),
            Self::InvalidChildIndex {
                parent_id,
                index,
                child_count,
            } => write!(
                f,
                "child index {index} is invalid for parent {} with {child_count} children",
                parent_id.as_str()
            ),
            Self::InvalidRootIndex { index, root_count } => write!(
                f,
                "root index {index} is invalid for {root_count} root-level nodes"
            ),
            Self::InvalidHardForest { detail } => {
                write!(f, "semantic graph hard forest is invalid: {detail}")
            }
            Self::DuplicateSoftLinkRelation {
                source_id,
                target_id,
                kind,
                existing_link_id,
                conflicting_link_id,
            } => write!(
                f,
                "soft-link relation {} --{}--> {} already exists as {} and conflicts with {}",
                source_id.as_str(),
                kind.as_str(),
                target_id.as_str(),
                existing_link_id.as_str(),
                conflicting_link_id.as_str()
            ),
            Self::EmptyThreadRefLabel { thread_ref_id } => write!(
                f,
                "thread ref {} must have a non-empty label",
                thread_ref_id.as_str()
            ),
            Self::DuplicateThreadRefBinding {
                node_id,
                thread_id,
                existing_thread_ref_id,
                conflicting_thread_ref_id,
            } => write!(
                f,
                "thread {} is already attached to node {} as {} and conflicts with {}",
                thread_id.as_str(),
                node_id.as_str(),
                existing_thread_ref_id.as_str(),
                conflicting_thread_ref_id.as_str()
            ),
        }
    }
}

impl Error for SemanticGraphError {}
