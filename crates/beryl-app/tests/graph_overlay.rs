#[path = "support/tempdir.rs"]
mod tempdir_support;

use beryl_app::{BerylWorkspacePersistence, GraphPatchWriteRequest, WorkspaceGraphToolService};
pub use beryl_app::{WorkspaceGraphMutationCommit, WorkspaceGraphRevision};
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

#[allow(dead_code)]
#[path = "../src/shell/column_selector.rs"]
mod column_selector;
#[allow(dead_code)]
#[path = "../src/shell/graph.rs"]
mod graph;

#[test]
fn graph_overlay_state_follows_node_and_soft_link_column_selection() {
    let graph = explorer_graph();
    let mut overlay = graph::GraphOverlayState::new(graph, WorkspaceGraphRevision::default(), None);
    let child_id = SemanticNodeId::new("child").unwrap();
    let peer_id = SemanticNodeId::new("peer").unwrap();
    let soft_link_id = SoftLinkId::new("child_to_peer").unwrap();

    assert_eq!(overlay.columns().len(), 1);
    assert_root_level_column(&overlay, 0);
    assert!(overlay.selected_node_id().is_none());

    assert!(overlay.select_node(0, &child_id));
    assert_eq!(overlay.columns().len(), 2);
    assert_node_column(&overlay, 1, &child_id);
    assert_eq!(overlay.selected_node_id(), Some(&child_id));

    assert!(overlay.select_soft_link(1, &soft_link_id, &peer_id));
    assert_eq!(overlay.columns().len(), 3);
    assert_node_column(&overlay, 2, &peer_id);
    assert_eq!(overlay.selected_node_id(), Some(&peer_id));
}

#[test]
fn graph_overlay_retained_counts_include_visible_and_committed_graphs() {
    let overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);

    let counts = overlay.retained_counts();
    assert_eq!(counts.visible_nodes, 4);
    assert_eq!(counts.visible_soft_links, 1);
    assert_eq!(counts.visible_thread_refs, 0);
    assert_eq!(counts.committed_nodes, 4);
    assert_eq!(counts.committed_soft_links, 1);
    assert_eq!(counts.committed_thread_refs, 0);
    assert_eq!(counts.columns, 1);
    assert_eq!(counts.pending_optimistic_mutations, 0);
    assert_eq!(counts.queued_commits, 0);
    assert!(counts.payload_bytes > 0);
}

#[test]
fn graph_thread_ref_rows_render_invalid_indicator_and_explicit_rebind_action() {
    let rows_source = include_str!("../src/shell/render/graph_overlay/rows.rs");
    let shell_source = include_str!("../src/shell.rs");
    let row_body = rust_function_body(rows_source, "fn render_graph_thread_ref_row");
    let invalid_actions_body =
        rust_function_body(rows_source, "fn render_invalid_thread_ref_actions");
    let select_body = rust_function_body(shell_source, "fn select_graph_thread_ref");

    assert!(row_body.contains("graph_thread_ref_availability"));
    assert!(row_body.contains("render_invalid_thread_ref_actions"));
    assert!(invalid_actions_body.contains(".child(\"!\")"));
    assert!(invalid_actions_body.contains("\"Rebind\""));
    assert!(invalid_actions_body.contains("open_graph_thread_ref_rebind_menu"));
    assert!(select_body.contains("graph_thread_ref_availability"));
    assert!(select_body.contains("availability.notice_title()"));
}

#[test]
fn graph_overlay_state_uses_root_level_first_column_for_ordered_roots() {
    let graph = multi_root_graph();
    let overlay =
        graph::GraphOverlayState::new(graph.clone(), WorkspaceGraphRevision::default(), None);
    let alpha_id = SemanticNodeId::new("alpha_root").unwrap();
    let beta_id = SemanticNodeId::new("beta_root").unwrap();

    assert!(overlay.graph_columns_available());
    assert_eq!(overlay.columns().len(), 1);
    assert_root_level_column(&overlay, 0);
    assert_eq!(graph.root_node_ids(), &[alpha_id, beta_id]);
}

#[test]
fn graph_overlay_column_keys_keep_fixed_header_chrome_for_root_level_and_node_columns() {
    let graph = multi_root_graph();
    let mut overlay = graph::GraphOverlayState::new(graph, WorkspaceGraphRevision::default(), None);
    let alpha_id = SemanticNodeId::new("alpha_root").unwrap();

    assert!(overlay.columns()[0].root_key().renders_fixed_header());
    assert!(overlay.select_node(0, &alpha_id));
    assert!(overlay.columns()[1].root_key().renders_fixed_header());
}

#[test]
fn graph_overlay_state_opens_selected_root_from_root_level_column() {
    let graph = multi_root_graph();
    let mut overlay = graph::GraphOverlayState::new(graph, WorkspaceGraphRevision::default(), None);
    let alpha_id = SemanticNodeId::new("alpha_root").unwrap();
    let beta_id = SemanticNodeId::new("beta_root").unwrap();

    assert!(overlay.select_node(0, &alpha_id));

    assert_eq!(overlay.columns().len(), 2);
    assert_root_level_column(&overlay, 0);
    assert_node_column(&overlay, 1, &alpha_id);
    assert_eq!(overlay.selected_node_id(), Some(&alpha_id));
    assert_eq!(overlay.graph().root_node_ids(), &[alpha_id, beta_id]);
}

#[test]
fn graph_overlay_state_opens_cross_root_soft_link_target_column() {
    let graph = multi_root_graph();
    let mut overlay = graph::GraphOverlayState::new(graph, WorkspaceGraphRevision::default(), None);
    let alpha_id = SemanticNodeId::new("alpha_root").unwrap();
    let beta_id = SemanticNodeId::new("beta_root").unwrap();
    let soft_link_id = SoftLinkId::new("alpha_child_to_beta").unwrap();

    assert!(overlay.select_node(0, &alpha_id));
    assert!(overlay.select_soft_link(1, &soft_link_id, &beta_id));

    assert_eq!(overlay.columns().len(), 3);
    assert_root_level_column(&overlay, 0);
    assert_node_column(&overlay, 1, &alpha_id);
    assert_node_column(&overlay, 2, &beta_id);
    assert_eq!(overlay.selected_node_id(), Some(&beta_id));
}

#[test]
fn graph_overlay_state_deleting_one_root_preserves_root_level_column_for_other_roots() {
    let graph = multi_root_graph();
    let mut overlay =
        graph::GraphOverlayState::new(graph.clone(), WorkspaceGraphRevision::default(), None);
    let alpha_id = SemanticNodeId::new("alpha_root").unwrap();
    let beta_id = SemanticNodeId::new("beta_root").unwrap();

    assert!(overlay.select_node(0, &alpha_id));

    overlay.finish_mutation(
        delete_subtree_graph(graph, &alpha_id),
        WorkspaceGraphRevision::new(1),
        None,
    );

    assert_eq!(overlay.columns().len(), 1);
    assert_root_level_column(&overlay, 0);
    assert_eq!(overlay.selected_node_id(), None);
    assert_eq!(overlay.graph().root_node_ids(), &[beta_id]);
}

