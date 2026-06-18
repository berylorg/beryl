#[path = "../src/threaded_decision_branch_core.rs"]
mod threaded_decision_branch_core;

use beryl_model::{
    conversation::{ConversationThreadId, ConversationTurnId},
    provenance::{MutationProvenance, MutationSource},
    semantic_graph::{
        ChecklistItemKind, ChecklistItemStatus, SemanticGraph, SemanticGraphPatch,
        SemanticGraphPatchOp, SemanticNodeDraft, SemanticNodeFacets, SemanticNodeId,
    },
    threaded_decision::{
        ThreadedDecisionOperationId, ThreadedDecisionRecord, ThreadedDecisionRecordId,
        ThreadedDecisionState,
    },
    workspace::WorkspaceId,
};
use threaded_decision_branch_core::{
    DecisionBranchStartBlocker, DecisionBranchStartGate, QueuedDecisionBranchRunBlocker,
    QueuedDecisionBranchRunGate, TopicDecisionItemPlanError, decision_branch_graph_patch,
    decision_branch_start_blocker, decision_child_progress_patch, default_topic_decision_title,
    queued_decision_branch_run_blocker, topic_decision_item_plan,
};

#[test]
fn decision_branch_start_blocks_existing_active_branch() {
    let blocker = decision_branch_start_blocker(DecisionBranchStartGate {
        graph_work_active: false,
        backend_available: true,
        checklist_item_exists: true,
        checklist_item: true,
        parent_thread_registered: true,
        branch_point_available: true,
        active_branch_exists: true,
        parent_thread_busy_without_queue: false,
        parent_compacting: false,
    });

    assert_eq!(
        blocker,
        Some(DecisionBranchStartBlocker::ActiveBranchExists)
    );
    assert!(
        DecisionBranchStartBlocker::ActiveBranchExists
            .message()
            .contains("active decision branch")
    );
}

#[test]
fn queued_decision_branch_waits_for_parent_turn_to_finish() {
    let blocker = queued_decision_branch_run_blocker(QueuedDecisionBranchRunGate {
        branch_worker_active: false,
        graph_work_active: false,
        parent_turn_active: true,
        parent_compacting: false,
        backend_available: true,
        checklist_item_exists: true,
        parent_thread_registered: true,
        branch_point_available: true,
    });

    assert_eq!(
        blocker,
        Some(QueuedDecisionBranchRunBlocker::ParentTurnActive)
    );

    let ready = queued_decision_branch_run_blocker(QueuedDecisionBranchRunGate {
        parent_turn_active: false,
        ..QueuedDecisionBranchRunGate {
            branch_worker_active: false,
            graph_work_active: false,
            parent_turn_active: true,
            parent_compacting: false,
            backend_available: true,
            checklist_item_exists: true,
            parent_thread_registered: true,
            branch_point_available: true,
        }
    });
    assert_eq!(ready, None);
}

#[test]
fn decision_branch_graph_patch_sets_item_kind_and_thread_ref() {
    let item_id = node_id("decision_item");
    let mut graph = graph_with_checklist_item(&item_id);
    let provenance = provenance("decision_branch", 1);
    let (_, patch) = decision_branch_graph_patch(
        &graph,
        &item_id,
        ConversationThreadId::new("child_thread"),
        WorkspaceId::host_windows(r"C:\work\beryl"),
        &provenance,
    )
    .expect("checklist item should produce decision branch graph patch");

    assert!(patch.operations().iter().any(|operation| {
        matches!(
            operation,
            SemanticGraphPatchOp::SetChecklistItemKind {
                node_id,
                kind: ChecklistItemKind::Decision,
                ..
            } if node_id == &item_id
        )
    }));
    graph.apply_patch(&patch).unwrap();
    let refs = graph.thread_refs_for_node(&item_id).collect::<Vec<_>>();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].thread_id().as_str(), "child_thread");
    assert_eq!(refs[0].label(), "Decision branch");
}

#[test]
fn inherited_branch_point_turn_does_not_mark_decision_in_progress() {
    let item_id = node_id("decision_item");
    let graph = graph_with_checklist_item(&item_id);
    let state = active_decision_state(&item_id);
    let progress = decision_child_progress_patch(
        &graph,
        &state,
        &ConversationThreadId::new("child_thread"),
        &ConversationTurnId::new("branch_point"),
        &provenance("child_progress", 10),
    );

    assert_eq!(progress, None);
}

