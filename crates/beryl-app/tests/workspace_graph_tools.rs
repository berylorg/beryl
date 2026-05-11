#[path = "support/tempdir.rs"]
mod tempdir_support;

use beryl_app::{
    BerylWorkspacePersistence, ChecklistReadRequest, GraphNeighborhoodRequest,
    GraphPatchWriteRequest, MAX_CHECKLIST_ITEM_COUNT, MAX_GRAPH_NEIGHBORHOOD_NODE_COUNT,
    MAX_GRAPH_SUMMARY_ROOT_COUNT, ThreadRefUpsertRequest, WorkspaceGraphSummaryRequest,
    WorkspaceGraphToolError, WorkspaceGraphToolService, WorkspaceMemberSnapshotKind,
    WorkspacePrimaryMemberSnapshot, WorkspaceStateReadRequest,
};
use beryl_model::conversation::{
    ConversationThreadId, ConversationThreadMemberBinding, ConversationTurnId,
    RegisteredConversationThread, WorkspaceConversationState,
};
use beryl_model::provenance::{MutationProvenance, MutationSource};
use beryl_model::semantic_graph::{
    ChecklistItemStatus, SemanticGraph, SemanticGraphPatch, SemanticGraphPatchOp,
    SemanticNodeDraft, SemanticNodeFacets, SemanticNodeId, SoftLinkDraft, SoftLinkId, SoftLinkKind,
    ThreadRefDraft, ThreadRefId,
};
use beryl_model::workspace::{BerylWorkspaceId, BerylWorkspaceManifest, RuntimeMode, WorkspaceId};

#[test]
fn workspace_graph_summary_rejects_missing_workspace() {
    let root = unique_temp_dir();
    let service = WorkspaceGraphToolService::new(BerylWorkspacePersistence::new(&root));
    let workspace_id = BerylWorkspaceId::new("missing_workspace").unwrap();

    let error = service
        .read_workspace_summary(&WorkspaceGraphSummaryRequest { workspace_id })
        .unwrap_err();

    assert!(matches!(
        error,
        WorkspaceGraphToolError::MissingWorkspace { .. }
    ));

    let _ = root.close();
}

#[test]
fn workspace_graph_summary_reads_ordered_root_snapshots_and_counts() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_tools").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Tools", 42);
    let graph = multi_root_graph();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let summary = service
        .read_workspace_summary(&WorkspaceGraphSummaryRequest {
            workspace_id: workspace_id.clone(),
        })
        .unwrap();

    assert_eq!(summary.manifest, manifest);
    assert_eq!(
        summary
            .root_nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        ["root_a", "root_b"]
    );
    assert_eq!(summary.root_node_count, 2);
    assert!(!summary.root_nodes_truncated);
    assert_eq!(summary.node_count, 4);
    assert_eq!(summary.soft_link_count, 1);
    assert_eq!(summary.thread_ref_count, 0);

    root.close().unwrap();
}

#[test]
fn workspace_graph_summary_truncates_many_root_snapshots() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_tools").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Tools", 42);
    let graph = wide_root_graph(MAX_GRAPH_SUMMARY_ROOT_COUNT + 3);

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let summary = service
        .read_workspace_summary(&WorkspaceGraphSummaryRequest {
            workspace_id: workspace_id.clone(),
        })
        .unwrap();

    assert_eq!(summary.root_node_count, MAX_GRAPH_SUMMARY_ROOT_COUNT + 3);
    assert!(summary.root_nodes_truncated);
    assert_eq!(summary.root_nodes.len(), MAX_GRAPH_SUMMARY_ROOT_COUNT);
    assert_eq!(summary.root_nodes[0].id.as_str(), "root_0");

    root.close().unwrap();
}