#[test]
fn graph_overlay_state_preserves_root_level_selection_and_expansion_across_root_reorder() {
    let graph = multi_root_graph();
    let mut overlay =
        graph::GraphOverlayState::new(graph.clone(), WorkspaceGraphRevision::default(), None);
    let alpha_id = SemanticNodeId::new("alpha_root").unwrap();
    let beta_id = SemanticNodeId::new("beta_root").unwrap();

    assert!(overlay.toggle_node_expansion(0, &alpha_id, 0));
    assert!(overlay.select_node(0, &alpha_id));

    overlay.finish_mutation(
        root_reordered_graph(graph, &beta_id),
        WorkspaceGraphRevision::new(1),
        None,
    );

    assert_eq!(overlay.columns().len(), 2);
    assert_root_level_column(&overlay, 0);
    assert_node_column(&overlay, 1, &alpha_id);
    assert_eq!(overlay.selected_node_id(), Some(&alpha_id));
    assert_eq!(
        overlay.graph().root_node_ids(),
        &[beta_id, alpha_id.clone()]
    );
    assert!(!graph_column_expanded(&overlay.columns()[0], &alpha_id, 0));
}

#[test]
fn graph_overlay_state_rebuilds_column_trail_when_selected_child_moves_to_root() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let root_id = SemanticNodeId::new("root").unwrap();
    let child_id = SemanticNodeId::new("child").unwrap();
    let grandchild_id = SemanticNodeId::new("grandchild").unwrap();

    assert!(overlay.select_node(0, &child_id));
    assert!(overlay.select_node(1, &grandchild_id));
    assert_eq!(overlay.columns().len(), 3);

    let move_to_root = SemanticGraphPatch::from_operation(SemanticGraphPatchOp::SetHardParent {
        child_id: grandchild_id.clone(),
        parent_id: None,
        index: Some(1),
        provenance: provenance(48),
    });
    let applied = overlay
        .finish_mutation_commit_update(graph_commit_update(0, 1, true, move_to_root, ""))
        .unwrap();

    assert!(matches!(
        applied,
        graph::GraphCommitApplication::Applied {
            graph_changed: true,
            ..
        }
    ));
    assert_eq!(
        overlay.graph().root_node_ids(),
        &[root_id, grandchild_id.clone()]
    );
    assert_eq!(overlay.columns().len(), 2);
    assert_root_level_column(&overlay, 0);
    assert_node_column(&overlay, 1, &grandchild_id);
    assert_eq!(overlay.selected_node_id(), Some(&grandchild_id));
    assert!(overlay.graph().node(&child_id).is_some());
}

#[test]
fn graph_overlay_state_preserves_selected_root_when_it_moves_under_another_root() {
    let mut overlay =
        graph::GraphOverlayState::new(multi_root_graph(), WorkspaceGraphRevision::default(), None);
    let alpha_id = SemanticNodeId::new("alpha_root").unwrap();
    let beta_id = SemanticNodeId::new("beta_root").unwrap();

    assert!(overlay.select_node(0, &beta_id));

    let move_under_alpha =
        SemanticGraphPatch::from_operation(SemanticGraphPatchOp::SetHardParent {
            child_id: beta_id.clone(),
            parent_id: Some(alpha_id.clone()),
            index: Some(1),
            provenance: provenance(49),
        });
    let applied = overlay
        .finish_mutation_commit_update(graph_commit_update(0, 1, true, move_under_alpha, ""))
        .unwrap();

    assert!(matches!(
        applied,
        graph::GraphCommitApplication::Applied {
            graph_changed: true,
            ..
        }
    ));
    assert_eq!(
        overlay.graph().root_node_ids(),
        std::slice::from_ref(&alpha_id)
    );
    assert_eq!(overlay.graph().parent_id_of(&beta_id), Some(&alpha_id));
    assert_eq!(overlay.columns().len(), 2);
    assert_root_level_column(&overlay, 0);
    assert_node_column(&overlay, 1, &beta_id);
    assert_eq!(overlay.selected_node_id(), Some(&beta_id));
}

#[test]
fn graph_overlay_state_uses_shallow_default_expansion_and_allows_toggling() {
    let graph = explorer_graph();
    let mut overlay = graph::GraphOverlayState::new(graph, WorkspaceGraphRevision::default(), None);
    let root_id = SemanticNodeId::new("root").unwrap();
    let child_id = SemanticNodeId::new("child").unwrap();
    let grandchild_id = SemanticNodeId::new("grandchild").unwrap();

    assert!(graph_column_expanded(&overlay.columns()[0], &root_id, 0));
    assert!(graph_column_expanded(&overlay.columns()[0], &child_id, 1));
    assert!(!graph_column_expanded(
        &overlay.columns()[0],
        &grandchild_id,
        2
    ));

    assert!(overlay.toggle_node_expansion(0, &child_id, 1));
    assert!(!graph_column_expanded(&overlay.columns()[0], &child_id, 1));
}

#[test]
fn graph_overlay_state_allows_toggling_leaf_nodes_with_attached_rows() {
    let graph = leaf_attachment_graph();
    let mut overlay = graph::GraphOverlayState::new(graph, WorkspaceGraphRevision::default(), None);
    let leaf_id = SemanticNodeId::new("leaf").unwrap();

    assert!(graph_column_expanded(&overlay.columns()[0], &leaf_id, 1));
    assert!(overlay.toggle_node_expansion(0, &leaf_id, 1));
    assert!(!graph_column_expanded(&overlay.columns()[0], &leaf_id, 1));
}

#[test]
fn graph_overlay_state_preserves_selection_when_graph_is_reloaded() {
    let graph = explorer_graph();
    let mut overlay = graph::GraphOverlayState::new(graph, WorkspaceGraphRevision::default(), None);
    let child_id = SemanticNodeId::new("child").unwrap();

    assert!(overlay.toggle_visibility());
    assert!(overlay.select_node(0, &child_id));

    overlay.finish_mutation(explorer_graph(), WorkspaceGraphRevision::new(1), None);

    assert!(overlay.visible());
    assert_eq!(overlay.selected_node_id(), Some(&child_id));
    assert_eq!(overlay.columns().len(), 2);
}

#[test]
fn graph_overlay_state_preserves_expansion_overrides_when_graph_is_reloaded() {
    let graph = explorer_graph();
    let mut overlay = graph::GraphOverlayState::new(graph, WorkspaceGraphRevision::default(), None);
    let child_id = SemanticNodeId::new("child").unwrap();

    assert!(overlay.toggle_node_expansion(0, &child_id, 1));

    overlay.finish_mutation(explorer_graph(), WorkspaceGraphRevision::new(1), None);

    assert!(!graph_column_expanded(&overlay.columns()[0], &child_id, 1));
}