#[test]
fn bootstrap_turn_does_not_mark_decision_in_progress() {
    let item_id = node_id("decision_item");
    let graph = graph_with_checklist_item(&item_id);
    let state = active_decision_state_with_bootstrap(&item_id);
    let progress = decision_child_progress_patch(
        &graph,
        &state,
        &ConversationThreadId::new("child_thread"),
        &ConversationTurnId::new("bootstrap_turn"),
        &provenance("child_progress", 10),
    );

    assert_eq!(progress, None);
}

#[test]
fn first_child_local_turn_marks_decision_in_progress() {
    let item_id = node_id("decision_item");
    let mut graph = graph_with_checklist_item(&item_id);
    let state = active_decision_state(&item_id);
    let progress = decision_child_progress_patch(
        &graph,
        &state,
        &ConversationThreadId::new("child_thread"),
        &ConversationTurnId::new("child_turn_1"),
        &provenance("child_progress", 10),
    )
    .expect("child-local turn should produce a progress patch");

    assert_eq!(progress.checklist_item_id, item_id);
    assert_eq!(progress.record_id.as_str(), "record");
    assert!(progress.patch.operations().iter().any(|operation| {
        matches!(
            operation,
            SemanticGraphPatchOp::SetChecklistItemStatus {
                node_id,
                status: ChecklistItemStatus::InProgress,
                ..
            } if node_id == &item_id
        )
    }));

    graph.apply_patch(&progress.patch).unwrap();
    assert_eq!(
        graph.node(&item_id).unwrap().checklist_item_status(),
        Some(ChecklistItemStatus::InProgress)
    );
    assert_eq!(
        decision_child_progress_patch(
            &graph,
            &state,
            &ConversationThreadId::new("child_thread"),
            &ConversationTurnId::new("child_turn_2"),
            &provenance("child_progress", 11),
        ),
        None
    );
}

#[test]
fn topic_decision_item_plan_creates_decision_checklist_item_under_topic() {
    let topic_id = node_id("topic");
    let mut graph = graph_with_topic(&topic_id);
    let provenance = provenance("start_topic_decision", 20);
    let plan = topic_decision_item_plan(&graph, &topic_id, " Choose queue ", "", &provenance)
        .expect("topic should produce decision item plan");

    assert_eq!(plan.title, "Choose queue");
    assert_eq!(
        plan.summary,
        "Decision item created from topic \"Architecture\"."
    );
    assert!(!plan.reused_existing_item());
    let patch = plan.patch.clone().expect("new item should need a patch");
    assert!(patch.operations().iter().any(|operation| {
        matches!(
            operation,
            SemanticGraphPatchOp::UpsertNode { node, .. }
                if node == &SemanticNodeDraft::new_with_checklist_item_kind(
                    plan.checklist_item_id.clone(),
                    "Choose queue",
                    "Decision item created from topic \"Architecture\".",
                    SemanticNodeFacets::topic_and_checklist_item(),
                    Some(ChecklistItemStatus::Todo),
                    Some(ChecklistItemKind::Decision),
                )
        )
    }));

    graph.apply_patch(&patch).unwrap();
    let item = graph.node(&plan.checklist_item_id).unwrap();
    assert_eq!(item.title(), "Choose queue");
    assert_eq!(
        item.checklist_item_status(),
        Some(ChecklistItemStatus::Todo)
    );
    assert_eq!(
        item.checklist_item_kind(),
        Some(ChecklistItemKind::Decision)
    );
    assert_eq!(graph.parent_id_of(&plan.checklist_item_id), Some(&topic_id));
}

#[test]
fn topic_decision_item_plan_reuses_existing_decision_item_with_same_title() {
    let topic_id = node_id("topic");
    let item_id = node_id("decision_topic_choose_queue");
    let graph = graph_with_topic_decision_item(&topic_id, &item_id, "Choose queue");
    let plan = topic_decision_item_plan(
        &graph,
        &topic_id,
        "Choose   queue",
        "Retry the previously created item.",
        &provenance("start_topic_decision", 21),
    )
    .expect("existing decision item title should be reusable");

    assert_eq!(plan.checklist_item_id, item_id);
    assert!(plan.reused_existing_item());
    assert!(plan.patch.is_none());
}

