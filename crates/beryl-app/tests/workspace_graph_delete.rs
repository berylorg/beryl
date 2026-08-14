#[path = "support/tempdir.rs"]
mod tempdir_support;

use beryl_app::{
    BerylWorkspacePersistence, NodeLeafDeleteRequest, NodeSubtreeDeleteRequest,
    WorkspaceGraphToolError, WorkspaceGraphToolService, WorkspacePersistenceError,
};
use beryl_model::conversation::{
    ConversationThreadId, ConversationTurnId, RegisteredConversationThread,
    WorkspaceConversationState,
};
use beryl_model::provenance::{MutationProvenance, MutationSource};
use beryl_model::semantic_graph::{
    ChecklistItemStatus, SemanticGraph, SemanticGraphError, SemanticGraphPatch,
    SemanticGraphPatchOp, SemanticNodeDraft, SemanticNodeFacets, SemanticNodeId, SoftLinkDraft,
    SoftLinkId, SoftLinkKind, ThreadRefDraft, ThreadRefId,
};
use beryl_model::workspace::{BerylWorkspaceId, BerylWorkspaceManifest, WorkspaceId};

#[test]
fn service_delete_node_subtree_uses_repository_path_and_returns_refreshed_summary() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_delete").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Delete", 42);
    let graph = sample_graph();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let response = service
        .delete_node_subtree(&delete_request(workspace_id.clone(), "checklist", 50))
        .unwrap();
    let stored_manifest = persistence
        .load_workspace_manifest(&workspace_id)
        .unwrap()
        .unwrap();
    let stored_graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();

    assert!(response.commit.changed);
    assert_eq!(response.summary.manifest, stored_manifest);
    assert!(stored_manifest.last_updated_at_millis() > manifest.last_updated_at_millis());
    assert_eq!(response.summary.root_node_count, 1);
    assert_eq!(response.summary.root_nodes[0].id.as_str(), "root");
    assert_eq!(response.summary.node_count, 2);
    assert_eq!(response.summary.soft_link_count, 0);
    assert_eq!(response.summary.thread_ref_count, 0);
    assert_remaining_graph_after_checklist_delete(&stored_graph);
    assert_delete_provenance_touched_surviving_order(&stored_graph);

    root.close().unwrap();
}

#[test]
fn service_delete_node_leaf_uses_repository_path_and_returns_refreshed_summary() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_delete").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Delete", 42);
    let graph = sample_graph();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let response = service
        .delete_node_leaf(&leaf_delete_request(workspace_id.clone(), "sibling", 51))
        .unwrap();
    let stored_manifest = persistence
        .load_workspace_manifest(&workspace_id)
        .unwrap()
        .unwrap();
    let stored_graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();

    assert!(response.commit.changed);
    assert_eq!(response.summary.manifest, stored_manifest);
    assert!(stored_manifest.last_updated_at_millis() > manifest.last_updated_at_millis());
    assert_eq!(response.summary.root_node_count, 1);
    assert_eq!(response.summary.root_nodes[0].id.as_str(), "root");
    assert_eq!(response.summary.node_count, 3);
    assert_eq!(response.summary.soft_link_count, 0);
    assert_eq!(response.summary.thread_ref_count, 1);
    assert_remaining_graph_after_sibling_leaf_delete(&stored_graph);
    assert_leaf_delete_provenance_touched_surviving_order(&stored_graph);

    root.close().unwrap();
}

#[test]
fn service_delete_root_subtree_preserves_unrelated_ordered_roots() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_delete_multi_root").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Delete", 42);
    let graph = multi_root_delete_graph();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let response = service
        .delete_node_subtree(&delete_request(workspace_id.clone(), "root_a", 52))
        .unwrap();
    let stored_graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();

    assert!(response.commit.changed);
    assert_eq!(response.summary.root_node_count, 1);
    assert_eq!(response.summary.root_nodes[0].id.as_str(), "root_b");
    assert_eq!(response.summary.node_count, 2);
    assert_eq!(response.summary.soft_link_count, 0);
    assert_eq!(response.summary.thread_ref_count, 1);
    assert_eq!(
        stored_graph.root_node_ids(),
        &[SemanticNodeId::new("root_b").unwrap()]
    );
    assert!(
        stored_graph
            .node(&SemanticNodeId::new("root_a").unwrap())
            .is_none()
    );
    assert!(
        stored_graph
            .node(&SemanticNodeId::new("child_a").unwrap())
            .is_none()
    );
    assert!(
        stored_graph
            .node(&SemanticNodeId::new("root_b").unwrap())
            .is_some()
    );
    assert!(
        stored_graph
            .node(&SemanticNodeId::new("child_b").unwrap())
            .is_some()
    );
    assert_eq!(stored_graph.soft_link_count(), 0);
    assert!(
        stored_graph
            .thread_ref(&ThreadRefId::new("child_b_thread").unwrap())
            .is_some()
    );

    root.close().unwrap();
}