#[test]
fn graph_overlay_state_clears_selection_when_selected_node_is_deleted() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let child_id = SemanticNodeId::new("child").unwrap();

    assert!(overlay.select_node(0, &child_id));

    overlay.finish_mutation(
        delete_subtree_graph(explorer_graph(), &child_id),
        WorkspaceGraphRevision::new(1),
        None,
    );

    assert_eq!(overlay.columns().len(), 1);
    assert_root_level_column(&overlay, 0);
    assert!(overlay.columns()[0].selection().is_none());
    assert!(overlay.selected_node_id().is_none());
    assert!(overlay.graph().node(&child_id).is_none());
}

#[test]
fn graph_overlay_state_recovers_after_selected_leaf_delete_with_attached_rows() {
    let mut overlay = graph::GraphOverlayState::new(
        leaf_attachment_graph(),
        WorkspaceGraphRevision::default(),
        None,
    );
    let leaf_id = SemanticNodeId::new("leaf").unwrap();
    let target_id = SemanticNodeId::new("target").unwrap();
    let soft_link_id = SoftLinkId::new("leaf_to_target").unwrap();

    assert!(overlay.select_node(0, &leaf_id));

    overlay.finish_mutation(
        delete_leaf_graph(leaf_attachment_graph(), &leaf_id),
        WorkspaceGraphRevision::new(1),
        None,
    );

    assert_eq!(overlay.columns().len(), 1);
    assert_root_level_column(&overlay, 0);
    assert!(overlay.columns()[0].selection().is_none());
    assert!(overlay.selected_node_id().is_none());
    assert!(overlay.graph().node(&leaf_id).is_none());
    assert!(overlay.graph().node(&target_id).is_some());
    assert!(overlay.graph().soft_link(&soft_link_id).is_none());
}

#[test]
fn graph_overlay_state_truncates_open_column_trail_when_deleted_node_was_in_trail() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let child_id = SemanticNodeId::new("child").unwrap();
    let grandchild_id = SemanticNodeId::new("grandchild").unwrap();

    assert!(overlay.select_node(0, &child_id));
    assert!(overlay.select_node(1, &grandchild_id));
    assert_eq!(overlay.columns().len(), 3);

    overlay.finish_mutation(
        delete_subtree_graph(explorer_graph(), &child_id),
        WorkspaceGraphRevision::new(1),
        None,
    );

    assert_eq!(overlay.columns().len(), 1);
    assert_root_level_column(&overlay, 0);
    assert!(overlay.columns()[0].selection().is_none());
    assert!(overlay.selected_node_id().is_none());
}

#[test]
fn graph_overlay_state_clears_soft_link_selection_when_deleted_source_removes_link() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let child_id = SemanticNodeId::new("child").unwrap();
    let peer_id = SemanticNodeId::new("peer").unwrap();
    let soft_link_id = SoftLinkId::new("child_to_peer").unwrap();

    assert!(overlay.select_soft_link(0, &soft_link_id, &peer_id));
    assert_eq!(overlay.columns().len(), 2);
    assert_eq!(overlay.selected_node_id(), Some(&peer_id));

    overlay.finish_mutation(
        delete_subtree_graph(explorer_graph(), &child_id),
        WorkspaceGraphRevision::new(1),
        None,
    );

    assert_eq!(overlay.columns().len(), 1);
    assert_root_level_column(&overlay, 0);
    assert!(overlay.columns()[0].selection().is_none());
    assert!(overlay.selected_node_id().is_none());
    assert!(overlay.graph().node(&peer_id).is_some());
    assert!(overlay.graph().soft_link(&soft_link_id).is_none());
}

#[test]
fn graph_overlay_state_recovers_to_empty_columns_when_root_is_deleted() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let root_id = SemanticNodeId::new("root").unwrap();
    let child_id = SemanticNodeId::new("child").unwrap();

    assert!(overlay.toggle_visibility());
    assert!(overlay.select_node(0, &child_id));

    overlay.finish_mutation(
        delete_subtree_graph(explorer_graph(), &root_id),
        WorkspaceGraphRevision::new(1),
        None,
    );

    assert!(overlay.visible());
    assert!(overlay.columns().is_empty());
    assert!(overlay.selected_node_id().is_none());
    assert!(overlay.graph().root_node_ids().is_empty());
}

#[test]
fn graph_overlay_state_applies_gapped_commits_after_missing_revision_arrives() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let first_id = SemanticNodeId::new("first_commit").unwrap();
    let second_id = SemanticNodeId::new("second_commit").unwrap();

    let second = graph_commit_update(
        1,
        2,
        true,
        upsert_root_child_patch(second_id.clone(), "Second Commit", 102),
        "second no-op",
    );
    let queued = overlay.finish_mutation_commit_update(second).unwrap();
    assert!(matches!(
        queued,
        graph::GraphCommitApplication::QueuedGap {
            queued_revision,
            waiting_for_revision
        } if queued_revision == WorkspaceGraphRevision::new(2)
            && waiting_for_revision == WorkspaceGraphRevision::new(1)
    ));
    assert_eq!(overlay.revision(), WorkspaceGraphRevision::default());
    assert_eq!(overlay.queued_commit_count(), 1);
    assert!(overlay.graph_columns_available());
    assert_eq!(
        overlay.status_message(),
        Some("Waiting for semantic graph revision 1 before applying revision 2.")
    );
    assert!(overlay.graph().node(&second_id).is_none());

    let first = graph_commit_update(
        0,
        1,
        true,
        upsert_root_child_patch(first_id.clone(), "First Commit", 101),
        "first no-op",
    );
    let applied = overlay.finish_mutation_commit_update(first).unwrap();
    assert!(matches!(
        applied,
        graph::GraphCommitApplication::Applied {
            graph_changed: true,
            ref applied_revisions,
            ..
        } if applied_revisions == &vec![
            WorkspaceGraphRevision::new(1),
            WorkspaceGraphRevision::new(2)
        ]
    ));
    assert_eq!(overlay.revision(), WorkspaceGraphRevision::new(2));
    assert_eq!(overlay.queued_commit_count(), 0);
    assert_eq!(overlay.status_message(), None);
    assert!(overlay.graph().node(&first_id).is_some());
    assert!(overlay.graph().node(&second_id).is_some());
}

#[test]
fn graph_overlay_state_recovers_when_gapped_commit_count_exceeds_budget() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);

    let mut application = None;
    for index in 0..257u64 {
        let node_id = SemanticNodeId::new(format!("queued_gap_{index}")).unwrap();
        application = Some(
            overlay
                .finish_mutation_commit_update(graph_commit_update(
                    index + 1,
                    index + 2,
                    true,
                    upsert_root_child_patch(node_id, "Queued Gap", 500 + index),
                    "",
                ))
                .unwrap(),
        );
    }

    assert!(matches!(
        application,
        Some(graph::GraphCommitApplication::RecoveryRequired { .. })
    ));
    assert_eq!(overlay.queued_commit_count(), 0);
    assert_eq!(overlay.pending_optimistic_mutation_count(), 0);
    assert_eq!(overlay.revision(), WorkspaceGraphRevision::default());
    assert_eq!(
        overlay.status_message(),
        Some("Recovering semantic graph projection from persisted state.")
    );
}

