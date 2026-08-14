#[path = "support/tempdir.rs"]
mod tempdir_support;

use beryl_app::{
    BERYL_GRAPH_DYNAMIC_TOOL_NAMESPACE, BerylWorkspacePersistence, SET_CHECKLIST_ITEM_STATUS_TOOL,
    WorkspaceGraphMutationCommit, WorkspaceGraphRevision, WorkspaceGraphToolService,
    dispatch_beryl_graph_dynamic_tool_call_with_metadata,
};
use beryl_backend::{DynamicToolCallRequest, parse_dynamic_tool_call_request};
use beryl_model::{
    provenance::{MutationProvenance, MutationSource},
    semantic_graph::{
        ChecklistItemStatus, SemanticGraph, SemanticGraphPatch, SemanticGraphPatchOp,
        SemanticNodeDraft, SemanticNodeFacets, SemanticNodeId,
    },
    workspace::{BerylWorkspaceId, BerylWorkspaceManifest},
};
use serde_json::{Value, json};

#[path = "../src/shell/checklist_sidebar_projection.rs"]
mod checklist_sidebar_projection;
#[allow(dead_code)]
#[path = "../src/shell/column_selector.rs"]
mod column_selector;
#[allow(dead_code)]
#[path = "../src/shell/graph.rs"]
mod graph;

#[test]
fn checklist_sidebar_projection_preserves_flat_item_order_numbers_and_status_labels() {
    let graph = checklist_graph();
    let checklist = graph.node(&node_id("release_checklist")).unwrap();

    let projection = checklist_sidebar_projection::project_checklist_projection(&graph, checklist);
    let rows = (0..projection.row_count())
        .map(|index| projection.row(&graph, index).unwrap())
        .collect::<Vec<_>>();

    assert_eq!(projection.title(), "Release checklist");
    assert_eq!(projection.checklist_id(), &node_id("release_checklist"));
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].node_id, node_id("draft"));
    assert_eq!(rows[0].number, 1);
    assert_eq!(rows[0].title, "Draft release notes");
    assert_eq!(rows[0].status_label, "todo");
    assert_eq!(rows[1].node_id, node_id("verify"));
    assert_eq!(rows[1].number, 2);
    assert_eq!(rows[1].title, "Verify installer");
    assert_eq!(rows[1].status_label, "doing");
    assert_eq!(rows[2].node_id, node_id("publish"));
    assert_eq!(rows[2].number, 3);
    assert_eq!(rows[2].title, "Publish artifacts");
    assert_eq!(rows[2].status_label, "done");
}

#[test]
fn checklist_sidebar_projection_cache_refreshes_only_when_selection_or_graph_changes() {
    let mut graph = checklist_graph();
    let checklist = node_id("release_checklist");
    let draft = node_id("draft");
    let mut cache = checklist_sidebar_projection::ChecklistSidebarProjectionCache::default();

    let first_refresh = cache.refresh(&graph, Some(&checklist));

    assert!(first_refresh.changed());
    assert!(first_refresh.selected_checklist_changed());
    assert_eq!(first_refresh.previous_row_count(), 0);
    assert_eq!(first_refresh.row_count(), 3);
    assert_eq!(cache.projection().unwrap().row_count(), 3);

    let repeat_refresh = cache.refresh(&graph, Some(&checklist));

    assert!(!repeat_refresh.changed());
    assert!(!repeat_refresh.selected_checklist_changed());

    graph
        .apply_patch(&SemanticGraphPatch::from_operation(
            SemanticGraphPatchOp::SetChecklistItemStatus {
                node_id: draft.clone(),
                status: ChecklistItemStatus::Done,
                provenance: provenance(8),
            },
        ))
        .unwrap();

    let graph_refresh = cache.refresh(&graph, Some(&checklist));

    assert!(graph_refresh.changed());
    assert!(!graph_refresh.selected_checklist_changed());
    assert_eq!(
        cache.projection().unwrap().row(&graph, 0).unwrap().status,
        Some(ChecklistItemStatus::Done)
    );

    let cleared_refresh = cache.refresh(&graph, None);

    assert!(cleared_refresh.changed());
    assert!(cleared_refresh.selected_checklist_changed());
    assert!(cache.projection().is_none());
}

