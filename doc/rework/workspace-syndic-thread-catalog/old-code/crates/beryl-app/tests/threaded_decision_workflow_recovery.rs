#[allow(dead_code)]
#[path = "../src/member_thread_inventory.rs"]
mod member_thread_inventory;
#[path = "support/tempdir.rs"]
mod tempdir_support;
#[allow(dead_code)]
#[path = "../src/thread_strip_breadcrumbs.rs"]
mod thread_strip_breadcrumbs;
#[allow(dead_code)]
#[path = "../src/threaded_decision_archive_core.rs"]
mod threaded_decision_archive_core;
#[allow(dead_code)]
#[path = "../src/threaded_decision_branch_core.rs"]
mod threaded_decision_branch_core;
#[allow(dead_code)]
#[path = "../src/threaded_decision_context.rs"]
mod threaded_decision_context;
#[allow(dead_code)]
#[path = "../src/threaded_decision_graph_presentation.rs"]
mod threaded_decision_graph_presentation;
#[allow(dead_code)]
#[path = "../src/threaded_decision_resolution_core.rs"]
mod threaded_decision_resolution_core;

use beryl_app::BerylWorkspacePersistence;
use beryl_model::{
    conversation::{
        ConversationThreadId, ConversationTurnId, RegisteredConversationThread,
        WorkspaceConversationState,
    },
    provenance::{MutationProvenance, MutationSource},
    semantic_graph::{
        ChecklistItemKind, ChecklistItemStatus, SemanticGraph, SemanticGraphPatch,
        SemanticGraphPatchOp, SemanticNodeDraft, SemanticNodeFacets, SemanticNodeId,
    },
    threaded_decision::{
        ThreadedDecisionArchiveState, ThreadedDecisionOperationId, ThreadedDecisionOutcome,
        ThreadedDecisionRecord, ThreadedDecisionRecordId, ThreadedDecisionState,
        ThreadedDecisionStatus,
    },
    workspace::{BerylWorkspaceId, BerylWorkspaceManifest, WorkspaceId},
};
use thread_strip_breadcrumbs::thread_strip_breadcrumb_trail;
use threaded_decision_archive_core::{
    archive_operation_id_for_record, child_thread_is_read_only_decision_branch,
    normal_selector_hidden_decision_child_thread_ids, record_needs_child_archive,
};
use threaded_decision_branch_core::{
    decision_branch_graph_patch, decision_child_progress_patch, topic_decision_item_plan,
};
use threaded_decision_context::{
    ThreadedDecisionBootstrapContextInput, threaded_decision_bootstrap_context,
};
use threaded_decision_graph_presentation::{
    active_decision_branch_record_for_item, archive_retry_record_for_item,
    decision_branch_start_label, decision_item_badges, decision_thread_ref_badge,
    latest_handoff_record_for_item,
};
use threaded_decision_resolution_core::{
    DecisionHandoffMessageInput, decision_handoff_message, decision_resolution_checklist_patch,
};

