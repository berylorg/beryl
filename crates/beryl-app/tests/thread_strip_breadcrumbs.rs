#[allow(dead_code)]
#[path = "../src/member_thread_inventory.rs"]
mod member_thread_inventory;

#[path = "../src/thread_strip_breadcrumbs.rs"]
mod thread_strip_breadcrumbs;

use beryl_backend::ThreadSummary;
use beryl_model::{
    conversation::{
        ConversationThreadId, RegisteredConversationThread, WorkspaceConversationState,
    },
    workspace::{BerylWorkspaceId, WorkspaceId},
};
use thread_strip_breadcrumbs::{TransientBranchParent, thread_strip_breadcrumb_trail};

#[test]
fn root_thread_has_no_thread_strip_breadcrumbs() {
    let target = WorkspaceId::host_windows(r"C:\work\beryl");
    let root_id = thread_id("root_thread");
    let mut state = WorkspaceConversationState::default();
    state.remember_thread(registered_thread(&target, &root_id, "Root thread"));

    assert!(
        thread_strip_breadcrumb_trail(&state, Some(root_id.as_str()), "Root thread", None)
            .is_none()
    );
    assert!(thread_strip_breadcrumb_trail(&state, Some(""), "Root thread", None).is_none());
    assert!(thread_strip_breadcrumb_trail(&state, None, "Root thread", None).is_none());
}

#[test]
fn durable_branch_projects_parent_and_active_segments() {
    let target = WorkspaceId::host_windows(r"C:\work\beryl");
    let parent_id = thread_id("parent_thread");
    let child_id = thread_id("child_thread");
    let mut state = WorkspaceConversationState::default();
    state.remember_thread(registered_thread(
        &target,
        &parent_id,
        "Thread Branching Test",
    ));
    state.remember_thread(
        registered_thread(&target, &child_id, "Three Pretend Decisions")
            .with_branch_parent_thread_id(parent_id.clone()),
    );

    let trail = thread_strip_breadcrumb_trail(
        &state,
        Some(child_id.as_str()),
        "Three Pretend Decisions",
        None,
    )
    .expect("branch child should have breadcrumbs");
    let labels = trail
        .segments()
        .iter()
        .map(|segment| segment.label())
        .collect::<Vec<_>>();

    assert_eq!(
        labels,
        vec!["Thread Branching Test", "Three Pretend Decisions"]
    );
    assert_eq!(trail.segments()[0].thread_id(), &parent_id);
    assert!(trail.segments()[0].activation_available());
    assert_eq!(trail.segments()[0].execution_target(), Some(&target));
    assert!(!trail.segments()[0].active());
    assert_eq!(trail.segments()[1].thread_id(), &child_id);
    assert!(trail.segments()[1].active());
    assert!(!trail.segments()[1].activation_available());
}

#[test]
fn inventory_fork_metadata_feeds_registered_breadcrumb_projection() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let target = WorkspaceId::host_windows(r"C:\work\beryl");
    let parent_id = thread_id("parent_thread");
    let child_id = thread_id("child_thread");
    let mut state = WorkspaceConversationState::default();

    state.designate_primary_execution_target(&target).unwrap();
    let snapshot = member_thread_inventory::build_member_thread_inventory_snapshot(
        workspace_id,
        &state,
        member_thread_inventory::empty_groups_for_workspace_state(&state),
        vec![
            summary_for_target(&target, parent_id.as_str(), "Thread Branching Test", None),
            summary_for_target(
                &target,
                child_id.as_str(),
                "Three Pretend Decisions",
                Some(parent_id.as_str()),
            ),
        ],
        50,
    );
    for thread in snapshot
        .groups()
        .iter()
        .flat_map(|group| group.threads().iter())
    {
        state.remember_thread(thread.to_registered_thread());
    }

    let trail = thread_strip_breadcrumb_trail(
        &state,
        Some(child_id.as_str()),
        "Three Pretend Decisions",
        None,
    )
    .expect("inventory-registered branch should have breadcrumbs");
    let labels = trail
        .segments()
        .iter()
        .map(|segment| segment.label())
        .collect::<Vec<_>>();

    assert_eq!(
        labels,
        vec!["Thread Branching Test", "Three Pretend Decisions"]
    );
}

#[test]
fn transient_foreground_branch_projects_parent_before_registration() {
    let target = WorkspaceId::host_windows(r"C:\work\beryl");
    let parent_id = thread_id("parent_thread");
    let child_id = thread_id("child_thread");
    let mut state = WorkspaceConversationState::default();
    state.remember_thread(registered_thread(&target, &parent_id, "Parent title"));

    let trail = thread_strip_breadcrumb_trail(
        &state,
        Some(child_id.as_str()),
        "Branch title",
        Some(TransientBranchParent {
            child_thread_id: &child_id,
            parent_thread_id: &parent_id,
        }),
    )
    .expect("foreground branch should use transient parent metadata");
    let labels = trail
        .segments()
        .iter()
        .map(|segment| segment.label())
        .collect::<Vec<_>>();

    assert_eq!(labels, vec!["Parent title", "Branch title"]);
}

