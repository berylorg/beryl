use std::path::PathBuf;

use beryl_backend::{JsonRpcError, ManagedBackendError, ThreadSummary};
use beryl_model::{
    conversation::{
        ConversationThreadId, RegisteredConversationThread, WorkspaceConversationState,
    },
    workspace::{BerylWorkspaceId, RuntimeMode, WorkspaceId, WorkspaceMemberId},
};
use serde_json::json;

#[allow(dead_code)]
#[path = "../src/member_thread_inventory.rs"]
mod member_thread_inventory;

#[test]
fn inventory_groups_threads_by_exact_member_cwd_and_sorts_by_updated_time() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let first = WorkspaceId::host_windows(r"C:\work\first");
    let second = WorkspaceId::host_windows(r"C:\work\second");
    let mut state = WorkspaceConversationState::default();

    state.designate_primary_execution_target(&first).unwrap();
    state.attach_execution_target(&second).unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        ConversationThreadId::new("thread_existing"),
        first.clone(),
        "Existing preview",
        None,
        1,
        2,
    ));
    state
        .set_thread_manual_title(
            &ConversationThreadId::new("thread_existing"),
            "Manual title",
            3,
        )
        .unwrap();

    let snapshot = member_thread_inventory::build_member_thread_inventory_snapshot(
        workspace_id,
        &state,
        member_thread_inventory::empty_groups_for_workspace_state(&state),
        vec![
            summary(
                "thread_old",
                first.canonical_path(),
                Some("Old backend"),
                1,
                10,
            ),
            summary("thread_existing", first.canonical_path(), None, 2, 20),
            summary(
                "thread_second",
                second.canonical_path(),
                Some("Second backend"),
                3,
                30,
            ),
            summary(
                "thread_other",
                PathBuf::from(r"C:\work\other").as_path(),
                Some("Other"),
                4,
                40,
            ),
        ],
        50,
    );

    assert_eq!(snapshot.groups().len(), 2);
    assert_eq!(snapshot.groups()[0].threads().len(), 2);
    assert_eq!(snapshot.groups()[0].threads()[0].title(), "Manual title");
    assert_eq!(snapshot.groups()[0].threads()[1].title(), "Old backend");
    assert_eq!(snapshot.groups()[1].threads().len(), 1);
    assert_eq!(snapshot.groups()[1].threads()[0].title(), "Second backend");

    let counts = snapshot.retained_counts();
    assert_eq!(counts.groups, 2);
    assert_eq!(counts.threads, 3);
    assert!(counts.payload_bytes > 0);
}

#[test]
fn inventory_preserves_optional_fork_parent_metadata() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let first = WorkspaceId::host_windows(r"C:\work\first");
    let mut state = WorkspaceConversationState::default();

    state.designate_primary_execution_target(&first).unwrap();

    let snapshot = member_thread_inventory::build_member_thread_inventory_snapshot(
        workspace_id,
        &state,
        member_thread_inventory::empty_groups_for_workspace_state(&state),
        vec![
            summary(
                "thread_parent",
                first.canonical_path(),
                Some("Parent"),
                1,
                10,
            ),
            summary_with_fork_parent(
                "thread_child",
                first.canonical_path(),
                Some("Child"),
                "thread_parent",
                2,
                20,
            ),
        ],
        50,
    );

    let parent = inventory_thread(&snapshot, "thread_parent");
    let child = inventory_thread(&snapshot, "thread_child");

    assert_eq!(parent.forked_from_id(), None);
    assert_eq!(
        child.forked_from_id().map(ConversationThreadId::as_str),
        Some("thread_parent")
    );
}

