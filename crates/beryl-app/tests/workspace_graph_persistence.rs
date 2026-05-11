#[path = "support/tempdir.rs"]
mod tempdir_support;

use beryl_app::{BerylWorkspacePersistence, WorkspaceGraphRevision, WorkspacePersistenceError};
use beryl_model::conversation::{
    ConversationThreadId, ConversationTurnId, RegisteredConversationThread,
    WorkspaceConversationState,
};
use beryl_model::provenance::{MutationProvenance, MutationSource};
use beryl_model::semantic_graph::{
    ChecklistItemStatus, SemanticGraph, SemanticGraphPatch, SemanticGraphPatchOp,
    SemanticNodeDraft, SemanticNodeFacets, SemanticNodeId, SoftLinkDraft, SoftLinkId, SoftLinkKind,
    ThreadRefDraft, ThreadRefId,
};
use beryl_model::workspace::{BerylWorkspaceId, BerylWorkspaceManifest, WorkspaceId};
use redb::{Database, TableDefinition};

const WORKSPACE_METADATA_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("workspace_metadata");

#[test]
fn missing_graph_record_defaults_to_an_empty_graph() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_testing").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Testing", 42);

    persistence.save_workspace_manifest(&manifest).unwrap();

    let loaded = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();
    let snapshot = persistence
        .load_workspace_graph_state_snapshot(&workspace_id)
        .unwrap();

    assert_eq!(loaded, SemanticGraph::default());
    assert_eq!(snapshot.graph, SemanticGraph::default());
    assert_eq!(snapshot.revision, WorkspaceGraphRevision::default());

    root.close().unwrap();
}

#[test]
fn graph_state_roundtrips_without_clobbering_conversation_state() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_testing").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Testing", 42);
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let mut conversation = WorkspaceConversationState::default();
    let thread = RegisteredConversationThread::new(
        ConversationThreadId::new("thread_1"),
        execution_target.clone(),
        "Inspect graph state",
        Some("Graph".to_string()),
        1,
        2,
    );
    let graph = sample_graph();

    persistence.save_workspace_manifest(&manifest).unwrap();
    conversation.remember_thread(thread);
    conversation
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    conversation.activate_thread(&ConversationThreadId::new("thread_1"));
    persistence
        .save_workspace_state(&workspace_id, &conversation)
        .unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let loaded_graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();
    let loaded_conversation = persistence.load_workspace_state(&workspace_id).unwrap();

    assert_eq!(loaded_graph, graph);
    assert_eq!(loaded_conversation, conversation);

    root.close().unwrap();
}

#[test]
fn graph_state_roundtrips_ordered_roots_and_cross_root_soft_links() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_multi_root").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Multi Root", 42);
    let graph = multi_root_cross_link_graph();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let snapshot = persistence
        .load_workspace_graph_state_snapshot(&workspace_id)
        .unwrap();
    let link = snapshot
        .graph
        .soft_link(&SoftLinkId::new("alpha_depends_on_beta").unwrap())
        .unwrap();

    assert_eq!(snapshot.graph, graph);
    assert_eq!(snapshot.revision, WorkspaceGraphRevision::default());
    assert_eq!(
        snapshot.graph.root_node_ids(),
        &[
            SemanticNodeId::new("alpha_root").unwrap(),
            SemanticNodeId::new("beta_root").unwrap()
        ]
    );
    assert_eq!(
        link.source_id(),
        &SemanticNodeId::new("alpha_child").unwrap()
    );
    assert_eq!(
        link.target_id(),
        &SemanticNodeId::new("beta_child").unwrap()
    );

    root.close().unwrap();
}

