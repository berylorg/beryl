#![allow(dead_code)]

use beryl_model::workspace::{BerylWorkspaceId, WorkspaceId};

#[path = "../src/shell/thread_navigation.rs"]
mod thread_navigation;

use thread_navigation::{
    PendingThreadNavigationActivation, ThreadNavigationActivationSource, ThreadNavigationEntry,
    ThreadNavigationHistory, WorkspaceThreadNavigationHistory,
};

#[test]
fn first_selected_thread_establishes_current_without_targets() {
    let mut history = ThreadNavigationHistory::default();

    assert!(history.record_selected_thread(Some(entry("thread_a"))));

    assert_eq!(history.current(), Some(&entry("thread_a")));
    assert_eq!(history.back_target(), None);
    assert_eq!(history.forward_target(), None);
}

#[test]
fn selected_threads_record_backward_targets_in_order() {
    let mut history = ThreadNavigationHistory::default();

    history.record_selected_thread(Some(entry("thread_a")));
    history.record_selected_thread(Some(entry("thread_b")));
    history.record_selected_thread(Some(entry("thread_c")));

    assert_eq!(history.current(), Some(&entry("thread_c")));
    assert_eq!(
        thread_ids(history.backward_targets()),
        vec!["thread_a", "thread_b"]
    );
    assert_eq!(history.back_target(), Some(&entry("thread_b")));
    assert_eq!(history.forward_target(), None);
}

#[test]
fn backward_then_forward_preserves_browser_order() {
    let mut history = ThreadNavigationHistory::default();
    history.record_selected_thread(Some(entry("thread_a")));
    history.record_selected_thread(Some(entry("thread_b")));
    history.record_selected_thread(Some(entry("thread_c")));

    assert_eq!(history.commit_backward(), Some(entry("thread_b")));
    assert_eq!(history.current(), Some(&entry("thread_b")));
    assert_eq!(history.back_target(), Some(&entry("thread_a")));
    assert_eq!(history.forward_target(), Some(&entry("thread_c")));

    assert_eq!(history.commit_backward(), Some(entry("thread_a")));
    assert_eq!(history.current(), Some(&entry("thread_a")));
    assert_eq!(history.back_target(), None);
    assert_eq!(history.forward_target(), Some(&entry("thread_b")));

    assert_eq!(history.commit_forward(), Some(entry("thread_b")));
    assert_eq!(history.current(), Some(&entry("thread_b")));
    assert_eq!(history.back_target(), Some(&entry("thread_a")));
    assert_eq!(history.forward_target(), Some(&entry("thread_c")));
}

#[test]
fn selecting_after_back_truncates_forward_stack() {
    let mut history = ThreadNavigationHistory::default();
    history.record_selected_thread(Some(entry("thread_a")));
    history.record_selected_thread(Some(entry("thread_b")));
    history.record_selected_thread(Some(entry("thread_c")));
    history.commit_backward();

    assert!(history.record_selected_thread(Some(entry("thread_d"))));

    assert_eq!(history.current(), Some(&entry("thread_d")));
    assert_eq!(
        thread_ids(history.backward_targets()),
        vec!["thread_a", "thread_b"]
    );
    assert_eq!(thread_ids(history.forward_targets()), Vec::<&str>::new());
}

#[test]
fn selecting_current_thread_is_noop() {
    let mut history = ThreadNavigationHistory::default();
    history.record_selected_thread(Some(entry("thread_a")));

    assert!(!history.record_selected_thread(Some(entry("thread_a"))));

    assert_eq!(history.current(), Some(&entry("thread_a")));
    assert_eq!(thread_ids(history.backward_targets()), Vec::<&str>::new());
    assert_eq!(thread_ids(history.forward_targets()), Vec::<&str>::new());
}

#[test]
fn absent_current_and_pending_new_thread_states_do_not_create_entries() {
    let mut history = ThreadNavigationHistory::default();

    assert!(!history.record_selected_thread(None));
    assert_eq!(history.current(), None);
    assert_eq!(history.back_target(), None);
    assert_eq!(history.forward_target(), None);

    history.record_selected_thread(Some(entry("thread_a")));
    assert!(!history.record_selected_thread(None));
    history.record_selected_thread(Some(entry("thread_b")));

    assert_eq!(history.current(), Some(&entry("thread_b")));
    assert_eq!(thread_ids(history.backward_targets()), vec!["thread_a"]);
    assert_eq!(history.forward_target(), None);
}

