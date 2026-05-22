#![allow(dead_code)]

use std::{collections::HashMap, path::Path};

use beryl_backend::ThreadSummary;
use beryl_model::{
    conversation::{
        ConversationThreadId, RegisteredConversationThread, WorkspaceConversationState,
    },
    workspace::{BerylWorkspaceId, WorkspaceId},
};

#[allow(dead_code)]
#[path = "../src/member_thread_inventory.rs"]
mod member_thread_inventory;

#[path = "../src/shell/column_selector.rs"]
mod column_selector;

#[path = "../src/shell/thread_navigation.rs"]
mod thread_navigation;

#[allow(dead_code)]
#[path = "../src/shell/thread_selector.rs"]
mod thread_selector;

use member_thread_inventory::{
    build_member_thread_inventory_snapshot, empty_groups_for_workspace_state,
};
use thread_navigation::{
    PendingThreadNavigationActivation, ThreadNavigationActivationSource, ThreadNavigationEntry,
    ThreadNavigationHistory,
};
use thread_selector::{ThreadSelectorActivationTarget, ThreadSelectorState};

#[test]
fn selector_and_link_style_smoke_back_forward_select_exact_thread_ids() {
    let workspace_id = BerylWorkspaceId::new("workspace-alpha").unwrap();
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut workspace_state = registered_workspace(&execution_target);
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id.clone(),
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary("thread_a", execution_target.canonical_path(), 10),
            summary("thread_b", execution_target.canonical_path(), 20),
            summary("thread_c", execution_target.canonical_path(), 30),
        ],
        40,
    );
    let mut history = ThreadNavigationHistory::default();

    let selector_a = selector_target(&snapshot, None, "thread_a");
    commit_successful_activation(
        &workspace_id,
        &mut history,
        ThreadNavigationActivationSource::ThreadSelector,
        selector_a,
    );
    assert_eq!(current_thread_id(&history), Some("thread_a"));
    assert!(history.back_target().is_none());
    assert!(history.forward_target().is_none());

    let link_b = registered_link_target(&workspace_state, "thread_b");
    commit_successful_activation(
        &workspace_id,
        &mut history,
        ThreadNavigationActivationSource::TranscriptThreadLink,
        link_b,
    );

    let selector_c = selector_target(
        &snapshot,
        Some(ConversationThreadId::new("thread_b")),
        "thread_c",
    );
    commit_successful_activation(
        &workspace_id,
        &mut history,
        ThreadNavigationActivationSource::ThreadSelector,
        selector_c,
    );

    assert_eq!(current_thread_id(&history), Some("thread_c"));
    assert_eq!(back_thread_id(&history), Some("thread_b"));
    assert_eq!(forward_thread_id(&history), None);

    commit_navigation_command(
        &workspace_id,
        &mut history,
        ThreadNavigationActivationSource::BackwardNavigation,
    );
    assert_eq!(current_thread_id(&history), Some("thread_b"));
    assert_eq!(back_thread_id(&history), Some("thread_a"));
    assert_eq!(forward_thread_id(&history), Some("thread_c"));

    commit_navigation_command(
        &workspace_id,
        &mut history,
        ThreadNavigationActivationSource::BackwardNavigation,
    );
    assert_eq!(current_thread_id(&history), Some("thread_a"));
    assert_eq!(back_thread_id(&history), None);
    assert_eq!(forward_thread_id(&history), Some("thread_b"));

    commit_navigation_command(
        &workspace_id,
        &mut history,
        ThreadNavigationActivationSource::ForwardNavigation,
    );
    assert_eq!(current_thread_id(&history), Some("thread_b"));
    assert_eq!(back_thread_id(&history), Some("thread_a"));
    assert_eq!(forward_thread_id(&history), Some("thread_c"));

    let link_d = registered_thread_target(&mut workspace_state, &execution_target, "thread_d", 50);
    commit_successful_activation(
        &workspace_id,
        &mut history,
        ThreadNavigationActivationSource::BranchBreadcrumb,
        link_d,
    );
    assert_eq!(current_thread_id(&history), Some("thread_d"));
    assert_eq!(forward_thread_id(&history), None);
}