#[test]
fn checklist_sidebar_projection_cache_reflects_optimistic_status_projection() {
    let checklist = node_id("release_checklist");
    let draft = node_id("draft");
    let mut overlay =
        graph::GraphOverlayState::new(checklist_graph(), WorkspaceGraphRevision::default(), None);
    let mut cache = checklist_sidebar_projection::ChecklistSidebarProjectionCache::default();

    cache.refresh(overlay.graph(), Some(&checklist));
    begin_optimistic_patch(
        &mut overlay,
        checklist_status_patch(&draft, ChecklistItemStatus::Done, 80),
        [draft.clone()],
    );
    let refresh = cache.refresh(overlay.graph(), Some(&checklist));

    assert!(refresh.changed());
    assert!(!refresh.selected_checklist_changed());
    assert_eq!(
        cache
            .projection()
            .unwrap()
            .row(overlay.graph(), 0)
            .unwrap()
            .status,
        Some(ChecklistItemStatus::Done)
    );
}

#[test]
fn checklist_sidebar_projection_cache_reflects_committed_status_projection() {
    let checklist = node_id("release_checklist");
    let draft = node_id("draft");
    let patch = checklist_status_patch(&draft, ChecklistItemStatus::Done, 81);
    let mut overlay =
        graph::GraphOverlayState::new(checklist_graph(), WorkspaceGraphRevision::default(), None);
    let mut cache = checklist_sidebar_projection::ChecklistSidebarProjectionCache::default();

    cache.refresh(overlay.graph(), Some(&checklist));
    overlay
        .finish_mutation_commit_update(graph_commit_update(0, 1, true, patch))
        .unwrap();
    let refresh = cache.refresh(overlay.graph(), Some(&checklist));

    assert!(refresh.changed());
    assert_eq!(
        cache
            .projection()
            .unwrap()
            .row(overlay.graph(), 0)
            .unwrap()
            .status,
        Some(ChecklistItemStatus::Done)
    );
}

#[test]
fn checklist_sidebar_projection_cache_drops_optimistically_deleted_item() {
    let checklist = node_id("release_checklist");
    let draft = node_id("draft");
    let mut overlay =
        graph::GraphOverlayState::new(checklist_graph(), WorkspaceGraphRevision::default(), None);
    let mut cache = checklist_sidebar_projection::ChecklistSidebarProjectionCache::default();

    cache.refresh(overlay.graph(), Some(&checklist));
    begin_optimistic_patch(
        &mut overlay,
        SemanticGraphPatch::from_operation(SemanticGraphPatchOp::DeleteNodeLeaf {
            node_id: draft.clone(),
            provenance: provenance(82),
        }),
        [draft.clone()],
    );
    let refresh = cache.refresh(overlay.graph(), Some(&checklist));
    let projection = cache.projection().unwrap();

    assert!(refresh.changed());
    assert_eq!(projection.row_count(), 2);
    assert!(
        projection
            .row(overlay.graph(), 0)
            .is_some_and(|row| row.node_id != draft)
    );
    assert!(
        projection
            .row(overlay.graph(), 1)
            .is_some_and(|row| row.node_id != draft)
    );
}

#[test]
fn checklist_sidebar_projection_cache_clears_when_selected_checklist_is_deleted() {
    let mut graph = checklist_graph();
    let checklist = node_id("release_checklist");
    let mut cache = checklist_sidebar_projection::ChecklistSidebarProjectionCache::default();

    cache.refresh(&graph, Some(&checklist));

    graph
        .apply_patch(&SemanticGraphPatch::from_operation(
            SemanticGraphPatchOp::DeleteNodeSubtree {
                node_id: checklist.clone(),
                provenance: provenance(8),
            },
        ))
        .unwrap();

    let refresh = cache.refresh(&graph, Some(&checklist));

    assert!(refresh.changed());
    assert!(refresh.selected_checklist_changed());
    assert_eq!(refresh.previous_row_count(), 3);
    assert_eq!(refresh.row_count(), 0);
    assert!(cache.projection().is_none());
}

