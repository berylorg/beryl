#[path = "support/tempdir.rs"]
mod tempdir_support;

use beryl_app::{
    BERYL_GRAPH_DYNAMIC_TOOL_NAMESPACE, BerylWorkspacePersistence, SET_CHECKLIST_ITEM_STATUS_TOOL,
    SET_GRAPH_NODE_PARENT_TOOL, UPSERT_GRAPH_NODE_TOOL, UPSERT_GRAPH_SOFT_LINK_TOOL,
    WorkspaceGraphRevision, WorkspaceGraphToolService,
    dispatch_beryl_graph_dynamic_tool_call_with_metadata,
};
use beryl_backend::{DynamicToolCallRequest, parse_dynamic_tool_call_request};
use beryl_model::{
    conversation::ConversationThreadId,
    provenance::{MutationProvenance, MutationSource},
    semantic_graph::{
        ChecklistItemStatus, SemanticGraph, SemanticGraphPatch, SemanticGraphPatchOp,
        SemanticNodeDraft, SemanticNodeFacets, SemanticNodeId, SoftLinkDraft, SoftLinkId,
        SoftLinkKind, ThreadRefDraft, ThreadRefId,
    },
    workspace::{BerylWorkspaceId, BerylWorkspaceManifest, WorkspaceId},
};
use serde_json::{Value, json};

#[test]
fn dynamic_write_dispatch_retains_operation_specific_committed_patches() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("dynamic_commit_patch").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Dynamic Patch", 42);
    persistence.save_workspace_manifest(&manifest).unwrap();

    let root_request = dynamic_tool_request(
        UPSERT_GRAPH_NODE_TOOL,
        json!({
            "nodeId": "root",
            "parentId": null,
            "title": "Root",
            "summary": "Root summary",
            "topic": true,
            "checklist": false,
            "checklistItem": false
        }),
    );
    let root_dispatch = dispatch_beryl_graph_dynamic_tool_call_with_metadata(
        &service,
        &workspace_id,
        &root_request,
    );
    let root_write = root_dispatch.graph_write().unwrap();
    let root_commit = root_write.commit();

    assert!(root_dispatch.response().success);
    assert!(root_commit.changed);
    assert_eq!(root_commit.base_revision, WorkspaceGraphRevision::default());
    assert_eq!(
        root_commit.committed_revision,
        WorkspaceGraphRevision::new(1)
    );
    assert_eq!(root_commit.patch.operations().len(), 2);
    assert_node_upsert_operation(
        &root_commit.patch.operations()[0],
        "root",
        "Root",
        UPSERT_GRAPH_NODE_TOOL,
    );
    assert_parent_operation(
        &root_commit.patch.operations()[1],
        "root",
        None,
        UPSERT_GRAPH_NODE_TOOL,
    );

    let checklist_request = dynamic_tool_request(
        UPSERT_GRAPH_NODE_TOOL,
        json!({
            "nodeId": "checklist",
            "parentId": "root",
            "title": "Checklist",
            "summary": "Checklist summary",
            "topic": true,
            "checklist": true,
            "checklistItem": false
        }),
    );
    dispatch_beryl_graph_dynamic_tool_call_with_metadata(
        &service,
        &workspace_id,
        &checklist_request,
    );
    let item_request = dynamic_tool_request(
        UPSERT_GRAPH_NODE_TOOL,
        json!({
            "nodeId": "draft",
            "parentId": "checklist",
            "title": "Draft",
            "summary": "Draft summary",
            "topic": true,
            "checklist": false,
            "checklistItem": true,
            "checklistItemStatus": "todo"
        }),
    );
    dispatch_beryl_graph_dynamic_tool_call_with_metadata(&service, &workspace_id, &item_request);

    let parent_request = dynamic_tool_request(
        SET_GRAPH_NODE_PARENT_TOOL,
        json!({
            "childId": "draft",
            "parentId": "checklist",
            "index": 0
        }),
    );
    let parent_commit = dispatch_beryl_graph_dynamic_tool_call_with_metadata(
        &service,
        &workspace_id,
        &parent_request,
    )
    .graph_write()
    .unwrap()
    .into_commit();
    assert_eq!(parent_commit.patch.operations().len(), 1);
    assert_parent_operation(
        &parent_commit.patch.operations()[0],
        "draft",
        Some("checklist"),
        SET_GRAPH_NODE_PARENT_TOOL,
    );

    let soft_link_request = dynamic_tool_request(
        UPSERT_GRAPH_SOFT_LINK_TOOL,
        json!({
            "linkId": "draft_depends_on_root",
            "sourceId": "draft",
            "targetId": "root",
            "kind": "depends_on"
        }),
    );
    let soft_link_commit = dispatch_beryl_graph_dynamic_tool_call_with_metadata(
        &service,
        &workspace_id,
        &soft_link_request,
    )
    .graph_write()
    .unwrap()
    .into_commit();
    assert_eq!(soft_link_commit.patch.operations().len(), 1);
    assert_soft_link_operation(
        &soft_link_commit.patch.operations()[0],
        "draft_depends_on_root",
        "draft",
        "root",
        UPSERT_GRAPH_SOFT_LINK_TOOL,
    );

    let status_request = dynamic_tool_request(
        SET_CHECKLIST_ITEM_STATUS_TOOL,
        json!({
            "nodeId": "draft",
            "status": "done"
        }),
    );
    let status_commit = dispatch_beryl_graph_dynamic_tool_call_with_metadata(
        &service,
        &workspace_id,
        &status_request,
    )
    .graph_write()
    .unwrap()
    .into_commit();
    assert_eq!(status_commit.patch.operations().len(), 1);
    assert_status_operation(
        &status_commit.patch.operations()[0],
        "draft",
        ChecklistItemStatus::Done,
        SET_CHECKLIST_ITEM_STATUS_TOOL,
    );

    root.close().unwrap();
}