#[test]
fn topic_decision_item_plan_rejects_duplicate_non_decision_sibling_title() {
    let topic_id = node_id("topic");
    let item_id = node_id("generic_item");
    let graph = graph_with_topic_generic_item(&topic_id, &item_id, "Choose queue");
    let error = topic_decision_item_plan(
        &graph,
        &topic_id,
        "Choose queue",
        "",
        &provenance("start_topic_decision", 22),
    )
    .expect_err("non-decision duplicate title should be rejected");

    assert_eq!(
        error,
        TopicDecisionItemPlanError::DuplicateSiblingTitle {
            existing_node_id: item_id
        }
    );
}

#[test]
fn default_topic_decision_title_is_concise_and_bounded() {
    let topic_id = node_id("topic");
    let graph = graph_with_topic(&topic_id);
    assert_eq!(
        default_topic_decision_title(&graph, &topic_id).as_deref(),
        Some("Decision: Architecture")
    );
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
                    "Pick architecture",
                    "Choose the architecture.",
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

fn graph_with_topic_decision_item(
    topic_id: &SemanticNodeId,
    item_id: &SemanticNodeId,
    title: &str,
) -> SemanticGraph {
    let mut graph = graph_with_topic(topic_id);
    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new_with_checklist_item_kind(
                    item_id.clone(),
                    title,
                    "Existing decision item.",
                    SemanticNodeFacets::topic_and_checklist_item(),
                    Some(ChecklistItemStatus::Todo),
                    Some(ChecklistItemKind::Decision),
                ),
                provenance: provenance("seed", 3),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: item_id.clone(),
                parent_id: Some(topic_id.clone()),
                index: None,
                provenance: provenance("seed", 4),
            },
        ]))
        .unwrap();
    graph
}

fn graph_with_topic_generic_item(
    topic_id: &SemanticNodeId,
    item_id: &SemanticNodeId,
    title: &str,
) -> SemanticGraph {
    let mut graph = graph_with_topic(topic_id);
    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new_with_checklist_item_kind(
                    item_id.clone(),
                    title,
                    "Existing generic item.",
                    SemanticNodeFacets::topic_and_checklist_item(),
                    Some(ChecklistItemStatus::Todo),
                    Some(ChecklistItemKind::Generic),
                ),
                provenance: provenance("seed", 3),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: item_id.clone(),
                parent_id: Some(topic_id.clone()),
                index: None,
                provenance: provenance("seed", 4),
            },
        ]))
        .unwrap();
    graph
}

fn active_decision_state(item_id: &SemanticNodeId) -> ThreadedDecisionState {
    let mut state = ThreadedDecisionState::default();
    state
        .insert_record(ThreadedDecisionRecord::active_branch(
            ThreadedDecisionRecordId::new("record").unwrap(),
            item_id.clone(),
            ConversationThreadId::new("parent_thread"),
            ConversationThreadId::new("child_thread"),
            Some(ConversationTurnId::new("branch_point")),
            ThreadedDecisionOperationId::new("branch_op").unwrap(),
            1,
            provenance("branch", 1),
        ))
        .unwrap();
    state
}

fn active_decision_state_with_bootstrap(item_id: &SemanticNodeId) -> ThreadedDecisionState {
    let mut state = ThreadedDecisionState::default();
    let record_id = ThreadedDecisionRecordId::new("record").unwrap();
    state
        .insert_record(ThreadedDecisionRecord::queued_branch(
            record_id.clone(),
            item_id.clone(),
            ConversationThreadId::new("parent_thread"),
            Some(ConversationTurnId::new("branch_point")),
            ThreadedDecisionOperationId::new("branch_op").unwrap(),
            1,
            provenance("branch", 1),
        ))
        .unwrap();
    state
        .activate_branch_with_bootstrap_turn(
            &record_id,
            ConversationThreadId::new("child_thread"),
            Some(ConversationTurnId::new("bootstrap_turn")),
            Some(ConversationTurnId::new("branch_point")),
            provenance("activate", 2),
        )
        .unwrap();
    state
}

fn node_id(value: &str) -> SemanticNodeId {
    SemanticNodeId::new(value).unwrap()
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