#[test]
fn workspace_title_change_moves_graph_state_and_revision() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_testing").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Testing", 42);
    let graph = sample_graph();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();
    persistence
        .apply_workspace_graph_patch(&workspace_id, &SemanticGraphPatch::default(), None)
        .unwrap();

    let renamed = persistence
        .set_workspace_manual_title(&workspace_id, "Graph Archive")
        .unwrap()
        .unwrap();
    let snapshot = persistence
        .load_workspace_graph_state_snapshot(renamed.id())
        .unwrap();

    assert_eq!(renamed.id().as_str(), "graph-archive");
    assert_eq!(snapshot.graph, graph);
    assert_eq!(snapshot.revision, WorkspaceGraphRevision::new(1));
    assert!(
        persistence
            .load_workspace_manifest(&workspace_id)
            .unwrap()
            .is_none()
    );

    root.close().unwrap();
}

#[test]
fn thread_selector_activation_state_update_does_not_mutate_semantic_graph_thread_refs() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_testing").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Testing", 42);
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let selector_thread_id = ConversationThreadId::new("thread_from_selector");
    let graph = sample_graph();
    let mut conversation = WorkspaceConversationState::default();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();
    conversation
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    conversation.remember_thread(RegisteredConversationThread::new(
        selector_thread_id.clone(),
        execution_target,
        "Selector preview",
        Some("Selector thread".to_string()),
        10,
        11,
    ));
    conversation.activate_thread(&selector_thread_id);
    persistence
        .save_workspace_state(&workspace_id, &conversation)
        .unwrap();

    let stored_graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();

    assert_eq!(stored_graph, graph);
    assert_eq!(stored_graph.thread_ref_count(), 1);
    assert!(
        stored_graph
            .thread_ref(&ThreadRefId::new("thread_ref").unwrap())
            .is_some()
    );

    root.close().unwrap();
}

#[test]
fn applying_graph_patch_updates_manifest_only_when_graph_changes() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_testing").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Testing", 42);
    let root_id = SemanticNodeId::new("root").unwrap();
    let patch = SemanticGraphPatch::new(vec![
        SemanticGraphPatchOp::UpsertNode {
            node: SemanticNodeDraft::new(
                root_id.clone(),
                "Root",
                "Root summary",
                SemanticNodeFacets::topic(),
                None,
            ),
            provenance: provenance(10),
        },
        SemanticGraphPatchOp::SetHardParent {
            child_id: root_id.clone(),
            parent_id: None,
            index: None,
            provenance: provenance(11),
        },
    ]);

    persistence.save_workspace_manifest(&manifest).unwrap();

    let commit = persistence
        .apply_workspace_graph_patch(&workspace_id, &patch, None)
        .unwrap();
    let touched_manifest = persistence
        .load_workspace_manifest(&workspace_id)
        .unwrap()
        .unwrap();
    let graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();

    assert!(commit.changed);
    assert_eq!(commit.base_revision, WorkspaceGraphRevision::default());
    assert_eq!(commit.committed_revision, WorkspaceGraphRevision::new(1));
    assert!(touched_manifest.last_updated_at_millis() >= manifest.last_updated_at_millis());
    assert!(graph.node(&root_id).is_some());
    assert_eq!(graph.root_node_ids(), std::slice::from_ref(&root_id));

    let no_change = persistence
        .apply_workspace_graph_patch(&workspace_id, &SemanticGraphPatch::default(), None)
        .unwrap();
    let untouched_manifest = persistence
        .load_workspace_manifest(&workspace_id)
        .unwrap()
        .unwrap();

    assert!(!no_change.changed);
    assert_eq!(no_change.base_revision, WorkspaceGraphRevision::new(1));
    assert_eq!(no_change.committed_revision, WorkspaceGraphRevision::new(2));
    assert_eq!(
        untouched_manifest.last_updated_at_millis(),
        touched_manifest.last_updated_at_millis()
    );

    root.close().unwrap();
}