#[test]
fn inventory_enrichment_fills_missing_fork_parent_from_metadata_read() {
    let cwd = PathBuf::from(r"C:\work\first");
    let mut backend_threads = vec![
        summary("thread_parent", cwd.as_path(), Some("Parent"), 1, 10),
        summary("thread_child", cwd.as_path(), Some("Child"), 2, 20),
    ];
    let mut read_thread_ids = Vec::new();

    member_thread_inventory::enrich_missing_thread_fork_parent_metadata(
        &mut backend_threads,
        |thread_id| {
            read_thread_ids.push(thread_id.to_string());
            match thread_id {
                "thread_parent" => Ok(summary(
                    "thread_parent",
                    cwd.as_path(),
                    Some("Parent"),
                    1,
                    10,
                )),
                "thread_child" => Ok(summary_with_fork_parent(
                    "thread_child",
                    cwd.as_path(),
                    Some("Child"),
                    "thread_parent",
                    2,
                    20,
                )),
                other => panic!("unexpected metadata read for {other}"),
            }
        },
    )
    .unwrap();

    assert_eq!(read_thread_ids, vec!["thread_parent", "thread_child"]);
    assert_eq!(backend_threads[0].forked_from_id.as_deref(), None);
    assert_eq!(
        backend_threads[1].forked_from_id.as_deref(),
        Some("thread_parent")
    );
}