#[test]
fn persistence_delete_node_subtree_is_durable_and_does_not_mutate_workspace_threads() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_delete").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Delete", 42);
    let graph = sample_graph();
    let state = sample_workspace_state();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    let commit = persistence
        .apply_workspace_graph_patch(&workspace_id, &delete_patch("checklist", 50), None)
        .unwrap();
    let reloaded = BerylWorkspacePersistence::new(&root);
    let stored_graph = reloaded.load_workspace_graph_state(&workspace_id).unwrap();
    let stored_state = reloaded.load_workspace_state(&workspace_id).unwrap();

    assert!(commit.changed);
    assert_remaining_graph_after_checklist_delete(&stored_graph);
    assert_eq!(stored_state, state);

    root.close().unwrap();
}

#[test]
fn persistence_delete_node_leaf_is_durable_and_does_not_mutate_workspace_threads() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_delete").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Delete", 42);
    let graph = sample_graph();
    let state = sample_workspace_state();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    let commit = persistence
        .apply_workspace_graph_patch(&workspace_id, &leaf_delete_patch("item", 51), None)
        .unwrap();
    let reloaded = BerylWorkspacePersistence::new(&root);
    let stored_graph = reloaded.load_workspace_graph_state(&workspace_id).unwrap();
    let stored_state = reloaded.load_workspace_state(&workspace_id).unwrap();

    assert!(commit.changed);
    assert_remaining_graph_after_item_leaf_delete(&stored_graph);
    assert_eq!(stored_state, state);

    root.close().unwrap();
}

#[test]
fn persistence_delete_missing_node_preserves_manifest_graph_and_workspace_threads() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_delete").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Delete", 42);
    let graph = sample_graph();
    let state = sample_workspace_state();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    let error = persistence
        .apply_workspace_graph_patch(&workspace_id, &delete_patch("missing", 50), None)
        .unwrap_err();
    let stored_manifest = persistence
        .load_workspace_manifest(&workspace_id)
        .unwrap()
        .unwrap();
    let stored_graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();
    let stored_state = persistence.load_workspace_state(&workspace_id).unwrap();

    assert!(error.to_string().contains("semantic graph patch"));
    assert_eq!(stored_manifest, manifest);
    assert_eq!(stored_graph, graph);
    assert_eq!(stored_state, state);

    root.close().unwrap();
}

#[test]
fn persistence_delete_non_leaf_node_leaf_preserves_manifest_graph_and_workspace_threads() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_delete").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Delete", 42);
    let graph = sample_graph();
    let state = sample_workspace_state();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    let error = persistence
        .apply_workspace_graph_patch(&workspace_id, &leaf_delete_patch("checklist", 51), None)
        .unwrap_err();
    let stored_manifest = persistence
        .load_workspace_manifest(&workspace_id)
        .unwrap()
        .unwrap();
    let stored_graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();
    let stored_state = persistence.load_workspace_state(&workspace_id).unwrap();

    assert!(matches!(
        error,
        WorkspacePersistenceError::ApplyWorkspaceGraphPatch {
            source: SemanticGraphError::NonLeafNode { .. },
            ..
        }
    ));
    assert_eq!(stored_manifest, manifest);
    assert_eq!(stored_graph, graph);
    assert_eq!(stored_state, state);

    root.close().unwrap();
}

#[test]
fn persistence_delete_missing_node_leaf_preserves_manifest_graph_and_workspace_threads() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_delete").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Delete", 42);
    let graph = sample_graph();
    let state = sample_workspace_state();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    let error = persistence
        .apply_workspace_graph_patch(&workspace_id, &leaf_delete_patch("missing", 51), None)
        .unwrap_err();
    let stored_manifest = persistence
        .load_workspace_manifest(&workspace_id)
        .unwrap()
        .unwrap();
    let stored_graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();
    let stored_state = persistence.load_workspace_state(&workspace_id).unwrap();

    assert!(error.to_string().contains("semantic graph patch"));
    assert_eq!(stored_manifest, manifest);
    assert_eq!(stored_graph, graph);
    assert_eq!(stored_state, state);

    root.close().unwrap();
}

