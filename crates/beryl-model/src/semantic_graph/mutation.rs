use std::collections::BTreeMap;

use crate::provenance::{ElementProvenance, MutationProvenance};

use super::{
    HardLink, OrderedChildren, OrderedRootNodes, SemanticGraph, SemanticGraphError,
    SemanticGraphPatch, SemanticGraphPatchOp, SemanticNodeDraft, SemanticNodeId, SoftLinkDraft,
    ThreadRefDraft, validate::ensure_node_exists,
};

impl SemanticGraph {
    pub fn apply_patch(&mut self, patch: &SemanticGraphPatch) -> Result<bool, SemanticGraphError> {
        let mut next = self.clone();

        for operation in patch.operations() {
            next.apply_operation(operation)?;
        }

        next.validate()?;

        if *self == next {
            return Ok(false);
        }

        *self = next;
        Ok(true)
    }

    fn apply_operation(
        &mut self,
        operation: &SemanticGraphPatchOp,
    ) -> Result<(), SemanticGraphError> {
        match operation {
            SemanticGraphPatchOp::UpsertNode { node, provenance } => {
                self.upsert_node(node.clone(), provenance.clone())
            }
            SemanticGraphPatchOp::SetHardParent {
                child_id,
                parent_id,
                index,
                provenance,
            } => self.set_hard_parent(child_id, parent_id.as_ref(), *index, provenance.clone()),
            SemanticGraphPatchOp::UpsertSoftLink { link, provenance } => {
                self.upsert_soft_link(link.clone(), provenance.clone())
            }
            SemanticGraphPatchOp::UpsertThreadRef {
                thread_ref,
                provenance,
            } => self.upsert_thread_ref(thread_ref.clone(), provenance.clone()),
            SemanticGraphPatchOp::DeleteNodeSubtree {
                node_id,
                provenance,
            } => self.delete_node_subtree(node_id, provenance.clone()),
            SemanticGraphPatchOp::DeleteNodeLeaf {
                node_id,
                provenance,
            } => self.delete_node_leaf(node_id, provenance.clone()),
            SemanticGraphPatchOp::SetChecklistItemStatus {
                node_id,
                status,
                provenance,
            } => self.set_checklist_item_status(node_id, *status, provenance.clone()),
        }
    }

    fn upsert_node(
        &mut self,
        node: SemanticNodeDraft,
        provenance: MutationProvenance,
    ) -> Result<(), SemanticGraphError> {
        if let Some(existing) = self.nodes.get_mut(&node.id) {
            existing.update_from_draft(node, provenance)?;
            return Ok(());
        }

        let record = super::SemanticNode::from_draft(node, provenance)?;
        self.nodes.insert(record.id.clone(), record);
        Ok(())
    }

    fn set_hard_parent(
        &mut self,
        child_id: &SemanticNodeId,
        parent_id: Option<&SemanticNodeId>,
        index: Option<usize>,
        provenance: MutationProvenance,
    ) -> Result<(), SemanticGraphError> {
        ensure_node_exists(&self.nodes, child_id)?;
        if let Some(parent_id) = parent_id {
            ensure_node_exists(&self.nodes, parent_id)?;
        }

        let current_parent_id = self
            .hard_links
            .get(child_id)
            .map(|link| link.parent_id.clone());
        if current_parent_id.as_ref() == parent_id {
            match (parent_id, index) {
                (Some(parent_id), Some(index)) => {
                    if child_index(&self.ordered_children, parent_id, child_id) == Some(index) {
                        return Ok(());
                    }
                }
                (None, Some(index)) => {
                    if root_index(self.ordered_roots.as_ref(), child_id) == Some(index) {
                        return Ok(());
                    }
                }
                (Some(_), None) => return Ok(()),
                (None, None) => {
                    if root_index(self.ordered_roots.as_ref(), child_id).is_some() {
                        return Ok(());
                    }
                }
            }
        }

        if let Some(current_parent_id) = current_parent_id.clone() {
            remove_child(
                &mut self.ordered_children,
                &current_parent_id,
                child_id,
                &provenance,
            );
        } else {
            remove_root(&mut self.ordered_roots, child_id, &provenance);
        }

        if let Some(parent_id) = parent_id {
            let link = self
                .hard_links
                .entry(child_id.clone())
                .or_insert_with(|| HardLink {
                    child_id: child_id.clone(),
                    parent_id: parent_id.clone(),
                    provenance: ElementProvenance::new(provenance.clone()),
                });
            link.parent_id = parent_id.clone();
            link.provenance.touch(provenance.clone());
            insert_child(
                &mut self.ordered_children,
                parent_id.clone(),
                child_id.clone(),
                index,
                provenance,
            )?;
        } else {
            self.hard_links.remove(child_id);
            insert_root(&mut self.ordered_roots, child_id.clone(), index, provenance)?;
        }

        Ok(())
    }

    fn upsert_soft_link(
        &mut self,
        link: SoftLinkDraft,
        provenance: MutationProvenance,
    ) -> Result<(), SemanticGraphError> {
        ensure_node_exists(&self.nodes, &link.source_id)?;
        ensure_node_exists(&self.nodes, &link.target_id)?;

        if let Some(existing) = self.soft_links.get_mut(&link.id) {
            existing.update_from_draft(link, provenance);
            return Ok(());
        }

        let record = super::SoftLink::from_draft(link, provenance);
        self.soft_links.insert(record.id.clone(), record);
        Ok(())
    }

