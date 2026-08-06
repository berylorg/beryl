use std::path::PathBuf;

use beryl_backend::ThreadSummary;
use beryl_model::conversation::{
    ConversationThreadId, ConversationThreadMemberBinding, RegisteredConversationThread,
    WorkspaceConversationState,
};
use beryl_model::provenance::{MutationProvenance, MutationSource};
use beryl_model::semantic_graph::{
    SemanticGraph, SemanticGraphPatch, SemanticGraphPatchOp, SemanticNodeDraft, SemanticNodeFacets,
    SemanticNodeId, ThreadRef, ThreadRefDraft, ThreadRefId,
};
use beryl_model::workspace::WorkspaceId;

#[path = "../src/shell/thread_selection.rs"]
mod thread_selection;

use thread_selection::{
    GraphThreadRefAvailability, KnownThreadSelection, ThreadSelectionRequest,
    backend_unavailable_thread_seed, graph_thread_ref_availability,
    persisted_active_thread_disconnect_selection_request, persisted_active_thread_recovery_target,
    persisted_active_thread_selection_request, resolve_known_thread_selection,
};

#[test]
fn exact_thread_selection_is_not_resolved_from_known_inventory() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let known_threads = vec![sample_thread("thread_a"), sample_thread("thread_b")];

    let selection = resolve_known_thread_selection(
        &known_threads,
        &execution_target,
        &ThreadSelectionRequest::exact("thread_b", "Release review"),
    );

    assert_eq!(selection, KnownThreadSelection::None);
}

#[test]
fn preferred_thread_selection_can_fall_back_to_the_first_known_thread() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let known_threads = vec![sample_thread("thread_a"), sample_thread("thread_b")];

    let selection = resolve_known_thread_selection(
        &known_threads,
        &execution_target,
        &ThreadSelectionRequest::RestorePreferred(Some("missing_thread".to_string())),
    );

    assert_eq!(
        selection,
        KnownThreadSelection::Selected {
            thread_id: "thread_a".to_string(),
            strict: false,
        }
    );
}

#[test]
fn persisted_active_thread_becomes_exact_recovery_for_its_recorded_target() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_a");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    workspace_state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target.clone(),
        "Persisted preview",
        Some("Persisted title".to_string()),
        1,
        2,
    ));
    workspace_state.activate_thread(&thread_id).unwrap();

    assert_eq!(
        persisted_active_thread_selection_request(&workspace_state, &execution_target),
        Some(ThreadSelectionRequest::Exact {
            thread_id: "thread_a".to_string(),
            label: "Persisted title".to_string(),
            expected_forked_from_id: None,
        })
    );
}

#[test]
fn persisted_phase_child_recovery_requires_backend_root_parent() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let root_id = ConversationThreadId::new("thread_root");
    let child_id = ConversationThreadId::new("thread_child");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    for thread_id in [&root_id, &child_id] {
        workspace_state.remember_thread(RegisteredConversationThread::new(
            thread_id.clone(),
            execution_target.clone(),
            format!("{} preview", thread_id.as_str()),
            None,
            1,
            2,
        ));
    }
    workspace_state
        .record_thread_as_orchestration_root(&root_id)
        .unwrap();
    workspace_state
        .record_thread_orchestration_root(&child_id, &root_id)
        .unwrap();
    workspace_state.activate_thread(&child_id).unwrap();

    assert_eq!(
        persisted_active_thread_selection_request(&workspace_state, &execution_target),
        Some(ThreadSelectionRequest::Exact {
            thread_id: "thread_child".to_string(),
            label: "thread_child preview".to_string(),
            expected_forked_from_id: Some("thread_root".to_string()),
        })
    );
}

#[test]
fn disconnect_recovery_preserves_phase_child_root_validation() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let root_id = ConversationThreadId::new("thread_root");
    let child_id = ConversationThreadId::new("thread_child");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    for thread_id in [&root_id, &child_id] {
        workspace_state.remember_thread(RegisteredConversationThread::new(
            thread_id.clone(),
            execution_target.clone(),
            format!("{} preview", thread_id.as_str()),
            None,
            1,
            2,
        ));
    }
    workspace_state
        .record_thread_as_orchestration_root(&root_id)
        .unwrap();
    workspace_state
        .record_thread_orchestration_root(&child_id, &root_id)
        .unwrap();
    workspace_state.activate_thread(&child_id).unwrap();

    assert_eq!(
        persisted_active_thread_disconnect_selection_request(
            &workspace_state,
            &execution_target,
            child_id.as_str(),
        ),
        Some(ThreadSelectionRequest::Exact {
            thread_id: "thread_child".to_string(),
            label: "thread_child preview".to_string(),
            expected_forked_from_id: Some("thread_root".to_string()),
        })
    );
    assert_eq!(
        persisted_active_thread_disconnect_selection_request(
            &workspace_state,
            &execution_target,
            "different_surface_thread",
        ),
        None
    );
}