#[test]
fn graph_overlay_state_recovers_when_gapped_commit_bytes_exceed_budget() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let node_id = SemanticNodeId::new("huge_gap").unwrap();
    let huge_title = "x".repeat(4 * 1024 * 1024 + 1);

    let application = overlay
        .finish_mutation_commit_update(graph_commit_update(
            1,
            2,
            true,
            upsert_root_child_patch(node_id, &huge_title, 900),
            "",
        ))
        .unwrap();

    assert!(matches!(
        application,
        graph::GraphCommitApplication::RecoveryRequired { .. }
    ));
    assert_eq!(overlay.queued_commit_count(), 0);
}

#[test]
fn graph_overlay_state_prunes_pending_optimistic_mutations_to_budget() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);

    for index in 0..260u64 {
        let node_id = SemanticNodeId::new(format!("optimistic_{index}")).unwrap();
        begin_optimistic_patch(
            &mut overlay,
            upsert_root_child_patch(node_id.clone(), "Optimistic", 1000 + index),
            [node_id],
        );
    }

    assert!(overlay.pending_optimistic_mutation_count() <= 256);
    assert!(
        overlay
            .graph()
            .node(&SemanticNodeId::new("optimistic_0").unwrap())
            .is_none()
    );
    assert!(
        overlay
            .graph()
            .node(&SemanticNodeId::new("optimistic_259").unwrap())
            .is_some()
    );
}

#[test]
fn graph_overlay_state_ignores_duplicate_commit_without_rewinding_projection() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let node_id = SemanticNodeId::new("duplicate_commit").unwrap();
    let update = graph_commit_update(
        0,
        1,
        true,
        upsert_root_child_patch(node_id.clone(), "Duplicate Commit", 110),
        "duplicate no-op",
    );

    let applied = overlay
        .finish_mutation_commit_update(update.clone())
        .unwrap();
    assert!(matches!(
        applied,
        graph::GraphCommitApplication::Applied {
            graph_changed: true,
            ..
        }
    ));

    let duplicate = overlay.finish_mutation_commit_update(update).unwrap();
    assert!(matches!(
        duplicate,
        graph::GraphCommitApplication::IgnoredStale {
            committed_revision,
            visible_revision
        } if committed_revision == WorkspaceGraphRevision::new(1)
            && visible_revision == WorkspaceGraphRevision::new(1)
    ));
    assert_eq!(overlay.revision(), WorkspaceGraphRevision::new(1));
    assert!(overlay.graph().node(&node_id).is_some());
}

#[test]
fn graph_overlay_state_rejects_conflicting_stale_base_commit_without_rewinding_projection() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let first_id = SemanticNodeId::new("first_commit").unwrap();
    let conflicting_id = SemanticNodeId::new("conflicting_commit").unwrap();

    overlay
        .finish_mutation_commit_update(graph_commit_update(
            0,
            1,
            true,
            upsert_root_child_patch(first_id.clone(), "First Commit", 110),
            "",
        ))
        .unwrap();

    let error = overlay
        .finish_mutation_commit_update(graph_commit_update(
            0,
            2,
            true,
            upsert_root_child_patch(conflicting_id.clone(), "Conflicting Commit", 120),
            "",
        ))
        .unwrap_err();

    assert!(matches!(
        error,
        graph::GraphCommitProjectionError::ConflictingRevision {
            visible,
            base,
            committed
        } if visible == WorkspaceGraphRevision::new(1)
            && base == WorkspaceGraphRevision::default()
            && committed == WorkspaceGraphRevision::new(2)
    ));
    assert_eq!(overlay.revision(), WorkspaceGraphRevision::new(1));
    assert!(overlay.graph().node(&first_id).is_some());
    assert!(overlay.graph().node(&conflicting_id).is_none());
}

#[test]
fn graph_overlay_state_advances_no_op_commit_without_disturbing_columns() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let child_id = SemanticNodeId::new("child").unwrap();

    assert!(overlay.toggle_visibility());
    assert!(overlay.select_node(0, &child_id));
    assert!(overlay.toggle_node_expansion(0, &child_id, 1));

    let no_op = graph_commit_update(
        0,
        1,
        false,
        SemanticGraphPatch::new(Vec::new()),
        "The graph was already current.",
    );
    let applied = overlay.finish_mutation_commit_update(no_op).unwrap();
    assert!(matches!(
        applied,
        graph::GraphCommitApplication::Applied {
            graph_changed: false,
            warning: Some(ref warning),
            ..
        } if warning == "The graph was already current."
    ));

    assert_eq!(overlay.revision(), WorkspaceGraphRevision::new(1));
    assert_eq!(overlay.selected_node_id(), Some(&child_id));
    assert_eq!(overlay.columns().len(), 2);
    assert!(!graph_column_expanded(&overlay.columns()[0], &child_id, 1));
    assert_eq!(overlay.last_error(), Some("The graph was already current."));
}

#[test]
fn graph_overlay_state_applies_dynamic_style_changed_commit_without_replacing_columns() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let child_id = SemanticNodeId::new("child").unwrap();
    let dynamic_id = SemanticNodeId::new("dynamic_commit").unwrap();

    assert!(overlay.toggle_visibility());
    assert!(overlay.select_node(0, &child_id));

    let commit = graph_commit_update(
        0,
        1,
        true,
        upsert_root_child_patch(dynamic_id.clone(), "Dynamic Commit", 115),
        "",
    );
    let applied = overlay.finish_mutation_commit_update(commit).unwrap();

    assert!(matches!(
        applied,
        graph::GraphCommitApplication::Applied {
            graph_changed: true,
            ..
        }
    ));
    assert!(overlay.visible());
    assert!(overlay.graph_columns_available());
    assert_eq!(overlay.status_message(), None);
    assert_eq!(overlay.last_error(), None);
    assert_eq!(overlay.selected_node_id(), Some(&child_id));
    assert_eq!(overlay.columns().len(), 2);
    assert!(overlay.graph().node(&dynamic_id).is_some());
}

#[test]
fn graph_overlay_state_records_failure_without_replacing_visible_graph() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let child_id = SemanticNodeId::new("child").unwrap();

    assert!(overlay.toggle_visibility());
    assert!(overlay.select_node(0, &child_id));
    overlay.begin_mutation("Deleting semantic node");

    assert!(overlay.graph_columns_available());
    assert_eq!(overlay.status_message(), Some("Deleting semantic node"));
    assert_eq!(overlay.columns().len(), 2);
    assert_eq!(overlay.selected_node_id(), Some(&child_id));

    overlay.fail_mutation("graph worker failed");

    assert!(overlay.visible());
    assert!(overlay.graph_columns_available());
    assert_eq!(overlay.status_message(), None);
    assert_eq!(overlay.last_error(), Some("graph worker failed"));
    assert_eq!(overlay.columns().len(), 2);
    assert_eq!(overlay.selected_node_id(), Some(&child_id));
    assert!(overlay.graph().node(&child_id).is_some());
}