#[test]
fn workspace_state_read_exposes_runtime_members_primary_and_thread_metadata() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("workspace_state").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Workspace State", 42);
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let mut state = WorkspaceConversationState::default();

    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    let member_id = state.primary_explicit_member().unwrap().id().clone();
    state.remember_thread(RegisteredConversationThread::new(
        ConversationThreadId::new("thread_1"),
        execution_target.clone(),
        "Inspect metadata",
        Some("Metadata thread".to_string()),
        1,
        2,
    ));
    state.activate_thread(&ConversationThreadId::new("thread_1"));

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    let response = service
        .read_workspace_state(&WorkspaceStateReadRequest {
            workspace_id: workspace_id.clone(),
        })
        .unwrap();

    assert_eq!(response.manifest, manifest);
    assert_eq!(response.selected_runtime, Some(RuntimeMode::HostWindows));
    assert_eq!(response.available_members.len(), 1);
    assert_eq!(
        response.available_members[0].kind,
        WorkspaceMemberSnapshotKind::Explicit
    );
    assert_eq!(
        response.available_members[0].member_id.as_ref(),
        Some(&member_id)
    );
    assert!(response.available_members[0].primary);
    assert!(matches!(
        response.primary_member,
        WorkspacePrimaryMemberSnapshot::Explicit {
            member_id: primary_member_id,
            ..
        } if primary_member_id == member_id
    ));
    assert_eq!(response.threads.len(), 1);
    assert_eq!(
        response.threads[0].backend_name.as_deref(),
        Some("Metadata thread")
    );
    assert!(response.threads[0].title.is_none());
    assert!(response.threads[0].active);
    assert!(matches!(
        response.threads[0].member_binding.as_ref(),
        Some(ConversationThreadMemberBinding::Explicit { .. })
    ));

    root.close().unwrap();
}

#[test]
fn workspace_state_read_exposes_implicit_home_without_resolving_filesystem_contents() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("workspace_state").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Workspace State", 42);
    let mut state = WorkspaceConversationState::default();

    state.select_runtime(RuntimeMode::HostWindows).unwrap();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    let response = service
        .read_workspace_state(&WorkspaceStateReadRequest {
            workspace_id: workspace_id.clone(),
        })
        .unwrap();

    assert_eq!(response.available_members.len(), 1);
    assert_eq!(
        response.available_members[0].kind,
        WorkspaceMemberSnapshotKind::ImplicitHome
    );
    assert!(response.available_members[0].canonical_path.is_none());
    assert!(response.available_members[0].primary);
    assert!(matches!(
        response.primary_member,
        WorkspacePrimaryMemberSnapshot::ImplicitHome {
            runtime: RuntimeMode::HostWindows
        }
    ));

    root.close().unwrap();
}

#[test]
fn workspace_state_read_exposes_multiple_members_primary_and_rebind_metadata() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("workspace_state").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Workspace State", 42);
    let first_target = WorkspaceId::host_windows(r"C:\work\first");
    let second_target = WorkspaceId::host_windows(r"C:\work\second");
    let thread_id = ConversationThreadId::new("thread_1");
    let mut state = WorkspaceConversationState::default();

    state
        .designate_primary_execution_target(&first_target)
        .unwrap();
    state.attach_execution_target(&second_target).unwrap();
    let second_member_id = state.explicit_members()[1].id().clone();
    state
        .set_primary_explicit_member(&second_member_id)
        .unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        first_target,
        "Needs rebind",
        Some("Needs rebind".to_string()),
        1,
        2,
    ));
    state
        .mark_thread_rebind_required(&thread_id, "Original member is unavailable")
        .unwrap();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_state(&workspace_id, &state)
        .unwrap();

    let response = service
        .read_workspace_state(&WorkspaceStateReadRequest {
            workspace_id: workspace_id.clone(),
        })
        .unwrap();

    assert_eq!(response.available_members.len(), 2);
    assert_eq!(
        response
            .available_members
            .iter()
            .filter(|member| member.primary)
            .count(),
        1
    );
    assert!(response.available_members.iter().any(|member| {
        member.member_id.as_ref() == Some(&second_member_id)
            && member.canonical_path.as_deref() == Some(second_target.canonical_path())
            && member.primary
    }));
    assert!(matches!(
        response.primary_member,
        WorkspacePrimaryMemberSnapshot::Explicit {
            member_id,
            ..
        } if member_id == second_member_id
    ));
    assert_eq!(
        response.threads[0]
            .rebind_required
            .as_ref()
            .unwrap()
            .detail(),
        "Original member is unavailable"
    );

    root.close().unwrap();
}

#[test]
fn graph_neighborhood_on_empty_graph_returns_empty_anchor() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_tools").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Tools", 42);

    persistence.save_workspace_manifest(&manifest).unwrap();

    let response = service
        .read_graph_neighborhood(&GraphNeighborhoodRequest {
            workspace_id: workspace_id.clone(),
            anchor_node_id: None,
            parent_depth: 1,
            child_depth: 1,
        })
        .unwrap();

    assert_eq!(response.summary.node_count, 0);
    assert_eq!(response.summary.root_node_count, 0);
    assert!(response.summary.root_nodes.is_empty());
    assert!(!response.summary.root_nodes_truncated);
    assert_eq!(response.anchor_node_id, None);
    assert!(response.anchor.is_none());
    assert!(response.lineage.is_empty());
    assert!(!response.truncated);

    root.close().unwrap();
}