#[test]
fn threaded_decision_workflow_roundtrips_partial_archive_and_superseding_recovery() {
    let root = tempdir_support::temp_dir("beryl-threaded-decision-workflow-");
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("workspace").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Workspace", 1);
    persistence.save_workspace_manifest(&manifest).unwrap();

    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let item_id = node_id("pick_storage");
    let parent_thread_id = thread_id("parent_thread");
    let branch_point_id = turn_id("parent_turn_1");
    let first_record_id = record_id("storage_record");
    let first_child_id = thread_id("storage_child");
    let handoff_turn_id = turn_id("parent_handoff");
    let mut graph = graph_with_checklist_item(&item_id);
    let mut decisions = ThreadedDecisionState::default();

    decisions
        .insert_record(ThreadedDecisionRecord::queued_branch(
            first_record_id.clone(),
            item_id.clone(),
            parent_thread_id.clone(),
            Some(branch_point_id.clone()),
            operation_id("branch_storage"),
            1,
            provenance("branch_queued", 1),
        ))
        .unwrap();
    let (_, first_ref_patch) = decision_branch_graph_patch(
        &graph,
        &item_id,
        first_child_id.clone(),
        execution_target.clone(),
        Some("Storage decision"),
        &provenance("branch_attached", 2),
    )
    .unwrap();
    graph.apply_patch(&first_ref_patch).unwrap();
    decisions
        .activate_branch(
            &first_record_id,
            first_child_id.clone(),
            None,
            provenance("branch_activated", 3),
        )
        .unwrap();

    assert_eq!(
        graph.node(&item_id).unwrap().checklist_item_kind(),
        Some(ChecklistItemKind::Decision)
    );
    assert_eq!(thread_refs_for_item(&graph, &item_id).len(), 1);
    assert_eq!(
        active_decision_branch_record_for_item(&decisions, &item_id)
            .unwrap()
            .record_id(),
        &first_record_id
    );
    assert_eq!(
        badge_labels(&graph, &decisions, &item_id),
        ["decision", "active"]
    );

    assert!(
        decision_child_progress_patch(
            &graph,
            &decisions,
            &first_child_id,
            &branch_point_id,
            &provenance("inherited_turn", 4),
        )
        .is_none()
    );
    let progress = decision_child_progress_patch(
        &graph,
        &decisions,
        &first_child_id,
        &turn_id("storage_child_turn_1"),
        &provenance("child_progress", 5),
    )
    .unwrap();
    graph.apply_patch(&progress.patch).unwrap();
    assert_eq!(
        graph.node(&item_id).unwrap().checklist_item_status(),
        Some(ChecklistItemStatus::InProgress)
    );

    decisions
        .mark_pending_resolution(
            &first_record_id,
            ThreadedDecisionOutcome::Accepted,
            "Use SQLite for the first slice.",
            "The branch compared flat files and SQLite. Use SQLite.",
            operation_id("resolve_storage"),
            provenance("resolve", 6),
        )
        .unwrap();
    let handoff_message = decision_handoff_message(DecisionHandoffMessageInput {
        checklist_item_id: &item_id,
        checklist_item_title: graph.node(&item_id).unwrap().title(),
        child_thread_id: &first_child_id,
        parent_thread_id: &parent_thread_id,
        branch_point_turn_id: Some(&branch_point_id),
        outcome: ThreadedDecisionOutcome::Accepted,
        summary: decisions
            .record(&first_record_id)
            .unwrap()
            .resolution_summary()
            .unwrap(),
        handoff_message: decisions
            .record(&first_record_id)
            .unwrap()
            .handoff_message()
            .unwrap(),
    });
    assert!(handoff_message.contains("Resolution: accepted"));
    assert!(handoff_message.contains("Decision branch thread: storage_child"));

    decisions
        .mark_handoff_started(
            &first_record_id,
            Some(handoff_turn_id.clone()),
            provenance("handoff_started", 7),
        )
        .unwrap();
    let checklist_patch = decision_resolution_checklist_patch(
        &graph,
        decisions.record(&first_record_id).unwrap(),
        &provenance("checklist_done", 8),
    )
    .unwrap();
    graph.apply_patch(&checklist_patch.patch).unwrap();
    decisions
        .mark_checklist_updated(
            &first_record_id,
            handoff_turn_id.clone(),
            provenance("checklist_updated", 9),
        )
        .unwrap();

    let restarted = save_and_load(&persistence, &workspace_id, &decisions);
    let restarted_record = restarted.record(&first_record_id).unwrap();
    assert_eq!(
        restarted_record.status(),
        ThreadedDecisionStatus::ChecklistUpdated
    );
    assert!(record_needs_child_archive(restarted_record));
    assert_eq!(
        latest_handoff_record_for_item(&restarted, &item_id)
            .unwrap()
            .handoff_turn_id(),
        Some(&handoff_turn_id)
    );
    assert_eq!(
        badge_labels(&graph, &restarted, &item_id),
        ["decision", "close queued"]
    );

    let first_archive_op = archive_operation_id_for_record(&first_record_id, 10).unwrap();
    decisions = restarted;
    decisions
        .mark_archive_pending(
            &first_record_id,
            first_archive_op.clone(),
            provenance("archive_pending", 10),
        )
        .unwrap();
    decisions
        .mark_archive_failed(
            &first_record_id,
            "backend disconnected",
            provenance("archive_failed", 11),
        )
        .unwrap();
    let failed_after_restart = save_and_load(&persistence, &workspace_id, &decisions);
    assert_eq!(
        archive_retry_record_for_item(&failed_after_restart, &item_id)
            .unwrap()
            .record_id(),
        &first_record_id
    );
    assert_eq!(
        badge_labels(&graph, &failed_after_restart, &item_id),
        ["decision", "close failed"]
    );
    assert!(child_thread_is_read_only_decision_branch(
        &failed_after_restart,
        &first_child_id
    ));
    assert_eq!(
        normal_selector_hidden_decision_child_thread_ids(&failed_after_restart),
        vec![first_child_id.clone()]
    );

    decisions = failed_after_restart;
    decisions
        .mark_archive_pending(
            &first_record_id,
            archive_operation_id_for_record(&first_record_id, 12).unwrap(),
            provenance("archive_retry", 12),
        )
        .unwrap();
    decisions
        .mark_closed(&first_record_id, provenance("archive_closed", 13))
        .unwrap();
    let closed_after_restart = save_and_load(&persistence, &workspace_id, &decisions);
    let closed_record = closed_after_restart.record(&first_record_id).unwrap();
    assert_eq!(closed_record.status(), ThreadedDecisionStatus::Closed);
    assert_eq!(
        closed_record.archive_status().state(),
        ThreadedDecisionArchiveState::Archived
    );
    assert_eq!(
        badge_labels(&graph, &closed_after_restart, &item_id),
        ["decision", "accepted"]
    );
    assert_eq!(
        decision_branch_start_label(&closed_after_restart, &item_id),
        "Start Superseding Branch"
    );

    decisions = closed_after_restart;
    let second_record_id = record_id("storage_record_2");
    let second_child_id = thread_id("storage_child_2");
    let second_branch_point_id = turn_id("parent_turn_2");
    let second_handoff_turn_id = turn_id("parent_handoff_2");
    decisions
        .insert_record(ThreadedDecisionRecord::queued_branch(
            second_record_id.clone(),
            item_id.clone(),
            parent_thread_id.clone(),
            Some(second_branch_point_id.clone()),
            operation_id("branch_storage_2"),
            20,
            provenance("superseding_branch_queued", 20),
        ))
        .unwrap();
    let (_, second_ref_patch) = decision_branch_graph_patch(
        &graph,
        &item_id,
        second_child_id.clone(),
        execution_target,
        Some("Storage decision retry"),
        &provenance("superseding_branch_attached", 21),
    )
    .unwrap();
    graph.apply_patch(&second_ref_patch).unwrap();
    decisions
        .activate_branch(
            &second_record_id,
            second_child_id.clone(),
            None,
            provenance("superseding_branch_activated", 22),
        )
        .unwrap();
    decisions
        .supersede_closed_records_for_item(
            &item_id,
            second_record_id.clone(),
            provenance("superseded", 23),
        )
        .unwrap();

    let superseded_after_restart = save_and_load(&persistence, &workspace_id, &decisions);
    assert_eq!(
        superseded_after_restart
            .record(&first_record_id)
            .unwrap()
            .status(),
        ThreadedDecisionStatus::Superseded
    );
    assert_eq!(
        superseded_after_restart
            .record(&first_record_id)
            .unwrap()
            .outcome(),
        Some(ThreadedDecisionOutcome::Accepted)
    );
    assert_eq!(
        active_decision_branch_record_for_item(&superseded_after_restart, &item_id)
            .unwrap()
            .record_id(),
        &second_record_id
    );
    assert_eq!(
        badge_labels(&graph, &superseded_after_restart, &item_id),
        ["decision", "active"]
    );
    let refs = thread_refs_for_item(&graph, &item_id);
    assert_eq!(refs.len(), 2);
    assert_eq!(
        decision_thread_ref_badge(
            &superseded_after_restart,
            refs.iter()
                .find(|thread_ref| thread_ref.thread_id() == &first_child_id)
                .unwrap()
        )
        .unwrap()
        .label(),
        "superseded"
    );
    assert_eq!(
        decision_thread_ref_badge(
            &superseded_after_restart,
            refs.iter()
                .find(|thread_ref| thread_ref.thread_id() == &second_child_id)
                .unwrap()
        )
        .unwrap()
        .label(),
        "active"
    );

    decisions = superseded_after_restart;
    decisions
        .mark_pending_resolution(
            &second_record_id,
            ThreadedDecisionOutcome::Rejected,
            "Do not replace the accepted storage choice yet.",
            "The follow-up branch rejected changing storage for this slice.",
            operation_id("resolve_storage_2"),
            provenance("resolve_rejected", 24),
        )
        .unwrap();
    let rejected_handoff = decision_handoff_message(DecisionHandoffMessageInput {
        checklist_item_id: &item_id,
        checklist_item_title: graph.node(&item_id).unwrap().title(),
        child_thread_id: &second_child_id,
        parent_thread_id: &parent_thread_id,
        branch_point_turn_id: Some(&second_branch_point_id),
        outcome: ThreadedDecisionOutcome::Rejected,
        summary: decisions
            .record(&second_record_id)
            .unwrap()
            .resolution_summary()
            .unwrap(),
        handoff_message: decisions
            .record(&second_record_id)
            .unwrap()
            .handoff_message()
            .unwrap(),
    });
    assert!(rejected_handoff.contains("Resolution: rejected"));
    decisions
        .mark_handoff_started(
            &second_record_id,
            Some(second_handoff_turn_id.clone()),
            provenance("handoff_rejected", 25),
        )
        .unwrap();
    let rejected_checklist_patch = decision_resolution_checklist_patch(
        &graph,
        decisions.record(&second_record_id).unwrap(),
        &provenance("checklist_rejected", 26),
    )
    .unwrap();
    graph.apply_patch(&rejected_checklist_patch.patch).unwrap();
    decisions
        .mark_checklist_updated(
            &second_record_id,
            second_handoff_turn_id,
            provenance("checklist_rejected_updated", 27),
        )
        .unwrap();
    decisions
        .mark_archive_pending(
            &second_record_id,
            archive_operation_id_for_record(&second_record_id, 28).unwrap(),
            provenance("archive_rejected_pending", 28),
        )
        .unwrap();
    decisions
        .mark_closed(&second_record_id, provenance("archive_rejected_closed", 29))
        .unwrap();
    let rejected_after_restart = save_and_load(&persistence, &workspace_id, &decisions);
    assert_eq!(
        rejected_after_restart
            .record(&second_record_id)
            .unwrap()
            .status(),
        ThreadedDecisionStatus::Closed
    );
    assert_eq!(
        rejected_after_restart
            .record(&second_record_id)
            .unwrap()
            .outcome(),
        Some(ThreadedDecisionOutcome::Rejected)
    );
    assert_eq!(
        badge_labels(&graph, &rejected_after_restart, &item_id),
        ["decision", "rejected", "history"]
    );
    let refs = thread_refs_for_item(&graph, &item_id);
    assert_eq!(
        decision_thread_ref_badge(
            &rejected_after_restart,
            refs.iter()
                .find(|thread_ref| thread_ref.thread_id() == &second_child_id)
                .unwrap()
        )
        .unwrap()
        .label(),
        "rejected"
    );

    root.close().unwrap();
}