#[test]
fn backend_unavailable_seed_never_selects_unvalidated_persisted_identity() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_unavailable");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    workspace_state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target,
        "Persisted preview",
        None,
        1,
        2,
    ));
    workspace_state.activate_thread(&thread_id).unwrap();

    let (known_threads, selected_thread_id) = backend_unavailable_thread_seed();

    assert!(known_threads.is_empty());
    assert_eq!(selected_thread_id, None);
    assert_eq!(workspace_state.active_thread(), Some(&thread_id));
}

#[test]
fn binding_mismatch_requires_repair_instead_of_disconnect_recovery() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_mismatch");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    workspace_state.remember_thread(
        RegisteredConversationThread::new(
            thread_id.clone(),
            execution_target.clone(),
            "Persisted preview",
            None,
            1,
            2,
        )
        .with_member_binding(ConversationThreadMemberBinding::implicit_home(
            execution_target.clone(),
        )),
    );
    workspace_state.activate_thread(&thread_id).unwrap();

    let selection = persisted_active_thread_disconnect_selection_request(
        &workspace_state,
        &execution_target,
        thread_id.as_str(),
    )
    .unwrap();
    let ThreadSelectionRequest::PersistedActiveRepairRequired { detail, .. } = selection else {
        panic!("mismatched binding must remain repair-required");
    };
    assert!(detail.contains("no longer matches"));
    assert_eq!(workspace_state.active_thread(), Some(&thread_id));
}

#[test]
fn persisted_active_thread_is_not_recovered_for_another_target() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_a");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    workspace_state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target,
        "Persisted preview",
        None,
        1,
        2,
    ));
    workspace_state.activate_thread(&thread_id).unwrap();

    assert_eq!(
        persisted_active_thread_selection_request(
            &workspace_state,
            &WorkspaceId::host_windows(r"C:\work\other"),
        ),
        None
    );
}

#[test]
fn persisted_active_thread_routes_startup_to_its_non_primary_member() {
    let primary_target = WorkspaceId::host_windows(r"C:\work\primary");
    let active_target = WorkspaceId::host_windows(r"C:\work\phase");
    let thread_id = ConversationThreadId::new("thread_phase");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&primary_target)
        .unwrap();
    workspace_state
        .attach_execution_target(&active_target)
        .unwrap();
    workspace_state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        active_target.clone(),
        "Phase preview",
        None,
        1,
        2,
    ));
    workspace_state.activate_thread(&thread_id).unwrap();

    assert_eq!(
        persisted_active_thread_recovery_target(&workspace_state),
        Some(active_target)
    );
}

#[test]
fn legacy_registration_without_member_binding_is_not_exact_startup_authority() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_a");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    workspace_state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target,
        "Persisted preview",
        None,
        1,
        2,
    ));
    workspace_state.activate_thread(&thread_id).unwrap();
    let mut value = serde_json::to_value(workspace_state).unwrap();
    value["threads"][0]
        .as_object_mut()
        .unwrap()
        .remove("member_binding");
    let workspace_state: WorkspaceConversationState = serde_json::from_value(value).unwrap();

    assert_eq!(
        persisted_active_thread_recovery_target(&workspace_state),
        None
    );
    let selection = persisted_active_thread_disconnect_selection_request(
        &workspace_state,
        &WorkspaceId::host_windows(r"C:\work\beryl"),
        thread_id.as_str(),
    )
    .unwrap();
    let ThreadSelectionRequest::PersistedActiveRepairRequired { detail, .. } = selection else {
        panic!("missing binding must remain repair-required");
    };
    assert!(detail.contains("does not include an exact workspace-member binding"));
}

#[test]
fn explicit_rebind_requirement_blocks_disconnect_exact_recovery() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_rebind");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    workspace_state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target.clone(),
        "Persisted preview",
        None,
        1,
        2,
    ));
    workspace_state.activate_thread(&thread_id).unwrap();
    workspace_state
        .mark_thread_rebind_required(&thread_id, "Original member detached")
        .unwrap();

    let selection = persisted_active_thread_disconnect_selection_request(
        &workspace_state,
        &execution_target,
        thread_id.as_str(),
    )
    .unwrap();
    let ThreadSelectionRequest::PersistedActiveRepairRequired { detail, .. } = selection else {
        panic!("explicit rebind must remain repair-required");
    };
    assert!(detail.contains("Original member detached"));
}

#[test]
fn graph_thread_ref_is_openable_when_target_is_in_workspace_scope() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    let thread_ref = sample_thread_ref(&execution_target);

    assert_eq!(
        graph_thread_ref_availability(&workspace_state, &thread_ref, None),
        GraphThreadRefAvailability::Openable
    );
}

#[test]
fn graph_thread_ref_is_invalid_when_target_is_outside_workspace_scope() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&WorkspaceId::host_windows(r"C:\work\other"))
        .unwrap();
    let thread_ref = sample_thread_ref(&execution_target);

    let availability = graph_thread_ref_availability(&workspace_state, &thread_ref, None);

    assert!(matches!(
        availability,
        GraphThreadRefAvailability::Invalid {
            notice_title: "Thread link unavailable",
            ..
        }
    ));
    assert!(
        availability
            .detail()
            .unwrap()
            .contains("outside the current workspace scope")
    );
}