#[test]
fn service_delete_missing_node_preserves_manifest_and_graph() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_delete").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Delete", 42);
    let graph = sample_graph();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let error = service
        .delete_node_subtree(&delete_request(workspace_id.clone(), "missing", 50))
        .unwrap_err();
    let stored_manifest = persistence
        .load_workspace_manifest(&workspace_id)
        .unwrap()
        .unwrap();
    let stored_graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();

    assert!(matches!(error, WorkspaceGraphToolError::MissingNode { .. }));
    assert_eq!(stored_manifest, manifest);
    assert_eq!(stored_graph, graph);

    root.close().unwrap();
}

#[test]
fn service_delete_non_leaf_node_leaf_preserves_manifest_and_graph() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_delete").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Delete", 42);
    let graph = sample_graph();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let error = service
        .delete_node_leaf(&leaf_delete_request(workspace_id.clone(), "checklist", 51))
        .unwrap_err();
    let stored_manifest = persistence
        .load_workspace_manifest(&workspace_id)
        .unwrap()
        .unwrap();
    let stored_graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();

    assert!(matches!(
        error,
        WorkspaceGraphToolError::Persistence(WorkspacePersistenceError::ApplyWorkspaceGraphPatch {
            source: SemanticGraphError::NonLeafNode { .. },
            ..
        })
    ));
    assert_eq!(stored_manifest, manifest);
    assert_eq!(stored_graph, graph);

    root.close().unwrap();
}

#[test]
fn service_delete_missing_node_leaf_preserves_manifest_and_graph() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_delete").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Delete", 42);
    let graph = sample_graph();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let error = service
        .delete_node_leaf(&leaf_delete_request(workspace_id.clone(), "missing", 51))
        .unwrap_err();
    let stored_manifest = persistence
        .load_workspace_manifest(&workspace_id)
        .unwrap()
        .unwrap();
    let stored_graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();

    assert!(matches!(error, WorkspaceGraphToolError::MissingNode { .. }));
    assert_eq!(stored_manifest, manifest);
    assert_eq!(stored_graph, graph);

    root.close().unwrap();
}

fn assert_remaining_graph_after_checklist_delete(graph: &SemanticGraph) {
    let root_id = SemanticNodeId::new("root").unwrap();
    let checklist_id = SemanticNodeId::new("checklist").unwrap();
    let item_id = SemanticNodeId::new("item").unwrap();
    let sibling_id = SemanticNodeId::new("sibling").unwrap();

    assert_eq!(graph.node_count(), 2);
    assert!(graph.node(&root_id).is_some());
    assert!(graph.node(&sibling_id).is_some());
    assert!(graph.node(&checklist_id).is_none());
    assert!(graph.node(&item_id).is_none());
    assert_eq!(graph.soft_link_count(), 0);
    assert_eq!(graph.thread_ref_count(), 0);
    assert_eq!(graph.child_ids_of(&root_id).unwrap(), &[sibling_id]);
}

fn assert_remaining_graph_after_item_leaf_delete(graph: &SemanticGraph) {
    let root_id = SemanticNodeId::new("root").unwrap();
    let checklist_id = SemanticNodeId::new("checklist").unwrap();
    let item_id = SemanticNodeId::new("item").unwrap();
    let sibling_id = SemanticNodeId::new("sibling").unwrap();

    assert_eq!(graph.node_count(), 3);
    assert!(graph.node(&root_id).is_some());
    assert!(graph.node(&checklist_id).is_some());
    assert!(graph.node(&sibling_id).is_some());
    assert!(graph.node(&item_id).is_none());
    assert_eq!(graph.soft_link_count(), 0);
    assert_eq!(graph.thread_ref_count(), 0);
    assert_eq!(
        graph.child_ids_of(&root_id).unwrap(),
        &[checklist_id.clone(), sibling_id]
    );
    assert!(graph.child_ids_of(&checklist_id).is_none());
}

fn assert_remaining_graph_after_sibling_leaf_delete(graph: &SemanticGraph) {
    let root_id = SemanticNodeId::new("root").unwrap();
    let checklist_id = SemanticNodeId::new("checklist").unwrap();
    let item_id = SemanticNodeId::new("item").unwrap();
    let sibling_id = SemanticNodeId::new("sibling").unwrap();
    let thread_ref_id = ThreadRefId::new("thread_ref").unwrap();

    assert_eq!(graph.node_count(), 3);
    assert!(graph.node(&root_id).is_some());
    assert!(graph.node(&checklist_id).is_some());
    assert!(graph.node(&item_id).is_some());
    assert!(graph.node(&sibling_id).is_none());
    assert_eq!(graph.soft_link_count(), 0);
    assert_eq!(graph.thread_ref_count(), 1);
    assert_eq!(
        graph.child_ids_of(&root_id).unwrap(),
        std::slice::from_ref(&checklist_id)
    );
    assert_eq!(
        graph.child_ids_of(&checklist_id).unwrap(),
        std::slice::from_ref(&item_id)
    );
    assert!(graph.thread_ref(&thread_ref_id).is_some());
}