#[test]
fn graph_overlay_state_keeps_columns_available_during_plain_mutation() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let child_id = SemanticNodeId::new("child").unwrap();

    assert!(overlay.toggle_visibility());
    assert!(overlay.select_node(0, &child_id));

    overlay.begin_mutation("Starting semantic-node thread");

    assert!(overlay.visible());
    assert!(overlay.graph_columns_available());
    assert_eq!(
        overlay.status_message(),
        Some("Starting semantic-node thread")
    );
    assert_eq!(overlay.last_error(), None);
    assert_eq!(overlay.columns().len(), 2);
    assert_eq!(overlay.selected_node_id(), Some(&child_id));
    assert!(overlay.graph().node(&child_id).is_some());
}

#[test]
fn graph_overlay_state_projects_optimistic_leaf_delete_and_rolls_back_on_failure() {
    let mut overlay = graph::GraphOverlayState::new(
        leaf_attachment_graph(),
        WorkspaceGraphRevision::default(),
        None,
    );
    let leaf_id = SemanticNodeId::new("leaf").unwrap();

    let mutation_id = begin_optimistic_patch(
        &mut overlay,
        delete_leaf_patch(&leaf_id, 120),
        [leaf_id.clone()],
    );

    assert_eq!(overlay.pending_optimistic_mutation_count(), 1);
    assert!(overlay.graph_columns_available());
    assert!(overlay.mutation_pending());
    assert_eq!(
        overlay.status_message(),
        Some("Applying optimistic graph mutation")
    );
    assert!(overlay.graph().node(&leaf_id).is_none());

    overlay
        .fail_optimistic_mutation(Some(mutation_id), "persistence failed")
        .unwrap();

    assert_eq!(overlay.pending_optimistic_mutation_count(), 0);
    assert_eq!(overlay.last_error(), Some("persistence failed"));
    assert!(overlay.graph().node(&leaf_id).is_some());
}

#[test]
fn graph_overlay_state_replays_and_rolls_back_pending_root_placement_after_commit() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let root_id = SemanticNodeId::new("root").unwrap();
    let pending_root_id = SemanticNodeId::new("pending_root").unwrap();
    let committed_child_id = SemanticNodeId::new("committed_child").unwrap();

    let mutation_id = begin_optimistic_patch(
        &mut overlay,
        upsert_root_node_patch(pending_root_id.clone(), "Pending Root", Some(0), 160),
        [pending_root_id.clone()],
    );

    assert_eq!(overlay.pending_optimistic_mutation_count(), 1);
    assert!(overlay.node_has_pending_optimistic_mutation(&pending_root_id));
    assert_eq!(
        overlay.graph().root_node_ids(),
        &[pending_root_id.clone(), root_id.clone()]
    );

    let commit = graph_commit_update(
        0,
        1,
        true,
        upsert_root_child_patch(committed_child_id.clone(), "Committed Child", 170),
        "",
    );
    let applied = overlay.finish_mutation_commit_update(commit).unwrap();

    assert!(matches!(
        applied,
        graph::GraphCommitApplication::Applied {
            graph_changed: true,
            ..
        }
    ));
    assert_eq!(overlay.revision(), WorkspaceGraphRevision::new(1));
    assert_eq!(overlay.pending_optimistic_mutation_count(), 1);
    assert!(overlay.graph().node(&committed_child_id).is_some());
    assert!(overlay.graph().node(&pending_root_id).is_some());
    assert_eq!(
        overlay.graph().root_node_ids(),
        &[pending_root_id.clone(), root_id.clone()]
    );

    overlay
        .fail_optimistic_mutation(Some(mutation_id), "root placement failed")
        .unwrap();

    assert_eq!(overlay.revision(), WorkspaceGraphRevision::new(1));
    assert_eq!(overlay.pending_optimistic_mutation_count(), 0);
    assert!(overlay.graph().node(&committed_child_id).is_some());
    assert!(overlay.graph().node(&pending_root_id).is_none());
    assert_eq!(
        overlay.graph().root_node_ids(),
        std::slice::from_ref(&root_id)
    );
    assert_eq!(overlay.last_error(), Some("root placement failed"));
}

#[test]
fn graph_overlay_state_rolls_back_only_the_failed_pending_root_mutation() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let root_id = SemanticNodeId::new("root").unwrap();
    let pending_a_id = SemanticNodeId::new("pending_a").unwrap();
    let pending_b_id = SemanticNodeId::new("pending_b").unwrap();

    let failed_mutation_id = begin_optimistic_patch(
        &mut overlay,
        upsert_root_node_patch(pending_a_id.clone(), "Pending A", Some(0), 180),
        [pending_a_id.clone()],
    );
    begin_optimistic_patch(
        &mut overlay,
        upsert_root_node_patch(pending_b_id.clone(), "Pending B", Some(0), 190),
        [pending_b_id.clone()],
    );

    assert_eq!(overlay.pending_optimistic_mutation_count(), 2);
    assert_eq!(
        overlay.graph().root_node_ids(),
        &[pending_b_id.clone(), pending_a_id.clone(), root_id.clone()]
    );

    overlay
        .fail_optimistic_mutation(Some(failed_mutation_id), "first root failed")
        .unwrap();

    assert_eq!(overlay.pending_optimistic_mutation_count(), 1);
    assert!(overlay.graph().node(&pending_a_id).is_none());
    assert!(overlay.graph().node(&pending_b_id).is_some());
    assert_eq!(
        overlay.graph().root_node_ids(),
        &[pending_b_id.clone(), root_id.clone()]
    );
    assert!(overlay.node_has_pending_optimistic_mutation(&pending_b_id));
    assert!(!overlay.node_has_pending_optimistic_mutation(&pending_a_id));
}

#[test]
fn graph_overlay_state_clears_matching_optimistic_thread_ref_commit_without_double_apply() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let child_id = SemanticNodeId::new("child").unwrap();
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_ref = thread_ref(
        "pending_child_thread",
        child_id.clone(),
        "pending child thread",
        &execution_target,
    );
    let patch = SemanticGraphPatch::from_operation(SemanticGraphPatchOp::UpsertThreadRef {
        thread_ref: thread_ref.clone(),
        provenance: provenance(130),
    });

    let mutation_id = begin_optimistic_patch(&mut overlay, patch.clone(), [child_id.clone()]);

    assert_eq!(overlay.pending_optimistic_mutation_count(), 1);
    assert!(overlay.graph_columns_available());
    assert!(overlay.mutation_pending());
    assert!(overlay.node_has_pending_optimistic_mutation(&child_id));
    assert!(overlay.graph().thread_ref(thread_ref.id()).is_some());

    let commit = graph_commit_update(0, 1, true, patch, "thread ref no-op")
        .with_optimistic_mutation_id(mutation_id);
    let applied = overlay.finish_mutation_commit_update(commit).unwrap();

    assert!(matches!(
        applied,
        graph::GraphCommitApplication::Applied {
            graph_changed: true,
            ..
        }
    ));
    assert_eq!(overlay.pending_optimistic_mutation_count(), 0);
    assert!(!overlay.mutation_pending());
    assert!(!overlay.node_has_pending_optimistic_mutation(&child_id));
    assert_eq!(overlay.graph().thread_ref_count(), 1);
    assert!(overlay.graph().thread_ref(thread_ref.id()).is_some());
    assert_eq!(overlay.revision(), WorkspaceGraphRevision::new(1));
}