#[test]
fn inventory_preparation_filters_unrelated_threads_before_lineage_reads() {
    let member_cwd = PathBuf::from(r"C:\work\first");
    let other_cwd = PathBuf::from(r"C:\work\other");
    let members = vec![inventory_group("member_first", member_cwd.as_path())];
    let mut backend_threads = vec![
        summary("thread_parent", member_cwd.as_path(), Some("Parent"), 1, 10),
        summary("thread_child", member_cwd.as_path(), Some("Child"), 2, 20),
        summary(
            "thread_unrelated",
            other_cwd.as_path(),
            Some("Other"),
            3,
            30,
        ),
    ];
    let mut read_thread_ids = Vec::new();

    member_thread_inventory::prepare_backend_threads_for_member_thread_inventory(
        &mut backend_threads,
        &members,
        |thread_id| {
            read_thread_ids.push(thread_id.to_string());
            match thread_id {
                "thread_parent" => Ok(summary(
                    "thread_parent",
                    member_cwd.as_path(),
                    Some("Parent"),
                    1,
                    10,
                )),
                "thread_child" => Ok(summary_with_fork_parent(
                    "thread_child",
                    member_cwd.as_path(),
                    Some("Child"),
                    "thread_parent",
                    2,
                    20,
                )),
                other => panic!("unexpected metadata read for {other}"),
            }
        },
    )
    .unwrap();

    assert_eq!(read_thread_ids, vec!["thread_parent", "thread_child"]);
    assert_eq!(
        backend_threads
            .iter()
            .map(|thread| thread.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread_parent", "thread_child"]
    );
    assert_eq!(
        backend_threads[1].forked_from_id.as_deref(),
        Some("thread_parent")
    );
}

#[test]
fn inventory_enrichment_skips_threads_that_already_have_fork_parent_metadata() {
    let cwd = PathBuf::from(r"C:\work\first");
    let mut backend_threads = vec![summary_with_fork_parent(
        "thread_child",
        cwd.as_path(),
        Some("Child"),
        "thread_parent",
        2,
        20,
    )];
    let mut read_called = false;

    member_thread_inventory::enrich_missing_thread_fork_parent_metadata(
        &mut backend_threads,
        |_| {
            read_called = true;
            Ok(summary("thread_unexpected", cwd.as_path(), None, 1, 1))
        },
    )
    .unwrap();

    assert!(!read_called);
    assert_eq!(
        backend_threads[0].forked_from_id.as_deref(),
        Some("thread_parent")
    );
}

#[test]
fn inventory_lineage_read_error_degrades_only_for_requested_thread_failures() {
    let specific = member_thread_inventory::thread_fork_parent_metadata_read_error(
        "thread_child",
        thread_read_request_failed(-32000, "thread thread_child is unavailable", None),
    );
    assert!(matches!(
        specific,
        member_thread_inventory::ThreadForkParentMetadataReadError::ThreadUnavailable(_)
    ));

    let data_specific = member_thread_inventory::thread_fork_parent_metadata_read_error(
        "thread_child",
        thread_read_request_failed(
            -32000,
            "thread unavailable",
            Some(json!({"threadId": "thread_child"})),
        ),
    );
    assert!(matches!(
        data_specific,
        member_thread_inventory::ThreadForkParentMetadataReadError::ThreadUnavailable(_)
    ));

    let method_missing = member_thread_inventory::thread_fork_parent_metadata_read_error(
        "thread_child",
        thread_read_request_failed(-32601, "thread/read missing for thread_child", None),
    );
    assert!(matches!(
        method_missing,
        member_thread_inventory::ThreadForkParentMetadataReadError::Fatal(_)
    ));

    let invalid_params = member_thread_inventory::thread_fork_parent_metadata_read_error(
        "thread_child",
        thread_read_request_failed(
            -32602,
            "invalid thread id",
            Some(json!({"threadId": "thread_child"})),
        ),
    );
    assert!(matches!(
        invalid_params,
        member_thread_inventory::ThreadForkParentMetadataReadError::Fatal(_)
    ));

    let generic_server_error = member_thread_inventory::thread_fork_parent_metadata_read_error(
        "thread_child",
        thread_read_request_failed(-32000, "server unavailable", None),
    );
    assert!(matches!(
        generic_server_error,
        member_thread_inventory::ThreadForkParentMetadataReadError::Fatal(_)
    ));
}

#[test]
fn inventory_enrichment_keeps_thread_when_metadata_read_reports_thread_unavailable() {
    let cwd = PathBuf::from(r"C:\work\first");
    let mut backend_threads = vec![summary("thread_child", cwd.as_path(), Some("Child"), 2, 20)];

    member_thread_inventory::enrich_missing_thread_fork_parent_metadata(
        &mut backend_threads,
        |_| {
            Err(
                member_thread_inventory::ThreadForkParentMetadataReadError::thread_unavailable(
                    "thread vanished",
                ),
            )
        },
    )
    .unwrap();

    assert_eq!(backend_threads.len(), 1);
    assert_eq!(backend_threads[0].id, "thread_child");
    assert_eq!(backend_threads[0].forked_from_id, None);
}

#[test]
fn inventory_enrichment_fails_when_metadata_read_loses_backend_transport() {
    let cwd = PathBuf::from(r"C:\work\first");
    let mut backend_threads = vec![summary("thread_child", cwd.as_path(), Some("Child"), 2, 20)];

    let error = member_thread_inventory::enrich_missing_thread_fork_parent_metadata(
        &mut backend_threads,
        |_| {
            Err(
                member_thread_inventory::ThreadForkParentMetadataReadError::fatal(
                    "backend transport closed",
                ),
            )
        },
    )
    .unwrap_err();

    assert_eq!(error, "backend transport closed");
    assert_eq!(backend_threads[0].forked_from_id, None);
}

#[test]
fn inventory_enrichment_rejects_mismatched_metadata_thread_id() {
    let cwd = PathBuf::from(r"C:\work\first");
    let mut backend_threads = vec![summary("thread_child", cwd.as_path(), Some("Child"), 2, 20)];

    let error = member_thread_inventory::enrich_missing_thread_fork_parent_metadata(
        &mut backend_threads,
        |_| Ok(summary("thread_other", cwd.as_path(), Some("Other"), 3, 30)),
    )
    .unwrap_err();

    assert!(error.contains("thread_child"));
    assert!(error.contains("thread_other"));
    assert_eq!(backend_threads[0].forked_from_id, None);
}

#[test]
fn inventory_keeps_cross_member_fork_parent_metadata_in_child_group() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let first = WorkspaceId::host_windows(r"C:\work\first");
    let second = WorkspaceId::host_windows(r"C:\work\second");
    let mut state = WorkspaceConversationState::default();

    state.designate_primary_execution_target(&first).unwrap();
    state.attach_execution_target(&second).unwrap();

    let snapshot = member_thread_inventory::build_member_thread_inventory_snapshot(
        workspace_id,
        &state,
        member_thread_inventory::empty_groups_for_workspace_state(&state),
        vec![
            summary(
                "thread_parent",
                first.canonical_path(),
                Some("Parent"),
                1,
                10,
            ),
            summary_with_fork_parent(
                "thread_child",
                second.canonical_path(),
                Some("Child"),
                "thread_parent",
                2,
                20,
            ),
        ],
        50,
    );

    assert_eq!(snapshot.groups().len(), 2);
    assert_eq!(snapshot.groups()[0].threads().len(), 1);
    assert_eq!(
        snapshot.groups()[0].threads()[0].thread_id().as_str(),
        "thread_parent"
    );
    assert_eq!(snapshot.groups()[1].threads().len(), 1);
    assert_eq!(
        snapshot.groups()[1].threads()[0].thread_id().as_str(),
        "thread_child"
    );

    let child = inventory_thread(&snapshot, "thread_child");
    assert_eq!(
        child.forked_from_id().map(ConversationThreadId::as_str),
        Some("thread_parent")
    );
}

