use std::collections::{BTreeMap, BTreeSet};

use super::{
    HardLink, OrderedChildren, OrderedRootNodes, SemanticGraph, SemanticGraphError, SemanticNode,
    SemanticNodeFacets, SemanticNodeId, SoftLink, SoftLinkId, ThreadRef, ThreadRefId,
};

impl SemanticGraph {
    pub(super) fn validate(&self) -> Result<(), SemanticGraphError> {
        for node in self.nodes.values() {
            validate_node(node)?;
        }

        validate_hard_forest(
            &self.nodes,
            self.ordered_roots.as_ref(),
            &self.hard_links,
            &self.ordered_children,
        )?;
        validate_soft_links(&self.nodes, &self.soft_links)?;
        validate_thread_refs(&self.nodes, &self.thread_refs)?;

        Ok(())
    }
}

pub(super) fn ensure_node_exists(
    nodes: &BTreeMap<SemanticNodeId, SemanticNode>,
    node_id: &SemanticNodeId,
) -> Result<(), SemanticGraphError> {
    if nodes.contains_key(node_id) {
        Ok(())
    } else {
        Err(SemanticGraphError::MissingNode {
            node_id: node_id.clone(),
        })
    }
}

pub(super) fn validate_node(node: &SemanticNode) -> Result<(), SemanticGraphError> {
    if node.title.trim().is_empty() {
        return Err(SemanticGraphError::EmptyNodeTitle {
            node_id: node.id.clone(),
        });
    }

    if let Err(detail) = SemanticNodeFacets::new(
        node.facets.topic,
        node.facets.checklist,
        node.facets.checklist_item,
    ) {
        return Err(SemanticGraphError::InvalidNodeFacets {
            node_id: node.id.clone(),
            detail,
        });
    }

    if node.facets.has_checklist_item() != node.checklist_item_status.is_some() {
        return Err(SemanticGraphError::InvalidChecklistItemStatus {
            node_id: node.id.clone(),
        });
    }

    Ok(())
}

fn validate_hard_forest(
    nodes: &BTreeMap<SemanticNodeId, SemanticNode>,
    ordered_roots: Option<&OrderedRootNodes>,
    hard_links: &BTreeMap<SemanticNodeId, HardLink>,
    ordered_children: &BTreeMap<SemanticNodeId, OrderedChildren>,
) -> Result<(), SemanticGraphError> {
    if nodes.is_empty() {
        if ordered_roots.is_some() {
            return Err(SemanticGraphError::InvalidHardForest {
                detail: "empty semantic graphs must not store root ordering".to_string(),
            });
        }
        if !hard_links.is_empty() || !ordered_children.is_empty() {
            return Err(SemanticGraphError::InvalidHardForest {
                detail: "empty semantic graphs must not store hard links".to_string(),
            });
        }
        return Ok(());
    }

    let Some(ordered_roots) = ordered_roots else {
        return Err(SemanticGraphError::InvalidHardForest {
            detail: "non-empty semantic graphs must have ordered roots".to_string(),
        });
    };
    if ordered_roots.node_ids.is_empty() {
        return Err(SemanticGraphError::InvalidHardForest {
            detail: "non-empty semantic graphs must have at least one root".to_string(),
        });
    }

    let mut root_ids = BTreeSet::new();
    for root_id in &ordered_roots.node_ids {
        ensure_node_exists(nodes, root_id)?;
        if hard_links.contains_key(root_id) {
            return Err(SemanticGraphError::InvalidHardForest {
                detail: format!(
                    "root-level node {} also has a hard-link parent",
                    root_id.as_str()
                ),
            });
        }
        if !root_ids.insert(root_id.clone()) {
            return Err(SemanticGraphError::InvalidHardForest {
                detail: format!(
                    "root-level node {} appears more than once",
                    root_id.as_str()
                ),
            });
        }
    }

    let mut listed_children = BTreeSet::new();
    for (child_id, link) in hard_links {
        ensure_node_exists(nodes, child_id)?;
        ensure_node_exists(nodes, &link.parent_id)?;
        if child_id == &link.parent_id {
            return Err(SemanticGraphError::InvalidHardForest {
                detail: format!("node {} cannot be its own parent", child_id.as_str()),
            });
        }
        if root_ids.contains(child_id) {
            return Err(SemanticGraphError::InvalidHardForest {
                detail: format!(
                    "child {} cannot also be listed as a root-level node",
                    child_id.as_str()
                ),
            });
        }
    }

    for (parent_id, ordered) in ordered_children {
        ensure_node_exists(nodes, parent_id)?;
        if ordered.child_ids.is_empty() {
            return Err(SemanticGraphError::InvalidHardForest {
                detail: format!(
                    "parent {} stores an empty child ordering",
                    parent_id.as_str()
                ),
            });
        }

        let mut unique_children = BTreeSet::new();
        for child_id in &ordered.child_ids {
            ensure_node_exists(nodes, child_id)?;
            let Some(link) = hard_links.get(child_id) else {
                return Err(SemanticGraphError::InvalidHardForest {
                    detail: format!(
                        "child {} appears under parent {} without a hard-link record",
                        child_id.as_str(),
                        parent_id.as_str()
                    ),
                });
            };
            if &link.parent_id != parent_id {
                return Err(SemanticGraphError::InvalidHardForest {
                    detail: format!(
                        "child {} is ordered under parent {} but links to parent {}",
                        child_id.as_str(),
                        parent_id.as_str(),
                        link.parent_id.as_str()
                    ),
                });
            }
            if !unique_children.insert(child_id.clone()) {
                return Err(SemanticGraphError::InvalidHardForest {
                    detail: format!(
                        "child {} appears more than once under parent {}",
                        child_id.as_str(),
                        parent_id.as_str()
                    ),
                });
            }
            listed_children.insert(child_id.clone());
        }

        if nodes[parent_id].facets.has_checklist() {
            for child_id in &ordered.child_ids {
                if !nodes[child_id].facets.has_checklist_item() {
                    return Err(SemanticGraphError::InvalidHardForest {
                        detail: format!(
                            "checklist parent {} owns non-checklist child {}",
                            parent_id.as_str(),
                            child_id.as_str()
                        ),
                    });
                }
            }
        }
    }

    for child_id in hard_links.keys() {
        if !listed_children.contains(child_id) {
            return Err(SemanticGraphError::InvalidHardForest {
                detail: format!(
                    "child {} has a hard-link parent but is missing from ordered children",
                    child_id.as_str()
                ),
            });
        }
    }

    for node in nodes.values() {
        if node.facets.has_checklist_item() {
            let Some(parent_id) = hard_links.get(&node.id).map(|link| &link.parent_id) else {
                return Err(SemanticGraphError::InvalidHardForest {
                    detail: format!(
                        "checklist-item node {} must have a checklist parent",
                        node.id.as_str()
                    ),
                });
            };
            if !nodes[parent_id].facets.has_checklist() {
                return Err(SemanticGraphError::InvalidHardForest {
                    detail: format!(
                        "checklist-item node {} must have a checklist parent",
                        node.id.as_str()
                    ),
                });
            }
        }
    }

    for node_id in nodes.keys() {
        let has_parent = hard_links.contains_key(node_id);
        let is_ordered_root = root_ids.contains(node_id);
        if has_parent == is_ordered_root {
            return Err(SemanticGraphError::InvalidHardForest {
                detail: format!(
                    "node {} must be exactly one of root-level or hard-linked child",
                    node_id.as_str()
                ),
            });
        }
    }

    let mut visited = BTreeSet::new();
    let mut stack: Vec<_> = ordered_roots.node_ids.iter().rev().cloned().collect();
    while let Some(parent_id) = stack.pop() {
        if !visited.insert(parent_id.clone()) {
            return Err(SemanticGraphError::InvalidHardForest {
                detail: format!(
                    "node {} is reachable from more than one root or through a cycle",
                    parent_id.as_str()
                ),
            });
        }
        if let Some(children) = ordered_children.get(&parent_id) {
            stack.extend(children.child_ids.iter().rev().cloned());
        }
    }

    if visited.len() != nodes.len() {
        return Err(SemanticGraphError::InvalidHardForest {
            detail: "hard semantic forest is disconnected or cyclic".to_string(),
        });
    }

    Ok(())
}