#[test]
fn bounds_prune_oldest_backward_entries_deterministically() {
    let mut history = ThreadNavigationHistory::with_limit(2);

    history.record_selected_thread(Some(entry("thread_a")));
    history.record_selected_thread(Some(entry("thread_b")));
    history.record_selected_thread(Some(entry("thread_c")));
    history.record_selected_thread(Some(entry("thread_d")));

    assert_eq!(history.current(), Some(&entry("thread_d")));
    assert_eq!(
        thread_ids(history.backward_targets()),
        vec!["thread_b", "thread_c"]
    );
    assert_eq!(history.back_target(), Some(&entry("thread_c")));
}

#[test]
fn zero_bound_keeps_current_but_no_history_targets() {
    let mut history = ThreadNavigationHistory::with_limit(0);

    history.record_selected_thread(Some(entry("thread_a")));
    history.record_selected_thread(Some(entry("thread_b")));
    history.record_selected_thread(Some(entry("thread_c")));

    assert_eq!(history.current(), Some(&entry("thread_c")));
    assert_eq!(history.back_target(), None);
    assert_eq!(history.forward_target(), None);
}

#[test]
fn entries_include_exact_execution_target_identity() {
    let host_entry = ThreadNavigationEntry::from_thread_id(
        "thread_a",
        "view_host_a",
        WorkspaceId::host_windows(r"C:\work\beryl"),
    )
    .unwrap();
    let wsl_entry = ThreadNavigationEntry::from_thread_id(
        "thread_a",
        "view_wsl_a",
        WorkspaceId::wsl_linux("Ubuntu", "/work/beryl"),
    )
    .unwrap();

    assert_ne!(host_entry, wsl_entry);
    assert_eq!(host_entry.thread_id().as_str(), "thread_a");
    assert_eq!(
        host_entry.execution_target(),
        &WorkspaceId::host_windows(r"C:\work\beryl")
    );
}

#[test]
fn same_thread_id_on_different_execution_targets_keeps_exact_navigation_identity() {
    let host_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let wsl_target = WorkspaceId::wsl_linux("Ubuntu", "/work/beryl");
    let host_entry = entry_on("thread_a", host_target.clone());
    let wsl_entry = entry_on("thread_a", wsl_target.clone());
    let mut history = ThreadNavigationHistory::default();

    history.record_selected_thread(Some(host_entry.clone()));
    history.record_selected_thread(Some(wsl_entry.clone()));

    assert_eq!(history.current(), Some(&wsl_entry));
    assert_eq!(history.back_target(), Some(&host_entry));

    let pending = PendingThreadNavigationActivation::new(
        BerylWorkspaceId::new("workspace-alpha").unwrap(),
        ThreadNavigationActivationSource::BackwardNavigation,
        history.current().cloned(),
        host_entry.clone(),
    )
    .unwrap();

    assert!(pending.commit(&mut history));
    assert_eq!(history.current(), Some(&host_entry));
    assert_eq!(history.forward_target(), Some(&wsl_entry));
    assert_eq!(history.current().unwrap().execution_target(), &host_target);
    assert_eq!(
        history.forward_target().unwrap().execution_target(),
        &wsl_target
    );
}

#[test]
fn workspace_history_records_owning_workspace_identity() {
    let workspace_id = BerylWorkspaceId::new("workspace-alpha").unwrap();
    let mut history = WorkspaceThreadNavigationHistory::with_limit(workspace_id.clone(), 1);

    history
        .history_mut()
        .record_selected_thread(Some(entry("thread_a")));

    assert_eq!(history.workspace_id(), &workspace_id);
    assert_eq!(history.history().current(), Some(&entry("thread_a")));
}

#[test]
fn discarding_non_current_backend_target_prunes_stale_stack_entries_only() {
    let alpha_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let beta_target = WorkspaceId::host_windows(r"C:\work\beta");
    let mut history = ThreadNavigationHistory::default();

    history.record_selected_thread(Some(entry_on("thread_alpha_a", alpha_target.clone())));
    history.record_selected_thread(Some(entry_on("thread_beta", beta_target.clone())));
    history.record_selected_thread(Some(entry_on("thread_alpha_b", alpha_target.clone())));

    assert!(history.discard_entries_for_execution_target(&beta_target));

    assert_eq!(
        history.current(),
        Some(&entry_on("thread_alpha_b", alpha_target.clone()))
    );
    assert_eq!(
        history.back_target(),
        Some(&entry_on("thread_alpha_a", alpha_target.clone()))
    );
    assert_eq!(history.forward_target(), None);
}

#[test]
fn discarding_current_backend_target_clears_navigation_context() {
    let alpha_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let beta_target = WorkspaceId::host_windows(r"C:\work\beta");
    let mut history = ThreadNavigationHistory::default();

    history.record_selected_thread(Some(entry_on("thread_alpha_a", alpha_target.clone())));
    history.record_selected_thread(Some(entry_on("thread_beta", beta_target)));
    history.record_selected_thread(Some(entry_on("thread_alpha_b", alpha_target.clone())));

    assert!(history.discard_entries_for_execution_target(&alpha_target));

    assert!(history.is_empty());
    assert_eq!(history.current(), None);
    assert_eq!(history.back_target(), None);
    assert_eq!(history.forward_target(), None);
}