#[test]
fn inventory_titles_resolve_manual_backend_generated_and_untitled_precedence() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let first = WorkspaceId::host_windows(r"C:\work\first");
    let mut state = WorkspaceConversationState::default();
    let manual_id = ConversationThreadId::new("thread_manual");
    let generated_id = ConversationThreadId::new("thread_generated");
    let generated_only_id = ConversationThreadId::new("thread_generated_only");

    state.designate_primary_execution_target(&first).unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        manual_id.clone(),
        first.clone(),
        "Manual preview",
        Some("Stored backend".to_string()),
        1,
        2,
    ));
    state
        .set_thread_manual_title(&manual_id, "Manual title", 3)
        .unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        generated_id.clone(),
        first.clone(),
        "Generated preview",
        None,
        1,
        2,
    ));
    state
        .set_thread_generated_title_if_absent(&generated_id, "Generated title", 4)
        .unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        generated_only_id.clone(),
        first.clone(),
        "Generated-only preview",
        None,
        1,
        2,
    ));
    state
        .set_thread_generated_title_if_absent(&generated_only_id, "Generated title", 5)
        .unwrap();

    let snapshot = member_thread_inventory::build_member_thread_inventory_snapshot(
        workspace_id,
        &state,
        member_thread_inventory::empty_groups_for_workspace_state(&state),
        vec![
            summary(
                "thread_manual",
                first.canonical_path(),
                Some("Fresh backend"),
                1,
                40,
            ),
            summary(
                "thread_generated",
                first.canonical_path(),
                Some("Backend over generated"),
                1,
                30,
            ),
            summary("thread_generated_only", first.canonical_path(), None, 1, 20),
            summary("thread_untitled", first.canonical_path(), None, 1, 10),
        ],
        50,
    );

    let titles = snapshot.groups()[0]
        .threads()
        .iter()
        .map(|thread| thread.title())
        .collect::<Vec<_>>();

    assert_eq!(
        titles,
        vec![
            "Manual title",
            "Backend over generated",
            "Generated title",
            "Untitled thread"
        ]
    );
}

#[test]
fn inventory_refresh_preserves_stored_backend_name_from_stale_unnamed_summary() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let first = WorkspaceId::host_windows(r"C:\work\first");
    let thread_id = ConversationThreadId::new("thread_named");
    let mut state = WorkspaceConversationState::default();

    state.designate_primary_execution_target(&first).unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        first.clone(),
        "Stored preview",
        Some("Stored backend".to_string()),
        1,
        2,
    ));

    let snapshot = member_thread_inventory::build_member_thread_inventory_snapshot(
        workspace_id,
        &state,
        member_thread_inventory::empty_groups_for_workspace_state(&state),
        vec![summary("thread_named", first.canonical_path(), None, 1, 20)],
        50,
    );

    let thread = inventory_thread(&snapshot, "thread_named");
    assert_eq!(thread.title(), "Stored backend");
    assert_eq!(
        thread.to_registered_thread().backend_name(),
        Some("Stored backend")
    );

    assert!(state.remember_thread(thread.to_registered_thread()));
    let registered = state.thread_registration(&thread_id).unwrap();
    assert_eq!(registered.backend_name(), Some("Stored backend"));
    assert_eq!(registered.title(), Some("Stored backend"));
}

#[test]
fn inventory_ignores_suppressed_backend_name_from_branch_summary() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let first = WorkspaceId::host_windows(r"C:\work\first");
    let thread_id = ConversationThreadId::new("thread_branch");
    let mut state = WorkspaceConversationState::default();

    state.designate_primary_execution_target(&first).unwrap();
    state.remember_thread(
        RegisteredConversationThread::new(
            thread_id.clone(),
            first.clone(),
            "Branch preview",
            None,
            1,
            2,
        )
        .with_beryl_created()
        .with_ignored_backend_name_for_automatic_title(Some("Source title".to_string())),
    );

    let mut snapshot = member_thread_inventory::build_member_thread_inventory_snapshot(
        workspace_id,
        &state,
        member_thread_inventory::empty_groups_for_workspace_state(&state),
        vec![summary(
            "thread_branch",
            first.canonical_path(),
            Some("Source title"),
            1,
            20,
        )],
        50,
    );

    let thread = inventory_thread(&snapshot, "thread_branch");
    assert_eq!(thread.title(), "Untitled thread");
    assert_eq!(thread.to_registered_thread().backend_name(), None);

    assert!(!snapshot.update_thread_backend_name(&state, &thread_id, Some("Source title")));
    let thread = inventory_thread(&snapshot, "thread_branch");
    assert_eq!(thread.title(), "Untitled thread");
    assert_eq!(thread.to_registered_thread().backend_name(), None);
}

