use beryl_model::conversation::{
    ConversationThreadId, ConversationTurnId, RegisteredConversationThread,
    WorkspaceConversationState,
};
use beryl_model::provenance::{MutationProvenance, MutationSource};
use beryl_model::semantic_graph::{
    ChecklistItemStatus, SemanticGraph, SemanticGraphPatch, SemanticGraphPatchOp,
    SemanticNodeDraft, SemanticNodeFacets, SemanticNodeId,
};
use beryl_model::threaded_decision::{
    ThreadedDecisionArchiveState, ThreadedDecisionOperationId, ThreadedDecisionOutcome,
    ThreadedDecisionRecord, ThreadedDecisionRecordId, ThreadedDecisionState,
    ThreadedDecisionStateError, ThreadedDecisionStatus,
};
use beryl_model::workspace::{RuntimeMode, WorkspaceId};

#[test]
fn state_rejects_duplicate_active_branch_for_checklist_item() {
    let item_id = node_id("decision");
    let mut state = ThreadedDecisionState::default();

    state
        .insert_record(active_record("record_a", &item_id, "child_a", 1))
        .unwrap();
    let error = state
        .insert_record(active_record("record_b", &item_id, "child_b", 2))
        .unwrap_err();

    assert!(matches!(
        error,
        ThreadedDecisionStateError::ActiveBranchExists { .. }
    ));
    assert_eq!(
        state.active_record_for_item(&item_id).unwrap().record_id(),
        &record_id("record_a")
    );
}

#[test]
fn active_record_can_be_found_by_child_thread() {
    let item_id = node_id("decision");
    let mut state = ThreadedDecisionState::default();

    state
        .insert_record(active_record("record_a", &item_id, "child_a", 1))
        .unwrap();

    assert_eq!(
        state
            .active_record_for_child_thread(&thread_id("child_a"))
            .unwrap()
            .record_id(),
        &record_id("record_a")
    );
    assert!(
        state
            .active_record_for_child_thread(&thread_id("parent"))
            .is_none()
    );
}

#[test]
fn record_can_be_found_by_child_thread_after_closure() {
    let item_id = node_id("decision");
    let record_id = record_id("record");
    let mut state = ThreadedDecisionState::default();

    state
        .insert_record(active_record(record_id.as_str(), &item_id, "child_a", 1))
        .unwrap();
    state
        .mark_pending_resolution(
            &record_id,
            ThreadedDecisionOutcome::Accepted,
            "Use it.",
            "Use the child result.",
            operation_id("resolve"),
            provenance("resolve", 2),
        )
        .unwrap();
    state
        .mark_checklist_updated(&record_id, turn_id("handoff"), provenance("checklist", 3))
        .unwrap();
    state
        .mark_archive_pending(
            &record_id,
            operation_id("archive"),
            provenance("archive", 4),
        )
        .unwrap();
    state
        .mark_closed(&record_id, provenance("closed", 5))
        .unwrap();

    assert!(
        state
            .active_record_for_child_thread(&thread_id("child_a"))
            .is_none()
    );
    assert_eq!(
        state
            .record_for_child_thread(&thread_id("child_a"))
            .unwrap()
            .status(),
        ThreadedDecisionStatus::Closed
    );
}

