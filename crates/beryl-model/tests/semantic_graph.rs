use beryl_model::conversation::ConversationThreadId;
use beryl_model::provenance::{MutationProvenance, MutationSource};
use beryl_model::semantic_graph::{
    ChecklistItemStatus, SemanticGraph, SemanticGraphPatch, SemanticGraphPatchOp,
    SemanticNodeDraft, SemanticNodeFacets, SemanticNodeId, SoftLinkDraft, SoftLinkId, SoftLinkKind,
    ThreadRefDraft, ThreadRefId,
};
use beryl_model::workspace::WorkspaceId;

#[test]
fn checklist_item_facets_require_topic() {
    assert!(SemanticNodeFacets::new(false, false, true).is_err());
    assert!(SemanticNodeFacets::new(false, false, false).is_err());
}

#[test]
fn invalid_patch_rolls_back_without_partial_graph_changes() {
    let root_id = SemanticNodeId::new("root").unwrap();
    let item_id = SemanticNodeId::new("item").unwrap();
    let mut graph = SemanticGraph::default();

    let error = graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(root_id.clone(), "Root"),
                provenance: provenance(1),
            },
            set_root_op(&root_id, None, 2),
            SemanticGraphPatchOp::UpsertNode {
                node: checklist_item_node(item_id.clone(), "Item", ChecklistItemStatus::Todo),
                provenance: provenance(3),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: item_id.clone(),
                parent_id: Some(root_id),
                index: None,
                provenance: provenance(4),
            },
        ]))
        .unwrap_err();

    assert!(error.to_string().contains("checklist parent"));
    assert!(graph.node(&item_id).is_none());
    assert!(graph.root_node_ids().is_empty());
}

#[test]
fn checklist_children_keep_order_and_status() {
    let list_id = SemanticNodeId::new("list").unwrap();
    let first_id = SemanticNodeId::new("first").unwrap();
    let second_id = SemanticNodeId::new("second").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: checklist_node(list_id.clone(), "List"),
                provenance: provenance(1),
            },
            set_root_op(&list_id, None, 2),
            SemanticGraphPatchOp::UpsertNode {
                node: checklist_item_node(first_id.clone(), "First", ChecklistItemStatus::Todo),
                provenance: provenance(3),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: checklist_item_node(
                    second_id.clone(),
                    "Second",
                    ChecklistItemStatus::InProgress,
                ),
                provenance: provenance(4),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: first_id.clone(),
                parent_id: Some(list_id.clone()),
                index: None,
                provenance: provenance(5),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: second_id.clone(),
                parent_id: Some(list_id.clone()),
                index: Some(0),
                provenance: provenance(6),
            },
        ]))
        .unwrap();

    let ordered_children = graph.child_ids_of(&list_id).unwrap();
    assert_eq!(ordered_children, &[second_id.clone(), first_id.clone()]);
    assert_eq!(graph.root_node_ids(), &[list_id]);
    assert_eq!(
        graph.node(&second_id).unwrap().checklist_item_status(),
        Some(ChecklistItemStatus::InProgress)
    );
}

#[test]
fn root_nodes_preserve_explicit_order_and_query_records() {
    let first_id = SemanticNodeId::new("first_root").unwrap();
    let second_id = SemanticNodeId::new("second_root").unwrap();
    let third_id = SemanticNodeId::new("third_root").unwrap();
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
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(third_id.clone(), "Third"),
                provenance: provenance(3),
            },
            set_root_op(&first_id, None, 4),
            set_root_op(&second_id, Some(0), 5),
            set_root_op(&third_id, Some(1), 6),
        ]))
        .unwrap();

    assert_eq!(
        graph.root_node_ids(),
        &[second_id.clone(), third_id.clone(), first_id.clone()]
    );
    assert_eq!(
        graph
            .root_nodes()
            .map(|node| node.title().to_string())
            .collect::<Vec<_>>(),
        vec!["Second", "Third", "First"]
    );
    let root_provenance = graph.root_order_provenance().unwrap();
    assert_eq!(root_provenance.created().recorded_at_millis(), 4);
    assert_eq!(root_provenance.last_updated().recorded_at_millis(), 6);
}

