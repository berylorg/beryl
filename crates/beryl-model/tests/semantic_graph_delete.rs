use beryl_model::conversation::ConversationThreadId;
use beryl_model::provenance::{MutationProvenance, MutationSource};
use beryl_model::semantic_graph::{
    SemanticGraph, SemanticGraphError, SemanticGraphPatch, SemanticGraphPatchOp, SemanticNodeDraft,
    SemanticNodeFacets, SemanticNodeId, SoftLinkDraft, SoftLinkId, SoftLinkKind, ThreadRefDraft,
    ThreadRefId,
};
use beryl_model::workspace::WorkspaceId;

#[test]
fn delete_node_subtree_removes_leaf_from_parent_order() {
    let root_id = SemanticNodeId::new("root").unwrap();
    let leaf_id = SemanticNodeId::new("leaf").unwrap();
    let sibling_id = SemanticNodeId::new("sibling").unwrap();
    let mut graph = SemanticGraph::default();

    apply_graph_patch(
        &mut graph,
        vec![
            upsert_topic_op(&root_id, "Root", 1),
            set_root_op(&root_id, None, 2),
            upsert_topic_op(&leaf_id, "Leaf", 2),
            upsert_topic_op(&sibling_id, "Sibling", 3),
            set_parent_op(&leaf_id, &root_id, 4),
            set_parent_op(&sibling_id, &root_id, 5),
        ],
    );

    let changed = graph
        .apply_patch(&SemanticGraphPatch::from_operation(delete_subtree_op(
            &leaf_id, 6,
        )))
        .unwrap();

    assert!(changed);
    assert!(graph.node(&leaf_id).is_none());
    assert_eq!(graph.child_ids_of(&root_id).unwrap(), &[sibling_id]);
    assert_eq!(graph.node_count(), 2);
}

#[test]
fn delete_node_subtree_removes_multi_level_hard_descendants() {
    let root_id = SemanticNodeId::new("root").unwrap();
    let before_id = SemanticNodeId::new("before").unwrap();
    let branch_id = SemanticNodeId::new("branch").unwrap();
    let child_id = SemanticNodeId::new("child").unwrap();
    let grandchild_id = SemanticNodeId::new("grandchild").unwrap();
    let after_id = SemanticNodeId::new("after").unwrap();
    let mut graph = SemanticGraph::default();

    apply_graph_patch(
        &mut graph,
        vec![
            upsert_topic_op(&root_id, "Root", 1),
            set_root_op(&root_id, None, 2),
            upsert_topic_op(&before_id, "Before", 2),
            upsert_topic_op(&branch_id, "Branch", 3),
            upsert_topic_op(&child_id, "Child", 4),
            upsert_topic_op(&grandchild_id, "Grandchild", 5),
            upsert_topic_op(&after_id, "After", 6),
            set_parent_op(&before_id, &root_id, 7),
            set_parent_op(&branch_id, &root_id, 8),
            set_parent_op(&after_id, &root_id, 9),
            set_parent_op(&child_id, &branch_id, 10),
            set_parent_op(&grandchild_id, &child_id, 11),
        ],
    );

    graph
        .apply_patch(&SemanticGraphPatch::from_operation(delete_subtree_op(
            &branch_id, 12,
        )))
        .unwrap();

    assert!(graph.node(&branch_id).is_none());
    assert!(graph.node(&child_id).is_none());
    assert!(graph.node(&grandchild_id).is_none());
    assert_eq!(
        graph.child_ids_of(&root_id).unwrap(),
        &[before_id, after_id]
    );
    assert_eq!(graph.node_count(), 3);
}

