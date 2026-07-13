use beryl_model::{
    conversation::{ConversationThreadId, ConversationTurnId},
    provenance::MutationProvenance,
    semantic_graph::{
        ChecklistItemStatus, SemanticGraph, SemanticGraphPatch, SemanticGraphPatchOp,
        SemanticNodeId,
    },
    threaded_decision::{ThreadedDecisionOutcome, ThreadedDecisionRecord},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DecisionResolutionChecklistPatch {
    pub(crate) checklist_item_id: SemanticNodeId,
    pub(crate) patch: SemanticGraphPatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DecisionHandoffMessageInput<'a> {
    pub(crate) checklist_item_id: &'a SemanticNodeId,
    pub(crate) checklist_item_title: &'a str,
    pub(crate) child_thread_id: &'a ConversationThreadId,
    pub(crate) parent_thread_id: &'a ConversationThreadId,
    pub(crate) branch_point_turn_id: Option<&'a ConversationTurnId>,
    pub(crate) outcome: ThreadedDecisionOutcome,
    pub(crate) summary: &'a str,
    pub(crate) handoff_message: &'a str,
}

pub(crate) fn decision_handoff_message(input: DecisionHandoffMessageInput<'_>) -> String {
    let checklist_item_title = non_empty(input.checklist_item_title, "Untitled decision item");
    let summary = non_empty(input.summary, "No resolution summary was supplied.");
    let handoff_message = non_empty(input.handoff_message, "No handoff message was supplied.");
    let branch_point = input
        .branch_point_turn_id
        .map(ConversationTurnId::as_str)
        .unwrap_or("unknown");

    format!(
        "This is an automatic handoff from a threaded decision branch.\n\n\
Checklist item: {checklist_item_title} ({checklist_item_id})\n\
Resolution: {outcome}\n\
Resolution summary: {summary}\n\
Parent thread: {parent_thread_id}\n\
Decision branch thread: {child_thread_id}\n\
Branch point turn: {branch_point}\n\n\
Handoff message:\n{handoff_message}",
        checklist_item_id = input.checklist_item_id.as_str(),
        outcome = resolution_outcome_label(input.outcome),
        parent_thread_id = input.parent_thread_id.as_str(),
        child_thread_id = input.child_thread_id.as_str(),
    )
}

pub(crate) fn decision_resolution_checklist_patch(
    graph: &SemanticGraph,
    record: &ThreadedDecisionRecord,
    provenance: &MutationProvenance,
) -> Option<DecisionResolutionChecklistPatch> {
    let node = graph.node(record.checklist_item_id())?;
    if !node.facets().has_checklist_item() {
        return None;
    }

    let checklist_item_id = record.checklist_item_id().clone();
    Some(DecisionResolutionChecklistPatch {
        checklist_item_id: checklist_item_id.clone(),
        patch: SemanticGraphPatch::from_operation(SemanticGraphPatchOp::SetChecklistItemStatus {
            node_id: checklist_item_id,
            status: ChecklistItemStatus::Done,
            provenance: provenance.clone(),
        }),
    })
}

pub(crate) fn resolution_outcome_label(outcome: ThreadedDecisionOutcome) -> &'static str {
    match outcome {
        ThreadedDecisionOutcome::Accepted => "accepted",
        ThreadedDecisionOutcome::Rejected => "rejected",
    }
}

fn non_empty<'a>(value: &'a str, fallback: &'static str) -> &'a str {
    let value = value.trim();
    if value.is_empty() { fallback } else { value }
}