#[test]
fn existing_root_reorder_updates_root_order_without_touching_node_provenance() {
    let first_id = SemanticNodeId::new("first_root").unwrap();
    let second_id = SemanticNodeId::new("second_root").unwrap();
    let third_id = SemanticNodeId::new("third_root").unwrap();
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
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(third_id.clone(), "Third"),
                provenance: provenance(3),
            },
            set_root_op(&first_id, None, 4),
            set_root_op(&second_id, None, 5),
            set_root_op(&third_id, None, 6),
        ]))
        .unwrap();
    let first_node_updated = graph
        .node(&first_id)
        .unwrap()
        .provenance()
        .last_updated()
        .recorded_at_millis();

    let changed = graph
        .apply_patch(&SemanticGraphPatch::from_operation(set_root_op(
            &first_id,
            Some(2),
            99,
        )))
        .unwrap();

    assert!(changed);
    assert_eq!(
        graph.root_node_ids(),
        &[second_id, third_id, first_id.clone()]
    );
    assert_eq!(
        graph
            .root_order_provenance()
            .unwrap()
            .last_updated()
            .recorded_at_millis(),
        99
    );
    assert_eq!(
        graph
            .node(&first_id)
            .unwrap()
            .provenance()
            .last_updated()
            .recorded_at_millis(),
        first_node_updated
    );
}

#[test]
fn hard_tree_cycle_is_rejected_without_losing_existing_tree() {
    let root_id = SemanticNodeId::new("root").unwrap();
    let child_id = SemanticNodeId::new("child").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(root_id.clone(), "Root"),
                provenance: provenance(1),
            },
            set_root_op(&root_id, None, 2),
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(child_id.clone(), "Child"),
                provenance: provenance(3),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: child_id.clone(),
                parent_id: Some(root_id.clone()),
                index: None,
                provenance: provenance(4),
            },
        ]))
        .unwrap();
    let before = graph.clone();

    graph
        .apply_patch(&SemanticGraphPatch::from_operation(
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_id.clone(),
                parent_id: Some(child_id.clone()),
                index: None,
                provenance: provenance(4),
            },
        ))
        .unwrap_err();

    assert_eq!(graph, before);
    assert_eq!(graph.root_node_ids(), &[root_id.clone()]);
    assert_eq!(graph.parent_id_of(&child_id), Some(&root_id));
}

#[test]
fn moving_child_to_root_and_root_under_child_updates_root_order() {
    let first_root_id = SemanticNodeId::new("first_root").unwrap();
    let second_root_id = SemanticNodeId::new("second_root").unwrap();
    let child_id = SemanticNodeId::new("child").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(first_root_id.clone(), "First Root"),
                provenance: provenance(1),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(second_root_id.clone(), "Second Root"),
                provenance: provenance(2),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(child_id.clone(), "Child"),
                provenance: provenance(3),
            },
            set_root_op(&first_root_id, None, 4),
            set_root_op(&second_root_id, None, 5),
            SemanticGraphPatchOp::SetHardParent {
                child_id: child_id.clone(),
                parent_id: Some(first_root_id.clone()),
                index: None,
                provenance: provenance(6),
            },
        ]))
        .unwrap();

    graph
        .apply_patch(&SemanticGraphPatch::from_operation(set_root_op(
            &child_id,
            Some(1),
            7,
        )))
        .unwrap();

    assert_eq!(
        graph.root_node_ids(),
        &[
            first_root_id.clone(),
            child_id.clone(),
            second_root_id.clone()
        ]
    );
    assert_eq!(graph.parent_id_of(&child_id), None);
    assert!(graph.child_ids_of(&first_root_id).is_none());

    graph
        .apply_patch(&SemanticGraphPatch::from_operation(
            SemanticGraphPatchOp::SetHardParent {
                child_id: second_root_id.clone(),
                parent_id: Some(child_id.clone()),
                index: None,
                provenance: provenance(8),
            },
        ))
        .unwrap();

    assert_eq!(
        graph.root_node_ids(),
        &[first_root_id.clone(), child_id.clone()]
    );
    assert_eq!(graph.parent_id_of(&second_root_id), Some(&child_id));
    assert_eq!(graph.child_ids_of(&child_id).unwrap(), &[second_root_id]);
}

#[test]
fn soft_links_may_form_cycles_without_changing_the_hard_tree() {
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
            SemanticGraphPatchOp::UpsertSoftLink {
                link: SoftLinkDraft::new(
                    SoftLinkId::new("first_to_second").unwrap(),
                    first_id.clone(),
                    second_id.clone(),
                    SoftLinkKind::new("related_to").unwrap(),
                ),
                provenance: provenance(7),
            },
            SemanticGraphPatchOp::UpsertSoftLink {
                link: SoftLinkDraft::new(
                    SoftLinkId::new("second_to_first").unwrap(),
                    second_id,
                    first_id,
                    SoftLinkKind::new("related_to").unwrap(),
                ),
                provenance: provenance(8),
            },
        ]))
        .unwrap();

    assert_eq!(graph.root_node_ids(), &[root_id]);
    assert_eq!(graph.soft_link_count(), 2);
}