#[test]
fn repeated_dynamic_writes_publish_noop_commits_without_identity_churn() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("dynamic_commit_noop").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Dynamic Noop", 42);
    let graph = graph_with_ordered_children_and_thread_ref();
    let first_id = SemanticNodeId::new("first").unwrap();
    let second_id = SemanticNodeId::new("second").unwrap();
    let root_id = SemanticNodeId::new("root").unwrap();
    let thread_ref_id = ThreadRefId::new("first_thread").unwrap();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let upsert_existing = dynamic_tool_request(
        UPSERT_GRAPH_NODE_TOOL,
        json!({
            "nodeId": "first",
            "parentId": "root",
            "title": "First",
            "summary": "First summary",
            "topic": true,
            "checklist": false,
            "checklistItem": false
        }),
    );
    let no_op_node_commit = dispatch_beryl_graph_dynamic_tool_call_with_metadata(
        &service,
        &workspace_id,
        &upsert_existing,
    )
    .graph_write()
    .unwrap()
    .into_commit();
    let upsert_existing_root = dynamic_tool_request(
        UPSERT_GRAPH_NODE_TOOL,
        json!({
            "nodeId": "root",
            "parentId": null,
            "title": "Root",
            "summary": "Root summary",
            "topic": true,
            "checklist": false,
            "checklistItem": false
        }),
    );
    let no_op_root_upsert_commit = dispatch_beryl_graph_dynamic_tool_call_with_metadata(
        &service,
        &workspace_id,
        &upsert_existing_root,
    )
    .graph_write()
    .unwrap()
    .into_commit();
    let root_to_root = dynamic_tool_request(
        SET_GRAPH_NODE_PARENT_TOOL,
        json!({
            "childId": "root",
            "parentId": null
        }),
    );
    let no_op_root_parent_commit = dispatch_beryl_graph_dynamic_tool_call_with_metadata(
        &service,
        &workspace_id,
        &root_to_root,
    )
    .graph_write()
    .unwrap()
    .into_commit();

    let soft_link = dynamic_tool_request(
        UPSERT_GRAPH_SOFT_LINK_TOOL,
        json!({
            "linkId": "first_depends_on_second",
            "sourceId": "first",
            "targetId": "second",
            "kind": "depends_on"
        }),
    );
    let soft_link_commit =
        dispatch_beryl_graph_dynamic_tool_call_with_metadata(&service, &workspace_id, &soft_link)
            .graph_write()
            .unwrap()
            .into_commit();
    let no_op_soft_link_commit =
        dispatch_beryl_graph_dynamic_tool_call_with_metadata(&service, &workspace_id, &soft_link)
            .graph_write()
            .unwrap()
            .into_commit();
    let stored = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();
    let first = stored.node(&first_id).unwrap();

    assert!(!no_op_node_commit.changed);
    assert_eq!(no_op_node_commit.patch.operations().len(), 2);
    assert!(!no_op_root_upsert_commit.changed);
    assert_eq!(no_op_root_upsert_commit.patch.operations().len(), 2);
    assert!(!no_op_root_parent_commit.changed);
    assert_eq!(no_op_root_parent_commit.patch.operations().len(), 1);
    assert_eq!(
        no_op_node_commit.committed_revision,
        WorkspaceGraphRevision::new(1)
    );
    assert_eq!(
        no_op_root_upsert_commit.committed_revision,
        WorkspaceGraphRevision::new(2)
    );
    assert_eq!(
        no_op_root_parent_commit.committed_revision,
        WorkspaceGraphRevision::new(3)
    );
    assert!(soft_link_commit.changed);
    assert!(!no_op_soft_link_commit.changed);
    assert_eq!(
        no_op_soft_link_commit.committed_revision,
        WorkspaceGraphRevision::new(5)
    );
    assert_eq!(
        stored.child_ids_of(&root_id).unwrap(),
        &[first_id.clone(), second_id.clone()]
    );
    assert_eq!(stored.root_node_ids(), std::slice::from_ref(&root_id));
    assert_eq!(
        stored
            .root_order_provenance()
            .unwrap()
            .last_updated()
            .recorded_at_millis(),
        4
    );
    assert_eq!(
        stored
            .node(&root_id)
            .unwrap()
            .provenance()
            .last_updated()
            .recorded_at_millis(),
        1
    );
    assert_eq!(first.provenance().last_updated().recorded_at_millis(), 2);
    assert!(stored.thread_ref(&thread_ref_id).is_some());
    let link = stored
        .soft_link(&SoftLinkId::new("first_depends_on_second").unwrap())
        .unwrap();
    assert_eq!(link.source_id(), &first_id);
    assert_eq!(link.target_id(), &second_id);

    root.close().unwrap();
}