#[test]
fn nested_branch_breadcrumbs_render_root_to_selected_and_stop_on_cycles() {
    let target = WorkspaceId::host_windows(r"C:\work\beryl");
    let root_id = thread_id("root_thread");
    let middle_id = thread_id("middle_thread");
    let leaf_id = thread_id("leaf_thread");
    let mut state = WorkspaceConversationState::default();
    state.remember_thread(registered_thread(&target, &root_id, "Root"));
    state.remember_thread(
        registered_thread(&target, &middle_id, "Middle")
            .with_branch_parent_thread_id(root_id.clone()),
    );
    state.remember_thread(
        registered_thread(&target, &leaf_id, "Leaf")
            .with_branch_parent_thread_id(middle_id.clone()),
    );

    let trail = thread_strip_breadcrumb_trail(&state, Some(leaf_id.as_str()), "Leaf", None)
        .expect("nested branch should have breadcrumbs");
    let labels = trail
        .segments()
        .iter()
        .map(|segment| segment.label())
        .collect::<Vec<_>>();
    assert_eq!(labels, vec!["Root", "Middle", "Leaf"]);

    state.remember_thread(
        registered_thread(&target, &root_id, "Root").with_branch_parent_thread_id(leaf_id.clone()),
    );
    let cycle_trail = thread_strip_breadcrumb_trail(&state, Some(leaf_id.as_str()), "Leaf", None)
        .expect("cycle should still produce bounded breadcrumbs");
    assert!(cycle_trail.segments().len() <= 4);
    assert_eq!(cycle_trail.segments().last().unwrap().label(), "Leaf");
}

#[test]
fn missing_or_rebind_parent_projects_disabled_segment() {
    let target = WorkspaceId::host_windows(r"C:\work\beryl");
    let parent_id = thread_id("parent_thread");
    let child_id = thread_id("child_thread");
    let mut missing_parent_state = WorkspaceConversationState::default();
    missing_parent_state.remember_thread(
        registered_thread(&target, &child_id, "Child")
            .with_branch_parent_thread_id(parent_id.clone()),
    );

    let missing = thread_strip_breadcrumb_trail(
        &missing_parent_state,
        Some(child_id.as_str()),
        "Child",
        None,
    )
    .unwrap();
    assert_eq!(missing.segments()[0].label(), "Parent unavailable");
    assert!(
        missing.segments()[0]
            .disabled_reason()
            .unwrap()
            .contains("no longer registered")
    );
    assert!(!missing.segments()[0].activation_available());

    let mut rebind_state = WorkspaceConversationState::default();
    rebind_state.remember_thread(registered_thread(&target, &parent_id, "Parent"));
    rebind_state.remember_thread(
        registered_thread(&target, &child_id, "Child")
            .with_branch_parent_thread_id(parent_id.clone()),
    );
    rebind_state
        .mark_thread_rebind_required(&parent_id, "Member path changed")
        .unwrap();

    let rebind =
        thread_strip_breadcrumb_trail(&rebind_state, Some(child_id.as_str()), "Child", None)
            .unwrap();
    assert_eq!(rebind.segments()[0].label(), "Parent");
    assert!(
        rebind.segments()[0]
            .disabled_reason()
            .unwrap()
            .contains("requires rebind")
    );
    assert!(!rebind.segments()[0].activation_available());
}

fn registered_thread(
    target: &WorkspaceId,
    thread_id: &ConversationThreadId,
    title: &str,
) -> RegisteredConversationThread {
    RegisteredConversationThread::new(
        thread_id.clone(),
        target.clone(),
        format!("{title} preview"),
        Some(title.to_string()),
        1,
        2,
    )
}

fn thread_id(value: &str) -> ConversationThreadId {
    ConversationThreadId::new(value.to_string())
}

fn summary_for_target(
    target: &WorkspaceId,
    thread_id: &str,
    title: &str,
    forked_from_id: Option<&str>,
) -> ThreadSummary {
    ThreadSummary {
        id: thread_id.to_string(),
        forked_from_id: forked_from_id.map(str::to_string),
        cwd: target.canonical_path().to_path_buf(),
        preview: format!("{title} preview"),
        name: Some(title.to_string()),
        agent_nickname: None,
        path: None,
        created_at: 1,
        updated_at: 2,
        model_provider: "openai".to_string(),
        ephemeral: false,
    }
}