#[test]
fn soft_links_may_cross_root_components() {
    let first_root_id = SemanticNodeId::new("first_root").unwrap();
    let second_root_id = SemanticNodeId::new("second_root").unwrap();
    let link_id = SoftLinkId::new("cross_root").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(first_root_id.clone(), "First Root"),
                provenance: provenance(1),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(second_root_id.clone(), "Second Root"),
                provenance: provenance(2),
            },
            set_root_op(&first_root_id, None, 3),
            set_root_op(&second_root_id, None, 4),
            SemanticGraphPatchOp::UpsertSoftLink {
                link: SoftLinkDraft::new(
                    link_id.clone(),
                    first_root_id.clone(),
                    second_root_id.clone(),
                    SoftLinkKind::new("depends_on").unwrap(),
                ),
                provenance: provenance(5),
            },
        ]))
        .unwrap();

    assert_eq!(graph.root_node_ids(), &[first_root_id, second_root_id]);
    assert!(graph.soft_link(&link_id).is_some());
}

#[test]
fn path_to_root_returns_the_component_local_root_path() {
    let first_root_id = SemanticNodeId::new("first_root").unwrap();
    let second_root_id = SemanticNodeId::new("second_root").unwrap();
    let child_id = SemanticNodeId::new("child").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(first_root_id.clone(), "First Root"),
                provenance: provenance(1),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(second_root_id.clone(), "Second Root"),
                provenance: provenance(2),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(child_id.clone(), "Child"),
                provenance: provenance(3),
            },
            set_root_op(&first_root_id, None, 4),
            set_root_op(&second_root_id, None, 5),
            SemanticGraphPatchOp::SetHardParent {
                child_id: child_id.clone(),
                parent_id: Some(second_root_id.clone()),
                index: None,
                provenance: provenance(6),
            },
        ]))
        .unwrap();

    assert_eq!(
        graph
            .path_to_root(&child_id)
            .unwrap()
            .into_iter()
            .map(|node| node.id().clone())
            .collect::<Vec<_>>(),
        vec![second_root_id, child_id]
    );
}

#[test]
fn checklist_item_cannot_be_placed_at_root() {
    let item_id = SemanticNodeId::new("item").unwrap();
    let mut graph = SemanticGraph::default();

    let error = graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: checklist_item_node(item_id.clone(), "Item", ChecklistItemStatus::Todo),
                provenance: provenance(1),
            },
            set_root_op(&item_id, None, 2),
        ]))
        .unwrap_err();

    assert!(error.to_string().contains("checklist-item node"));
    assert!(graph.root_node_ids().is_empty());
    assert_eq!(graph.node_count(), 0);
}

#[test]
fn existing_checklist_item_cannot_be_moved_to_root_and_rolls_back() {
    let checklist_id = SemanticNodeId::new("checklist").unwrap();
    let item_id = SemanticNodeId::new("item").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: checklist_node(checklist_id.clone(), "Checklist"),
                provenance: provenance(1),
            },
            set_root_op(&checklist_id, None, 2),
            SemanticGraphPatchOp::UpsertNode {
                node: checklist_item_node(item_id.clone(), "Item", ChecklistItemStatus::Todo),
                provenance: provenance(3),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: item_id.clone(),
                parent_id: Some(checklist_id.clone()),
                index: None,
                provenance: provenance(4),
            },
        ]))
        .unwrap();
    let before = graph.clone();

    let error = graph
        .apply_patch(&SemanticGraphPatch::from_operation(set_root_op(
            &item_id, None, 5,
        )))
        .unwrap_err();

    assert!(error.to_string().contains("checklist-item node"));
    assert_eq!(graph, before);
    assert_eq!(graph.root_node_ids(), std::slice::from_ref(&checklist_id));
    assert_eq!(graph.parent_id_of(&item_id), Some(&checklist_id));
}