#[test]
fn repeated_dynamic_root_writes_preserve_ordered_roots_without_identity_churn() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("dynamic_commit_multi_root_noop").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Dynamic Noop", 42);
    let graph = graph_with_two_roots_and_cross_link();
    let root_a_id = SemanticNodeId::new("root_a").unwrap();
    let root_b_id = SemanticNodeId::new("root_b").unwrap();
    let link_id = SoftLinkId::new("root_a_depends_on_root_b").unwrap();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();

    let no_op_root_upsert = dynamic_tool_request(
        UPSERT_GRAPH_NODE_TOOL,
        json!({
            "nodeId": "root_b",
            "parentId": null,
            "title": "Root B",
            "summary": "Root B summary",
            "topic": true,
            "checklist": false,
            "checklistItem": false
        }),
    );
    let no_op_root_upsert_commit = dispatch_beryl_graph_dynamic_tool_call_with_metadata(
        &service,
        &workspace_id,
        &no_op_root_upsert,
    )
    .graph_write()
    .unwrap()
    .into_commit();
    let no_op_parent = dynamic_tool_request(
        SET_GRAPH_NODE_PARENT_TOOL,
        json!({
            "childId": "root_a",
            "parentId": null
        }),
    );
    let no_op_parent_commit = dispatch_beryl_graph_dynamic_tool_call_with_metadata(
        &service,
        &workspace_id,
        &no_op_parent,
    )
    .graph_write()
    .unwrap()
    .into_commit();
    let no_op_soft_link = dynamic_tool_request(
        UPSERT_GRAPH_SOFT_LINK_TOOL,
        json!({
            "linkId": "root_a_depends_on_root_b",
            "sourceId": "child_a",
            "targetId": "child_b",
            "kind": "depends_on"
        }),
    );
    let no_op_soft_link_commit = dispatch_beryl_graph_dynamic_tool_call_with_metadata(
        &service,
        &workspace_id,
        &no_op_soft_link,
    )
    .graph_write()
    .unwrap()
    .into_commit();
    let stored = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();
    let link = stored.soft_link(&link_id).unwrap();

    assert!(!no_op_root_upsert_commit.changed);
    assert!(!no_op_parent_commit.changed);
    assert!(!no_op_soft_link_commit.changed);
    assert_eq!(
        stored.root_node_ids(),
        &[root_a_id.clone(), root_b_id.clone()]
    );
    assert_eq!(
        stored
            .root_order_provenance()
            .unwrap()
            .last_updated()
            .recorded_at_millis(),
        6
    );
    assert_eq!(
        stored
            .node(&root_b_id)
            .unwrap()
            .provenance()
            .last_updated()
            .recorded_at_millis(),
        2
    );
    assert_eq!(link.source_id(), &SemanticNodeId::new("child_a").unwrap());
    assert_eq!(link.target_id(), &SemanticNodeId::new("child_b").unwrap());

    root.close().unwrap();
}

fn graph_with_ordered_children_and_thread_ref() -> SemanticGraph {
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
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(first_id.clone(), "First"),
                provenance: provenance(2),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(second_id.clone(), "Second"),
                provenance: provenance(3),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_id.clone(),
                parent_id: None,
                index: None,
                provenance: provenance(4),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: first_id.clone(),
                parent_id: Some(root_id.clone()),
                index: None,
                provenance: provenance(5),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: second_id,
                parent_id: Some(root_id),
                index: None,
                provenance: provenance(6),
            },
            SemanticGraphPatchOp::UpsertThreadRef {
                thread_ref: ThreadRefDraft::new(
                    ThreadRefId::new("first_thread").unwrap(),
                    first_id,
                    ConversationThreadId::new("thread_first"),
                    WorkspaceId::host_windows(r"C:\work\beryl"),
                    "First thread",
                ),
                provenance: provenance(7),
            },
        ]))
        .unwrap();

    graph
}