#[test]
fn delete_node_subtree_does_not_follow_soft_links() {
    let root_id = SemanticNodeId::new("root").unwrap();
    let delete_id = SemanticNodeId::new("delete").unwrap();
    let deleted_child_id = SemanticNodeId::new("deleted_child").unwrap();
    let outside_id = SemanticNodeId::new("outside").unwrap();
    let incident_link_id = SoftLinkId::new("delete_to_outside").unwrap();
    let reverse_incident_link_id = SoftLinkId::new("outside_to_deleted_child").unwrap();
    let preserved_link_id = SoftLinkId::new("root_to_outside").unwrap();
    let mut graph = SemanticGraph::default();

    apply_graph_patch(
        &mut graph,
        vec![
            upsert_topic_op(&root_id, "Root", 1),
            set_root_op(&root_id, None, 2),
            upsert_topic_op(&delete_id, "Delete", 2),
            upsert_topic_op(&deleted_child_id, "Deleted Child", 3),
            upsert_topic_op(&outside_id, "Outside", 4),
            set_parent_op(&delete_id, &root_id, 5),
            set_parent_op(&outside_id, &root_id, 6),
            set_parent_op(&deleted_child_id, &delete_id, 7),
            soft_link_op(&incident_link_id, &delete_id, &outside_id, 8),
            soft_link_op(&reverse_incident_link_id, &outside_id, &deleted_child_id, 9),
            soft_link_op(&preserved_link_id, &root_id, &outside_id, 10),
        ],
    );

    graph
        .apply_patch(&SemanticGraphPatch::from_operation(delete_subtree_op(
            &delete_id, 11,
        )))
        .unwrap();

    assert!(graph.node(&outside_id).is_some());
    assert!(graph.node(&deleted_child_id).is_none());
    assert!(graph.soft_link(&incident_link_id).is_none());
    assert!(graph.soft_link(&reverse_incident_link_id).is_none());
    assert!(graph.soft_link(&preserved_link_id).is_some());
    assert_eq!(graph.soft_link_count(), 1);
}

#[test]
fn delete_node_subtree_removes_thread_refs_for_deleted_nodes_only() {
    let root_id = SemanticNodeId::new("root").unwrap();
    let delete_id = SemanticNodeId::new("delete").unwrap();
    let kept_id = SemanticNodeId::new("kept").unwrap();
    let root_ref_id = ThreadRefId::new("root_ref").unwrap();
    let deleted_ref_id = ThreadRefId::new("deleted_ref").unwrap();
    let kept_ref_id = ThreadRefId::new("kept_ref").unwrap();
    let mut graph = SemanticGraph::default();

    apply_graph_patch(
        &mut graph,
        vec![
            upsert_topic_op(&root_id, "Root", 1),
            set_root_op(&root_id, None, 2),
            upsert_topic_op(&delete_id, "Delete", 2),
            upsert_topic_op(&kept_id, "Kept", 3),
            set_parent_op(&delete_id, &root_id, 4),
            set_parent_op(&kept_id, &root_id, 5),
            thread_ref_op(&root_ref_id, &root_id, "thread_root", "Root thread", 6),
            thread_ref_op(
                &deleted_ref_id,
                &delete_id,
                "thread_deleted",
                "Deleted thread",
                7,
            ),
            thread_ref_op(&kept_ref_id, &kept_id, "thread_kept", "Kept thread", 8),
        ],
    );

    graph
        .apply_patch(&SemanticGraphPatch::from_operation(delete_subtree_op(
            &delete_id, 9,
        )))
        .unwrap();

    assert!(graph.thread_ref(&root_ref_id).is_some());
    assert!(graph.thread_ref(&deleted_ref_id).is_none());
    assert!(graph.thread_ref(&kept_ref_id).is_some());
    assert_eq!(graph.thread_ref_count(), 2);
}

#[test]
fn delete_root_node_subtree_leaves_empty_graph() {
    let root_id = SemanticNodeId::new("root").unwrap();
    let child_id = SemanticNodeId::new("child").unwrap();
    let link_id = SoftLinkId::new("root_to_child").unwrap();
    let ref_id = ThreadRefId::new("root_ref").unwrap();
    let mut graph = SemanticGraph::default();

    apply_graph_patch(
        &mut graph,
        vec![
            upsert_topic_op(&root_id, "Root", 1),
            set_root_op(&root_id, None, 2),
            upsert_topic_op(&child_id, "Child", 2),
            set_parent_op(&child_id, &root_id, 3),
            soft_link_op(&link_id, &root_id, &child_id, 4),
            thread_ref_op(&ref_id, &root_id, "thread_root", "Root thread", 5),
        ],
    );

    graph
        .apply_patch(&SemanticGraphPatch::from_operation(delete_subtree_op(
            &root_id, 6,
        )))
        .unwrap();

    assert_eq!(graph.node_count(), 0);
    assert!(graph.root_node_ids().is_empty());
    assert_eq!(graph.soft_link_count(), 0);
    assert_eq!(graph.thread_ref_count(), 0);
}