#[test]
fn graph_overlay_state_rolls_back_started_thread_ref_when_persistence_fails() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let child_id = SemanticNodeId::new("child").unwrap();
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_ref = thread_ref(
        "pending_started_thread",
        child_id.clone(),
        "created thread",
        &execution_target,
    );
    let patch = SemanticGraphPatch::from_operation(SemanticGraphPatchOp::UpsertThreadRef {
        thread_ref: thread_ref.clone(),
        provenance: provenance(135),
    });

    let mutation_id = begin_optimistic_patch(&mut overlay, patch, [child_id.clone()]);

    assert!(overlay.graph_columns_available());
    assert!(overlay.mutation_pending());
    assert!(overlay.node_has_pending_optimistic_mutation(&child_id));
    assert!(overlay.graph().thread_ref(thread_ref.id()).is_some());

    overlay
        .fail_optimistic_mutation(
            Some(mutation_id),
            "Beryl created Codex thread semantic_thread but could not attach it to the semantic graph: persistence failed",
        )
        .unwrap();

    assert!(overlay.graph_columns_available());
    assert_eq!(overlay.pending_optimistic_mutation_count(), 0);
    assert!(!overlay.mutation_pending());
    assert!(!overlay.node_has_pending_optimistic_mutation(&child_id));
    assert!(overlay.graph().thread_ref(thread_ref.id()).is_none());
    assert!(overlay.graph().node(&child_id).is_some());
    assert_eq!(
        overlay.last_error(),
        Some(
            "Beryl created Codex thread semantic_thread but could not attach it to the semantic graph: persistence failed"
        )
    );
}

#[test]
fn graph_overlay_state_rejects_stale_optimistic_target_without_projection() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let missing_id = SemanticNodeId::new("missing").unwrap();
    let mutation_id = overlay.reserve_optimistic_mutation_id();
    let mutation = graph::GraphOptimisticMutation::new(
        mutation_id,
        overlay.revision(),
        delete_leaf_patch(&missing_id, 140),
        [missing_id],
        "Deleting semantic node",
    );

    assert!(overlay.begin_optimistic_mutation(mutation).is_err());
    assert_eq!(overlay.pending_optimistic_mutation_count(), 0);
    assert_eq!(overlay.graph().node_count(), explorer_graph().node_count());
}

#[test]
fn graph_overlay_state_projects_optimistic_recursive_delete_and_truncates_columns() {
    let mut overlay =
        graph::GraphOverlayState::new(explorer_graph(), WorkspaceGraphRevision::default(), None);
    let child_id = SemanticNodeId::new("child").unwrap();
    let grandchild_id = SemanticNodeId::new("grandchild").unwrap();
    let soft_link_id = SoftLinkId::new("child_to_peer").unwrap();

    assert!(overlay.select_node(0, &child_id));
    assert!(overlay.select_node(1, &grandchild_id));
    assert_eq!(overlay.columns().len(), 3);

    begin_optimistic_patch(
        &mut overlay,
        delete_subtree_patch(&child_id, 150),
        [child_id.clone(), grandchild_id.clone()],
    );

    assert_eq!(overlay.pending_optimistic_mutation_count(), 1);
    assert!(overlay.graph_columns_available());
    assert!(overlay.mutation_pending());
    assert!(overlay.graph().node(&child_id).is_none());
    assert!(overlay.graph().node(&grandchild_id).is_none());
    assert!(overlay.graph().soft_link(&soft_link_id).is_none());
    assert_eq!(overlay.columns().len(), 1);
    assert_root_level_column(&overlay, 0);
    assert!(overlay.selected_node_id().is_none());
}

#[test]
fn seed_patch_can_attach_under_an_existing_root_through_the_tool_service() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_overlay").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Overlay", 42);
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_graph_state(&workspace_id, &existing_root_graph())
        .unwrap();

    let response = service
        .apply_graph_patch(&GraphPatchWriteRequest {
            workspace_id: workspace_id.clone(),
            patch: workspace_management_seed_patch(
                Some(SemanticNodeId::new("existing_root").unwrap()),
                &execution_target,
            ),
            expected_base_revision: None,
        })
        .unwrap();
    let stored = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();

    assert!(response.commit.changed);
    assert_eq!(
        stored.root_node_ids(),
        &[SemanticNodeId::new("existing_root").unwrap()]
    );
    let children = stored
        .child_ids_of(&SemanticNodeId::new("existing_root").unwrap())
        .unwrap();
    assert!(children.contains(&SemanticNodeId::new("workspace_management").unwrap()));
    assert_eq!(stored.thread_ref_count(), 2);

    root.close().unwrap();
}

#[test]
fn mutation_patch_updates_statuses_and_adds_release_review_nodes() {
    let root = unique_temp_dir();
    let persistence = BerylWorkspacePersistence::new(&root);
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let workspace_id = BerylWorkspaceId::new("graph_overlay").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Graph Overlay", 42);
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");

    persistence.save_workspace_manifest(&manifest).unwrap();
    service
        .apply_graph_patch(&GraphPatchWriteRequest {
            workspace_id: workspace_id.clone(),
            patch: workspace_management_seed_patch(None, &execution_target),
            expected_base_revision: None,
        })
        .unwrap();

    let response = service
        .apply_graph_patch(&GraphPatchWriteRequest {
            workspace_id: workspace_id.clone(),
            patch: workspace_management_mutation_patch(&execution_target),
            expected_base_revision: None,
        })
        .unwrap();
    let stored = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();

    assert!(response.commit.changed);
    assert_eq!(
        stored
            .node(&SemanticNodeId::new("workspace_picker_item").unwrap())
            .unwrap()
            .checklist_item_status(),
        Some(ChecklistItemStatus::Done)
    );
    assert_eq!(
        stored
            .node(&SemanticNodeId::new("members_popup_item").unwrap())
            .unwrap()
            .checklist_item_status(),
        Some(ChecklistItemStatus::InProgress)
    );
    assert!(
        stored
            .node(&SemanticNodeId::new("release_review").unwrap())
            .is_some()
    );
    assert!(
        stored
            .thread_ref(
                &beryl_model::semantic_graph::ThreadRefId::new("release_review_thread").unwrap()
            )
            .is_some()
    );

    root.close().unwrap();
}

