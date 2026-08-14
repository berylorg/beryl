use std::collections::{BTreeMap, BTreeSet};

use crate::provenance::MutationProvenance;

use super::{
    OrderedChildren, SemanticGraph, SemanticGraphError, SemanticNodeId,
    mutation::{remove_child, remove_root},
    validate::ensure_node_exists,
};

impl SemanticGraph {
    pub(super) fn delete_node_leaf(
        &mut self,
        node_id: &SemanticNodeId,
        provenance: MutationProvenance,
    ) -> Result<(), SemanticGraphError> {
        ensure_node_exists(&self.nodes, node_id)?;
        if self
            .ordered_children
            .get(node_id)
            .is_some_and(|children| !children.child_ids.is_empty())
        {
            return Err(SemanticGraphError::NonLeafNode {
                node_id: node_id.clone(),
            });
        }

        let deletion_set = BTreeSet::from([node_id.clone()]);
        self.delete_node_set(node_id, &deletion_set, provenance);
        Ok(())
    }

    pub(super) fn delete_node_subtree(
        &mut self,
        node_id: &SemanticNodeId,
        provenance: MutationProvenance,
    ) -> Result<(), SemanticGraphError> {
        ensure_node_exists(&self.nodes, node_id)?;

        let deletion_set = collect_hard_subtree_node_ids(&self.ordered_children, node_id);
        self.delete_node_set(node_id, &deletion_set, provenance);

        Ok(())
    }

    fn delete_node_set(
        &mut self,
        root_id: &SemanticNodeId,
        deletion_set: &BTreeSet<SemanticNodeId>,
        provenance: MutationProvenance,
    ) {
        if let Some(parent_id) = self
            .hard_links
            .get(root_id)
            .map(|link| link.parent_id.clone())
        {
            remove_child(&mut self.ordered_children, &parent_id, root_id, &provenance);
        } else {
            remove_root(&mut self.ordered_roots, root_id, &provenance);
        }

        for deleted_node_id in deletion_set {
            self.nodes.remove(deleted_node_id);
            self.hard_links.remove(deleted_node_id);
            self.ordered_children.remove(deleted_node_id);
        }

        self.soft_links.retain(|_, link| {
            !deletion_set.contains(link.source_id()) && !deletion_set.contains(link.target_id())
        });
        self.thread_refs
            .retain(|_, thread_ref| !deletion_set.contains(thread_ref.node_id()));
    }
}

fn collect_hard_subtree_node_ids(
    ordered_children: &BTreeMap<SemanticNodeId, OrderedChildren>,
    root_id: &SemanticNodeId,
) -> BTreeSet<SemanticNodeId> {
    let mut subtree = BTreeSet::new();
    let mut stack = vec![root_id.clone()];

    while let Some(node_id) = stack.pop() {
        if !subtree.insert(node_id.clone()) {
            continue;
        }
        if let Some(children) = ordered_children.get(&node_id) {
            stack.extend(children.child_ids.iter().cloned());
        }
    }

    subtree
}