#[test]
fn graph_neighborhood_without_anchor_returns_ordered_root_level_summary() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_tools").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Tools", 42);

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &multi_root_graph())
        .unwrap();

    let response = service
        .read_graph_neighborhood(&GraphNeighborhoodRequest {
            workspace_id: workspace_id.clone(),
            anchor_node_id: None,
            parent_depth: 1,
            child_depth: 1,
        })
        .unwrap();

    assert_eq!(response.anchor_node_id, None);
    assert!(response.anchor.is_none());
    assert!(response.lineage.is_empty());
    assert!(!response.truncated);
    assert_eq!(response.summary.root_node_count, 2);
    assert_eq!(
        response
            .summary
            .root_nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        ["root_a", "root_b"]
    );

    root.close().unwrap();
}

#[test]
fn graph_neighborhood_rejects_missing_anchor_node() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_tools").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Tools", 42);

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &sample_graph())
        .unwrap();

    let error = service
        .read_graph_neighborhood(&GraphNeighborhoodRequest {
            workspace_id: workspace_id.clone(),
            anchor_node_id: Some(SemanticNodeId::new("missing").unwrap()),
            parent_depth: 1,
            child_depth: 1,
        })
        .unwrap_err();

    assert!(matches!(error, WorkspaceGraphToolError::MissingNode { .. }));

    root.close().unwrap();
}

#[test]
fn graph_neighborhood_returns_targeted_slice_with_attached_rows() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_tools").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Tools", 42);
    let graph = sample_graph();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let response = service
        .read_graph_neighborhood(&GraphNeighborhoodRequest {
            workspace_id: workspace_id.clone(),
            anchor_node_id: Some(SemanticNodeId::new("checklist").unwrap()),
            parent_depth: 1,
            child_depth: 1,
        })
        .unwrap();

    assert_eq!(
        response.anchor_node_id,
        Some(SemanticNodeId::new("checklist").unwrap())
    );
    assert!(!response.truncated);
    assert_eq!(response.lineage.len(), 1);
    assert_eq!(response.lineage[0].id, SemanticNodeId::new("root").unwrap());

    let anchor = response.anchor.unwrap();
    assert_eq!(anchor.node.id, SemanticNodeId::new("checklist").unwrap());
    assert_eq!(anchor.children.len(), 2);
    assert_eq!(
        anchor.children[0].node.id,
        SemanticNodeId::new("item_a").unwrap()
    );
    assert_eq!(
        anchor.children[1].node.id,
        SemanticNodeId::new("item_b").unwrap()
    );
    assert_eq!(anchor.children[0].soft_links.len(), 1);
    assert_eq!(
        anchor.children[0].soft_links[0].id,
        SoftLinkId::new("depends").unwrap()
    );
    assert_eq!(
        anchor.children[0].soft_links[0].target.id,
        SemanticNodeId::new("sibling").unwrap()
    );
    assert_eq!(anchor.children[0].thread_refs.len(), 1);
    assert_eq!(
        anchor.children[0].thread_refs[0].id,
        ThreadRefId::new("thread_ref").unwrap()
    );

    root.close().unwrap();
}

#[test]
fn graph_neighborhood_reads_explicit_anchor_in_another_root_component() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_tools").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Tools", 42);

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &multi_root_graph())
        .unwrap();

    let response = service
        .read_graph_neighborhood(&GraphNeighborhoodRequest {
            workspace_id: workspace_id.clone(),
            anchor_node_id: Some(SemanticNodeId::new("child_b").unwrap()),
            parent_depth: 1,
            child_depth: 0,
        })
        .unwrap();

    assert_eq!(
        response.anchor_node_id,
        Some(SemanticNodeId::new("child_b").unwrap())
    );
    assert_eq!(response.lineage.len(), 1);
    assert_eq!(
        response.lineage[0].id,
        SemanticNodeId::new("root_b").unwrap()
    );
    let anchor = response.anchor.unwrap();
    assert_eq!(anchor.node.id, SemanticNodeId::new("child_b").unwrap());
    assert_eq!(anchor.children.len(), 0);
    assert_eq!(anchor.soft_links.len(), 1);
    assert_eq!(
        anchor.soft_links[0].target.id,
        SemanticNodeId::new("child_a").unwrap()
    );

    root.close().unwrap();
}

