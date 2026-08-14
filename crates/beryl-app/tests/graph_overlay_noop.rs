use beryl_app::{WorkspaceGraphMutationCommit, WorkspaceGraphRevision};
use beryl_model::{
    provenance::{MutationProvenance, MutationSource},
    semantic_graph::{
        SemanticGraph, SemanticGraphPatch, SemanticGraphPatchOp, SemanticNodeDraft,
        SemanticNodeFacets, SemanticNodeId, SoftLinkDraft, SoftLinkId, SoftLinkKind,
    },
    workspace::{BerylWorkspaceId, BerylWorkspaceManifest},
};

#[allow(dead_code)]
#[path = "../src/shell/column_selector.rs"]
mod column_selector;
#[allow(dead_code)]
#[path = "../src/shell/graph.rs"]
mod graph;

#[test]
fn quiet_noop_commit_preserves_visible_graph_without_error_or_body_replacement() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let root_id = SemanticNodeId::new("root").unwrap();
    let child_id = SemanticNodeId::new("child").unwrap();

    assert!(overlay.toggle_visibility());
    assert!(overlay.select_node(0, &child_id));
    assert!(overlay.toggle_node_expansion(0, &root_id, 0));

    let applied = overlay
        .finish_mutation_commit_update(graph::GraphMutationCommitUpdate::new(
            WorkspaceGraphMutationCommit::new(
                BerylWorkspaceId::new("graph_overlay_noop").unwrap(),
                WorkspaceGraphRevision::default(),
                WorkspaceGraphRevision::new(1),
                false,
                SemanticGraphPatch::new(Vec::new()),
                BerylWorkspaceManifest::named(
                    BerylWorkspaceId::new("graph_overlay_noop").unwrap(),
                    "Graph Overlay",
                    42,
                ),
            ),
            "",
        ))
        .unwrap();

    assert!(matches!(
        applied,
        graph::GraphCommitApplication::Applied {
            graph_changed: false,
            warning: None,
            ..
        }
    ));
    assert!(overlay.visible());
    assert!(overlay.graph_columns_available());
    assert!(!overlay.mutation_pending());
    assert_eq!(overlay.last_error(), None);
    assert_eq!(overlay.status_message(), None);
    assert_eq!(overlay.selected_node_id(), Some(&child_id));
    assert_eq!(overlay.columns().len(), 2);
}

#[test]
fn quiet_noop_commit_preserves_cross_root_soft_link_selection() {
    let mut overlay = graph::GraphOverlayState::new(
        multi_root_cross_link_graph(),
        WorkspaceGraphRevision::default(),
        None,
    );
    let root_a_id = SemanticNodeId::new("root_a").unwrap();
    let root_b_id = SemanticNodeId::new("root_b").unwrap();
    let child_a_id = SemanticNodeId::new("child_a").unwrap();
    let child_b_id = SemanticNodeId::new("child_b").unwrap();
    let link_id = SoftLinkId::new("child_a_depends_on_child_b").unwrap();

    assert!(overlay.toggle_visibility());
    assert!(overlay.select_node(0, &root_a_id));
    assert!(overlay.select_node(1, &child_a_id));
    assert!(overlay.select_soft_link(2, &link_id, &child_b_id));
    assert_eq!(overlay.selected_node_id(), Some(&child_b_id));
    assert_eq!(overlay.columns().len(), 4);

    let applied = overlay
        .finish_mutation_commit_update(graph::GraphMutationCommitUpdate::new(
            WorkspaceGraphMutationCommit::new(
                BerylWorkspaceId::new("graph_overlay_noop").unwrap(),
                WorkspaceGraphRevision::default(),
                WorkspaceGraphRevision::new(1),
                false,
                SemanticGraphPatch::new(Vec::new()),
                BerylWorkspaceManifest::named(
                    BerylWorkspaceId::new("graph_overlay_noop").unwrap(),
                    "Graph Overlay",
                    42,
                ),
            ),
            "",
        ))
        .unwrap();

    assert!(matches!(
        applied,
        graph::GraphCommitApplication::Applied {
            graph_changed: false,
            warning: None,
            ..
        }
    ));
    assert_eq!(
        overlay.graph().root_node_ids(),
        &[root_a_id.clone(), root_b_id]
    );
    assert!(overlay.visible());
    assert!(overlay.graph_columns_available());
    assert_eq!(overlay.last_error(), None);
    assert_eq!(overlay.status_message(), None);
    assert_eq!(overlay.selected_node_id(), Some(&child_b_id));
    assert_eq!(overlay.columns().len(), 4);
}

fn explorer_graph() -> SemanticGraph {
    let root_id = SemanticNodeId::new("root").unwrap();
    let child_id = SemanticNodeId::new("child").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(root_id.clone(), "Root"),
                provenance: provenance(1),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(child_id.clone(), "Child"),
                provenance: provenance(2),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_id.clone(),
                parent_id: None,
                index: Some(0),
                provenance: provenance(3),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id,
                parent_id: Some(root_id),
                index: None,
                provenance: provenance(4),
            },
        ]))
        .unwrap();

    graph
}

fn multi_root_cross_link_graph() -> SemanticGraph {
    let root_a_id = SemanticNodeId::new("root_a").unwrap();
    let root_b_id = SemanticNodeId::new("root_b").unwrap();
    let child_a_id = SemanticNodeId::new("child_a").unwrap();
    let child_b_id = SemanticNodeId::new("child_b").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(root_a_id.clone(), "Root A"),
                provenance: provenance(10),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(root_b_id.clone(), "Root B"),
                provenance: provenance(11),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(child_a_id.clone(), "Child A"),
                provenance: provenance(12),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(child_b_id.clone(), "Child B"),
                provenance: provenance(13),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_a_id.clone(),
                parent_id: None,
                index: None,
                provenance: provenance(14),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_b_id.clone(),
                parent_id: None,
                index: None,
                provenance: provenance(15),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: child_a_id.clone(),
                parent_id: Some(root_a_id),
                index: None,
                provenance: provenance(16),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: child_b_id.clone(),
                parent_id: Some(root_b_id),
                index: None,
                provenance: provenance(17),
            },
            SemanticGraphPatchOp::UpsertSoftLink {
                link: SoftLinkDraft::new(
                    SoftLinkId::new("child_a_depends_on_child_b").unwrap(),
                    child_a_id,
                    child_b_id,
                    SoftLinkKind::new("depends_on").unwrap(),
                ),
                provenance: provenance(18),
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
        MutationSource::workspace_action("graph_overlay_noop").unwrap(),
        Some(100),
    )
    .unwrap()
}