#[test]
fn invalid_graph_patch_preserves_manifest_and_existing_graph() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_testing").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Testing", 42);
    let graph = sample_graph();
    let invalid_patch = SemanticGraphPatch::from_operation(SemanticGraphPatchOp::UpsertNode {
        node: SemanticNodeDraft::new(
            SemanticNodeId::new("second_root").unwrap(),
            "Second root",
            "Second root summary",
            SemanticNodeFacets::topic(),
            None,
        ),
        provenance: provenance(20),
    });

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let error = persistence
        .apply_workspace_graph_patch(&workspace_id, &invalid_patch, None)
        .unwrap_err();
    let stored_manifest = persistence
        .load_workspace_manifest(&workspace_id)
        .unwrap()
        .unwrap();
    let stored_graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();
    let stored_revision = persistence
        .load_workspace_graph_revision(&workspace_id)
        .unwrap();

    assert!(error.to_string().contains("semantic graph patch"));
    assert_eq!(stored_manifest, manifest);
    assert_eq!(stored_graph, graph);
    assert_eq!(stored_revision, WorkspaceGraphRevision::default());

    root.close().unwrap();
}

#[test]
fn stale_expected_graph_revision_rejects_without_mutating_graph() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_testing").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Testing", 42);
    let root_id = SemanticNodeId::new("root").unwrap();
    let sibling_id = SemanticNodeId::new("sibling").unwrap();
    let root_patch = SemanticGraphPatch::new(vec![
        SemanticGraphPatchOp::UpsertNode {
            node: SemanticNodeDraft::new(
                root_id.clone(),
                "Root",
                "Root summary",
                SemanticNodeFacets::topic(),
                None,
            ),
            provenance: provenance(10),
        },
        SemanticGraphPatchOp::SetHardParent {
            child_id: root_id.clone(),
            parent_id: None,
            index: None,
            provenance: provenance(11),
        },
    ]);
    let sibling_patch = SemanticGraphPatch::new(vec![
        SemanticGraphPatchOp::UpsertNode {
            node: SemanticNodeDraft::new(
                sibling_id.clone(),
                "Sibling",
                "Sibling summary",
                SemanticNodeFacets::topic(),
                None,
            ),
            provenance: provenance(12),
        },
        SemanticGraphPatchOp::SetHardParent {
            child_id: sibling_id.clone(),
            parent_id: None,
            index: None,
            provenance: provenance(13),
        },
    ]);

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .apply_workspace_graph_patch(&workspace_id, &root_patch, None)
        .unwrap();

    let error = persistence
        .apply_workspace_graph_patch(
            &workspace_id,
            &sibling_patch,
            Some(WorkspaceGraphRevision::default()),
        )
        .unwrap_err();
    let snapshot = persistence
        .load_workspace_graph_state_snapshot(&workspace_id)
        .unwrap();

    assert!(matches!(
        error,
        WorkspacePersistenceError::WorkspaceGraphRevisionConflict {
            expected_revision,
            actual_revision,
            ..
        } if expected_revision == WorkspaceGraphRevision::default()
            && actual_revision == WorkspaceGraphRevision::new(1)
    ));
    assert!(
        snapshot
            .graph
            .node(&SemanticNodeId::new("root").unwrap())
            .is_some()
    );
    assert!(
        snapshot
            .graph
            .node(&SemanticNodeId::new("sibling").unwrap())
            .is_none()
    );
    assert_eq!(snapshot.revision, WorkspaceGraphRevision::new(1));

    root.close().unwrap();
}