#[test]
fn graph_neighborhood_truncates_descendants_at_the_contract_limit() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_tools").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Tools", 42);
    let graph = wide_graph(MAX_GRAPH_NEIGHBORHOOD_NODE_COUNT + 3);

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let response = service
        .read_graph_neighborhood(&GraphNeighborhoodRequest {
            workspace_id: workspace_id.clone(),
            anchor_node_id: Some(SemanticNodeId::new("root").unwrap()),
            parent_depth: 0,
            child_depth: 1,
        })
        .unwrap();

    let anchor = response.anchor.unwrap();
    assert!(response.truncated);
    assert_eq!(anchor.children.len(), MAX_GRAPH_NEIGHBORHOOD_NODE_COUNT - 1);

    root.close().unwrap();
}

#[test]
fn checklist_reads_preserve_item_order_and_thread_refs() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_tools").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Tools", 42);
    let graph = sample_graph();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let response = service
        .read_checklist(&ChecklistReadRequest {
            workspace_id: workspace_id.clone(),
            checklist_node_id: SemanticNodeId::new("checklist").unwrap(),
        })
        .unwrap();

    assert_eq!(
        response.checklist.id,
        SemanticNodeId::new("checklist").unwrap()
    );
    assert_eq!(response.summary.root_node_count, 1);
    assert_eq!(response.summary.root_nodes[0].id.as_str(), "root");
    assert!(!response.truncated);
    assert_eq!(response.items.len(), 2);
    assert_eq!(
        response.items[0].node.id,
        SemanticNodeId::new("item_a").unwrap()
    );
    assert_eq!(
        response.items[1].node.id,
        SemanticNodeId::new("item_b").unwrap()
    );
    assert_eq!(response.items[0].thread_refs.len(), 1);
    assert!(response.items[1].thread_refs.is_empty());

    root.close().unwrap();
}

#[test]
fn checklist_read_under_later_root_returns_ordered_root_summary() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_tools").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Tools", 42);

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &multi_root_checklist_graph())
        .unwrap();

    let response = service
        .read_checklist(&ChecklistReadRequest {
            workspace_id: workspace_id.clone(),
            checklist_node_id: SemanticNodeId::new("checklist_b").unwrap(),
        })
        .unwrap();

    assert_eq!(
        response
            .summary
            .root_nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        ["root_a", "root_b"]
    );
    assert_eq!(response.summary.root_node_count, 2);
    assert_eq!(
        response.checklist.id,
        SemanticNodeId::new("checklist_b").unwrap()
    );
    assert_eq!(response.items.len(), 1);
    assert_eq!(
        response.items[0].node.id,
        SemanticNodeId::new("item_b").unwrap()
    );

    root.close().unwrap();
}

#[test]
fn checklist_read_rejects_non_checklist_nodes() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_tools").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Tools", 42);
    let graph = sample_graph();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let error = service
        .read_checklist(&ChecklistReadRequest {
            workspace_id: workspace_id.clone(),
            checklist_node_id: SemanticNodeId::new("root").unwrap(),
        })
        .unwrap_err();

    assert!(matches!(
        error,
        WorkspaceGraphToolError::NodeNotChecklist { .. }
    ));

    root.close().unwrap();
}

#[test]
fn checklist_read_truncates_large_item_sets() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_tools").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Tools", 42);
    let graph = wide_checklist(MAX_CHECKLIST_ITEM_COUNT + 3);

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let response = service
        .read_checklist(&ChecklistReadRequest {
            workspace_id: workspace_id.clone(),
            checklist_node_id: SemanticNodeId::new("checklist").unwrap(),
        })
        .unwrap();

    assert!(response.truncated);
    assert_eq!(response.items.len(), MAX_CHECKLIST_ITEM_COUNT);
    assert_eq!(
        response.items[0].node.id,
        SemanticNodeId::new("item_0").unwrap()
    );

    root.close().unwrap();
}