#[test]
fn user_source_commit_seeds_origin_before_recording_target() {
    let mut history = ThreadNavigationHistory::default();
    let pending = pending(
        ThreadNavigationActivationSource::ThreadSelector,
        Some(entry("thread_a")),
        entry("thread_b"),
    );

    assert!(pending.commit(&mut history));

    assert_eq!(history.current(), Some(&entry("thread_b")));
    assert_eq!(history.back_target(), Some(&entry("thread_a")));
    assert_eq!(history.forward_target(), None);
}

#[test]
fn transcript_and_breadcrumb_sources_record_new_user_selections() {
    for source in [
        ThreadNavigationActivationSource::TranscriptThreadLink,
        ThreadNavigationActivationSource::BranchBreadcrumb,
    ] {
        let mut history = ThreadNavigationHistory::default();
        history.record_selected_thread(Some(entry("thread_a")));
        let pending = pending(source, Some(entry("thread_a")), entry("thread_b"));

        assert!(pending.commit(&mut history));

        assert_eq!(history.current(), Some(&entry("thread_b")));
        assert_eq!(history.back_target(), Some(&entry("thread_a")));
    }
}

#[test]
fn non_history_source_does_not_create_pending_activation() {
    let workspace_id = BerylWorkspaceId::new("workspace-alpha").unwrap();

    assert_eq!(
        PendingThreadNavigationActivation::new(
            workspace_id,
            ThreadNavigationActivationSource::NonHistory,
            Some(entry("thread_a")),
            entry("thread_b"),
        ),
        None
    );
}

#[test]
fn backward_and_forward_sources_commit_only_after_success() {
    let mut history = ThreadNavigationHistory::default();
    history.record_selected_thread(Some(entry("thread_a")));
    history.record_selected_thread(Some(entry("thread_b")));
    history.record_selected_thread(Some(entry("thread_c")));

    let back = pending(
        ThreadNavigationActivationSource::BackwardNavigation,
        Some(entry("thread_c")),
        entry("thread_b"),
    );
    assert!(back.commit(&mut history));
    assert_eq!(history.current(), Some(&entry("thread_b")));
    assert_eq!(history.back_target(), Some(&entry("thread_a")));
    assert_eq!(history.forward_target(), Some(&entry("thread_c")));

    let forward = pending(
        ThreadNavigationActivationSource::ForwardNavigation,
        Some(entry("thread_b")),
        entry("thread_c"),
    );
    assert!(forward.commit(&mut history));
    assert_eq!(history.current(), Some(&entry("thread_c")));
    assert_eq!(history.back_target(), Some(&entry("thread_b")));
    assert_eq!(history.forward_target(), None);
}

#[test]
fn navigation_command_target_mismatch_leaves_history_unchanged() {
    let mut history = ThreadNavigationHistory::default();
    history.record_selected_thread(Some(entry("thread_a")));
    history.record_selected_thread(Some(entry("thread_b")));

    let pending = pending(
        ThreadNavigationActivationSource::BackwardNavigation,
        Some(entry("thread_b")),
        entry("thread_c"),
    );

    assert!(!pending.commit(&mut history));
    assert_eq!(history.current(), Some(&entry("thread_b")));
    assert_eq!(history.back_target(), Some(&entry("thread_a")));
    assert_eq!(history.forward_target(), None);
}

fn entry(thread_id: &str) -> ThreadNavigationEntry {
    entry_on(thread_id, WorkspaceId::host_windows(r"C:\work\beryl"))
}

fn entry_on(thread_id: &str, execution_target: WorkspaceId) -> ThreadNavigationEntry {
    ThreadNavigationEntry::from_thread_id(thread_id, format!("view:{thread_id}"), execution_target)
        .unwrap()
}

fn pending(
    source: ThreadNavigationActivationSource,
    origin: Option<ThreadNavigationEntry>,
    target: ThreadNavigationEntry,
) -> PendingThreadNavigationActivation {
    PendingThreadNavigationActivation::new(
        BerylWorkspaceId::new("workspace-alpha").unwrap(),
        source,
        origin,
        target,
    )
    .unwrap()
}

fn thread_ids<'a>(entries: impl IntoIterator<Item = &'a ThreadNavigationEntry>) -> Vec<&'a str> {
    entries
        .into_iter()
        .map(|entry| entry.thread_id().as_str())
        .collect()
}
