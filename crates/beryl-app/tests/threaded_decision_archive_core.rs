#[path = "../src/threaded_decision_archive_core.rs"]
mod threaded_decision_archive_core;

use beryl_model::{
    conversation::{ConversationThreadId, ConversationTurnId},
    provenance::{MutationProvenance, MutationSource},
    semantic_graph::SemanticNodeId,
    threaded_decision::{
        ThreadedDecisionOperationId, ThreadedDecisionOutcome, ThreadedDecisionRecord,
        ThreadedDecisionRecordId, ThreadedDecisionState, ThreadedDecisionStatus,
    },
};
use threaded_decision_archive_core::{
    archive_operation_id_for_record, child_thread_is_read_only_decision_branch,
    normal_selector_hidden_decision_child_thread_ids, record_needs_child_archive,
};

#[test]
fn checklist_updated_records_are_archive_work() {
    let record_id = record_id("record");
    let mut state = ThreadedDecisionState::default();
    state
        .insert_record(active_record(record_id.clone(), "child"))
        .unwrap();
    resolve_to_checklist_updated(&mut state, &record_id);

    let record = state.record(&record_id).unwrap();
    assert_eq!(record.status(), ThreadedDecisionStatus::ChecklistUpdated);
    assert!(record_needs_child_archive(record));
}

#[test]
fn resolved_child_branches_are_read_only_and_hidden_from_normal_inventory() {
    let closed_id = record_id("closed_record");
    let active_id = record_id("active_record");
    let mut state = ThreadedDecisionState::default();
    state
        .insert_record(active_record(closed_id.clone(), "closed_child"))
        .unwrap();
    resolve_to_checklist_updated(&mut state, &closed_id);
    state
        .mark_archive_pending(
            &closed_id,
            operation_id("archive"),
            provenance("archive", 5),
        )
        .unwrap();
    state
        .mark_closed(&closed_id, provenance("closed", 6))
        .unwrap();
    state
        .insert_record(active_record(active_id, "active_child"))
        .unwrap();

    assert!(child_thread_is_read_only_decision_branch(
        &state,
        &thread_id("closed_child")
    ));
    assert!(!child_thread_is_read_only_decision_branch(
        &state,
        &thread_id("active_child")
    ));
    assert_eq!(
        normal_selector_hidden_decision_child_thread_ids(&state),
        vec![thread_id("closed_child")]
    );
}

#[test]
fn archive_operation_ids_are_stable_and_non_empty() {
    let operation_id = archive_operation_id_for_record(&record_id("Record With Spaces"), 42)
        .expect("operation id");

    assert_eq!(
        operation_id.as_str(),
        "archive_decision_branch_record_with_spaces_42"
    );
}

fn resolve_to_checklist_updated(
    state: &mut ThreadedDecisionState,
    record_id: &ThreadedDecisionRecordId,
) {
    state
        .mark_pending_resolution(
            record_id,
            ThreadedDecisionOutcome::Accepted,
            "Accept the branch.",
            "Use the branch result.",
            operation_id("resolve"),
            provenance("resolve", 2),
        )
        .unwrap();
    state
        .mark_handoff_started(
            record_id,
            Some(turn_id("handoff")),
            provenance("handoff", 3),
        )
        .unwrap();
    state
        .mark_checklist_updated(record_id, turn_id("handoff"), provenance("checklist", 4))
        .unwrap();
}

fn active_record(
    record_id: ThreadedDecisionRecordId,
    child_thread_id: &str,
) -> ThreadedDecisionRecord {
    ThreadedDecisionRecord::active_branch(
        record_id,
        node_id("decision"),
        thread_id("parent"),
        thread_id(child_thread_id),
        Some(turn_id("branch_point")),
        operation_id("branch"),
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