#[test]
fn existing_graph_record_without_revision_is_rejected() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("graph_testing").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Testing", 42);
    let graph = sample_graph();

    persistence.save_workspace_manifest(&manifest).unwrap();
    write_graph_record_without_revision(&persistence, &workspace_id, &graph);

    let load_error = persistence
        .load_workspace_graph_state_snapshot(&workspace_id)
        .unwrap_err();
    let write_error = persistence
        .apply_workspace_graph_patch(&workspace_id, &SemanticGraphPatch::default(), None)
        .unwrap_err();

    assert!(matches!(
        load_error,
        WorkspacePersistenceError::MissingWorkspaceGraphRevision { .. }
    ));
    assert!(matches!(
        write_error,
        WorkspacePersistenceError::MissingWorkspaceGraphRevision { .. }
    ));

    root.close().unwrap();
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
                provenance: provenance(1),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    checklist_id.clone(),
                    "Checklist",
                    "Checklist summary",
                    SemanticNodeFacets::topic_and_checklist(),
                    None,
                ),
                provenance: provenance(2),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    item_id.clone(),
                    "Item",
                    "Item summary",
                    SemanticNodeFacets::topic_and_checklist_item(),
                    Some(ChecklistItemStatus::InProgress),
                ),
                provenance: provenance(3),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    sibling_id.clone(),
                    "Sibling",
                    "Sibling summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: provenance(4),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_id.clone(),
                parent_id: None,
                index: None,
                provenance: provenance(5),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: checklist_id.clone(),
                parent_id: Some(root_id.clone()),
                index: None,
                provenance: provenance(6),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: item_id.clone(),
                parent_id: Some(checklist_id),
                index: None,
                provenance: provenance(7),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: sibling_id.clone(),
                parent_id: Some(root_id),
                index: None,
                provenance: provenance(8),
            },
            SemanticGraphPatchOp::UpsertSoftLink {
                link: SoftLinkDraft::new(link_id, item_id.clone(), sibling_id, link_kind),
                provenance: provenance(9),
            },
            SemanticGraphPatchOp::UpsertThreadRef {
                thread_ref: ThreadRefDraft::new(
                    thread_ref_id,
                    item_id,
                    ConversationThreadId::new("thread_1"),
                    WorkspaceId::host_windows(r"C:\work\beryl"),
                    "Item thread",
                ),
                provenance: provenance(10),
            },
        ]))
        .unwrap();

    graph
}

fn multi_root_cross_link_graph() -> SemanticGraph {
    let alpha_root_id = SemanticNodeId::new("alpha_root").unwrap();
    let beta_root_id = SemanticNodeId::new("beta_root").unwrap();
    let alpha_child_id = SemanticNodeId::new("alpha_child").unwrap();
    let beta_child_id = SemanticNodeId::new("beta_child").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    alpha_root_id.clone(),
                    "Alpha Root",
                    "Alpha root summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: provenance(20),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    beta_root_id.clone(),
                    "Beta Root",
                    "Beta root summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: provenance(21),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    alpha_child_id.clone(),
                    "Alpha Child",
                    "Alpha child summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: provenance(22),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    beta_child_id.clone(),
                    "Beta Child",
                    "Beta child summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: provenance(23),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: alpha_root_id.clone(),
                parent_id: None,
                index: None,
                provenance: provenance(24),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: beta_root_id.clone(),
                parent_id: None,
                index: None,
                provenance: provenance(25),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: alpha_child_id.clone(),
                parent_id: Some(alpha_root_id),
                index: None,
                provenance: provenance(26),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: beta_child_id.clone(),
                parent_id: Some(beta_root_id),
                index: None,
                provenance: provenance(27),
            },
            SemanticGraphPatchOp::UpsertSoftLink {
                link: SoftLinkDraft::new(
                    SoftLinkId::new("alpha_depends_on_beta").unwrap(),
                    alpha_child_id,
                    beta_child_id,
                    SoftLinkKind::new("depends_on").unwrap(),
                ),
                provenance: provenance(28),
            },
        ]))
        .unwrap();

    graph
}

fn write_graph_record_without_revision(
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    graph: &SemanticGraph,
) {
    let database_path = persistence.workspace_database_path(workspace_id);
    let database = Database::open(&database_path).unwrap();
    let graph_bytes = serde_json::to_vec(graph).unwrap();
    let write_txn = database.begin_write().unwrap();
    {
        let mut table = write_txn.open_table(WORKSPACE_METADATA_TABLE).unwrap();
        table
            .insert("semantic_graph_state", graph_bytes.as_slice())
            .unwrap();
    }
    write_txn.commit().unwrap();
}

fn provenance(recorded_at_millis: u64) -> MutationProvenance {
    MutationProvenance::new(
        "operator",
        recorded_at_millis,
        MutationSource::conversation_turn(
            ConversationThreadId::new("thread_1"),
            ConversationTurnId::new(format!("turn_{recorded_at_millis}")),
        ),
        Some(100),
    )
    .unwrap()
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-workspace-graph-persistence-test-")
}