#[test]
fn checklist_item_thread_ref_upsert_attaches_to_existing_item_node() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_tools").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Tools", 42);
    let graph = sample_graph();
    let item_b_id = SemanticNodeId::new("item_b").unwrap();
    let request = ThreadRefUpsertRequest {
        workspace_id: workspace_id.clone(),
        thread_ref: ThreadRefDraft::new(
            ThreadRefId::new("item_b_thread").unwrap(),
            item_b_id.clone(),
            ConversationThreadId::new("thread_item_b"),
            WorkspaceId::host_windows(r"C:\work\beryl"),
            "Item B thread",
        ),
        provenance: provenance(20),
        expected_base_revision: None,
    };

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let response = service.upsert_thread_ref(&request).unwrap();
    let checklist = service
        .read_checklist(&ChecklistReadRequest {
            workspace_id: workspace_id.clone(),
            checklist_node_id: SemanticNodeId::new("checklist").unwrap(),
        })
        .unwrap();
    let stored = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();

    assert!(response.commit.changed);
    assert_eq!(stored.node_count(), graph.node_count());
    assert_eq!(
        stored
            .child_ids_of(&SemanticNodeId::new("checklist").unwrap())
            .unwrap()
            .len(),
        2
    );
    let item_b = checklist
        .items
        .iter()
        .find(|item| item.node.id == item_b_id)
        .unwrap();
    assert_eq!(item_b.thread_refs.len(), 1);
    assert_eq!(item_b.thread_refs[0].thread_id.as_str(), "thread_item_b");

    root.close().unwrap();
}

#[test]
fn graph_patch_writes_use_the_durable_repository_path() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_tools").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Tools", 42);
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

    let response = service
        .apply_graph_patch(&GraphPatchWriteRequest {
            workspace_id: workspace_id.clone(),
            patch: patch.clone(),
            expected_base_revision: None,
        })
        .unwrap();
    let repeat = service
        .apply_graph_patch(&GraphPatchWriteRequest {
            workspace_id: workspace_id.clone(),
            patch,
            expected_base_revision: None,
        })
        .unwrap();
    let graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();

    assert!(response.commit.changed);
    assert_eq!(response.summary.node_count, 1);
    assert_eq!(response.summary.root_node_count, 1);
    assert_eq!(response.summary.root_nodes[0].id, root_id);
    assert!(!repeat.commit.changed);
    assert!(graph.node(&SemanticNodeId::new("root").unwrap()).is_some());

    root.close().unwrap();
}

#[test]
fn thread_ref_upserts_use_the_same_patch_path_and_report_noops() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_tools").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Tools", 42);
    let graph = sample_graph();
    let request = ThreadRefUpsertRequest {
        workspace_id: workspace_id.clone(),
        thread_ref: ThreadRefDraft::new(
            ThreadRefId::new("sibling_thread").unwrap(),
            SemanticNodeId::new("sibling").unwrap(),
            ConversationThreadId::new("thread_2"),
            WorkspaceId::host_windows(r"C:\work\beryl"),
            "Sibling thread",
        ),
        provenance: provenance(20),
        expected_base_revision: None,
    };

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let response = service.upsert_thread_ref(&request).unwrap();
    let repeat = service.upsert_thread_ref(&request).unwrap();
    let stored = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();

    assert!(response.commit.changed);
    assert_eq!(response.summary.thread_ref_count, 2);
    assert!(!repeat.commit.changed);
    assert!(
        stored
            .thread_ref(&ThreadRefId::new("sibling_thread").unwrap())
            .is_some()
    );

    root.close().unwrap();
}

#[test]
fn graph_neighborhood_rejects_excessive_depth_requests() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_tools").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Tools", 42);

    persistence.save_workspace_manifest(&manifest).unwrap();

    let error = service
        .read_graph_neighborhood(&GraphNeighborhoodRequest {
            workspace_id: workspace_id.clone(),
            anchor_node_id: None,
            parent_depth: 0,
            child_depth: 99,
        })
        .unwrap_err();

    assert!(matches!(
        error,
        WorkspaceGraphToolError::ChildDepthTooLarge { requested: 99, .. }
    ));

    let error = service
        .read_graph_neighborhood(&GraphNeighborhoodRequest {
            workspace_id: workspace_id.clone(),
            anchor_node_id: None,
            parent_depth: 99,
            child_depth: 0,
        })
        .unwrap_err();

    assert!(matches!(
        error,
        WorkspaceGraphToolError::ParentDepthTooLarge { requested: 99, .. }
    ));

    root.close().unwrap();
}

