#[path = "support/tempdir.rs"]
mod tempdir_support;

use beryl_app::BerylWorkspacePersistence;
use beryl_model::conversation::{ConversationThreadId, ConversationTurnId};
use beryl_model::provenance::{MutationProvenance, MutationSource};
use beryl_model::semantic_graph::SemanticNodeId;
use beryl_model::threaded_decision::{
    ThreadedDecisionOperationId, ThreadedDecisionOutcome, ThreadedDecisionRecord,
    ThreadedDecisionRecordId, ThreadedDecisionState, ThreadedDecisionStatus,
};
use beryl_model::workspace::{BerylWorkspaceId, BerylWorkspaceManifest};

#[test]
fn missing_threaded_decision_state_loads_as_empty() {
    let root = tempdir_support::temp_dir("beryl-threaded-decision-empty-");
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("workspace").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Workspace", 1);

    persistence.save_workspace_manifest(&manifest).unwrap();

    let loaded = persistence
        .load_workspace_threaded_decision_state(&workspace_id)
        .unwrap();

    assert!(loaded.records().is_empty());
    root.close().unwrap();
}

#[test]
fn threaded_decision_state_roundtrips_active_partial_closed_and_superseded_records() {
    let root = tempdir_support::temp_dir("beryl-threaded-decision-roundtrip-");
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new("workspace").unwrap();
    let manifest = BerylWorkspaceManifest::named(workspace_id.clone(), "Workspace", 1);
    let item_id = node_id("decision");
    let active_id = record_id("active");
    let partial_id = record_id("partial");
    let closed_id = record_id("closed");
    let superseding_id = record_id("superseding");
    let mut state = ThreadedDecisionState::default();

    state
        .insert_record(active_record_with_id(
            active_id,
            &item_id,
            "active_child",
            1,
        ))
        .unwrap();
    state
        .insert_record(active_record_with_id(
            partial_id.clone(),
            &node_id("partial_decision"),
            "partial_child",
            10,
        ))
        .unwrap();
    state
        .mark_pending_resolution(
            &partial_id,
            ThreadedDecisionOutcome::Accepted,
            "Accepted partial",
            "Pending parent handoff",
            operation_id("partial_resolve"),
            provenance("resolve", 11),
        )
        .unwrap();
    state
        .mark_handoff_started(&partial_id, None, provenance("handoff", 12))
        .unwrap();
    state
        .insert_record(active_record_with_id(
            closed_id.clone(),
            &node_id("closed_decision"),
            "closed_child",
            20,
        ))
        .unwrap();
    resolve_and_close(&mut state, &closed_id, 21);
    state
        .insert_record(active_record_with_id(
            superseding_id.clone(),
            &node_id("closed_decision"),
            "superseding_child",
            30,
        ))
        .unwrap();
    state
        .supersede_closed_records_for_item(
            &node_id("closed_decision"),
            superseding_id,
            provenance("supersede", 31),
        )
        .unwrap();

    persistence.save_workspace_manifest(&manifest).unwrap();
    persistence
        .save_workspace_threaded_decision_state(&workspace_id, &state)
        .unwrap();

    let loaded = persistence
        .load_workspace_threaded_decision_state(&workspace_id)
        .unwrap();

    assert_eq!(loaded, state);
    assert_eq!(
        loaded.record(&partial_id).unwrap().status(),
        ThreadedDecisionStatus::HandoffStarted
    );
    assert_eq!(
        loaded.record(&closed_id).unwrap().status(),
        ThreadedDecisionStatus::Superseded
    );
    root.close().unwrap();
}

fn resolve_and_close(
    state: &mut ThreadedDecisionState,
    record_id: &ThreadedDecisionRecordId,
    start_at_millis: u64,
) {
    state
        .mark_pending_resolution(
            record_id,
            ThreadedDecisionOutcome::Rejected,
            "Rejected",
            "Rejected handoff",
            operation_id("resolve"),
            provenance("resolve", start_at_millis),
        )
        .unwrap();
    state
        .mark_handoff_started(
            record_id,
            Some(turn_id("handoff")),
            provenance("handoff", start_at_millis + 1),
        )
        .unwrap();
    state
        .mark_checklist_updated(
            record_id,
            turn_id("handoff"),
            provenance("checklist", start_at_millis + 2),
        )
        .unwrap();
    state
        .mark_archive_pending(
            record_id,
            operation_id("archive"),
            provenance("archive", start_at_millis + 3),
        )
        .unwrap();
    state
        .mark_closed(record_id, provenance("closed", start_at_millis + 4))
        .unwrap();
}

fn active_record_with_id(
    record_id: ThreadedDecisionRecordId,
    item_id: &SemanticNodeId,
    child_thread_id: &str,
    created_at_millis: u64,
) -> ThreadedDecisionRecord {
    ThreadedDecisionRecord::active_branch(
        record_id,
        item_id.clone(),
        thread_id("parent"),
        thread_id(child_thread_id),
        Some(turn_id("branch_point")),
        operation_id("branch"),
        created_at_millis,
        provenance("branch", created_at_millis),
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
