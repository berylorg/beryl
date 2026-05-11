use beryl_model::provenance::{MutationProvenance, MutationSource};
use beryl_model::semantic_graph::{
    ChecklistItemStatus, SemanticGraph, SemanticGraphPatch, SemanticGraphPatchOp,
    SemanticNodeDraft, SemanticNodeFacets, SemanticNodeId,
};

#[test]
fn repeated_node_upsert_preserves_provenance_and_child_order_as_noop() {
    let root_id = SemanticNodeId::new("root").unwrap();
    let first_id = SemanticNodeId::new("first").unwrap();
    let second_id = SemanticNodeId::new("second").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(root_id.clone(), "Root"),
                provenance: provenance(1),
            },
            set_root_op(&root_id, None, 2),
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(first_id.clone(), "First"),
                provenance: provenance(3),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(second_id.clone(), "Second"),
                provenance: provenance(4),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: first_id.clone(),
                parent_id: Some(root_id.clone()),
                index: None,
                provenance: provenance(5),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: second_id.clone(),
                parent_id: Some(root_id.clone()),
                index: None,
                provenance: provenance(6),
            },
        ]))
        .unwrap();

    let changed = graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(first_id.clone(), "First"),
                provenance: provenance(99),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: first_id.clone(),
                parent_id: Some(root_id.clone()),
                index: None,
                provenance: provenance(100),
            },
        ]))
        .unwrap();

    assert!(!changed);
    assert_eq!(
        graph.child_ids_of(&root_id).unwrap(),
        &[first_id.clone(), second_id]
    );
    assert_eq!(
        graph
            .root_order_provenance()
            .unwrap()
            .last_updated()
            .recorded_at_millis(),
        2
    );
    assert_eq!(
        graph
            .node(&first_id)
            .unwrap()
            .provenance()
            .last_updated()
            .recorded_at_millis(),
        3
    );
}

#[test]
fn repeated_root_placement_without_index_preserves_root_order_as_noop() {
    let first_id = SemanticNodeId::new("first").unwrap();
    let second_id = SemanticNodeId::new("second").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(first_id.clone(), "First"),
                provenance: provenance(1),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(second_id.clone(), "Second"),
                provenance: provenance(2),
            },
            set_root_op(&first_id, None, 3),
            set_root_op(&second_id, None, 4),
        ]))
        .unwrap();

    let changed = graph
        .apply_patch(&SemanticGraphPatch::from_operation(set_root_op(
            &first_id, None, 99,
        )))
        .unwrap();

    assert!(!changed);
    assert_eq!(graph.root_node_ids(), &[first_id, second_id]);
    assert_eq!(
        graph
            .root_order_provenance()
            .unwrap()
            .last_updated()
            .recorded_at_millis(),
        4
    );
}

#[test]
fn repeated_root_placement_at_current_index_preserves_root_order_as_noop() {
    let first_id = SemanticNodeId::new("first").unwrap();
    let second_id = SemanticNodeId::new("second").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(first_id.clone(), "First"),
                provenance: provenance(1),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(second_id.clone(), "Second"),
                provenance: provenance(2),
            },
            set_root_op(&first_id, None, 3),
            set_root_op(&second_id, None, 4),
        ]))
        .unwrap();

    let changed = graph
        .apply_patch(&SemanticGraphPatch::from_operation(set_root_op(
            &second_id,
            Some(1),
            99,
        )))
        .unwrap();

    assert!(!changed);
    assert_eq!(graph.root_node_ids(), &[first_id, second_id]);
    assert_eq!(
        graph
            .root_order_provenance()
            .unwrap()
            .last_updated()
            .recorded_at_millis(),
        4
    );
}

#[test]
fn repeated_checklist_status_write_preserves_provenance_as_noop() {
    let list_id = SemanticNodeId::new("list").unwrap();
    let item_id = SemanticNodeId::new("item").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: checklist_node(list_id.clone(), "List"),
                provenance: provenance(1),
            },
            set_root_op(&list_id, None, 2),
            SemanticGraphPatchOp::UpsertNode {
                node: checklist_item_node(item_id.clone(), "Item", ChecklistItemStatus::Done),
                provenance: provenance(3),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: item_id.clone(),
                parent_id: Some(list_id),
                index: None,
                provenance: provenance(4),
            },
        ]))
        .unwrap();

    let changed = graph
        .apply_patch(&SemanticGraphPatch::from_operation(
            SemanticGraphPatchOp::SetChecklistItemStatus {
                node_id: item_id.clone(),
                status: ChecklistItemStatus::Done,
                provenance: provenance(99),
            },
        ))
        .unwrap();

    assert!(!changed);
    assert_eq!(
        graph
            .node(&item_id)
            .unwrap()
            .provenance()
            .last_updated()
            .recorded_at_millis(),
        3
    );
}

fn provenance(recorded_at_millis: u64) -> MutationProvenance {
    MutationProvenance::new(
        "operator",
        recorded_at_millis,
        MutationSource::workspace_action("seed_graph").unwrap(),
        Some(100),
    )
    .unwrap()
}

fn topic_node(node_id: SemanticNodeId, title: &str) -> SemanticNodeDraft {
    SemanticNodeDraft::new(
        node_id,
        title,
        format!("{title} summary"),
        SemanticNodeFacets::topic(),
        None,
    )
}

fn checklist_node(node_id: SemanticNodeId, title: &str) -> SemanticNodeDraft {
    SemanticNodeDraft::new(
        node_id,
        title,
        format!("{title} summary"),
        SemanticNodeFacets::topic_and_checklist(),
        None,
    )
}

fn checklist_item_node(
    node_id: SemanticNodeId,
    title: &str,
    status: ChecklistItemStatus,
) -> SemanticNodeDraft {
    SemanticNodeDraft::new(
        node_id,
        title,
        format!("{title} summary"),
        SemanticNodeFacets::topic_and_checklist_item(),
        Some(status),
    )
}

fn set_root_op(
    node_id: &SemanticNodeId,
    index: Option<usize>,
    recorded_at_millis: u64,
) -> SemanticGraphPatchOp {
    SemanticGraphPatchOp::SetHardParent {
        child_id: node_id.clone(),
        parent_id: None,
        index,
        provenance: provenance(recorded_at_millis),
    }
}