fn assert_delete_provenance_touched_surviving_order(graph: &SemanticGraph) {
    let graph_json = serde_json::to_value(graph).unwrap();
    let last_updated = &graph_json["ordered_children"]["root"]["provenance"]["last_updated"];

    assert_eq!(last_updated["actor"], "operator");
    assert_eq!(last_updated["recorded_at_millis"], 50);
    assert_eq!(
        last_updated["source"]["WorkspaceAction"]["action"],
        "delete_graph_node_subtree"
    );
}

fn assert_leaf_delete_provenance_touched_surviving_order(graph: &SemanticGraph) {
    let graph_json = serde_json::to_value(graph).unwrap();
    let last_updated = &graph_json["ordered_children"]["root"]["provenance"]["last_updated"];

    assert_eq!(last_updated["actor"], "operator");
    assert_eq!(last_updated["recorded_at_millis"], 51);
    assert_eq!(
        last_updated["source"]["WorkspaceAction"]["action"],
        "delete_graph_node_leaf"
    );
}

fn delete_request(
    workspace_id: BerylWorkspaceId,
    node_id: &str,
    recorded_at_millis: u64,
) -> NodeSubtreeDeleteRequest {
    NodeSubtreeDeleteRequest {
        workspace_id,
        node_id: SemanticNodeId::new(node_id).unwrap(),
        provenance: delete_provenance(recorded_at_millis),
        expected_base_revision: None,
    }
}

fn leaf_delete_request(
    workspace_id: BerylWorkspaceId,
    node_id: &str,
    recorded_at_millis: u64,
) -> NodeLeafDeleteRequest {
    NodeLeafDeleteRequest {
        workspace_id,
        node_id: SemanticNodeId::new(node_id).unwrap(),
        provenance: leaf_delete_provenance(recorded_at_millis),
        expected_base_revision: None,
    }
}

fn delete_patch(node_id: &str, recorded_at_millis: u64) -> SemanticGraphPatch {
    SemanticGraphPatch::from_operation(SemanticGraphPatchOp::DeleteNodeSubtree {
        node_id: SemanticNodeId::new(node_id).unwrap(),
        provenance: delete_provenance(recorded_at_millis),
    })
}

fn leaf_delete_patch(node_id: &str, recorded_at_millis: u64) -> SemanticGraphPatch {
    SemanticGraphPatch::from_operation(SemanticGraphPatchOp::DeleteNodeLeaf {
        node_id: SemanticNodeId::new(node_id).unwrap(),
        provenance: leaf_delete_provenance(recorded_at_millis),
    })
}

fn sample_workspace_state() -> WorkspaceConversationState {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let mut state = WorkspaceConversationState::default();

    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        ConversationThreadId::new("thread_1"),
        execution_target.clone(),
        "Item thread preview",
        Some("Item thread".to_string()),
        11,
        12,
    ));
    state.remember_thread(RegisteredConversationThread::new(
        ConversationThreadId::new("thread_sibling"),
        execution_target,
        "Sibling thread preview",
        Some("Sibling thread".to_string()),
        13,
        14,
    ));
    state.activate_thread(&ConversationThreadId::new("thread_1"));

    state
}