#[test]
fn topic_started_decision_retries_without_duplicate_item_after_branch_failure() {
    let topic_id = node_id("architecture");
    let parent_thread_id = thread_id("parent_thread");
    let branch_point_id = turn_id("parent_turn_1");
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let mut graph = graph_with_topic(&topic_id);
    let mut decisions = ThreadedDecisionState::default();

    let first_plan = topic_decision_item_plan(
        &graph,
        &topic_id,
        "Choose queue backend",
        "",
        &provenance("topic_decision_item", 1),
    )
    .unwrap();
    let item_id = first_plan.checklist_item_id.clone();
    let first_record_id = record_id("queue_decision_1");
    decisions
        .insert_record(ThreadedDecisionRecord::queued_branch(
            first_record_id.clone(),
            item_id.clone(),
            parent_thread_id.clone(),
            Some(branch_point_id.clone()),
            operation_id("queue_branch_1"),
            2,
            provenance("queue_branch_1", 2),
        ))
        .unwrap();
    assert!(decisions.remove_record(&first_record_id));
    assert!(decisions.records().is_empty());
    assert!(graph.node(&item_id).is_none());
    assert!(graph.checklist_item_children_of(&topic_id).is_empty());

    let retry_plan = topic_decision_item_plan(
        &graph,
        &topic_id,
        "Choose queue backend",
        "",
        &provenance("topic_decision_item_retry", 4),
    )
    .unwrap();
    assert_eq!(retry_plan.checklist_item_id, item_id);
    assert!(!retry_plan.reused_existing_item());
    graph
        .apply_patch(retry_plan.patch.as_ref().unwrap())
        .unwrap();
    assert_eq!(graph.checklist_item_children_of(&topic_id).len(), 1);

    let second_record_id = record_id("queue_decision_2");
    let child_thread_id = thread_id("queue_child");
    decisions
        .insert_record(ThreadedDecisionRecord::queued_branch(
            second_record_id.clone(),
            item_id.clone(),
            parent_thread_id,
            Some(branch_point_id),
            operation_id("queue_branch_2"),
            5,
            provenance("queue_branch_2", 5),
        ))
        .unwrap();
    let (_, ref_patch) = decision_branch_graph_patch(
        &graph,
        &item_id,
        child_thread_id.clone(),
        execution_target,
        Some("Queue decision"),
        &provenance("attach_branch", 6),
    )
    .unwrap();
    graph.apply_patch(&ref_patch).unwrap();
    decisions
        .activate_branch(
            &second_record_id,
            child_thread_id,
            None,
            provenance("activate_retry", 7),
        )
        .unwrap();

    assert_eq!(graph.checklist_item_children_of(&topic_id).len(), 1);
    assert_eq!(thread_refs_for_item(&graph, &item_id).len(), 1);
    assert_eq!(
        active_decision_branch_record_for_item(&decisions, &item_id)
            .unwrap()
            .record_id(),
        &second_record_id
    );
    assert!(decisions.record(&first_record_id).is_none());
}

