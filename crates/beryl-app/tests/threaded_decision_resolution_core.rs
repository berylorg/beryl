#[path = "../src/threaded_decision_resolution_core.rs"]
mod threaded_decision_resolution_core;

use beryl_model::{
    conversation::{ConversationThreadId, ConversationTurnId},
    provenance::{MutationProvenance, MutationSource},
    semantic_graph::{
        ChecklistItemStatus, SemanticGraph, SemanticGraphPatch, SemanticGraphPatchOp,
        SemanticNodeDraft, SemanticNodeFacets, SemanticNodeId,
    },
    threaded_decision::{
        ThreadedDecisionOperationId, ThreadedDecisionOutcome, ThreadedDecisionRecord,
        ThreadedDecisionRecordId,
    },
};
use threaded_decision_resolution_core::{
    DecisionHandoffMessageInput, decision_handoff_message, decision_resolution_checklist_patch,
};

#[test]
fn handoff_message_names_decision_identity_and_resolution() {
    let message = decision_handoff_message(DecisionHandoffMessageInput {
        checklist_item_id: &node_id("pick_storage"),
        checklist_item_title: "Pick storage backend",
        child_thread_id: &thread_id("child_thread"),
        parent_thread_id: &thread_id("parent_thread"),
        branch_point_turn_id: Some(&turn_id("branch_point")),
        outcome: ThreadedDecisionOutcome::Accepted,
        summary: "Use SQLite for the first slice.",
        handoff_message: "The child branch compared flat files and SQLite. Use SQLite.",
    });

    assert!(message.contains("automatic handoff from a threaded decision branch"));
    assert!(message.contains("Checklist item: Pick storage backend (pick_storage)"));
    assert!(message.contains("Resolution: accepted"));
    assert!(message.contains("Resolution summary: Use SQLite for the first slice."));
    assert!(message.contains("Parent thread: parent_thread"));
    assert!(message.contains("Decision branch thread: child_thread"));
    assert!(message.contains("Branch point turn: branch_point"));
    assert!(message.ends_with("The child branch compared flat files and SQLite. Use SQLite."));
}

#[test]
fn decision_resolution_patch_marks_checklist_item_done() {
    let item_id = node_id("pick_storage");
    let mut graph = graph_with_checklist_item(&item_id);
    let record = active_record(&item_id);
    let patch = decision_resolution_checklist_patch(&graph, &record, &provenance("resolve", 10))
        .expect("decision checklist item should produce completion patch");

    assert_eq!(patch.checklist_item_id, item_id);
    assert!(patch.patch.operations().iter().any(|operation| {
        matches!(
            operation,
            SemanticGraphPatchOp::SetChecklistItemStatus {
                node_id,
                status: ChecklistItemStatus::Done,
                ..
            } if node_id.as_str() == "pick_storage"
        )
    }));

    graph.apply_patch(&patch.patch).unwrap();
    assert_eq!(
        graph
            .node(&node_id("pick_storage"))
            .unwrap()
            .checklist_item_status(),
        Some(ChecklistItemStatus::Done)
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
                    "Decision list",
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
                    "Choose storage.",
                    SemanticNodeFacets::topic_and_checklist_item(),
                    Some(ChecklistItemStatus::InProgress),
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

fn active_record(item_id: &SemanticNodeId) -> ThreadedDecisionRecord {
    ThreadedDecisionRecord::active_branch(
        ThreadedDecisionRecordId::new("record").unwrap(),
        item_id.clone(),
        thread_id("parent_thread"),
        thread_id("child_thread"),
        Some(turn_id("branch_point")),
        ThreadedDecisionOperationId::new("branch_op").unwrap(),
        1,
        provenance("branch", 1),
    )
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

fn node_id(value: &str) -> SemanticNodeId {
    SemanticNodeId::new(value).unwrap()
}

fn thread_id(value: &str) -> ConversationThreadId {
    ConversationThreadId::new(value)
}

fn turn_id(value: &str) -> ConversationTurnId {
    ConversationTurnId::new(value)
}