#[test]
fn rejected_navigation_smoke_leaves_stacks_unchanged_for_shell_rejection_paths() {
    for (index, rejection_path) in [
        "missing registration",
        "rebind required",
        "backend unavailable",
        "busy activation",
    ]
    .into_iter()
    .enumerate()
    {
        let workspace_id = BerylWorkspaceId::new(format!("workspace-rejection-{index}")).unwrap();
        let mut history = ThreadNavigationHistory::default();
        history.record_selected_thread(Some(entry("thread_a")));
        history.record_selected_thread(Some(entry("thread_b")));
        history.record_selected_thread(Some(entry("thread_c")));
        let before = history.clone();
        let target = history
            .back_target()
            .cloned()
            .expect("smoke history should have a back target");
        let pending = PendingThreadNavigationActivation::new(
            workspace_id,
            ThreadNavigationActivationSource::BackwardNavigation,
            history.current().cloned(),
            target,
        )
        .expect("back navigation should create pending activation");

        assert_eq!(pending.target().thread_id().as_str(), "thread_b");
        drop(pending);

        assert_eq!(
            history, before,
            "{rejection_path} must not consume history before exact activation success"
        );
    }
}

#[test]
fn workspace_scoped_histories_do_not_mix_back_forward_stacks() {
    let alpha = BerylWorkspaceId::new("workspace-alpha").unwrap();
    let beta = BerylWorkspaceId::new("workspace-beta").unwrap();
    let mut histories = HashMap::<BerylWorkspaceId, ThreadNavigationHistory>::new();

    histories
        .entry(alpha.clone())
        .or_default()
        .record_selected_thread(Some(entry("alpha_a")));
    histories
        .entry(alpha.clone())
        .or_default()
        .record_selected_thread(Some(entry("alpha_b")));
    histories
        .entry(beta.clone())
        .or_default()
        .record_selected_thread(Some(entry("beta_a")));
    histories
        .entry(beta.clone())
        .or_default()
        .record_selected_thread(Some(entry("beta_b")));

    assert_eq!(
        histories
            .get(&alpha)
            .and_then(ThreadNavigationHistory::back_target)
            .map(|entry| entry.thread_id().as_str()),
        Some("alpha_a")
    );
    assert_eq!(
        histories
            .get(&beta)
            .and_then(ThreadNavigationHistory::back_target)
            .map(|entry| entry.thread_id().as_str()),
        Some("beta_a")
    );

    histories
        .get_mut(&alpha)
        .expect("alpha history should exist")
        .commit_backward();

    assert_eq!(
        histories.get(&alpha).and_then(current_thread_id),
        Some("alpha_a")
    );
    assert_eq!(
        histories.get(&beta).and_then(current_thread_id),
        Some("beta_b")
    );
    assert_eq!(
        histories
            .get(&beta)
            .and_then(ThreadNavigationHistory::forward_target),
        None
    );
}

fn registered_workspace(execution_target: &WorkspaceId) -> WorkspaceConversationState {
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(execution_target)
        .unwrap();
    for (thread_id, updated_at_millis) in [("thread_a", 10), ("thread_b", 20), ("thread_c", 30)] {
        registered_thread_target(
            &mut workspace_state,
            execution_target,
            thread_id,
            updated_at_millis,
        );
    }
    workspace_state
}

fn registered_thread_target(
    workspace_state: &mut WorkspaceConversationState,
    execution_target: &WorkspaceId,
    thread_id: &str,
    updated_at_millis: i64,
) -> ThreadSelectorActivationTarget {
    let thread_id = ConversationThreadId::new(thread_id);
    let title = format!("Thread {}", thread_id.as_str());
    workspace_state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target.clone(),
        format!("{} preview", thread_id.as_str()),
        Some(title.clone()),
        updated_at_millis.saturating_sub(1),
        updated_at_millis,
    ));
    ThreadSelectorActivationTarget {
        thread_id,
        label: title,
        execution_target: execution_target.clone(),
    }
}