    fn upsert_thread_ref(
        &mut self,
        thread_ref: ThreadRefDraft,
        provenance: MutationProvenance,
    ) -> Result<(), SemanticGraphError> {
        ensure_node_exists(&self.nodes, &thread_ref.node_id)?;

        if let Some(existing) = self.thread_refs.get_mut(&thread_ref.id) {
            existing.update_from_draft(thread_ref, provenance)?;
            return Ok(());
        }

        let record = super::ThreadRef::from_draft(thread_ref, provenance)?;
        self.thread_refs.insert(record.id.clone(), record);
        Ok(())
    }

    fn set_checklist_item_status(
        &mut self,
        node_id: &SemanticNodeId,
        status: super::ChecklistItemStatus,
        provenance: MutationProvenance,
    ) -> Result<(), SemanticGraphError> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| SemanticGraphError::MissingNode {
                node_id: node_id.clone(),
            })?;
        if !node.facets.has_checklist_item() {
            return Err(SemanticGraphError::InvalidChecklistItemStatus {
                node_id: node_id.clone(),
            });
        }
        if node.checklist_item_status == Some(status) {
            return Ok(());
        }

        node.checklist_item_status = Some(status);
        node.provenance.touch(provenance);
        Ok(())
    }
}

pub(super) fn remove_root(
    ordered_roots: &mut Option<OrderedRootNodes>,
    node_id: &SemanticNodeId,
    provenance: &MutationProvenance,
) {
    let Some(roots) = ordered_roots else {
        return;
    };

    let previous_len = roots.node_ids.len();
    roots.node_ids.retain(|candidate| candidate != node_id);
    if roots.node_ids.len() == previous_len {
        return;
    }
    if roots.node_ids.is_empty() {
        *ordered_roots = None;
    } else {
        roots.provenance.touch(provenance.clone());
    }
}

pub(super) fn remove_child(
    ordered_children: &mut BTreeMap<SemanticNodeId, OrderedChildren>,
    parent_id: &SemanticNodeId,
    child_id: &SemanticNodeId,
    provenance: &MutationProvenance,
) {
    if let Some(ordered) = ordered_children.get_mut(parent_id) {
        ordered.child_ids.retain(|candidate| candidate != child_id);
        if ordered.child_ids.is_empty() {
            ordered_children.remove(parent_id);
        } else {
            ordered.provenance.touch(provenance.clone());
        }
    }
}

fn insert_root(
    ordered_roots: &mut Option<OrderedRootNodes>,
    node_id: SemanticNodeId,
    index: Option<usize>,
    provenance: MutationProvenance,
) -> Result<(), SemanticGraphError> {
    let roots = ordered_roots.get_or_insert_with(|| OrderedRootNodes {
        node_ids: Vec::new(),
        provenance: ElementProvenance::new(provenance.clone()),
    });
    roots.node_ids.retain(|candidate| candidate != &node_id);

    let insertion_index = index.unwrap_or(roots.node_ids.len());
    if insertion_index > roots.node_ids.len() {
        return Err(SemanticGraphError::InvalidRootIndex {
            index: insertion_index,
            root_count: roots.node_ids.len(),
        });
    }

    roots.node_ids.insert(insertion_index, node_id);
    roots.provenance.touch(provenance);
    Ok(())
}

fn insert_child(
    ordered_children: &mut BTreeMap<SemanticNodeId, OrderedChildren>,
    parent_id: SemanticNodeId,
    child_id: SemanticNodeId,
    index: Option<usize>,
    provenance: MutationProvenance,
) -> Result<(), SemanticGraphError> {
    let ordered = ordered_children
        .entry(parent_id.clone())
        .or_insert_with(|| OrderedChildren {
            parent_id: parent_id.clone(),
            child_ids: Vec::new(),
            provenance: ElementProvenance::new(provenance.clone()),
        });
    ordered.child_ids.retain(|candidate| candidate != &child_id);

    let insertion_index = index.unwrap_or(ordered.child_ids.len());
    if insertion_index > ordered.child_ids.len() {
        return Err(SemanticGraphError::InvalidChildIndex {
            parent_id,
            index: insertion_index,
            child_count: ordered.child_ids.len(),
        });
    }

    ordered.child_ids.insert(insertion_index, child_id);
    ordered.provenance.touch(provenance);
    Ok(())
}

fn child_index(
    ordered_children: &BTreeMap<SemanticNodeId, OrderedChildren>,
    parent_id: &SemanticNodeId,
    child_id: &SemanticNodeId,
) -> Option<usize> {
    ordered_children
        .get(parent_id)?
        .child_ids
        .iter()
        .position(|candidate| candidate == child_id)
}

fn root_index(ordered_roots: Option<&OrderedRootNodes>, node_id: &SemanticNodeId) -> Option<usize> {
    ordered_roots?
        .node_ids
        .iter()
        .position(|candidate| candidate == node_id)
}