#[test]
fn updating_existing_node_refreshes_last_updated_provenance() {
    let node_id = SemanticNodeId::new("topic").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(node_id.clone(), "Topic"),
                provenance: provenance(10),
            },
            set_root_op(&node_id, None, 11),
        ]))
        .unwrap();
    graph
        .apply_patch(&SemanticGraphPatch::from_operation(
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    node_id.clone(),
                    "Updated Topic",
                    "Updated summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: provenance(20),
            },
        ))
        .unwrap();

    let node = graph.node(&node_id).unwrap();
    assert_eq!(node.title(), "Updated Topic");
    assert_eq!(node.provenance().created().recorded_at_millis(), 10);
    assert_eq!(node.provenance().last_updated().recorded_at_millis(), 20);
}

#[test]
fn duplicate_soft_link_relations_are_rejected() {
    let root_id = SemanticNodeId::new("root").unwrap();
    let left_id = SemanticNodeId::new("left").unwrap();
    let right_id = SemanticNodeId::new("right").unwrap();
    let link_kind = SoftLinkKind::new("depends_on").unwrap();
    let link_one = SoftLinkId::new("link_one").unwrap();
    let link_two = SoftLinkId::new("link_two").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(root_id.clone(), "Root"),
                provenance: provenance(1),
            },
            set_root_op(&root_id, None, 2),
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(left_id.clone(), "Left"),
                provenance: provenance(3),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(right_id.clone(), "Right"),
                provenance: provenance(4),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: left_id.clone(),
                parent_id: Some(root_id.clone()),
                index: None,
                provenance: provenance(5),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: right_id.clone(),
                parent_id: Some(root_id),
                index: None,
                provenance: provenance(6),
            },
            SemanticGraphPatchOp::UpsertSoftLink {
                link: SoftLinkDraft::new(
                    link_one.clone(),
                    left_id.clone(),
                    right_id.clone(),
                    link_kind.clone(),
                ),
                provenance: provenance(7),
            },
        ]))
        .unwrap();

    let error = graph
        .apply_patch(&SemanticGraphPatch::from_operation(
            SemanticGraphPatchOp::UpsertSoftLink {
                link: SoftLinkDraft::new(link_two.clone(), left_id, right_id, link_kind),
                provenance: provenance(7),
            },
        ))
        .unwrap_err();

    assert!(error.to_string().contains("soft-link relation"));
    assert!(graph.soft_link(&link_one).is_some());
    assert!(graph.soft_link(&link_two).is_none());
}

#[test]
fn thread_refs_allow_many_to_many_bindings_but_reject_duplicates_per_node() {
    let root_id = SemanticNodeId::new("root").unwrap();
    let second_id = SemanticNodeId::new("second").unwrap();
    let thread_id = ConversationThreadId::new("thread_1");
    let ref_one = ThreadRefId::new("ref_one").unwrap();
    let ref_two = ThreadRefId::new("ref_two").unwrap();
    let ref_three = ThreadRefId::new("ref_three").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(root_id.clone(), "Root"),
                provenance: provenance(1),
            },
            set_root_op(&root_id, None, 2),
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(second_id.clone(), "Second"),
                provenance: provenance(3),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: second_id.clone(),
                parent_id: Some(root_id.clone()),
                index: None,
                provenance: provenance(4),
            },
            SemanticGraphPatchOp::UpsertThreadRef {
                thread_ref: ThreadRefDraft::new(
                    ref_one.clone(),
                    root_id.clone(),
                    thread_id.clone(),
                    WorkspaceId::host_windows(r"C:\work\beryl"),
                    "Root thread",
                ),
                provenance: provenance(5),
            },
            SemanticGraphPatchOp::UpsertThreadRef {
                thread_ref: ThreadRefDraft::new(
                    ref_two.clone(),
                    second_id,
                    thread_id.clone(),
                    WorkspaceId::host_windows(r"C:\work\beryl"),
                    "Second thread",
                ),
                provenance: provenance(6),
            },
        ]))
        .unwrap();

    let error = graph
        .apply_patch(&SemanticGraphPatch::from_operation(
            SemanticGraphPatchOp::UpsertThreadRef {
                thread_ref: ThreadRefDraft::new(
                    ref_three.clone(),
                    root_id,
                    thread_id,
                    WorkspaceId::host_windows(r"C:\work\beryl"),
                    "Duplicate root thread",
                ),
                provenance: provenance(6),
            },
        ))
        .unwrap_err();

    assert!(error.to_string().contains("already attached"));
    assert!(graph.thread_ref(&ref_one).is_some());
    assert!(graph.thread_ref(&ref_two).is_some());
    assert!(graph.thread_ref(&ref_three).is_none());
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