#[test]
fn delete_one_root_subtree_preserves_unrelated_roots_in_order() {
    let first_root_id = SemanticNodeId::new("first_root").unwrap();
    let second_root_id = SemanticNodeId::new("second_root").unwrap();
    let third_root_id = SemanticNodeId::new("third_root").unwrap();
    let child_id = SemanticNodeId::new("child").unwrap();
    let mut graph = SemanticGraph::default();

    apply_graph_patch(
        &mut graph,
        vec![
            upsert_topic_op(&first_root_id, "First Root", 1),
            upsert_topic_op(&second_root_id, "Second Root", 2),
            upsert_topic_op(&third_root_id, "Third Root", 3),
            upsert_topic_op(&child_id, "Child", 4),
            set_root_op(&first_root_id, None, 5),
            set_root_op(&second_root_id, None, 6),
            set_root_op(&third_root_id, None, 7),
            set_parent_op(&child_id, &second_root_id, 8),
        ],
    );

    graph
        .apply_patch(&SemanticGraphPatch::from_operation(delete_subtree_op(
            &second_root_id,
            9,
        )))
        .unwrap();

    assert_eq!(graph.root_node_ids(), &[first_root_id, third_root_id]);
    assert!(graph.node(&second_root_id).is_none());
    assert!(graph.node(&child_id).is_none());
    assert_eq!(graph.node_count(), 2);
}

#[test]
fn delete_root_subtree_removes_cross_root_incident_soft_links_and_thread_refs() {
    let first_root_id = SemanticNodeId::new("first_root").unwrap();
    let second_root_id = SemanticNodeId::new("second_root").unwrap();
    let first_child_id = SemanticNodeId::new("first_child").unwrap();
    let second_child_id = SemanticNodeId::new("second_child").unwrap();
    let deleted_to_surviving_link_id = SoftLinkId::new("deleted_to_surviving").unwrap();
    let surviving_to_deleted_link_id = SoftLinkId::new("surviving_to_deleted").unwrap();
    let surviving_link_id = SoftLinkId::new("surviving_link").unwrap();
    let deleted_thread_ref_id = ThreadRefId::new("deleted_thread").unwrap();
    let surviving_thread_ref_id = ThreadRefId::new("surviving_thread").unwrap();
    let mut graph = SemanticGraph::default();

    apply_graph_patch(
        &mut graph,
        vec![
            upsert_topic_op(&first_root_id, "First Root", 1),
            upsert_topic_op(&second_root_id, "Second Root", 2),
            upsert_topic_op(&first_child_id, "First Child", 3),
            upsert_topic_op(&second_child_id, "Second Child", 4),
            set_root_op(&first_root_id, None, 5),
            set_root_op(&second_root_id, None, 6),
            set_parent_op(&first_child_id, &first_root_id, 7),
            set_parent_op(&second_child_id, &second_root_id, 8),
            soft_link_op(
                &deleted_to_surviving_link_id,
                &first_child_id,
                &second_child_id,
                9,
            ),
            soft_link_op(
                &surviving_to_deleted_link_id,
                &second_child_id,
                &first_root_id,
                10,
            ),
            soft_link_op(&surviving_link_id, &second_root_id, &second_child_id, 11),
            thread_ref_op(
                &deleted_thread_ref_id,
                &first_child_id,
                "deleted_thread",
                "Deleted thread",
                12,
            ),
            thread_ref_op(
                &surviving_thread_ref_id,
                &second_child_id,
                "surviving_thread",
                "Surviving thread",
                13,
            ),
        ],
    );

    graph
        .apply_patch(&SemanticGraphPatch::from_operation(delete_subtree_op(
            &first_root_id,
            14,
        )))
        .unwrap();

    assert_eq!(graph.root_node_ids(), std::slice::from_ref(&second_root_id));
    assert!(graph.node(&first_root_id).is_none());
    assert!(graph.node(&first_child_id).is_none());
    assert!(graph.node(&second_root_id).is_some());
    assert!(graph.node(&second_child_id).is_some());
    assert!(graph.soft_link(&deleted_to_surviving_link_id).is_none());
    assert!(graph.soft_link(&surviving_to_deleted_link_id).is_none());
    assert!(graph.soft_link(&surviving_link_id).is_some());
    assert!(graph.thread_ref(&deleted_thread_ref_id).is_none());
    assert!(graph.thread_ref(&surviving_thread_ref_id).is_some());
    assert_eq!(graph.soft_link_count(), 1);
    assert_eq!(graph.thread_ref_count(), 1);
}