#[test]
fn queued_record_transitions_through_partial_resolution_and_archive_failure() {
    let item_id = node_id("decision");
    let record_id = record_id("record");
    let mut state = ThreadedDecisionState::default();

    state
        .insert_record(ThreadedDecisionRecord::queued_branch(
            record_id.clone(),
            item_id.clone(),
            thread_id("parent"),
            Some(turn_id("branch_point")),
            operation_id("branch_op"),
            1,
            provenance("branch", 1),
        ))
        .unwrap();
    state
        .activate_branch(
            &record_id,
            thread_id("child"),
            None,
            provenance("activate", 2),
        )
        .unwrap();
    state
        .mark_pending_resolution(
            &record_id,
            ThreadedDecisionOutcome::Accepted,
            "Use the focused design.",
            "Carry the selected design back to the parent.",
            operation_id("resolve_op"),
            provenance("resolve", 3),
        )
        .unwrap();
    state
        .mark_handoff_started(&record_id, None, provenance("handoff", 4))
        .unwrap();
    state
        .mark_checklist_updated(
            &record_id,
            turn_id("parent_handoff"),
            provenance("checklist", 5),
        )
        .unwrap();
    state
        .mark_archive_pending(
            &record_id,
            operation_id("archive_op"),
            provenance("archive", 6),
        )
        .unwrap();
    state
        .mark_archive_failed(
            &record_id,
            "backend returned no rollout",
            provenance("archive_failed", 7),
        )
        .unwrap();

    let record = state.record(&record_id).unwrap();
    assert_eq!(record.status(), ThreadedDecisionStatus::ArchiveFailed);
    assert_eq!(record.outcome(), Some(ThreadedDecisionOutcome::Accepted));
    assert_eq!(
        record.handoff_turn_id().map(ConversationTurnId::as_str),
        Some("parent_handoff")
    );
    assert_eq!(
        record.archive_status().state(),
        ThreadedDecisionArchiveState::Failed
    );
    assert_eq!(
        record.archive_status().failure_message(),
        Some("backend returned no rollout")
    );
    assert_eq!(
        state
            .protected_resolved_checklist_item_ids()
            .collect::<Vec<_>>(),
        vec![&item_id]
    );
}

#[test]
fn activation_records_bootstrap_turn_id_when_known() {
    let item_id = node_id("decision");
    let record_id = record_id("record");
    let mut state = ThreadedDecisionState::default();

    state
        .insert_record(ThreadedDecisionRecord::queued_branch(
            record_id.clone(),
            item_id,
            thread_id("parent"),
            Some(turn_id("branch_point")),
            operation_id("branch_op"),
            1,
            provenance("branch", 1),
        ))
        .unwrap();
    state
        .activate_branch_with_bootstrap_turn(
            &record_id,
            thread_id("child"),
            Some(turn_id("bootstrap_turn")),
            Some(turn_id("branch_point")),
            provenance("activate", 2),
        )
        .unwrap();

    let record = state.record(&record_id).unwrap();
    assert_eq!(record.child_thread_id(), Some(&thread_id("child")));
    assert_eq!(record.bootstrap_turn_id(), Some(&turn_id("bootstrap_turn")));
    assert_eq!(
        record.branch_point_turn_id(),
        Some(&turn_id("branch_point"))
    );
}

#[test]
fn remove_record_clears_failed_branch_job_state() {
    let item_id = node_id("decision");
    let record_id = record_id("record");
    let mut state = ThreadedDecisionState::default();

    state
        .insert_record(ThreadedDecisionRecord::queued_branch(
            record_id.clone(),
            item_id,
            thread_id("parent"),
            Some(turn_id("branch_point")),
            operation_id("branch_op"),
            1,
            provenance("branch", 1),
        ))
        .unwrap();

    assert!(state.remove_record(&record_id));
    assert!(state.records().is_empty());
    assert!(!state.remove_record(&record_id));
}

#[test]
fn closed_records_can_be_superseded_without_losing_original_outcome() {
    let item_id = node_id("decision");
    let old_record_id = record_id("old_record");
    let new_record_id = record_id("new_record");
    let mut state = ThreadedDecisionState::default();

    state
        .insert_record(active_record_with_id(
            old_record_id.clone(),
            &item_id,
            "old_child",
            1,
        ))
        .unwrap();
    resolve_and_close(&mut state, &old_record_id);
    state
        .insert_record(active_record_with_id(
            new_record_id.clone(),
            &item_id,
            "new_child",
            10,
        ))
        .unwrap();

    assert!(
        state
            .supersede_closed_records_for_item(
                &item_id,
                new_record_id.clone(),
                provenance("supersede", 20),
            )
            .unwrap()
    );

    let old_record = state.record(&old_record_id).unwrap();
    assert_eq!(old_record.status(), ThreadedDecisionStatus::Superseded);
    assert_eq!(
        old_record.outcome(),
        Some(ThreadedDecisionOutcome::Rejected)
    );
    assert_eq!(
        old_record.supersession().unwrap().superseded_by_record_id(),
        &new_record_id
    );
}