#[test]
fn refreshed_inventory_suppresses_stale_copied_branch_backend_name() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let first = WorkspaceId::host_windows(r"C:\work\first");
    let thread_id = ConversationThreadId::new("thread_branch");
    let mut stale_state = WorkspaceConversationState::default();
    let mut current_state = WorkspaceConversationState::default();

    stale_state
        .designate_primary_execution_target(&first)
        .unwrap();
    current_state
        .designate_primary_execution_target(&first)
        .unwrap();
    current_state.remember_thread(
        RegisteredConversationThread::new(thread_id, first.clone(), "Branch preview", None, 1, 2)
            .with_beryl_created()
            .with_ignored_backend_name_for_automatic_title(Some("Source title".to_string())),
    );

    let stale_snapshot = member_thread_inventory::build_member_thread_inventory_snapshot(
        workspace_id.clone(),
        &stale_state,
        member_thread_inventory::empty_groups_for_workspace_state(&stale_state),
        vec![summary(
            "thread_branch",
            first.canonical_path(),
            Some("Source title"),
            1,
            20,
        )],
        50,
    );
    let mut inventory =
        member_thread_inventory::MemberThreadInventoryState::new(workspace_id, &current_state);

    inventory.finish_refresh(stale_snapshot, &current_state);

    let thread = inventory_thread(inventory.snapshot(), "thread_branch");
    assert_eq!(thread.title(), "Untitled thread");
    assert_eq!(thread.to_registered_thread().backend_name(), None);
}

#[test]
fn live_backend_name_update_recomputes_inventory_titles_without_overriding_manual_titles() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let first = WorkspaceId::host_windows(r"C:\work\first");
    let mut state = WorkspaceConversationState::default();
    let generated_id = ConversationThreadId::new("thread_generated");
    let manual_id = ConversationThreadId::new("thread_manual");

    state.designate_primary_execution_target(&first).unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        generated_id.clone(),
        first.clone(),
        "Generated preview",
        None,
        1,
        2,
    ));
    state
        .set_thread_generated_title_if_absent(&generated_id, "Generated title", 3)
        .unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        manual_id.clone(),
        first.clone(),
        "Manual preview",
        Some("Old backend".to_string()),
        1,
        2,
    ));
    state
        .set_thread_manual_title(&manual_id, "Manual title", 4)
        .unwrap();

    let mut snapshot = member_thread_inventory::build_member_thread_inventory_snapshot(
        workspace_id,
        &state,
        member_thread_inventory::empty_groups_for_workspace_state(&state),
        vec![
            summary("thread_generated", first.canonical_path(), None, 1, 20),
            summary(
                "thread_manual",
                first.canonical_path(),
                Some("Old backend"),
                1,
                10,
            ),
        ],
        50,
    );

    state
        .set_thread_backend_name(&generated_id, Some(" Backend title ".to_string()))
        .unwrap();
    assert!(snapshot.update_thread_backend_name(&state, &generated_id, Some(" Backend title ")));
    let generated_thread = inventory_thread(&snapshot, "thread_generated");
    assert_eq!(generated_thread.title(), "Backend title");
    assert_eq!(
        generated_thread.to_registered_thread().backend_name(),
        Some("Backend title")
    );

    state
        .set_thread_backend_name(&manual_id, Some("Fresh backend".to_string()))
        .unwrap();
    assert!(snapshot.update_thread_backend_name(&state, &manual_id, Some("Fresh backend")));
    let manual_thread = inventory_thread(&snapshot, "thread_manual");
    assert_eq!(manual_thread.title(), "Manual title");
    assert_eq!(
        manual_thread.to_registered_thread().backend_name(),
        Some("Fresh backend")
    );

    state.set_thread_backend_name(&generated_id, None).unwrap();
    assert!(snapshot.update_thread_backend_name(&state, &generated_id, None));
    let generated_thread = inventory_thread(&snapshot, "thread_generated");
    assert_eq!(generated_thread.title(), "Generated title");
    assert_eq!(generated_thread.to_registered_thread().backend_name(), None);

    assert!(!snapshot.update_thread_backend_name(&state, &generated_id, None));
    assert!(!snapshot.update_thread_backend_name(
        &state,
        &ConversationThreadId::new("missing_thread"),
        Some("Missing"),
    ));
}