fn multi_root_graph() -> SemanticGraph {
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
                provenance: provenance(1),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    root_b_id.clone(),
                    "Root B",
                    "Root B summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: provenance(2),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    child_a_id.clone(),
                    "Child A",
                    "Child A summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: provenance(3),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    child_b_id.clone(),
                    "Child B",
                    "Child B summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: provenance(4),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_a_id.clone(),
                parent_id: None,
                index: None,
                provenance: provenance(5),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_b_id.clone(),
                parent_id: None,
                index: None,
                provenance: provenance(6),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: child_a_id.clone(),
                parent_id: Some(root_a_id),
                index: None,
                provenance: provenance(7),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: child_b_id.clone(),
                parent_id: Some(root_b_id),
                index: None,
                provenance: provenance(8),
            },
            SemanticGraphPatchOp::UpsertSoftLink {
                link: SoftLinkDraft::new(
                    SoftLinkId::new("child_b_depends_on_child_a").unwrap(),
                    child_b_id,
                    child_a_id,
                    SoftLinkKind::new("depends_on").unwrap(),
                ),
                provenance: provenance(9),
            },
        ]))
        .unwrap();

    graph
}

fn multi_root_checklist_graph() -> SemanticGraph {
    let root_a_id = SemanticNodeId::new("root_a").unwrap();
    let root_b_id = SemanticNodeId::new("root_b").unwrap();
    let checklist_id = SemanticNodeId::new("checklist_b").unwrap();
    let item_id = SemanticNodeId::new("item_b").unwrap();
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
                provenance: provenance(20),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    root_b_id.clone(),
                    "Root B",
                    "Root B summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: provenance(21),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    checklist_id.clone(),
                    "Checklist B",
                    "Checklist B summary",
                    SemanticNodeFacets::topic_and_checklist(),
                    None,
                ),
                provenance: provenance(22),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    item_id.clone(),
                    "Item B",
                    "Item B summary",
                    SemanticNodeFacets::topic_and_checklist_item(),
                    Some(ChecklistItemStatus::Todo),
                ),
                provenance: provenance(23),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_a_id,
                parent_id: None,
                index: None,
                provenance: provenance(24),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_b_id.clone(),
                parent_id: None,
                index: None,
                provenance: provenance(25),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: checklist_id.clone(),
                parent_id: Some(root_b_id),
                index: None,
                provenance: provenance(26),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: item_id,
                parent_id: Some(checklist_id),
                index: None,
                provenance: provenance(27),
            },
        ]))
        .unwrap();

    graph
}

fn wide_root_graph(root_count: usize) -> SemanticGraph {
    let mut operations = Vec::new();

    for index in 0..root_count {
        let root_id = SemanticNodeId::new(format!("root_{index}")).unwrap();
        operations.push(SemanticGraphPatchOp::UpsertNode {
            node: SemanticNodeDraft::new(
                root_id.clone(),
                format!("Root {index}"),
                format!("Root {index} summary"),
                SemanticNodeFacets::topic(),
                None,
            ),
            provenance: provenance((index + 1) as u64),
        });
        operations.push(SemanticGraphPatchOp::SetHardParent {
            child_id: root_id,
            parent_id: None,
            index: None,
            provenance: provenance((index + root_count + 1) as u64),
        });
    }

    let mut graph = SemanticGraph::default();
    graph
        .apply_patch(&SemanticGraphPatch::new(operations))
        .unwrap();
    graph
}

