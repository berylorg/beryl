#[path = "../src/threaded_decision_context.rs"]
mod threaded_decision_context;

use beryl_model::{
    conversation::{ConversationThreadId, ConversationTurnId},
    provenance::{MutationProvenance, MutationSource},
    semantic_graph::{
        ChecklistItemStatus, SemanticGraph, SemanticGraphPatch, SemanticGraphPatchOp,
        SemanticNodeDraft, SemanticNodeFacets, SemanticNodeId,
    },
    threaded_decision::ThreadedDecisionRecordId,
};
use threaded_decision_context::{
    ThreadedDecisionBootstrapContextInput, threaded_decision_bootstrap_context,
};

#[test]
fn decision_bootstrap_context_names_bound_decision_and_workflow() {
    let item_id = node_id("pick_parser");
    let graph = graph_with_decision_item(&item_id);

    let context = threaded_decision_bootstrap_context(ThreadedDecisionBootstrapContextInput {
        graph: &graph,
        checklist_item_id: &item_id,
        checklist_item_title: "Pick parser",
        checklist_item_summary: "Decide which parser architecture to use.",
        planned_parent_topic_id: None,
        parent_thread_id: &thread_id("parent_thread"),
        parent_thread_title: Some("Parent decision planning"),
        parent_thread_summary: Some("Parent thread summary"),
        child_thread_id: &thread_id("child_thread"),
        parent_context_turn_id: Some(&turn_id("branch_point")),
        parent_context_source: Some(
            "User:\nWhich parser should we use?\n\nAssistant:\nCompare parser combinators and a markdown AST parser.",
        ),
        record_id: &record_id("record"),
    });

    assert!(
        context
            .text()
            .starts_with("Beryl threaded-decision branch context:")
    );
    assert!(context.text().contains("Pick parser (pick_parser)"));
    assert!(
        context
            .text()
            .contains("Branch purpose: Decide which parser architecture to use.")
    );
    assert!(
        context
            .text()
            .contains("Graph path: Decisions > Pick parser")
    );
    assert!(context.text().contains("Graph ancestor summaries:"));
    assert!(context.text().contains("Parent thread: parent_thread"));
    assert!(
        context
            .text()
            .contains("Parent thread title: Parent decision planning")
    );
    assert!(
        context
            .text()
            .contains("Parent thread summary: Parent thread summary")
    );
    assert!(
        context
            .text()
            .contains("Child decision thread: child_thread")
    );
    assert!(
        context
            .text()
            .contains("Parent context source turn: branch_point")
    );
    assert!(context.text().contains("Parent context source content:"));
    assert!(
        context
            .text()
            .contains("  > User:\n  > Which parser should we use?")
    );
    assert!(
        context
            .text()
            .contains("  > Assistant:\n  > Compare parser combinators")
    );
    assert!(context.text().contains("Decision record: record"));
    assert!(context.text().contains("bootstrap turn records context"));
    assert!(context.text().contains("outcome accepted or rejected"));
}

#[test]
fn topic_decision_bootstrap_context_projects_planned_item_under_topic() {
    let topic_id = node_id("architecture");
    let item_id = node_id("decision_architecture_choose_queue");
    let graph = graph_with_topic(&topic_id);

    let context = threaded_decision_bootstrap_context(ThreadedDecisionBootstrapContextInput {
        graph: &graph,
        checklist_item_id: &item_id,
        checklist_item_title: "Choose queue backend",
        checklist_item_summary: "Compare queue backends for async work.",
        planned_parent_topic_id: Some(&topic_id),
        parent_thread_id: &thread_id("parent_thread"),
        parent_thread_title: None,
        parent_thread_summary: None,
        child_thread_id: &thread_id("queue_child"),
        parent_context_turn_id: Some(&turn_id("parent_turn_1")),
        parent_context_source: None,
        record_id: &record_id("queue_record"),
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
}

fn graph_with_decision_item(item_id: &SemanticNodeId) -> SemanticGraph {
    let list_id = node_id("decisions");
    let provenance = provenance("seed", 1);
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
                provenance: provenance.clone(),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: list_id.clone(),
                parent_id: None,
                index: None,
                provenance: provenance.clone(),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    item_id.clone(),
                    "Pick parser",
                    "Decide which parser architecture to use.",
                    SemanticNodeFacets::topic_and_checklist_item(),
                    Some(ChecklistItemStatus::Todo),
                ),
                provenance: provenance.clone(),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: item_id.clone(),
                parent_id: Some(list_id),
                index: None,
                provenance,
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

fn node_id(value: &str) -> SemanticNodeId {
    SemanticNodeId::new(value).unwrap()
}

fn thread_id(value: &str) -> ConversationThreadId {
    ConversationThreadId::new(value)
}

fn turn_id(value: &str) -> ConversationTurnId {
    ConversationTurnId::new(value)
}

fn record_id(value: &str) -> ThreadedDecisionRecordId {
    ThreadedDecisionRecordId::new(value).unwrap()
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