fn graph_column_expanded(
    column: &graph::GraphColumnState,
    node_id: &SemanticNodeId,
    depth: usize,
) -> bool {
    column.is_expanded(node_id, depth < graph::DEFAULT_GRAPH_COLUMN_EXPANDED_DEPTH)
}

fn assert_root_level_column(overlay: &graph::GraphOverlayState, column_index: usize) {
    assert_eq!(
        overlay.columns()[column_index].root_key(),
        &graph::GraphColumnKey::RootLevel
    );
}

fn assert_node_column(
    overlay: &graph::GraphOverlayState,
    column_index: usize,
    node_id: &SemanticNodeId,
) {
    assert_eq!(
        overlay.columns()[column_index].root_key(),
        &graph::GraphColumnKey::Node(node_id.clone())
    );
}

fn graph_commit_update(
    base_revision: u64,
    committed_revision: u64,
    changed: bool,
    patch: SemanticGraphPatch,
    no_op_message: &str,
) -> graph::GraphMutationCommitUpdate {
    graph::GraphMutationCommitUpdate::new(
        WorkspaceGraphMutationCommit::new(
            BerylWorkspaceId::new("graph_overlay").unwrap(),
            WorkspaceGraphRevision::new(base_revision),
            WorkspaceGraphRevision::new(committed_revision),
            changed,
            patch,
            BerylWorkspaceManifest::named(
                BerylWorkspaceId::new("graph_overlay").unwrap(),
                format!("Graph Overlay {committed_revision}"),
                1000 + committed_revision,
            ),
        ),
        no_op_message,
    )
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
        "Applying optimistic graph mutation",
    );
    overlay.begin_optimistic_mutation(mutation).unwrap();
    mutation_id
}

fn delete_subtree_patch(node_id: &SemanticNodeId, recorded_at_millis: u64) -> SemanticGraphPatch {
    SemanticGraphPatch::from_operation(SemanticGraphPatchOp::DeleteNodeSubtree {
        node_id: node_id.clone(),
        provenance: provenance(recorded_at_millis),
    })
}

fn delete_leaf_patch(node_id: &SemanticNodeId, recorded_at_millis: u64) -> SemanticGraphPatch {
    SemanticGraphPatch::from_operation(SemanticGraphPatchOp::DeleteNodeLeaf {
        node_id: node_id.clone(),
        provenance: provenance(recorded_at_millis),
    })
}

fn upsert_root_child_patch(
    node_id: SemanticNodeId,
    title: &str,
    provenance_seed: u64,
) -> SemanticGraphPatch {
    SemanticGraphPatch::new(vec![
        SemanticGraphPatchOp::UpsertNode {
            node: topic_node(node_id.clone(), title),
            provenance: provenance(provenance_seed),
        },
        SemanticGraphPatchOp::SetHardParent {
            child_id: node_id,
            parent_id: Some(SemanticNodeId::new("root").unwrap()),
            index: None,
            provenance: provenance(provenance_seed + 1),
        },
    ])
}

fn upsert_root_node_patch(
    node_id: SemanticNodeId,
    title: &str,
    index: Option<usize>,
    provenance_seed: u64,
) -> SemanticGraphPatch {
    SemanticGraphPatch::new(vec![
        SemanticGraphPatchOp::UpsertNode {
            node: topic_node(node_id.clone(), title),
            provenance: provenance(provenance_seed),
        },
        SemanticGraphPatchOp::SetHardParent {
            child_id: node_id,
            parent_id: None,
            index,
            provenance: provenance(provenance_seed + 1),
        },
    ])
}

fn delete_subtree_graph(mut graph: SemanticGraph, node_id: &SemanticNodeId) -> SemanticGraph {
    graph
        .apply_patch(&SemanticGraphPatch::from_operation(
            SemanticGraphPatchOp::DeleteNodeSubtree {
                node_id: node_id.clone(),
                provenance: provenance(90),
            },
        ))
        .unwrap();
    graph
}

fn delete_leaf_graph(mut graph: SemanticGraph, node_id: &SemanticNodeId) -> SemanticGraph {
    graph
        .apply_patch(&SemanticGraphPatch::from_operation(
            SemanticGraphPatchOp::DeleteNodeLeaf {
                node_id: node_id.clone(),
                provenance: provenance(91),
            },
        ))
        .unwrap();
    graph
}

fn explorer_graph() -> SemanticGraph {
    let root_id = SemanticNodeId::new("root").unwrap();
    let child_id = SemanticNodeId::new("child").unwrap();
    let grandchild_id = SemanticNodeId::new("grandchild").unwrap();
    let peer_id = SemanticNodeId::new("peer").unwrap();
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
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(grandchild_id.clone(), "Grandchild"),
                provenance: provenance(3),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(peer_id.clone(), "Peer"),
                provenance: provenance(4),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_id.clone(),
                parent_id: None,
                index: Some(0),
                provenance: provenance(5),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: child_id.clone(),
                parent_id: Some(root_id.clone()),
                index: Some(0),
                provenance: provenance(6),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: grandchild_id,
                parent_id: Some(child_id.clone()),
                index: Some(0),
                provenance: provenance(7),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: peer_id.clone(),
                parent_id: Some(root_id),
                index: Some(1),
                provenance: provenance(8),
            },
            SemanticGraphPatchOp::UpsertSoftLink {
                link: SoftLinkDraft::new(
                    SoftLinkId::new("child_to_peer").unwrap(),
                    child_id,
                    peer_id,
                    SoftLinkKind::new("depends_on").unwrap(),
                ),
                provenance: provenance(9),
            },
        ]))
        .unwrap();

    graph
}

fn existing_root_graph() -> SemanticGraph {
    let root_id = SemanticNodeId::new("existing_root").unwrap();
    let mut graph = SemanticGraph::default();
    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(root_id.clone(), "Existing Root"),
                provenance: provenance(1),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_id,
                parent_id: None,
                index: Some(0),
                provenance: provenance(2),
            },
        ]))
        .unwrap();
    graph
}

fn leaf_attachment_graph() -> SemanticGraph {
    let root_id = SemanticNodeId::new("root").unwrap();
    let leaf_id = SemanticNodeId::new("leaf").unwrap();
    let target_id = SemanticNodeId::new("target").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(root_id.clone(), "Root"),
                provenance: provenance(1),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(leaf_id.clone(), "Leaf"),
                provenance: provenance(2),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(target_id.clone(), "Target"),
                provenance: provenance(3),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: root_id.clone(),
                parent_id: None,
                index: Some(0),
                provenance: provenance(4),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: leaf_id.clone(),
                parent_id: Some(root_id.clone()),
                index: Some(0),
                provenance: provenance(5),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: target_id.clone(),
                parent_id: Some(root_id),
                index: Some(1),
                provenance: provenance(6),
            },
            SemanticGraphPatchOp::UpsertSoftLink {
                link: SoftLinkDraft::new(
                    SoftLinkId::new("leaf_to_target").unwrap(),
                    leaf_id,
                    target_id,
                    SoftLinkKind::new("depends_on").unwrap(),
                ),
                provenance: provenance(7),
            },
        ]))
        .unwrap();

    graph
}