#[test]
fn graph_thread_ref_rebind_requirement_takes_precedence_over_scope() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_a");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    workspace_state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target.clone(),
        "Preview",
        Some("Thread A".to_string()),
        1,
        2,
    ));
    workspace_state
        .mark_thread_rebind_required(&thread_id, "Original member detached")
        .unwrap();
    let thread_ref = sample_thread_ref_with_thread(&execution_target, thread_id);

    let availability = graph_thread_ref_availability(&workspace_state, &thread_ref, None);

    assert!(matches!(
        availability,
        GraphThreadRefAvailability::Invalid {
            notice_title: "Thread requires rebind",
            ..
        }
    ));
    assert!(
        availability
            .detail()
            .unwrap()
            .contains("Original member detached")
    );
}

#[test]
fn graph_thread_ref_implicit_home_scope_requires_exact_home_target() {
    let home_target = WorkspaceId::host_windows(r"C:\Users\operator");
    let missing_member_target = WorkspaceId::host_windows(r"C:\work\missing");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .select_runtime(beryl_model::workspace::RuntimeMode::HostWindows)
        .unwrap();

    let missing_ref = sample_thread_ref(&missing_member_target);
    let home_ref = sample_thread_ref(&home_target);

    assert!(matches!(
        graph_thread_ref_availability(&workspace_state, &missing_ref, Some(&home_target)),
        GraphThreadRefAvailability::Invalid {
            notice_title: "Thread link unavailable",
            ..
        }
    ));
    assert_eq!(
        graph_thread_ref_availability(&workspace_state, &home_ref, Some(&home_target)),
        GraphThreadRefAvailability::Openable
    );
}

#[test]
fn graph_thread_ref_opens_after_implicit_home_rebind_restoration() {
    let home_target = WorkspaceId::host_windows(r"C:\Users\operator");
    let explicit_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_home");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .select_runtime(beryl_model::workspace::RuntimeMode::HostWindows)
        .unwrap();
    workspace_state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        home_target.clone(),
        "Home preview",
        None,
        1,
        2,
    ));
    workspace_state
        .attach_execution_target(&explicit_target)
        .unwrap();
    let explicit_member_id = workspace_state
        .primary_explicit_member()
        .unwrap()
        .id()
        .clone();
    let thread_ref = sample_thread_ref_with_thread(&home_target, thread_id);
    assert!(matches!(
        graph_thread_ref_availability(&workspace_state, &thread_ref, Some(&home_target)),
        GraphThreadRefAvailability::Invalid {
            notice_title: "Thread requires rebind",
            ..
        }
    ));

    workspace_state
        .detach_explicit_member(&explicit_member_id)
        .unwrap();
    workspace_state.restore_implicit_home_threads_for_execution_target(&home_target);

    assert_eq!(
        graph_thread_ref_availability(&workspace_state, &thread_ref, Some(&home_target)),
        GraphThreadRefAvailability::Openable
    );
}

fn sample_thread(id: &str) -> ThreadSummary {
    ThreadSummary {
        id: id.to_string(),
        forked_from_id: None,
        cwd: PathBuf::from(r"C:\work\beryl"),
        preview: format!("Preview for {id}"),
        name: Some(format!("Thread {id}")),
        agent_nickname: None,
        path: None,
        created_at: 1,
        updated_at: 2,
        model_provider: "openai".to_string(),
        ephemeral: false,
    }
}

fn sample_thread_ref(execution_target: &WorkspaceId) -> ThreadRef {
    sample_thread_ref_with_thread(execution_target, ConversationThreadId::new("thread_a"))
}

fn sample_thread_ref_with_thread(
    execution_target: &WorkspaceId,
    thread_id: ConversationThreadId,
) -> ThreadRef {
    let node_id = SemanticNodeId::new("node").unwrap();
    let thread_ref_id = ThreadRefId::new("thread_ref").unwrap();
    let mut graph = SemanticGraph::default();

    graph
        .apply_patch(&SemanticGraphPatch::new(vec![
            SemanticGraphPatchOp::UpsertNode {
                node: SemanticNodeDraft::new(
                    node_id.clone(),
                    "Node",
                    "Node summary",
                    SemanticNodeFacets::topic(),
                    None,
                ),
                provenance: provenance(1),
            },
            SemanticGraphPatchOp::SetHardParent {
                child_id: node_id.clone(),
                parent_id: None,
                index: None,
                provenance: provenance(2),
            },
            SemanticGraphPatchOp::UpsertThreadRef {
                thread_ref: ThreadRefDraft::new(
                    thread_ref_id.clone(),
                    node_id,
                    thread_id,
                    execution_target.clone(),
                    "Thread A",
                ),
                provenance: provenance(3),
            },
        ]))
        .unwrap();

    graph.thread_ref(&thread_ref_id).unwrap().clone()
}

fn provenance(recorded_at_millis: u64) -> MutationProvenance {
    MutationProvenance::new(
        "operator",
        recorded_at_millis,
        MutationSource::workspace_action("thread_selection_test").unwrap(),
        Some(100),
    )
    .unwrap()
}
