use beryl_model::conversation::ConversationThreadId;
use beryl_model::provenance::{MutationProvenance, MutationSource};
use beryl_model::semantic_graph::{
    SemanticGraph, SemanticGraphError, SemanticGraphPatch, SemanticGraphPatchOp, SemanticNodeDraft,
    SemanticNodeFacets, SemanticNodeId, SoftLinkDraft, SoftLinkId, SoftLinkKind, ThreadRefDraft,
    ThreadRefId,
};
use beryl_model::workspace::WorkspaceId;

#[test]
fn delete_node_leaf_removes_leaf_from_parent_order() {
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
        .apply_patch(&SemanticGraphPatch::from_operation(delete_leaf_op(
            &leaf_id, 6,
        )))
        .unwrap();

    assert!(changed);
    assert!(graph.node(&leaf_id).is_none());
    assert_eq!(graph.child_ids_of(&root_id).unwrap(), &[sibling_id]);
    assert_eq!(graph.node_count(), 2);
}

#[test]
fn delete_node_leaf_allows_root_leaf_to_leave_empty_graph() {
    let root_id = SemanticNodeId::new("root").unwrap();
    let root_ref_id = ThreadRefId::new("root_ref").unwrap();
    let mut graph = SemanticGraph::default();

    apply_graph_patch(
        &mut graph,
        vec![
            upsert_topic_op(&root_id, "Root", 1),
            set_root_op(&root_id, None, 2),
            thread_ref_op(&root_ref_id, &root_id, "thread_root", "Root thread", 2),
        ],
    );

    let changed = graph
        .apply_patch(&SemanticGraphPatch::from_operation(delete_leaf_op(
            &root_id, 3,
        )))
        .unwrap();

    assert!(changed);
    assert_eq!(graph.node_count(), 0);
    assert!(graph.root_node_ids().is_empty());
    assert_eq!(graph.thread_ref_count(), 0);
}

#[test]
fn delete_root_leaf_preserves_unrelated_roots_in_order() {
    let first_root_id = SemanticNodeId::new("first_root").unwrap();
    let second_root_id = SemanticNodeId::new("second_root").unwrap();
    let third_root_id = SemanticNodeId::new("third_root").unwrap();
    let mut graph = SemanticGraph::default();

    apply_graph_patch(
        &mut graph,
        vec![
            upsert_topic_op(&first_root_id, "First Root", 1),
            upsert_topic_op(&second_root_id, "Second Root", 2),
            upsert_topic_op(&third_root_id, "Third Root", 3),
            set_root_op(&first_root_id, None, 4),
            set_root_op(&second_root_id, None, 5),
            set_root_op(&third_root_id, None, 6),
        ],
    );

    graph
        .apply_patch(&SemanticGraphPatch::from_operation(delete_leaf_op(
            &second_root_id,
            7,
        )))
        .unwrap();

    assert_eq!(graph.root_node_ids(), &[first_root_id, third_root_id]);
    assert!(graph.node(&second_root_id).is_none());
    assert_eq!(graph.node_count(), 2);
}

#[test]
fn delete_node_leaf_removes_incident_soft_links_without_following_them() {
    let root_id = SemanticNodeId::new("root").unwrap();
    let leaf_id = SemanticNodeId::new("leaf").unwrap();
    let outside_id = SemanticNodeId::new("outside").unwrap();
    let incident_link_id = SoftLinkId::new("leaf_to_outside").unwrap();
    let reverse_incident_link_id = SoftLinkId::new("outside_to_leaf").unwrap();
    let preserved_link_id = SoftLinkId::new("root_to_outside").unwrap();
    let mut graph = SemanticGraph::default();

    apply_graph_patch(
        &mut graph,
        vec![
            upsert_topic_op(&root_id, "Root", 1),
            set_root_op(&root_id, None, 2),
            upsert_topic_op(&leaf_id, "Leaf", 2),
            upsert_topic_op(&outside_id, "Outside", 3),
            set_parent_op(&leaf_id, &root_id, 4),
            set_parent_op(&outside_id, &root_id, 5),
            soft_link_op(&incident_link_id, &leaf_id, &outside_id, 6),
            soft_link_op(&reverse_incident_link_id, &outside_id, &leaf_id, 7),
            soft_link_op(&preserved_link_id, &root_id, &outside_id, 8),
        ],
    );

    graph
        .apply_patch(&SemanticGraphPatch::from_operation(delete_leaf_op(
            &leaf_id, 9,
        )))
        .unwrap();

    assert!(graph.node(&outside_id).is_some());
    assert!(graph.node(&leaf_id).is_none());
    assert!(graph.soft_link(&incident_link_id).is_none());
    assert!(graph.soft_link(&reverse_incident_link_id).is_none());
    assert!(graph.soft_link(&preserved_link_id).is_some());
    assert_eq!(graph.soft_link_count(), 1);
}