fn multi_root_graph() -> SemanticGraph {
    let alpha_id = SemanticNodeId::new("alpha_root").unwrap();
    let alpha_child_id = SemanticNodeId::new("alpha_child").unwrap();
    let beta_id = SemanticNodeId::new("beta_root").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(alpha_id.clone(), "Alpha Root"),
                provenance: provenance(40),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(alpha_child_id.clone(), "Alpha Child"),
                provenance: provenance(41),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: topic_node(beta_id.clone(), "Beta Root"),
                provenance: provenance(42),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: alpha_id.clone(),
                parent_id: None,
                index: Some(0),
                provenance: provenance(43),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: beta_id.clone(),
                parent_id: None,
                index: Some(1),
                provenance: provenance(44),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: alpha_child_id.clone(),
                parent_id: Some(alpha_id),
                index: Some(0),
                provenance: provenance(45),
            },
            SemanticGraphPatchOp::UpsertSoftLink {
                link: SoftLinkDraft::new(
                    SoftLinkId::new("alpha_child_to_beta").unwrap(),
                    alpha_child_id,
                    beta_id,
                    SoftLinkKind::new("relates_to").unwrap(),
                ),
                provenance: provenance(46),
            },
        ]))
        .unwrap();

    graph
}

fn root_reordered_graph(mut graph: SemanticGraph, first_root_id: &SemanticNodeId) -> SemanticGraph {
    graph
        .apply_patch(&SemanticGraphPatch::from_operation(
            SemanticGraphPatchOp::SetHardParent {
                child_id: first_root_id.clone(),
                parent_id: None,
                index: Some(0),
                provenance: provenance(47),
            },
        ))
        .unwrap();
    graph
}

fn workspace_management_seed_patch(
    parent_id: Option<SemanticNodeId>,
    execution_target: &WorkspaceId,
) -> SemanticGraphPatch {
    let root_id = SemanticNodeId::new("workspace_management").unwrap();
    let picker_item_id = SemanticNodeId::new("workspace_picker_item").unwrap();
    let members_item_id = SemanticNodeId::new("members_popup_item").unwrap();

    SemanticGraphPatch::new(vec![
        SemanticGraphPatchOp::UpsertNode {
            node: SemanticNodeDraft::new(
                root_id.clone(),
                "Workspace Management",
                "Workspace management test root",
                SemanticNodeFacets::topic_and_checklist(),
                None,
            ),
            provenance: provenance(20),
        },
        SemanticGraphPatchOp::SetHardParent {
            child_id: root_id.clone(),
            parent_id,
            index: None,
            provenance: provenance(21),
        },
        SemanticGraphPatchOp::UpsertNode {
            node: checklist_item_node(
                picker_item_id.clone(),
                "Workspace picker",
                ChecklistItemStatus::InProgress,
            ),
            provenance: provenance(22),
        },
        SemanticGraphPatchOp::UpsertNode {
            node: checklist_item_node(
                members_item_id.clone(),
                "Members popup",
                ChecklistItemStatus::Todo,
            ),
            provenance: provenance(23),
        },
        SemanticGraphPatchOp::SetHardParent {
            child_id: picker_item_id.clone(),
            parent_id: Some(root_id.clone()),
            index: Some(0),
            provenance: provenance(24),
        },
        SemanticGraphPatchOp::SetHardParent {
            child_id: members_item_id.clone(),
            parent_id: Some(root_id),
            index: Some(1),
            provenance: provenance(25),
        },
        SemanticGraphPatchOp::UpsertThreadRef {
            thread_ref: thread_ref(
                "workspace_picker_thread",
                picker_item_id,
                "workspace picker thread",
                execution_target,
            ),
            provenance: provenance(26),
        },
        SemanticGraphPatchOp::UpsertThreadRef {
            thread_ref: thread_ref(
                "members_popup_thread",
                members_item_id,
                "members popup thread",
                execution_target,
            ),
            provenance: provenance(27),
        },
    ])
}

fn workspace_management_mutation_patch(execution_target: &WorkspaceId) -> SemanticGraphPatch {
    let seed_patch = workspace_management_seed_patch(None, execution_target);
    let mut operations = seed_patch.operations().to_vec();
    operations.extend([
        SemanticGraphPatchOp::SetChecklistItemStatus {
            node_id: SemanticNodeId::new("workspace_picker_item").unwrap(),
            status: ChecklistItemStatus::Done,
            provenance: provenance(30),
        },
        SemanticGraphPatchOp::SetChecklistItemStatus {
            node_id: SemanticNodeId::new("members_popup_item").unwrap(),
            status: ChecklistItemStatus::InProgress,
            provenance: provenance(31),
        },
        SemanticGraphPatchOp::UpsertNode {
            node: checklist_item_node(
                SemanticNodeId::new("release_review").unwrap(),
                "Release Review",
                ChecklistItemStatus::Todo,
            ),
            provenance: provenance(32),
        },
        SemanticGraphPatchOp::SetHardParent {
            child_id: SemanticNodeId::new("release_review").unwrap(),
            parent_id: Some(SemanticNodeId::new("workspace_management").unwrap()),
            index: Some(2),
            provenance: provenance(33),
        },
        SemanticGraphPatchOp::UpsertThreadRef {
            thread_ref: thread_ref(
                "release_review_thread",
                SemanticNodeId::new("release_review").unwrap(),
                "release review thread",
                execution_target,
            ),
            provenance: provenance(34),
        },
    ]);
    SemanticGraphPatch::new(operations)
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

fn thread_ref(
    thread_ref_id: &str,
    node_id: SemanticNodeId,
    label: &str,
    execution_target: &WorkspaceId,
) -> ThreadRefDraft {
    ThreadRefDraft::new(
        ThreadRefId::new(thread_ref_id).unwrap(),
        node_id,
        ConversationThreadId::new(thread_ref_id.to_string()),
        execution_target.clone(),
        label,
    )
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
        MutationSource::workspace_action("graph_overlay_test").unwrap(),
        Some(100),
    )
    .unwrap()
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-graph-overlay-test-")
}

fn rust_function_body<'a>(source: &'a str, function_signature: &str) -> &'a str {
    let start = source.find(function_signature).unwrap_or_else(|| {
        panic!("missing function signature {function_signature:?}");
    });
    let body_start = source[start..]
        .find('{')
        .map(|offset| start + offset)
        .expect("function has an opening brace");
    let mut depth = 0usize;
    for (offset, ch) in source[body_start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[body_start..=body_start + offset];
                }
            }
            _ => {}
        }
    }
    panic!("function body did not close");
}
