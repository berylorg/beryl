#![allow(dead_code, unused_imports)]

use beryl_model::conversation::{
    ConversationThreadId, RegisteredConversationThread, SyndicConversationId,
    SyndicConversationViewId, WorkspaceConversationState,
};
use beryl_model::provenance::{MutationProvenance, MutationSource};
use beryl_model::semantic_graph::{
    SemanticGraph, SemanticGraphPatch, SemanticGraphPatchOp, SemanticNodeDraft, SemanticNodeFacets,
    SemanticNodeId, ThreadRef, ThreadRefDraft, ThreadRefId,
};
use beryl_model::workspace::WorkspaceId;

#[path = "../src/shell/thread_selection.rs"]
mod thread_selection;

use thread_selection::{GraphThreadRefAvailability, graph_thread_ref_availability};

#[test]
fn graph_thread_ref_is_openable_when_target_is_in_workspace_scope() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    workspace_state.remember_thread(registered_thread(
        "thread_a",
        execution_target.clone(),
        "Preview",
    ));
    let thread_ref = sample_thread_ref(&execution_target);

    assert_eq!(
        graph_thread_ref_availability(&workspace_state, &thread_ref, None),
        GraphThreadRefAvailability::Openable
    );
}

#[test]
fn graph_thread_ref_is_invalid_without_syndic_view_registration() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    workspace_state.remember_thread(RegisteredConversationThread::new(
        ConversationThreadId::new("thread_a"),
        execution_target.clone(),
        "Preview",
        1,
        2,
    ));
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
            .contains("not registered as a workspace Syndic conversation view")
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
    workspace_state.remember_thread(registered_thread(
        "thread_a",
        home_target.clone(),
        "Home preview",
    ));

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
    workspace_state.remember_thread(registered_thread(
        thread_id.as_str(),
        home_target.clone(),
        "Home preview",
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

fn sample_thread_ref(execution_target: &WorkspaceId) -> ThreadRef {
    sample_thread_ref_with_thread(execution_target, ConversationThreadId::new("thread_a"))
}

fn registered_thread(
    thread_id: impl Into<String>,
    execution_target: WorkspaceId,
    preview: impl Into<String>,
) -> RegisteredConversationThread {
    let thread_id = thread_id.into();
    RegisteredConversationThread::new(
        ConversationThreadId::new(thread_id.clone()),
        execution_target,
        preview,
        1,
        2,
    )
    .with_syndic_view_registration(
        SyndicConversationId::new(format!("conversation:test:{thread_id}")),
        SyndicConversationViewId::new(format!("view:test:{thread_id}")),
    )
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