#[test]
fn refreshed_inventory_reconciles_stale_worker_titles_against_current_state() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let first = WorkspaceId::host_windows(r"C:\work\first");
    let mut state = WorkspaceConversationState::default();
    let thread_id = ConversationThreadId::new("thread_generated");

    state.designate_primary_execution_target(&first).unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        first.clone(),
        "Initial preview",
        None,
        1,
        2,
    ));

    let stale_snapshot = member_thread_inventory::build_member_thread_inventory_snapshot(
        workspace_id.clone(),
        &state,
        member_thread_inventory::empty_groups_for_workspace_state(&state),
        vec![summary(
            "thread_generated",
            first.canonical_path(),
            None,
            1,
            20,
        )],
        50,
    );
    assert_eq!(
        inventory_thread(&stale_snapshot, "thread_generated").title(),
        "Untitled thread"
    );

    state
        .set_thread_generated_title_if_absent(&thread_id, "Generated title", 3)
        .unwrap();
    let mut inventory =
        member_thread_inventory::MemberThreadInventoryState::new(workspace_id, &state);

    inventory.finish_refresh(stale_snapshot, &state);

    assert_eq!(
        inventory_thread(inventory.snapshot(), "thread_generated").title(),
        "Generated title"
    );
}

#[test]
fn inventory_exposes_implicit_home_group_without_resolving_path_in_initial_snapshot() {
    let mut state = WorkspaceConversationState::default();
    state.select_runtime(RuntimeMode::HostWindows).unwrap();

    let groups = member_thread_inventory::empty_groups_for_workspace_state(&state);

    assert_eq!(groups.len(), 1);
    assert!(groups[0].canonical_path().is_none());
    assert!(groups[0].threads().is_empty());
}

#[test]
fn inventory_has_no_groups_or_refresh_work_without_selected_runtime() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let state = WorkspaceConversationState::default();
    let inventory = member_thread_inventory::MemberThreadInventoryState::new(workspace_id, &state);

    assert!(inventory.snapshot().groups().is_empty());
    assert!(!inventory.needs_refresh());
}

#[test]
fn inventory_sorts_groups_and_uses_stable_thread_tie_breaks() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let zeta = WorkspaceId::host_windows(r"C:\work\zeta");
    let alpha = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut state = WorkspaceConversationState::default();

    state.designate_primary_execution_target(&zeta).unwrap();
    state.attach_execution_target(&alpha).unwrap();

    let snapshot = member_thread_inventory::build_member_thread_inventory_snapshot(
        workspace_id,
        &state,
        member_thread_inventory::empty_groups_for_workspace_state(&state),
        vec![
            summary("thread_b", alpha.canonical_path(), Some("B"), 10, 20),
            summary("thread_a", alpha.canonical_path(), Some("A"), 10, 20),
            summary("thread_z", zeta.canonical_path(), Some("Z"), 10, 20),
        ],
        50,
    );

    assert_eq!(
        snapshot.groups()[0].canonical_path(),
        Some(alpha.canonical_path())
    );
    assert_eq!(
        snapshot.groups()[1].canonical_path(),
        Some(zeta.canonical_path())
    );
    assert_eq!(
        snapshot.groups()[0].threads()[0].thread_id().as_str(),
        "thread_a"
    );
    assert_eq!(
        snapshot.groups()[0].threads()[1].thread_id().as_str(),
        "thread_b"
    );
}

