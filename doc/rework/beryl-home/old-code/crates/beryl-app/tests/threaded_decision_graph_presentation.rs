#[path = "../src/threaded_decision_graph_presentation.rs"]
mod threaded_decision_graph_presentation;

use beryl_model::{
    conversation::{ConversationThreadId, ConversationTurnId},
    provenance::{MutationProvenance, MutationSource},
    semantic_graph::{
        ChecklistItemKind, ChecklistItemStatus, SemanticGraph, SemanticGraphPatch,
        SemanticGraphPatchOp, SemanticNodeDraft, SemanticNodeFacets, SemanticNodeId,
        ThreadRefDraft, ThreadRefId,
    },
    threaded_decision::{
        ThreadedDecisionOperationId, ThreadedDecisionOutcome, ThreadedDecisionRecord,
        ThreadedDecisionRecordId, ThreadedDecisionState, ThreadedDecisionStatus,
    },
    workspace::WorkspaceId,
};
use threaded_decision_graph_presentation::{
    DecisionGraphBadgeTone, active_decision_branch_record_for_item, archive_retry_record_for_item,
    checklist_update_retry_record_for_item, decision_branch_start_label, decision_item_badges,
    decision_thread_ref_badge, latest_handoff_record_for_item,
};

#[test]
fn active_decision_item_presents_kind_and_active_branch() {
    let item_id = node_id("decision_item");
    let graph = graph_with_decision_item(&item_id);
    let state = active_state(&item_id, "record", "child_thread", 10);
    let node = graph.node(&item_id).unwrap();

    let badges = decision_item_badges(node, &state);

    assert_eq!(badge_labels(&badges), vec!["decision", "active"]);
    assert_eq!(badges[1].tone(), DecisionGraphBadgeTone::Pending);
    assert_eq!(
        active_decision_branch_record_for_item(&state, &item_id)
            .unwrap()
            .child_thread_id()
            .unwrap()
            .as_str(),
        "child_thread"
    );
    assert_eq!(
        decision_branch_start_label(&state, &item_id),
        "Start Decision Branch"
    );
}

#[test]
fn closed_decision_history_presents_outcome_and_superseding_start_label() {
    let item_id = node_id("decision_item");
    let graph = graph_with_decision_item(&item_id);
    let mut state = active_state(&item_id, "old_record", "old_child", 10);
    close_record(
        &mut state,
        "old_record",
        ThreadedDecisionOutcome::Rejected,
        20,
    );

    state
        .insert_record(ThreadedDecisionRecord::active_branch(
            record_id("new_record"),
            item_id.clone(),
            thread_id("parent_thread"),
            thread_id("new_child"),
            Some(turn_id("branch_point_2")),
            operation_id("new_branch_op"),
            30,
            provenance("branch_again", 30),
        ))
        .unwrap();
    close_record(
        &mut state,
        "new_record",
        ThreadedDecisionOutcome::Accepted,
        40,
    );
    state
        .supersede_closed_records_for_item(
            &item_id,
            record_id("new_record"),
            provenance("supersede", 50),
        )
        .unwrap();

    let node = graph.node(&item_id).unwrap();
    let badges = decision_item_badges(node, &state);

    assert_eq!(
        badge_labels(&badges),
        vec!["decision", "accepted", "history"]
    );
    assert_eq!(
        decision_branch_start_label(&state, &item_id),
        "Start Superseding Branch"
    );
    assert_eq!(
        latest_handoff_record_for_item(&state, &item_id)
            .unwrap()
            .handoff_turn_id()
            .unwrap()
            .as_str(),
        "handoff_new_record"
    );
}

#[test]
fn decision_thread_ref_badge_uses_decision_binding_not_generic_refs() {
    let item_id = node_id("decision_item");
    let state = active_state(&item_id, "record", "child_thread", 10);
    let mut graph = graph_with_decision_item(&item_id);
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertThreadRef {
                thread_ref: ThreadRefDraft::new(
                    ThreadRefId::new("decision_ref").unwrap(),
                    item_id.clone(),
                    thread_id("child_thread"),
                    execution_target.clone(),
                    "Decision branch",
                ),
                provenance: provenance("thread_ref", 20),
            },
            SemanticGraphPatchOp::UpsertThreadRef {
                thread_ref: ThreadRefDraft::new(
                    ThreadRefId::new("generic_ref").unwrap(),
                    item_id,
                    thread_id("generic_thread"),
                    execution_target,
                    "Generic thread",
                ),
                provenance: provenance("thread_ref", 21),
            },
        ]))
        .unwrap();

    assert_eq!(
        decision_thread_ref_badge(
            &state,
            graph
                .thread_ref(&ThreadRefId::new("decision_ref").unwrap())
                .unwrap()
        )
        .unwrap()
        .label(),
        "active"
    );
    assert!(
        decision_thread_ref_badge(
            &state,
            graph
                .thread_ref(&ThreadRefId::new("generic_ref").unwrap())
                .unwrap()
        )
        .is_none()
    );
}