fn sample_graph() -> SemanticGraph {
    let root_id = SemanticNodeId::new("root").unwrap();
    let checklist_id = SemanticNodeId::new("checklist").unwrap();
    let item_id = SemanticNodeId::new("item").unwrap();
    let sibling_id = SemanticNodeId::new("sibling").unwrap();
    let link_id = SoftLinkId::new("depends").unwrap();
    let link_kind = SoftLinkKind::new("depends_on").unwrap();
    let thread_ref_id = ThreadRefId::new("thread_ref").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    root_id.clone(),
                    "Root",
                    "Root summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: seed_provenance(1),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    checklist_id.clone(),
                    "Checklist",
                    "Checklist summary",
                    SemanticNodeFacets::topic_and_checklist(),
                    None,
                ),
                provenance: seed_provenance(2),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    item_id.clone(),
                    "Item",
                    "Item summary",
                    SemanticNodeFacets::topic_and_checklist_item(),
                    Some(ChecklistItemStatus::InProgress),
                ),
                provenance: seed_provenance(3),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    sibling_id.clone(),
                    "Sibling",
                    "Sibling summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: seed_provenance(4),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_id.clone(),
                parent_id: None,
                index: None,
                provenance: seed_provenance(5),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: checklist_id.clone(),
                parent_id: Some(root_id.clone()),
                index: None,
                provenance: seed_provenance(6),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: item_id.clone(),
                parent_id: Some(checklist_id.clone()),
                index: None,
                provenance: seed_provenance(7),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: sibling_id.clone(),
                parent_id: Some(root_id),
                index: None,
                provenance: seed_provenance(8),
            },
            SemanticGraphPatchOp::UpsertSoftLink {
                link: SoftLinkDraft::new(link_id, item_id.clone(), sibling_id, link_kind),
                provenance: seed_provenance(9),
            },
            SemanticGraphPatchOp::UpsertThreadRef {
                thread_ref: ThreadRefDraft::new(
                    thread_ref_id,
                    item_id,
                    ConversationThreadId::new("thread_1"),
                    WorkspaceId::host_windows(r"C:\work\beryl"),
                    "Item thread",
                ),
                provenance: seed_provenance(10),
            },
        ]))
        .unwrap();

    graph
}

fn multi_root_delete_graph() -> SemanticGraph {
    let root_a_id = SemanticNodeId::new("root_a").unwrap();
    let root_b_id = SemanticNodeId::new("root_b").unwrap();
    let child_a_id = SemanticNodeId::new("child_a").unwrap();
    let child_b_id = SemanticNodeId::new("child_b").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    root_a_id.clone(),
                    "Root A",
                    "Root A summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: seed_provenance(20),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    root_b_id.clone(),
                    "Root B",
                    "Root B summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: seed_provenance(21),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    child_a_id.clone(),
                    "Child A",
                    "Child A summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: seed_provenance(22),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    child_b_id.clone(),
                    "Child B",
                    "Child B summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: seed_provenance(23),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_a_id.clone(),
                parent_id: None,
                index: None,
                provenance: seed_provenance(24),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_b_id.clone(),
                parent_id: None,
                index: None,
                provenance: seed_provenance(25),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: child_a_id.clone(),
                parent_id: Some(root_a_id),
                index: None,
                provenance: seed_provenance(26),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: child_b_id.clone(),
                parent_id: Some(root_b_id),
                index: None,
                provenance: seed_provenance(27),
            },
            SemanticGraphPatchOp::UpsertSoftLink {
                link: SoftLinkDraft::new(
                    SoftLinkId::new("child_a_depends_on_child_b").unwrap(),
                    child_a_id.clone(),
                    child_b_id.clone(),
                    SoftLinkKind::new("depends_on").unwrap(),
                ),
                provenance: seed_provenance(28),
            },
            SemanticGraphPatchOp::UpsertThreadRef {
                thread_ref: ThreadRefDraft::new(
                    ThreadRefId::new("child_a_thread").unwrap(),
                    child_a_id,
                    ConversationThreadId::new("thread_child_a"),
                    WorkspaceId::host_windows(r"C:\work\beryl"),
                    "Child A thread",
                ),
                provenance: seed_provenance(29),
            },
            SemanticGraphPatchOp::UpsertThreadRef {
                thread_ref: ThreadRefDraft::new(
                    ThreadRefId::new("child_b_thread").unwrap(),
                    child_b_id,
                    ConversationThreadId::new("thread_child_b"),
                    WorkspaceId::host_windows(r"C:\work\beryl"),
                    "Child B thread",
                ),
                provenance: seed_provenance(30),
            },
        ]))
        .unwrap();

    graph
}

fn seed_provenance(recorded_at_millis: u64) -> MutationProvenance {
    MutationProvenance::new(
        "codex",
        recorded_at_millis,
        MutationSource::conversation_turn(
            ConversationThreadId::new("seed_thread"),
            ConversationTurnId::new(format!("turn_{recorded_at_millis}")),
        ),
        Some(100),
    )
    .unwrap()
}

fn delete_provenance(recorded_at_millis: u64) -> MutationProvenance {
    MutationProvenance::new(
        "operator",
        recorded_at_millis,
        MutationSource::workspace_action("delete_graph_node_subtree").unwrap(),
        Some(100),
    )
    .unwrap()
}

fn leaf_delete_provenance(recorded_at_millis: u64) -> MutationProvenance {
    MutationProvenance::new(
        "operator",
        recorded_at_millis,
        MutationSource::workspace_action("delete_graph_node_leaf").unwrap(),
        Some(100),
    )
    .unwrap()
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-workspace-graph-delete-test-")
}