#[test]
fn checklist_sidebar_projection_cache_drops_deleted_checklist_item() {
    let mut graph = checklist_graph();
    let checklist = node_id("release_checklist");
    let draft = node_id("draft");
    let mut cache = checklist_sidebar_projection::ChecklistSidebarProjectionCache::default();

    cache.refresh(&graph, Some(&checklist));

    graph
        .apply_patch(&SemanticGraphPatch::from_operation(
            SemanticGraphPatchOp::DeleteNodeSubtree {
                node_id: draft.clone(),
                provenance: provenance(8),
            },
        ))
        .unwrap();

    let refresh = cache.refresh(&graph, Some(&checklist));
    let projection = cache.projection().unwrap();

    assert!(refresh.changed());
    assert!(!refresh.selected_checklist_changed());
    assert_eq!(refresh.previous_row_count(), 3);
    assert_eq!(refresh.row_count(), 2);
    assert!(
        projection
            .row(&graph, 0)
            .is_some_and(|row| row.node_id != draft)
    );
    assert!(
        projection
            .row(&graph, 1)
            .is_some_and(|row| row.node_id != draft)
    );
}

#[test]
fn checklist_sidebar_projection_preserves_checklist_under_reordered_second_root() {
    let root_b = node_id("root_b");
    let checklist = node_id("release_checklist");
    let reorder_roots = SemanticGraphPatch::from_operation(SemanticGraphPatchOp::SetHardParent {
        child_id: root_b,
        parent_id: None,
        index: Some(0),
        provenance: provenance(90),
    });
    let mut overlay = graph::GraphOverlayState::new(
        multi_root_checklist_graph(),
        WorkspaceGraphRevision::default(),
        None,
    );
    let mut cache = checklist_sidebar_projection::ChecklistSidebarProjectionCache::default();

    assert!(overlay.select_node(0, &node_id("root_b")));
    assert!(overlay.select_node(1, &checklist));
    cache.refresh(overlay.graph(), overlay.selected_node_id());

    overlay
        .finish_mutation_commit_update(graph_commit_update(0, 1, true, reorder_roots))
        .unwrap();
    let refresh = cache.refresh(overlay.graph(), overlay.selected_node_id());

    assert_eq!(overlay.selected_node_id(), Some(&checklist));
    assert!(!refresh.selected_checklist_changed());
    assert_eq!(
        overlay.graph().root_node_ids(),
        &[node_id("root_b"), node_id("root_a")]
    );
    assert_eq!(cache.projection().unwrap().row_count(), 1);
}

#[test]
fn checklist_sidebar_projection_preserves_selection_when_unrelated_root_is_deleted() {
    let root_a = node_id("root_a");
    let checklist = node_id("release_checklist");
    let delete_unrelated_root =
        SemanticGraphPatch::from_operation(SemanticGraphPatchOp::DeleteNodeSubtree {
            node_id: root_a,
            provenance: provenance(91),
        });
    let mut overlay = graph::GraphOverlayState::new(
        multi_root_checklist_graph(),
        WorkspaceGraphRevision::default(),
        None,
    );
    let mut cache = checklist_sidebar_projection::ChecklistSidebarProjectionCache::default();

    assert!(overlay.select_node(0, &node_id("root_b")));
    assert!(overlay.select_node(1, &checklist));
    cache.refresh(overlay.graph(), overlay.selected_node_id());

    overlay
        .finish_mutation_commit_update(graph_commit_update(0, 1, true, delete_unrelated_root))
        .unwrap();
    let refresh = cache.refresh(overlay.graph(), overlay.selected_node_id());

    assert_eq!(overlay.selected_node_id(), Some(&checklist));
    assert!(!refresh.selected_checklist_changed());
    assert_eq!(overlay.graph().root_node_ids(), &[node_id("root_b")]);
    assert!(cache.projection().is_some());
}

#[test]
fn checklist_sidebar_projection_clears_when_selected_checklist_root_is_deleted() {
    let root_b = node_id("root_b");
    let delete_selected_root =
        SemanticGraphPatch::from_operation(SemanticGraphPatchOp::DeleteNodeSubtree {
            node_id: root_b,
            provenance: provenance(92),
        });
    let mut overlay = graph::GraphOverlayState::new(
        multi_root_checklist_graph(),
        WorkspaceGraphRevision::default(),
        None,
    );
    let mut cache = checklist_sidebar_projection::ChecklistSidebarProjectionCache::default();

    assert!(overlay.select_node(0, &node_id("root_b")));
    assert!(overlay.select_node(1, &node_id("release_checklist")));
    cache.refresh(overlay.graph(), overlay.selected_node_id());

    overlay
        .finish_mutation_commit_update(graph_commit_update(0, 1, true, delete_selected_root))
        .unwrap();
    let refresh = cache.refresh(overlay.graph(), overlay.selected_node_id());

    assert_eq!(overlay.selected_node_id(), None);
    assert!(refresh.selected_checklist_changed());
    assert_eq!(overlay.graph().root_node_ids(), &[node_id("root_a")]);
    assert!(cache.projection().is_none());
}