#[test]
fn delete_node_leaf_removes_thread_refs_for_target_only() {
    let root_id = SemanticNodeId::new("root").unwrap();
    let leaf_id = SemanticNodeId::new("leaf").unwrap();
    let kept_id = SemanticNodeId::new("kept").unwrap();
    let root_ref_id = ThreadRefId::new("root_ref").unwrap();
    let leaf_ref_id = ThreadRefId::new("leaf_ref").unwrap();
    let kept_ref_id = ThreadRefId::new("kept_ref").unwrap();
    let mut graph = SemanticGraph::default();

    apply_graph_patch(
        &mut graph,
        vec![
            upsert_topic_op(&root_id, "Root", 1),
            set_root_op(&root_id, None, 2),
            upsert_topic_op(&leaf_id, "Leaf", 2),
            upsert_topic_op(&kept_id, "Kept", 3),
            set_parent_op(&leaf_id, &root_id, 4),
            set_parent_op(&kept_id, &root_id, 5),
            thread_ref_op(&root_ref_id, &root_id, "thread_root", "Root thread", 6),
            thread_ref_op(&leaf_ref_id, &leaf_id, "thread_leaf", "Leaf thread", 7),
            thread_ref_op(&kept_ref_id, &kept_id, "thread_kept", "Kept thread", 8),
        ],
    );

    graph
        .apply_patch(&SemanticGraphPatch::from_operation(delete_leaf_op(
            &leaf_id, 9,
        )))
        .unwrap();

    assert!(graph.thread_ref(&root_ref_id).is_some());
    assert!(graph.thread_ref(&leaf_ref_id).is_none());
    assert!(graph.thread_ref(&kept_ref_id).is_some());
    assert_eq!(graph.thread_ref_count(), 2);
}

#[test]
fn delete_node_leaf_rejects_patch_time_non_leaf_and_rolls_back() {
    let root_id = SemanticNodeId::new("root").unwrap();
    let leaf_id = SemanticNodeId::new("leaf").unwrap();
    let child_id = SemanticNodeId::new("child").unwrap();
    let mut graph = SemanticGraph::default();

    apply_graph_patch(
        &mut graph,
        vec![
            upsert_topic_op(&root_id, "Root", 1),
            set_root_op(&root_id, None, 2),
            upsert_topic_op(&leaf_id, "Leaf", 2),
            set_parent_op(&leaf_id, &root_id, 3),
        ],
    );
    let before = graph.clone();

    let error = graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            upsert_topic_op(&child_id, "Child", 4),
            set_parent_op(&child_id, &leaf_id, 5),
            delete_leaf_op(&leaf_id, 6),
        ]))
        .unwrap_err();

    assert_eq!(error, SemanticGraphError::NonLeafNode { node_id: leaf_id });
    assert_eq!(graph, before);
    assert!(graph.node(&child_id).is_none());
}

#[test]
fn delete_node_leaf_missing_target_rolls_back() {
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
        .apply_patch(&SemanticGraphPatch::from_operation(delete_leaf_op(
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

fn delete_leaf_op(node_id: &SemanticNodeId, recorded_at_millis: u64) -> SemanticGraphPatchOp {
    SemanticGraphPatchOp::DeleteNodeLeaf {
        node_id: node_id.clone(),
        provenance: provenance(recorded_at_millis),
    }
}

fn provenance(recorded_at_millis: u64) -> MutationProvenance {
    MutationProvenance::new(
        "operator",
        recorded_at_millis,
        MutationSource::workspace_action("delete_graph_node").unwrap(),
        Some(100),
    )
    .unwrap()
}