fn validate_soft_links(
    nodes: &BTreeMap<SemanticNodeId, SemanticNode>,
    soft_links: &BTreeMap<SoftLinkId, SoftLink>,
) -> Result<(), SemanticGraphError> {
    let mut relations = BTreeMap::new();
    for (link_id, link) in soft_links {
        ensure_node_exists(nodes, &link.source_id)?;
        ensure_node_exists(nodes, &link.target_id)?;

        let relation = (
            link.source_id.clone(),
            link.target_id.clone(),
            link.kind.clone(),
        );
        if let Some(existing_link_id) = relations.insert(relation, link_id.clone()) {
            return Err(SemanticGraphError::DuplicateSoftLinkRelation {
                source_id: link.source_id.clone(),
                target_id: link.target_id.clone(),
                kind: link.kind.clone(),
                existing_link_id,
                conflicting_link_id: link_id.clone(),
            });
        }
    }

    Ok(())
}

fn validate_thread_refs(
    nodes: &BTreeMap<SemanticNodeId, SemanticNode>,
    thread_refs: &BTreeMap<ThreadRefId, ThreadRef>,
) -> Result<(), SemanticGraphError> {
    let mut bindings = BTreeMap::new();
    for (thread_ref_id, thread_ref) in thread_refs {
        ensure_node_exists(nodes, &thread_ref.node_id)?;
        validate_thread_ref(thread_ref)?;

        let binding = (thread_ref.node_id.clone(), thread_ref.thread_id.clone());
        if let Some(existing_thread_ref_id) = bindings.insert(binding, thread_ref_id.clone()) {
            return Err(SemanticGraphError::DuplicateThreadRefBinding {
                node_id: thread_ref.node_id.clone(),
                thread_id: thread_ref.thread_id.clone(),
                existing_thread_ref_id,
                conflicting_thread_ref_id: thread_ref_id.clone(),
            });
        }
    }

    Ok(())
}

pub(super) fn validate_thread_ref(thread_ref: &ThreadRef) -> Result<(), SemanticGraphError> {
    if thread_ref.label.trim().is_empty() {
        return Err(SemanticGraphError::EmptyThreadRefLabel {
            thread_ref_id: thread_ref.id.clone(),
        });
    }

    Ok(())
}