#[test]
fn checklist_sidebar_row_element_key_uses_stable_semantic_node_id() {
    let mut graph = checklist_graph();
    let checklist = graph.node(&node_id("release_checklist")).unwrap();
    let before = checklist_sidebar_projection::project_checklist_projection(&graph, checklist);
    let verify_key = before.row(&graph, 1).unwrap().element_key();

    graph
        .apply_patch(&SemanticGraphPatch::from_operation(
            SemanticGraphPatchOp::SetHardParent {
                child_id: node_id("verify"),
                parent_id: Some(node_id("release_checklist")),
                index: Some(0),
                provenance: provenance(8),
            },
        ))
        .unwrap();

    let checklist = graph.node(&node_id("release_checklist")).unwrap();
    let after = checklist_sidebar_projection::project_checklist_projection(&graph, checklist);

    assert_eq!(verify_key, "checklist-item-row-verify");
    let after_row = after.row(&graph, 0).unwrap();
    assert_eq!(after_row.node_id, node_id("verify"));
    assert_eq!(after_row.number, 1);
    assert_eq!(after_row.element_key(), verify_key);
}

#[test]
fn checklist_status_label_defaults_missing_status_to_todo() {
    assert_eq!(
        checklist_sidebar_projection::checklist_status_label(None),
        "todo"
    );
}

#[test]
fn checklist_projection_reflects_dynamic_tool_commit_projection() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("checklist_dynamic_projection").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Checklist Dynamic", 42);
    let checklist_id = node_id("release_checklist");
    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &checklist_graph())
        .unwrap();
    let request = dynamic_tool_request(
        SET_CHECKLIST_ITEM_STATUS_TOOL,
        json!({
            "nodeId": "draft",
            "status": "done"
        }),
    );

    let dispatch =
        dispatch_beryl_graph_dynamic_tool_call_with_metadata(&service, &workspace_id, &request);
    let commit = dispatch.graph_write().unwrap().into_commit();
    let mut overlay =
        graph::GraphOverlayState::new(checklist_graph(), WorkspaceGraphRevision::default(), None);
    let mut cache = checklist_sidebar_projection::ChecklistSidebarProjectionCache::default();
    cache.refresh(overlay.graph(), Some(&checklist_id));

    overlay
        .finish_mutation_commit_update(graph::GraphMutationCommitUpdate::new(commit, ""))
        .unwrap();
    let refresh = cache.refresh(overlay.graph(), Some(&checklist_id));
    let row = cache.projection().unwrap().row(overlay.graph(), 0).unwrap();

    assert!(dispatch.response().success);
    assert!(refresh.changed());
    assert!(!refresh.selected_checklist_changed());
    assert_eq!(row.node_id, node_id("draft"));
    assert_eq!(row.status, Some(ChecklistItemStatus::Done));
    assert_eq!(row.status_label, "done");

    root.close().unwrap();
}

fn checklist_graph() -> SemanticGraph {
    let checklist_id = node_id("release_checklist");
    let draft_id = node_id("draft");
    let verify_id = node_id("verify");
    let publish_id = node_id("publish");
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    checklist_id.clone(),
                    "Release checklist",
                    "Prepare a release.",
                    SemanticNodeFacets::topic_and_checklist(),
                    None,
                ),
                provenance: provenance(1),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: checklist_item_node(
                    draft_id.clone(),
                    "Draft release notes",
                    ChecklistItemStatus::Todo,
                ),
                provenance: provenance(2),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: checklist_item_node(
                    verify_id.clone(),
                    "Verify installer",
                    ChecklistItemStatus::InProgress,
                ),
                provenance: provenance(3),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: checklist_item_node(
                    publish_id.clone(),
                    "Publish artifacts",
                    ChecklistItemStatus::Done,
                ),
                provenance: provenance(4),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: checklist_id.clone(),
                parent_id: None,
                index: None,
                provenance: provenance(5),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: draft_id,
                parent_id: Some(checklist_id.clone()),
                index: Some(0),
                provenance: provenance(6),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: verify_id,
                parent_id: Some(checklist_id.clone()),
                index: Some(1),
                provenance: provenance(7),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: publish_id,
                parent_id: Some(checklist_id),
                index: Some(2),
                provenance: provenance(8),
            },
        ]))
        .unwrap();

    graph
}