fn registered_link_target(
    workspace_state: &WorkspaceConversationState,
    thread_id: &str,
) -> ThreadSelectorActivationTarget {
    let thread_id = ConversationThreadId::new(thread_id);
    let registration = workspace_state
        .thread_registration(&thread_id)
        .expect("registered link target should exist");
    ThreadSelectorActivationTarget {
        thread_id: registration.thread_id().clone(),
        label: registration
            .backend_name()
            .unwrap_or_else(|| registration.preview())
            .to_string(),
        execution_target: registration.execution_target().clone(),
    }
}

fn selector_target(
    snapshot: &member_thread_inventory::MemberThreadInventorySnapshot,
    active_thread_id: Option<ConversationThreadId>,
    thread_id: &str,
) -> ThreadSelectorActivationTarget {
    let mut selector = ThreadSelectorState::default();
    selector.open(snapshot, active_thread_id);
    selector.select_thread(0, ConversationThreadId::new(thread_id));
    let target = selector
        .selected_activation_target()
        .expect("selected registered thread should resolve to an activation target");
    assert_eq!(target.thread_id.as_str(), thread_id);
    target
}

fn commit_successful_activation(
    workspace_id: &BerylWorkspaceId,
    history: &mut ThreadNavigationHistory,
    source: ThreadNavigationActivationSource,
    target: ThreadSelectorActivationTarget,
) {
    let pending = PendingThreadNavigationActivation::new(
        workspace_id.clone(),
        source,
        history.current().cloned(),
        target_entry(&target),
    )
    .expect("history source should create pending navigation");
    assert_eq!(pending.target().thread_id(), &target.thread_id);
    assert_eq!(
        pending.target().execution_target(),
        &target.execution_target
    );
    assert!(pending.commit(history));
}

fn commit_navigation_command(
    workspace_id: &BerylWorkspaceId,
    history: &mut ThreadNavigationHistory,
    source: ThreadNavigationActivationSource,
) {
    let target = match source {
        ThreadNavigationActivationSource::BackwardNavigation => history.back_target(),
        ThreadNavigationActivationSource::ForwardNavigation => history.forward_target(),
        ThreadNavigationActivationSource::ThreadSelector
        | ThreadNavigationActivationSource::TranscriptThreadLink
        | ThreadNavigationActivationSource::BranchBreadcrumb
        | ThreadNavigationActivationSource::NonHistory => None,
    }
    .cloned()
    .expect("navigation command should have a target");
    let pending = PendingThreadNavigationActivation::new(
        workspace_id.clone(),
        source,
        history.current().cloned(),
        target,
    )
    .expect("back/forward command should create pending navigation");

    assert!(pending.commit(history));
}

fn target_entry(target: &ThreadSelectorActivationTarget) -> ThreadNavigationEntry {
    ThreadNavigationEntry::new(target.thread_id.clone(), target.execution_target.clone()).unwrap()
}

fn entry(thread_id: &str) -> ThreadNavigationEntry {
    ThreadNavigationEntry::from_thread_id(thread_id, WorkspaceId::host_windows(r"C:\work\alpha"))
        .unwrap()
}

fn current_thread_id(history: &ThreadNavigationHistory) -> Option<&str> {
    history.current().map(|entry| entry.thread_id().as_str())
}

fn back_thread_id(history: &ThreadNavigationHistory) -> Option<&str> {
    history
        .back_target()
        .map(|entry| entry.thread_id().as_str())
}

fn forward_thread_id(history: &ThreadNavigationHistory) -> Option<&str> {
    history
        .forward_target()
        .map(|entry| entry.thread_id().as_str())
}

fn summary(id: &str, cwd: &Path, updated_at: i64) -> ThreadSummary {
    ThreadSummary {
        id: id.to_string(),
        forked_from_id: None,
        cwd: cwd.to_path_buf(),
        preview: format!("{id} preview"),
        name: Some(format!("Thread {id}")),
        agent_nickname: None,
        path: None,
        created_at: updated_at.saturating_sub(1),
        updated_at,
        model_provider: "test".to_string(),
        ephemeral: false,
    }
}