#[test]
fn retry_selectors_find_partial_resolution_and_archive_failure() {
    let item_id = node_id("decision_item");
    let mut state = active_state(&item_id, "checklist_retry", "child_thread", 10);
    state
        .mark_pending_resolution(
            &record_id("checklist_retry"),
            ThreadedDecisionOutcome::Accepted,
            "Use the branch.",
            "Parent handoff.",
            operation_id("resolve_retry"),
            provenance("resolve", 20),
        )
        .unwrap();
    state
        .mark_handoff_started(
            &record_id("checklist_retry"),
            Some(turn_id("handoff_retry")),
            provenance("handoff", 30),
        )
        .unwrap();

    let archive_item_id = node_id("archive_item");
    let mut archive_state = active_state(&archive_item_id, "archive_retry", "archive_child", 40);
    close_record_until_checklist_updated(
        &mut archive_state,
        "archive_retry",
        ThreadedDecisionOutcome::Rejected,
        50,
    );
    archive_state
        .mark_archive_pending(
            &record_id("archive_retry"),
            operation_id("archive_op"),
            provenance("archive", 60),
        )
        .unwrap();
    archive_state
        .mark_archive_failed(
            &record_id("archive_retry"),
            "backend refused archive",
            provenance("archive_failed", 70),
        )
        .unwrap();
    assert_eq!(
        checklist_update_retry_record_for_item(&state, &item_id)
            .unwrap()
            .record_id()
            .as_str(),
        "checklist_retry"
    );
    assert_eq!(
        archive_retry_record_for_item(&archive_state, &archive_item_id)
            .unwrap()
            .record_id()
            .as_str(),
        "archive_retry"
    );
    assert_eq!(
        archive_state
            .record(&record_id("archive_retry"))
            .unwrap()
            .status(),
        ThreadedDecisionStatus::ArchiveFailed
    );
}

fn badge_labels(
    badges: &[threaded_decision_graph_presentation::DecisionGraphBadge],
) -> Vec<&'static str> {
    badges.iter().map(|badge| badge.label()).collect()
}

fn graph_with_decision_item(item_id: &SemanticNodeId) -> SemanticGraph {
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
                node: SemanticNodeDraft::new_with_checklist_item_kind(
                    item_id.clone(),
                    "Pick architecture",
                    "Choose the architecture.",
                    SemanticNodeFacets::topic_and_checklist_item(),
                    Some(ChecklistItemStatus::Done),
                    Some(ChecklistItemKind::Decision),
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

fn active_state(
    item_id: &SemanticNodeId,
    record: &str,
    child: &str,
    created_at: u64,
) -> ThreadedDecisionState {
    let mut state = ThreadedDecisionState::default();
    state
        .insert_record(ThreadedDecisionRecord::active_branch(
            record_id(record),
            item_id.clone(),
            thread_id("parent_thread"),
            thread_id(child),
            Some(turn_id("branch_point")),
            operation_id(format!("{record}_branch_op")),
            created_at,
            provenance("branch", created_at),
        ))
        .unwrap();
    state
}

fn close_record(
    state: &mut ThreadedDecisionState,
    record: &str,
    outcome: ThreadedDecisionOutcome,
    timestamp: u64,
) {
    close_record_until_checklist_updated(state, record, outcome, timestamp);
    state
        .mark_archive_pending(
            &record_id(record),
            operation_id(format!("{record}_archive_op")),
            provenance("archive", timestamp + 3),
        )
        .unwrap();
    state
        .mark_closed(&record_id(record), provenance("closed", timestamp + 4))
        .unwrap();
}

fn close_record_until_checklist_updated(
    state: &mut ThreadedDecisionState,
    record: &str,
    outcome: ThreadedDecisionOutcome,
    timestamp: u64,
) {
    state
        .mark_pending_resolution(
            &record_id(record),
            outcome,
            "Summary",
            "Handoff",
            operation_id(format!("{record}_resolve_op")),
            provenance("resolve", timestamp),
        )
        .unwrap();
    state
        .mark_handoff_started(
            &record_id(record),
            Some(turn_id(format!("handoff_{record}"))),
            provenance("handoff", timestamp + 1),
        )
        .unwrap();
    state
        .mark_checklist_updated(
            &record_id(record),
            turn_id(format!("handoff_{record}")),
            provenance("checklist", timestamp + 2),
        )
        .unwrap();
}

fn record_id(value: impl Into<String>) -> ThreadedDecisionRecordId {
    ThreadedDecisionRecordId::new(value).unwrap()
}

fn operation_id(value: impl Into<String>) -> ThreadedDecisionOperationId {
    ThreadedDecisionOperationId::new(value).unwrap()
}

fn thread_id(value: &str) -> ConversationThreadId {
    ConversationThreadId::new(value.to_string())
}

fn turn_id(value: impl Into<String>) -> ConversationTurnId {
    ConversationTurnId::new(value.into())
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