#[test]
fn topic_started_bootstrap_context_and_parent_breadcrumb_survive_restart() {
    let root = tempdir_support::temp_dir("beryl-threaded-decision-topic-context-");
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("workspace").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Workspace", 1);
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let topic_id = node_id("architecture");
    let parent_thread_id = thread_id("parent_thread");
    let child_thread_id = thread_id("queue_child");
    let branch_point_id = turn_id("parent_turn_1");
    let bootstrap_turn_id = turn_id("bootstrap_turn");
    let record_id = record_id("queue_record");
    let mut graph = graph_with_topic(&topic_id);
    let mut decisions = ThreadedDecisionState::default();
    let mut workspace_state = WorkspaceConversationState::default();

    persistence.save_workspace_manifest(&manifest).unwrap();
    workspace_state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    workspace_state.remember_thread(RegisteredConversationThread::new(
        parent_thread_id.clone(),
        execution_target.clone(),
        "Parent planning summary",
        Some("Parent planning".to_string()),
        1,
        2,
    ));
    workspace_state.remember_thread(
        RegisteredConversationThread::new(
            child_thread_id.clone(),
            execution_target.clone(),
            "",
            Some("Queue decision".to_string()),
            3,
            4,
        )
        .with_branch_parent_thread_id(parent_thread_id.clone()),
    );

    let plan = topic_decision_item_plan(
        &graph,
        &topic_id,
        "Choose queue backend",
        "Compare queue backends for async work.",
        &provenance("topic_decision_item", 1),
    )
    .unwrap();
    graph.apply_patch(plan.patch.as_ref().unwrap()).unwrap();
    let item_id = plan.checklist_item_id.clone();
    decisions
        .insert_record(ThreadedDecisionRecord::queued_branch(
            record_id.clone(),
            item_id.clone(),
            parent_thread_id.clone(),
            Some(branch_point_id.clone()),
            operation_id("queue_branch"),
            2,
            provenance("queue_branch", 2),
        ))
        .unwrap();
    let (_, thread_ref_patch) = decision_branch_graph_patch(
        &graph,
        &item_id,
        child_thread_id.clone(),
        execution_target.clone(),
        Some("Queue decision"),
        &provenance("attach_branch", 3),
    )
    .unwrap();
    graph.apply_patch(&thread_ref_patch).unwrap();
    decisions
        .activate_branch_with_bootstrap_turn(
            &record_id,
            child_thread_id.clone(),
            Some(bootstrap_turn_id.clone()),
            None,
            provenance("activate_branch", 4),
        )
        .unwrap();

    persistence
        .save_workspace_graph_state(&workspace_id, &graph)
        .unwrap();
    persistence
        .save_workspace_state(&workspace_id, &workspace_state)
        .unwrap();
    persistence
        .save_workspace_threaded_decision_state(&workspace_id, &decisions)
        .unwrap();

    let loaded_graph = persistence
        .load_workspace_graph_state(&workspace_id)
        .unwrap();
    let loaded_workspace_state = persistence.load_workspace_state(&workspace_id).unwrap();
    let loaded_decisions = persistence
        .load_workspace_threaded_decision_state(&workspace_id)
        .unwrap();

    assert_eq!(
        loaded_graph.node(&item_id).unwrap().checklist_item_status(),
        Some(ChecklistItemStatus::Todo)
    );
    assert!(
        decision_child_progress_patch(
            &loaded_graph,
            &loaded_decisions,
            &child_thread_id,
            &bootstrap_turn_id,
            &provenance("bootstrap_turn_replayed", 5),
        )
        .is_none()
    );

    let record = loaded_decisions.record(&record_id).unwrap();
    assert_eq!(record.bootstrap_turn_id(), Some(&bootstrap_turn_id));
    let item = loaded_graph.node(&item_id).unwrap();
    let parent = loaded_workspace_state
        .thread_registration(&parent_thread_id)
        .unwrap();
    let context = threaded_decision_bootstrap_context(ThreadedDecisionBootstrapContextInput {
        graph: &loaded_graph,
        checklist_item_id: &item_id,
        checklist_item_title: item.title(),
        checklist_item_summary: item.summary(),
        planned_parent_topic_id: None,
        parent_thread_id: &parent_thread_id,
        parent_thread_title: parent.title(),
        parent_thread_summary: Some(parent.preview()),
        child_thread_id: &child_thread_id,
        parent_context_turn_id: Some(&branch_point_id),
        parent_context_source: Some(
            "User:\nPlan the queue backend decision.\n\nAssistant:\nCreated a checklist item for comparing queue backends.",
        ),
        record_id: &record_id,
    });
    assert!(
        context
            .text()
            .contains("Decision checklist item: Choose queue backend")
    );
    assert!(
        context
            .text()
            .contains("Branch purpose: Compare queue backends for async work.")
    );
    assert!(
        context
            .text()
            .contains("Graph path: Architecture > Choose queue backend")
    );
    assert!(context.text().contains("Graph ancestor summaries:"));
    assert!(
        context
            .text()
            .contains("Architecture: Architecture decisions.")
    );
    assert!(
        context
            .text()
            .contains("Parent thread title: Parent planning")
    );
    assert!(
        context
            .text()
            .contains("Parent thread summary: Parent planning summary")
    );
    assert!(context.text().contains("Parent context source content:"));
    assert!(
        context
            .text()
            .contains("Created a checklist item for comparing queue backends.")
    );
    assert!(
        context
            .text()
            .contains("Resolution workflow: Explore only this decision in the child thread.")
    );

    let breadcrumb = thread_strip_breadcrumb_trail(
        &loaded_workspace_state,
        Some(child_thread_id.as_str()),
        "Queue decision",
        None,
    )
    .expect("restarted decision child should keep its parent breadcrumb");
    assert_eq!(breadcrumb.segments()[0].thread_id(), &parent_thread_id);
    assert_eq!(breadcrumb.segments()[0].label(), "Parent planning");
    assert_eq!(
        breadcrumb.segments()[0].execution_target(),
        Some(&execution_target)
    );
    assert!(breadcrumb.segments()[0].activation_available());
    assert_eq!(breadcrumb.segments()[1].label(), "Queue decision");

    root.close().unwrap();
}