#[test]
fn failed_inventory_refresh_records_error_without_requeueing() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .select_runtime(RuntimeMode::HostWindows)
        .unwrap();
    let mut inventory =
        member_thread_inventory::MemberThreadInventoryState::new(workspace_id, &workspace_state);

    inventory.begin_refresh();
    inventory.fail_refresh("backend unavailable");

    assert!(!inventory.refreshing());
    assert!(!inventory.needs_refresh());
    assert_eq!(inventory.last_error(), Some("backend unavailable"));
}

#[test]
fn inventory_refreshing_state_is_requeued_for_backend_reopen() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .select_runtime(RuntimeMode::HostWindows)
        .unwrap();
    let mut inventory =
        member_thread_inventory::MemberThreadInventoryState::new(workspace_id, &workspace_state);

    assert!(inventory.needs_refresh());
    inventory.begin_refresh();
    assert!(inventory.refreshing());
    assert!(!inventory.needs_refresh());

    inventory.prepare_for_backend_reopen();

    assert!(!inventory.refreshing());
    assert!(inventory.needs_refresh());
    assert!(inventory.last_error().is_none());
}

#[test]
fn disconnected_inventory_refresh_keeps_snapshot_and_requests_retry() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let first = WorkspaceId::host_windows(r"C:\work\first");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&first)
        .unwrap();
    let mut inventory = member_thread_inventory::MemberThreadInventoryState::new(
        workspace_id.clone(),
        &workspace_state,
    );
    let snapshot = member_thread_inventory::build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        member_thread_inventory::empty_groups_for_workspace_state(&workspace_state),
        vec![summary(
            "thread_existing",
            first.canonical_path(),
            Some("Existing"),
            1,
            2,
        )],
        50,
    );

    inventory.finish_refresh(snapshot.clone(), &workspace_state);
    inventory.begin_refresh();
    inventory.abandon_refresh_for_backend_reopen("backend disconnected");

    assert_eq!(inventory.snapshot(), &snapshot);
    assert!(!inventory.refreshing());
    assert!(inventory.needs_refresh());
    assert_eq!(inventory.last_error(), Some("backend disconnected"));
}

fn summary(
    id: &str,
    cwd: &std::path::Path,
    name: Option<&str>,
    created_at: i64,
    updated_at: i64,
) -> ThreadSummary {
    ThreadSummary {
        id: id.to_string(),
        forked_from_id: None,
        cwd: cwd.to_path_buf(),
        preview: format!("{id} preview"),
        name: name.map(str::to_string),
        agent_nickname: None,
        path: None,
        created_at,
        updated_at,
        model_provider: "test".to_string(),
        ephemeral: false,
    }
}

fn summary_with_fork_parent(
    id: &str,
    cwd: &std::path::Path,
    name: Option<&str>,
    forked_from_id: &str,
    created_at: i64,
    updated_at: i64,
) -> ThreadSummary {
    let mut summary = summary(id, cwd, name, created_at, updated_at);
    summary.forked_from_id = Some(forked_from_id.to_string());
    summary
}

fn inventory_group(
    id: &str,
    cwd: &std::path::Path,
) -> member_thread_inventory::MemberThreadInventoryGroup {
    member_thread_inventory::MemberThreadInventoryGroup::new(
        member_thread_inventory::MemberThreadInventoryMemberKey::Explicit(
            WorkspaceMemberId::new(id).unwrap(),
        ),
        member_thread_inventory::MemberThreadInventoryMemberKind::Explicit,
        cwd.display().to_string(),
        RuntimeMode::HostWindows,
        Some(cwd.to_path_buf()),
        Vec::new(),
    )
}

fn thread_read_request_failed(
    code: i64,
    message: &str,
    data: Option<serde_json::Value>,
) -> ManagedBackendError {
    ManagedBackendError::RequestFailed {
        method: "thread/read".to_string(),
        error: JsonRpcError {
            code,
            message: message.to_string(),
            data,
        },
    }
}

fn inventory_thread<'a>(
    snapshot: &'a member_thread_inventory::MemberThreadInventorySnapshot,
    thread_id: &str,
) -> &'a member_thread_inventory::MemberThreadInventoryThread {
    snapshot
        .groups()
        .iter()
        .flat_map(|group| group.threads())
        .find(|thread| thread.thread_id().as_str() == thread_id)
        .expect("thread should exist in snapshot")
}