fn graph_with_two_roots_and_cross_link() -> SemanticGraph {
    let root_a_id = SemanticNodeId::new("root_a").unwrap();
    let root_b_id = SemanticNodeId::new("root_b").unwrap();
    let child_a_id = SemanticNodeId::new("child_a").unwrap();
    let child_b_id = SemanticNodeId::new("child_b").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(root_a_id.clone(), "Root A"),
                provenance: provenance(1),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(root_b_id.clone(), "Root B"),
                provenance: provenance(2),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(child_a_id.clone(), "Child A"),
                provenance: provenance(3),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(child_b_id.clone(), "Child B"),
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
                    SoftLinkId::new("root_a_depends_on_root_b").unwrap(),
                    child_a_id,
                    child_b_id,
                    SoftLinkKind::new("depends_on").unwrap(),
                ),
                provenance: provenance(9),
            },
        ]))
        .unwrap();

    graph
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

fn provenance(recorded_at_millis: u64) -> MutationProvenance {
    MutationProvenance::new(
        "operator",
        recorded_at_millis,
        MutationSource::workspace_action("dynamic_commit_test").unwrap(),
        Some(100),
    )
    .unwrap()
}

fn assert_node_upsert_operation(
    operation: &SemanticGraphPatchOp,
    node_id: &str,
    title: &str,
    tool_name: &str,
) {
    let SemanticGraphPatchOp::UpsertNode { node, provenance } = operation else {
        panic!("expected node upsert operation, got {operation:?}");
    };

    assert_eq!(
        node,
        &SemanticNodeDraft::new(
            SemanticNodeId::new(node_id).unwrap(),
            title,
            format!("{title} summary"),
            SemanticNodeFacets::topic(),
            None,
        )
    );
    assert_dynamic_provenance(provenance, tool_name);
}

fn assert_parent_operation(
    operation: &SemanticGraphPatchOp,
    child_id: &str,
    parent_id: Option<&str>,
    tool_name: &str,
) {
    let SemanticGraphPatchOp::SetHardParent {
        child_id: actual_child_id,
        parent_id: actual_parent_id,
        provenance,
        ..
    } = operation
    else {
        panic!("expected parent operation, got {operation:?}");
    };

    assert_eq!(actual_child_id.as_str(), child_id);
    assert_eq!(actual_parent_id.as_ref().map(|id| id.as_str()), parent_id);
    assert_dynamic_provenance(provenance, tool_name);
}

fn assert_soft_link_operation(
    operation: &SemanticGraphPatchOp,
    link_id: &str,
    source_id: &str,
    target_id: &str,
    tool_name: &str,
) {
    let SemanticGraphPatchOp::UpsertSoftLink { link, provenance } = operation else {
        panic!("expected soft-link operation, got {operation:?}");
    };

    assert_eq!(
        link,
        &SoftLinkDraft::new(
            SoftLinkId::new(link_id).unwrap(),
            SemanticNodeId::new(source_id).unwrap(),
            SemanticNodeId::new(target_id).unwrap(),
            SoftLinkKind::new("depends_on").unwrap(),
        )
    );
    assert_dynamic_provenance(provenance, tool_name);
}

fn assert_status_operation(
    operation: &SemanticGraphPatchOp,
    node_id: &str,
    status: ChecklistItemStatus,
    tool_name: &str,
) {
    let SemanticGraphPatchOp::SetChecklistItemStatus {
        node_id: actual_node_id,
        status: actual_status,
        provenance,
    } = operation
    else {
        panic!("expected checklist status operation, got {operation:?}");
    };

    assert_eq!(actual_node_id.as_str(), node_id);
    assert_eq!(*actual_status, status);
    assert_dynamic_provenance(provenance, tool_name);
}

fn assert_dynamic_provenance(provenance: &MutationProvenance, tool_name: &str) {
    assert_eq!(provenance.actor(), "codex");
    match provenance.source() {
        MutationSource::DynamicToolCall {
            thread_id,
            turn_id,
            tool_name: actual_tool_name,
            call_id,
        } => {
            assert_eq!(thread_id.as_str(), "thread_1");
            assert_eq!(turn_id.as_str(), "turn_1");
            assert_eq!(actual_tool_name, tool_name);
            assert_eq!(call_id, "call_1");
        }
        other => panic!("expected dynamic tool provenance, got {other:?}"),
    }
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
    tempdir_support::temp_dir("beryl-workspace-graph-dynamic-commits-test-")
}