fn save_and_load(
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    state: &ThreadedDecisionState,
) -> ThreadedDecisionState {
    persistence
        .save_workspace_threaded_decision_state(workspace_id, state)
        .unwrap();
    persistence
        .load_workspace_threaded_decision_state(workspace_id)
        .unwrap()
}

fn badge_labels<const N: usize>(
    graph: &SemanticGraph,
    decisions: &ThreadedDecisionState,
    item_id: &SemanticNodeId,
) -> [&'static str; N] {
    decision_item_badges(graph.node(item_id).unwrap(), decisions)
        .into_iter()
        .map(|badge| badge.label())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}

fn thread_refs_for_item<'a>(
    graph: &'a SemanticGraph,
    item_id: &SemanticNodeId,
) -> Vec<&'a beryl_model::semantic_graph::ThreadRef> {
    graph.thread_refs_for_node(item_id).collect()
}

fn graph_with_checklist_item(item_id: &SemanticNodeId) -> SemanticGraph {
    let list_id = node_id("decisions");
    let mut graph = SemanticGraph::default();
    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    list_id.clone(),
                    "Decisions",
                    "Decision checklist",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: provenance("seed", 1),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: list_id.clone(),
                parent_id: None,
                index: None,
                provenance: provenance("seed", 2),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    item_id.clone(),
                    "Pick storage backend",
                    "Choose the storage backend.",
                    SemanticNodeFacets::topic_and_checklist_item(),
                    Some(ChecklistItemStatus::Todo),
                ),
                provenance: provenance("seed", 3),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: item_id.clone(),
                parent_id: Some(list_id),
                index: None,
                provenance: provenance("seed", 4),
            },
        ]))
        .unwrap();
    graph
}

fn graph_with_topic(topic_id: &SemanticNodeId) -> SemanticGraph {
    let mut graph = SemanticGraph::default();
    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    topic_id.clone(),
                    "Architecture",
                    "Architecture decisions.",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: provenance("seed", 1),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: topic_id.clone(),
                parent_id: None,
                index: None,
                provenance: provenance("seed", 2),
            },
        ]))
        .unwrap();
    graph
}

fn provenance(action: &str, recorded_at_millis: u64) -> MutationProvenance {
    MutationProvenance::new(
        "beryl",
        recorded_at_millis,
        MutationSource::workspace_action(action).unwrap(),
        Some(100),
    )
    .unwrap()
}

fn record_id(value: &str) -> ThreadedDecisionRecordId {
    ThreadedDecisionRecordId::new(value).unwrap()
}

fn operation_id(value: &str) -> ThreadedDecisionOperationId {
    ThreadedDecisionOperationId::new(value).unwrap()
}

fn node_id(value: &str) -> SemanticNodeId {
    SemanticNodeId::new(value).unwrap()
}

fn thread_id(value: &str) -> ConversationThreadId {
    ConversationThreadId::new(value)
}

fn turn_id(value: &str) -> ConversationTurnId {
    ConversationTurnId::new(value)
}