#[test]
fn reference_reconciliation_invalidates_missing_semantic_nodes() {
    let item_id = node_id("decision");
    let record_id = record_id("record");
    let mut state = ThreadedDecisionState::default();
    let workspace_state = workspace_state_with_threads(["parent", "child"]);

    state
        .insert_record(active_record_with_id(
            record_id.clone(),
            &item_id,
            "child",
            1,
        ))
        .unwrap();

    assert!(state.reconcile_references(
        &SemanticGraph::default(),
        &workspace_state,
        provenance("reconcile", 2),
    ));

    let record = state.record(&record_id).unwrap();
    assert_eq!(record.status(), ThreadedDecisionStatus::Invalidated);
    assert!(record.invalidation().is_some());
}

#[test]
fn reference_reconciliation_invalidates_missing_child_threads() {
    let item_id = node_id("decision");
    let record_id = record_id("record");
    let mut state = ThreadedDecisionState::default();
    let graph = graph_with_item(&item_id);
    let workspace_state = workspace_state_with_threads(["parent"]);

    state
        .insert_record(active_record_with_id(
            record_id.clone(),
            &item_id,
            "child",
            1,
        ))
        .unwrap();

    assert!(state.reconcile_references(&graph, &workspace_state, provenance("reconcile", 2)));
    assert_eq!(
        state.record(&record_id).unwrap().status(),
        ThreadedDecisionStatus::Invalidated
    );
}

fn resolve_and_close(state: &mut ThreadedDecisionState, record_id: &ThreadedDecisionRecordId) {
    state
        .mark_pending_resolution(
            record_id,
            ThreadedDecisionOutcome::Rejected,
            "Do not take this path.",
            "Rejected after exploration.",
            operation_id("resolve_op"),
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
    state
        .mark_archive_pending(
            record_id,
            operation_id("archive_op"),
            provenance("archive", 5),
        )
        .unwrap();
    state
        .mark_closed(record_id, provenance("closed", 6))
        .unwrap();
}

fn active_record(
    record_id: &str,
    item_id: &SemanticNodeId,
    child_thread_id: &str,
    created_at_millis: u64,
) -> ThreadedDecisionRecord {
    active_record_with_id(
        self::record_id(record_id),
        item_id,
        child_thread_id,
        created_at_millis,
    )
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
        operation_id("branch_op"),
        created_at_millis,
        provenance("branch", created_at_millis),
    )
}

fn workspace_state_with_threads<const N: usize>(
    thread_ids: [&str; N],
) -> WorkspaceConversationState {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let mut state = WorkspaceConversationState::default();
    state.select_runtime(RuntimeMode::HostWindows).unwrap();
    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    for thread_id in thread_ids {
        state.remember_thread(RegisteredConversationThread::new(
            self::thread_id(thread_id),
            execution_target.clone(),
            "Preview",
            None,
            1,
            1,
        ));
    }
    state
}

fn graph_with_item(item_id: &SemanticNodeId) -> SemanticGraph {
    let list_id = node_id("list");
    let mut graph = SemanticGraph::default();
    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    list_id.clone(),
                    "List",
                    "Decision list",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: provenance("graph", 1),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: list_id.clone(),
                parent_id: None,
                index: None,
                provenance: provenance("graph", 2),
            },
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    item_id.clone(),
                    "Decision",
                    "Decision item",
                    SemanticNodeFacets::topic_and_checklist_item(),
                    Some(ChecklistItemStatus::Todo),
                ),
                provenance: provenance("graph", 3),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: item_id.clone(),
                parent_id: Some(list_id),
                index: None,
                provenance: provenance("graph", 4),
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