fn multi_root_checklist_graph() -> SemanticGraph {
    let root_a_id = node_id("root_a");
    let root_b_id = node_id("root_b");
    let checklist_id = node_id("release_checklist");
    let item_id = node_id("draft");
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
                provenance: provenance(30),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    root_b_id.clone(),
                    "Root B",
                    "Root B summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: provenance(31),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    checklist_id.clone(),
                    "Release checklist",
                    "Prepare a release.",
                    SemanticNodeFacets::topic_and_checklist(),
                    None,
                ),
                provenance: provenance(32),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: checklist_item_node(
                    item_id.clone(),
                    "Draft release notes",
                    ChecklistItemStatus::Todo,
                ),
                provenance: provenance(33),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_a_id,
                parent_id: None,
                index: None,
                provenance: provenance(34),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_b_id.clone(),
                parent_id: None,
                index: None,
                provenance: provenance(35),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: checklist_id.clone(),
                parent_id: Some(root_b_id),
                index: None,
                provenance: provenance(36),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: item_id,
                parent_id: Some(checklist_id),
                index: None,
                provenance: provenance(37),
            },
        ]))
        .unwrap();

    graph
}

fn checklist_item_node(
    node_id: SemanticNodeId,
    title: &str,
    status: ChecklistItemStatus,
) -> SemanticNodeDraft {
    SemanticNodeDraft::new(
        node_id,
        title,
        format!("{title}."),
        SemanticNodeFacets::topic_and_checklist_item(),
        Some(status),
    )
}

fn node_id(value: &str) -> SemanticNodeId {
    SemanticNodeId::new(value).unwrap()
}

fn provenance(recorded_at_millis: u64) -> MutationProvenance {
    MutationProvenance::new(
        "operator",
        recorded_at_millis,
        MutationSource::workspace_action("checklist_sidebar_projection_test").unwrap(),
        Some(100),
    )
    .unwrap()
}

fn begin_optimistic_patch(
    overlay: &mut graph::GraphOverlayState,
    patch: SemanticGraphPatch,
    affected_node_ids: impl IntoIterator<Item = SemanticNodeId>,
) -> graph::OptimisticGraphMutationId {
    let mutation_id = overlay.reserve_optimistic_mutation_id();
    let mutation = graph::GraphOptimisticMutation::new(
        mutation_id,
        overlay.revision(),
        patch,
        affected_node_ids,
        "Applying optimistic checklist graph mutation",
    );
    overlay.begin_optimistic_mutation(mutation).unwrap();
    mutation_id
}

fn checklist_status_patch(
    node_id: &SemanticNodeId,
    status: ChecklistItemStatus,
    recorded_at_millis: u64,
) -> SemanticGraphPatch {
    SemanticGraphPatch::from_operation(SemanticGraphPatchOp::SetChecklistItemStatus {
        node_id: node_id.clone(),
        status,
        provenance: provenance(recorded_at_millis),
    })
}

fn graph_commit_update(
    base_revision: u64,
    committed_revision: u64,
    changed: bool,
    patch: SemanticGraphPatch,
) -> graph::GraphMutationCommitUpdate {
    graph::GraphMutationCommitUpdate::new(
        WorkspaceGraphMutationCommit::new(
            BerylWorkspaceId::new("checklist_dynamic_projection").unwrap(),
            WorkspaceGraphRevision::new(base_revision),
            WorkspaceGraphRevision::new(committed_revision),
            changed,
            patch,
            BerylWorkspaceManifest::named(
                BerylWorkspaceId::new("checklist_dynamic_projection").unwrap(),
                format!("Checklist Dynamic {committed_revision}"),
                1000 + committed_revision,
            ),
        ),
        "checklist graph no-op",
    )
}

fn dynamic_tool_request(tool: &str, arguments: Value) -> DynamicToolCallRequest {
    parse_dynamic_tool_call_request(
        json!("dynamic-request-1"),
        "item/tool/call",
        Some(json!({
            "threadId": "thread_1",
            "turnId": "turn_1",
            "callId": "call_1",
            "namespace": BERYL_GRAPH_DYNAMIC_TOOL_NAMESPACE,
            "tool": tool,
            "arguments": arguments
        })),
    )
    .unwrap()
    .unwrap()
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-checklist-sidebar-projection-test-")
}