fn sample_graph() -> SemanticGraph {
    let root_id = SemanticNodeId::new("root").unwrap();
    let checklist_id = SemanticNodeId::new("checklist").unwrap();
    let item_a_id = SemanticNodeId::new("item_a").unwrap();
    let item_b_id = SemanticNodeId::new("item_b").unwrap();
    let sibling_id = SemanticNodeId::new("sibling").unwrap();
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
                    item_a_id.clone(),
                    "Item A",
                    "Item A summary",
                    SemanticNodeFacets::topic_and_checklist_item(),
                    Some(ChecklistItemStatus::InProgress),
                ),
                provenance: provenance(3),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    item_b_id.clone(),
                    "Item B",
                    "Item B summary",
                    SemanticNodeFacets::topic_and_checklist_item(),
                    Some(ChecklistItemStatus::Todo),
                ),
                provenance: provenance(4),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    sibling_id.clone(),
                    "Sibling",
                    "Sibling summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: provenance(5),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_id.clone(),
                parent_id: None,
                index: None,
                provenance: provenance(6),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: checklist_id.clone(),
                parent_id: Some(root_id.clone()),
                index: None,
                provenance: provenance(7),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: item_a_id.clone(),
                parent_id: Some(checklist_id.clone()),
                index: Some(0),
                provenance: provenance(8),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: item_b_id.clone(),
                parent_id: Some(checklist_id.clone()),
                index: Some(1),
                provenance: provenance(9),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: sibling_id.clone(),
                parent_id: Some(root_id),
                index: Some(1),
                provenance: provenance(10),
            },
            SemanticGraphPatchOp::UpsertSoftLink {
                link: SoftLinkDraft::new(
                    SoftLinkId::new("depends").unwrap(),
                    item_a_id.clone(),
                    sibling_id,
                    SoftLinkKind::new("depends_on").unwrap(),
                ),
                provenance: provenance(11),
            },
            SemanticGraphPatchOp::UpsertThreadRef {
                thread_ref: ThreadRefDraft::new(
                    ThreadRefId::new("thread_ref").unwrap(),
                    item_a_id,
                    ConversationThreadId::new("thread_1"),
                    WorkspaceId::host_windows(r"C:\work\beryl"),
                    "Item thread",
                ),
                provenance: provenance(12),
            },
        ]))
        .unwrap();

    graph
}

fn wide_graph(child_count: usize) -> SemanticGraph {
    let root_id = SemanticNodeId::new("root").unwrap();
    let mut operations = vec![SemanticGraphPatchOp::UpsertNode {
        node: SemanticNodeDraft::new(
            root_id.clone(),
            "Root",
            "Root summary",
            SemanticNodeFacets::topic(),
            None,
        ),
        provenance: provenance(1),
    }];
    operations.push(SemanticGraphPatchOp::SetHardParent {
        child_id: root_id.clone(),
        parent_id: None,
        index: None,
        provenance: provenance(2),
    });

    for index in 0..child_count {
        let child_id = SemanticNodeId::new(format!("child_{index}")).unwrap();
        operations.push(SemanticGraphPatchOp::UpsertNode {
            node: SemanticNodeDraft::new(
                child_id.clone(),
                format!("Child {index}"),
                format!("Child {index} summary"),
                SemanticNodeFacets::topic(),
                None,
            ),
            provenance: provenance((index + 3) as u64),
        });
        operations.push(SemanticGraphPatchOp::SetHardParent {
            child_id,
            parent_id: Some(root_id.clone()),
            index: None,
            provenance: provenance((index + child_count + 3) as u64),
        });
    }

    let mut graph = SemanticGraph::default();
    graph
        .apply_patch(&SemanticGraphPatch::new(operations))
        .unwrap();
    graph
}

fn wide_checklist(item_count: usize) -> SemanticGraph {
    let checklist_id = SemanticNodeId::new("checklist").unwrap();
    let mut operations = vec![SemanticGraphPatchOp::UpsertNode {
        node: SemanticNodeDraft::new(
            checklist_id.clone(),
            "Checklist",
            "Checklist summary",
            SemanticNodeFacets::topic_and_checklist(),
            None,
        ),
        provenance: provenance(1),
    }];
    operations.push(SemanticGraphPatchOp::SetHardParent {
        child_id: checklist_id.clone(),
        parent_id: None,
        index: None,
        provenance: provenance(2),
    });

    for index in 0..item_count {
        let item_id = SemanticNodeId::new(format!("item_{index}")).unwrap();
        operations.push(SemanticGraphPatchOp::UpsertNode {
            node: SemanticNodeDraft::new(
                item_id.clone(),
                format!("Item {index}"),
                format!("Item {index} summary"),
                SemanticNodeFacets::topic_and_checklist_item(),
                Some(ChecklistItemStatus::Todo),
            ),
            provenance: provenance((index + 3) as u64),
        });
        operations.push(SemanticGraphPatchOp::SetHardParent {
            child_id: item_id,
            parent_id: Some(checklist_id.clone()),
            index: None,
            provenance: provenance((index + item_count + 3) as u64),
        });
    }

    let mut graph = SemanticGraph::default();
    graph
        .apply_patch(&SemanticGraphPatch::new(operations))
        .unwrap();
    graph
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
    tempdir_support::temp_dir("beryl-workspace-graph-tools-test-")
}