#[test]
fn delete_node_subtree_missing_target_rolls_back() {
    let root_id = SemanticNodeId::new("root").unwrap();
    let missing_id = SemanticNodeId::new("missing").unwrap();
    let mut graph = SemanticGraph::default();

    apply_graph_patch(
        &mut graph,
        vec![
            upsert_topic_op(&root_id, "Root", 1),
            set_root_op(&root_id, None, 2),
        ],
    );
    let before = graph.clone();
    let error = graph
        .apply_patch(&SemanticGraphPatch::from_operation(delete_subtree_op(
            &missing_id,
            2,
        )))
        .unwrap_err();

    assert_eq!(
        error,
        SemanticGraphError::MissingNode {
            node_id: missing_id
        }
    );
    assert_eq!(graph, before);
}

fn apply_graph_patch(graph: &mut SemanticGraph, operations: Vec<SemanticGraphPatchOp>) {
    graph
        .apply_patch(&SemanticGraphPatch::new(operations))
        .unwrap();
}

fn upsert_topic_op(
    node_id: &SemanticNodeId,
    title: &str,
    recorded_at_millis: u64,
) -> SemanticGraphPatchOp {
    SemanticGraphPatchOp::UpsertNode {
        node: SemanticNodeDraft::new(
            node_id.clone(),
            title,
            format!("{title} summary"),
            SemanticNodeFacets::topic(),
            None,
        ),
        provenance: provenance(recorded_at_millis),
    }
}

fn set_parent_op(
    child_id: &SemanticNodeId,
    parent_id: &SemanticNodeId,
    recorded_at_millis: u64,
) -> SemanticGraphPatchOp {
    SemanticGraphPatchOp::SetHardParent {
        child_id: child_id.clone(),
        parent_id: Some(parent_id.clone()),
        index: None,
        provenance: provenance(recorded_at_millis),
    }
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

fn soft_link_op(
    link_id: &SoftLinkId,
    source_id: &SemanticNodeId,
    target_id: &SemanticNodeId,
    recorded_at_millis: u64,
) -> SemanticGraphPatchOp {
    SemanticGraphPatchOp::UpsertSoftLink {
        link: SoftLinkDraft::new(
            link_id.clone(),
            source_id.clone(),
            target_id.clone(),
            SoftLinkKind::new("related_to").unwrap(),
        ),
        provenance: provenance(recorded_at_millis),
    }
}

fn thread_ref_op(
    thread_ref_id: &ThreadRefId,
    node_id: &SemanticNodeId,
    thread_id: &str,
    label: &str,
    recorded_at_millis: u64,
) -> SemanticGraphPatchOp {
    SemanticGraphPatchOp::UpsertThreadRef {
        thread_ref: ThreadRefDraft::new(
            thread_ref_id.clone(),
            node_id.clone(),
            ConversationThreadId::new(thread_id),
            WorkspaceId::host_windows(r"C:\work\beryl"),
            label,
        ),
        provenance: provenance(recorded_at_millis),
    }
}

fn delete_subtree_op(node_id: &SemanticNodeId, recorded_at_millis: u64) -> SemanticGraphPatchOp {
    SemanticGraphPatchOp::DeleteNodeSubtree {
        node_id: node_id.clone(),
        provenance: provenance(recorded_at_millis),
    }
}

fn provenance(recorded_at_millis: u64) -> MutationProvenance {
    MutationProvenance::new(
        "operator",
        recorded_at_millis,
        MutationSource::workspace_action("delete_graph_node_subtree").unwrap(),
        Some(100),
    )
    .unwrap()
}
