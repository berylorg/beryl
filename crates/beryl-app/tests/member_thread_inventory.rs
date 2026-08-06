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
fn inventory_groups_match_backend_threads_by_runtime_and_cwd() {
    let workspace_id = BerylWorkspaceId::new("runtime_inventory").unwrap();
    let host = WorkspaceId::host_windows(r"C:\work\shared");
    let wsl = WorkspaceId::wsl_linux("Ubuntu", r"C:\work\shared");
    let mut state = WorkspaceConversationState::default();
    state.designate_primary_execution_target(&host).unwrap();
    state.attach_execution_target(&wsl).unwrap();

    let snapshot =
        member_thread_inventory::build_member_thread_inventory_snapshot_for_backend_threads(
            workspace_id,
            &state,
            member_thread_inventory::empty_groups_for_workspace_state(&state),
            vec![
                member_thread_inventory::MemberThreadInventoryBackendThread::new(
                    RuntimeMode::HostWindows,
                    summary("host_thread", host.canonical_path(), None, 1, 10),
                ),
                member_thread_inventory::MemberThreadInventoryBackendThread::new(
                    RuntimeMode::WslLinux {
                        distro_name: "Ubuntu".to_string(),
                    },
                    summary("wsl_thread", wsl.canonical_path(), None, 2, 20),
                ),
            ],
            50,
        );

    let host_group = snapshot
        .groups()
        .iter()
        .find(|group| group.runtime() == host.runtime_mode())
        .unwrap();
    let wsl_group = snapshot
        .groups()
        .iter()
        .find(|group| group.runtime() == wsl.runtime_mode())
        .unwrap();

    assert_eq!(host_group.threads().len(), 1);
    assert_eq!(host_group.threads()[0].thread_id().as_str(), "host_thread");
    assert_eq!(wsl_group.threads().len(), 1);
    assert_eq!(wsl_group.threads()[0].thread_id().as_str(), "wsl_thread");
}

#[test]
fn inventory_deduplicates_backend_threads_by_runtime_thread_and_cwd() {
    let workspace_id = BerylWorkspaceId::new("runtime_inventory").unwrap();
    let host = WorkspaceId::host_windows(r"C:\work\shared");
    let mut state = WorkspaceConversationState::default();
    state.designate_primary_execution_target(&host).unwrap();

    let snapshot =
        member_thread_inventory::build_member_thread_inventory_snapshot_for_backend_threads(
            workspace_id,
            &state,
            member_thread_inventory::empty_groups_for_workspace_state(&state),
            vec![
                member_thread_inventory::MemberThreadInventoryBackendThread::new(
                    RuntimeMode::HostWindows,
                    summary("host_thread", host.canonical_path(), None, 1, 10),
                ),
                member_thread_inventory::MemberThreadInventoryBackendThread::new(
                    RuntimeMode::HostWindows,
                    summary("host_thread", host.canonical_path(), None, 1, 10),
                ),
            ],
            50,
        );

    assert_eq!(snapshot.groups().len(), 1);
    assert_eq!(snapshot.groups()[0].threads().len(), 1);
    assert_eq!(
        snapshot.groups()[0].threads()[0].thread_id().as_str(),
        "host_thread"
    );
}

#[test]
fn inventory_groups_exclude_unavailable_members_and_use_implicit_home_when_none_available() {
    let available = WorkspaceId::host_windows(r"C:\work\available");
    let missing = WorkspaceId::host_windows(r"C:\work\missing");
    let mut state = WorkspaceConversationState::default();
    state
        .designate_primary_execution_target(&available)
        .unwrap();
    state.attach_execution_target(&missing).unwrap();
    let available_id = state.explicit_members()[0].id().clone();
    let missing_id = state.explicit_members()[1].id().clone();

    state
        .mark_explicit_member_path_not_found(&missing_id)
        .unwrap();
    let groups = member_thread_inventory::empty_groups_for_workspace_state(&state);

    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].key(),
        &member_thread_inventory::MemberThreadInventoryMemberKey::Explicit(available_id)
    );

    state
        .mark_explicit_member_path_not_found(&state.explicit_members()[0].id().clone())
        .unwrap();
    let groups = member_thread_inventory::empty_groups_for_workspace_state(&state);

    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].kind(),
        &member_thread_inventory::MemberThreadInventoryMemberKind::ImplicitHome
    );
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
fn inventory_reconciliation_keeps_backend_fork_parent_separate_from_orchestration_root() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let execution_target = WorkspaceId::host_windows(r"C:\work\first");
    let root_id = ConversationThreadId::new("orchestration_root");
    let phase_child_id = ConversationThreadId::new("phase_child");
    let backend_only_child_id = ConversationThreadId::new("backend_only_child");
    let mut state = WorkspaceConversationState::default();

    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        root_id.clone(),
        execution_target.clone(),
        "Root preview",
        None,
        1,
        2,
    ));
    state.remember_thread(RegisteredConversationThread::new(
        phase_child_id.clone(),
        execution_target.clone(),
        "Phase child preview",
        Some("Stale backend summary".to_string()),
        3,
        4,
    ));
    state.record_thread_as_orchestration_root(&root_id).unwrap();
    state
        .record_thread_orchestration_root(&phase_child_id, &root_id)
        .unwrap();

    let snapshot = member_thread_inventory::build_member_thread_inventory_snapshot(
        workspace_id,
        &state,
        member_thread_inventory::empty_groups_for_workspace_state(&state),
        vec![
            summary_with_fork_parent(
                "phase_child",
                execution_target.canonical_path(),
                Some("Fresh backend summary"),
                "backend_fork_parent",
                3,
                8,
            ),
            summary_with_fork_parent(
                "backend_only_child",
                execution_target.canonical_path(),
                Some("Backend-only summary"),
                "backend_fork_parent",
                5,
                6,
            ),
        ],
        50,
    );

    let phase_child = inventory_thread(&snapshot, "phase_child");
    assert_eq!(
        phase_child
            .forked_from_id()
            .map(ConversationThreadId::as_str),
        Some("backend_fork_parent")
    );
    assert!(state.remember_thread(phase_child.to_registered_thread()));
    let reconciled_phase_child = state.thread_registration(&phase_child_id).unwrap();
    assert_eq!(
        reconciled_phase_child.orchestration_root_thread_id(),
        Some(&root_id)
    );
    assert_eq!(
        reconciled_phase_child.backend_name(),
        Some("Fresh backend summary")
    );

    let backend_only_child = inventory_thread(&snapshot, "backend_only_child");
    assert_eq!(
        backend_only_child
            .forked_from_id()
            .map(ConversationThreadId::as_str),
        Some("backend_fork_parent")
    );
    assert!(state.remember_thread(backend_only_child.to_registered_thread()));
    assert!(
        state
            .thread_registration(&backend_only_child_id)
            .unwrap()
            .orchestration_root_thread_id()
            .is_none()
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
fn inventory_preparation_truncates_large_backend_thread_sets_before_lineage_reads() {
    let member_cwd = PathBuf::from(r"C:\work\first");
    let members = vec![inventory_group("member_first", member_cwd.as_path())];
    let total = member_thread_inventory::MEMBER_THREAD_INVENTORY_MAX_BACKEND_THREADS + 5;
    let mut backend_threads = (0..total)
        .map(|index| {
            summary(
                &format!("thread_{index:04}"),
                member_cwd.as_path(),
                Some("Thread"),
                index as i64,
                index as i64,
            )
        })
        .collect::<Vec<_>>();
    let mut read_count = 0;

    member_thread_inventory::prepare_backend_threads_for_member_thread_inventory(
        &mut backend_threads,
        &members,
        |thread_id| {
            read_count += 1;
            Ok(summary(
                thread_id,
                member_cwd.as_path(),
                Some("Thread"),
                1,
                1,
            ))
        },
    )
    .unwrap();

    assert_eq!(
        backend_threads.len(),
        member_thread_inventory::MEMBER_THREAD_INVENTORY_MAX_BACKEND_THREADS
    );
    assert_eq!(
        read_count,
        member_thread_inventory::MEMBER_THREAD_INVENTORY_MAX_BACKEND_THREADS
    );
    assert_eq!(backend_threads[0].id, format!("thread_{:04}", total - 1));
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
fn bounded_inventory_enrichment_stops_at_read_budget_and_records_partial_lineage() {
    let cwd = PathBuf::from(r"C:\work\first");
    let mut backend_threads = vec![
        summary("thread_a", cwd.as_path(), Some("A"), 1, 3),
        summary("thread_b", cwd.as_path(), Some("B"), 2, 4),
    ];
    let mut requested = Vec::new();

    let (reads, partial) =
        member_thread_inventory::enrich_missing_thread_fork_parent_metadata_bounded(
            &mut backend_threads,
            1,
            |thread_id| {
                requested.push(thread_id.to_string());
                Ok(summary(thread_id, cwd.as_path(), None, 1, 1))
            },
        );

    assert_eq!(reads, 1);
    assert_eq!(requested, vec!["thread_a"]);
    let partial = partial.expect("metadata budget exhaustion should be explicit");
    assert!(!partial.row_coverage_truncated());
    assert!(partial.lineage_incomplete());
    assert_eq!(partial.reasons().len(), 1);
}

#[test]
fn bounded_inventory_enrichment_does_not_trust_mismatched_metadata_id() {
    let cwd = PathBuf::from(r"C:\work\first");
    let mut backend_threads = vec![summary("thread_child", cwd.as_path(), Some("Child"), 2, 20)];

    let (reads, partial) =
        member_thread_inventory::enrich_missing_thread_fork_parent_metadata_bounded(
            &mut backend_threads,
            4,
            |_| {
                Ok(summary_with_fork_parent(
                    "thread_other",
                    cwd.as_path(),
                    Some("Other"),
                    "thread_parent",
                    3,
                    30,
                ))
            },
        );

    assert_eq!(reads, 1);
    assert_eq!(backend_threads[0].forked_from_id, None);
    let partial = partial.expect("mismatched metadata should make lineage partial");
    assert!(partial.lineage_incomplete());
    assert!(partial.reasons()[0].contains("mismatched thread id"));
}

#[test]
fn inventory_snapshot_preserves_explicit_partial_coverage() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let first = WorkspaceId::host_windows(r"C:\work\first");
    let mut state = WorkspaceConversationState::default();
    state.designate_primary_execution_target(&first).unwrap();
    let coverage = member_thread_inventory::MemberThreadInventoryCoverage::Partial(
        member_thread_inventory::MemberThreadInventoryPartialCoverage::new()
            .with_row_coverage_truncated("result budget exhausted")
            .with_lineage_incomplete("metadata budget exhausted"),
    );

    let snapshot =
        member_thread_inventory::build_member_thread_inventory_snapshot_for_backend_threads_with_coverage(
            workspace_id,
            &state,
            member_thread_inventory::empty_groups_for_workspace_state(&state),
            vec![member_thread_inventory::MemberThreadInventoryBackendThread::new(
                RuntimeMode::HostWindows,
                summary("thread_a", first.canonical_path(), Some("A"), 1, 2),
            )],
            50,
            coverage,
        );

    let partial = snapshot
        .coverage()
        .partial()
        .expect("snapshot should remain explicitly partial");
    assert!(partial.row_coverage_truncated());
    assert!(partial.lineage_incomplete());
    assert_eq!(partial.reasons().len(), 2);
}

#[test]
fn inventory_partial_reasons_are_count_and_byte_bounded_and_counted_as_payload() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let state = WorkspaceConversationState::default();
    let mut partial = member_thread_inventory::MemberThreadInventoryPartialCoverage::new();
    for index in 0..20 {
        partial = partial.with_lineage_incomplete(format!("{index}:{}", "é".repeat(600)));
    }
    let snapshot = member_thread_inventory::MemberThreadInventorySnapshot::new_with_coverage(
        workspace_id,
        50,
        member_thread_inventory::MemberThreadInventoryCoverage::Partial(partial),
        member_thread_inventory::empty_groups_for_workspace_state(&state),
    );

    let partial = snapshot.coverage().partial().unwrap();
    assert_eq!(partial.reasons().len(), 8);
    assert!(partial.reasons().iter().all(|reason| reason.len() <= 512));
    assert_eq!(
        snapshot.retained_counts().payload_bytes,
        partial.reasons().iter().map(String::len).sum::<usize>()
    );
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
fn member_set_event_replaces_implicit_home_inventory_and_invalidates_in_flight_refresh() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let first = WorkspaceId::host_windows(r"C:\work\first");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .select_runtime(RuntimeMode::HostWindows)
        .unwrap();
    let mut inventory = member_thread_inventory::MemberThreadInventoryState::new(
        workspace_id.clone(),
        &workspace_state,
    );

    assert_eq!(
        inventory.snapshot().groups()[0].kind(),
        &member_thread_inventory::MemberThreadInventoryMemberKind::ImplicitHome
    );
    let stale_token = inventory.begin_refresh();

    workspace_state
        .designate_primary_execution_target(&first)
        .unwrap();
    inventory.apply_event(
        member_thread_inventory::MemberThreadInventoryEvent::MemberSetChanged,
        workspace_id.clone(),
        &workspace_state,
    );

    assert!(!inventory.refreshing());
    assert!(inventory.needs_refresh());
    assert_eq!(inventory.snapshot().groups().len(), 1);
    assert_eq!(
        inventory.snapshot().groups()[0].kind(),
        &member_thread_inventory::MemberThreadInventoryMemberKind::Explicit
    );
    assert_eq!(
        inventory.snapshot().groups()[0].canonical_path(),
        Some(first.canonical_path())
    );

    let stale_snapshot = member_thread_inventory::build_member_thread_inventory_snapshot(
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
    assert!(!inventory.finish_refresh_for_token(stale_token, stale_snapshot, &workspace_state));
    assert!(inventory.snapshot().groups()[0].threads().is_empty());
}

#[test]
fn freshness_events_requeue_inventory_without_clearing_snapshot() {
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
        workspace_id.clone(),
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

    inventory.apply_event(
        member_thread_inventory::MemberThreadInventoryEvent::SelectorFreshnessRequested,
        workspace_id.clone(),
        &workspace_state,
    );
    assert!(inventory.needs_refresh());
    assert_eq!(inventory.snapshot(), &snapshot);

    let stale_token = inventory.begin_refresh();
    inventory.apply_event(
        member_thread_inventory::MemberThreadInventoryEvent::BackendTargetOpening,
        workspace_id.clone(),
        &workspace_state,
    );
    assert!(!inventory.refreshing());
    assert!(inventory.needs_refresh());
    assert_eq!(inventory.snapshot(), &snapshot);
    assert!(!inventory.fail_refresh_for_token(stale_token, "stale backend failure"));

    inventory.apply_event(
        member_thread_inventory::MemberThreadInventoryEvent::BackendTargetAvailable,
        workspace_id,
        &workspace_state,
    );
    assert!(inventory.needs_refresh());
    assert!(inventory.last_error().is_none());
}

#[test]
fn content_change_during_refresh_schedules_follow_up_after_old_snapshot_publishes() {
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
    let stale_snapshot = member_thread_inventory::build_member_thread_inventory_snapshot(
        workspace_id.clone(),
        &workspace_state,
        member_thread_inventory::empty_groups_for_workspace_state(&workspace_state),
        vec![summary(
            "thread_before_content_change",
            first.canonical_path(),
            Some("Before content change"),
            1,
            2,
        )],
        50,
    );

    let token = inventory.begin_refresh();
    inventory.apply_event(
        member_thread_inventory::MemberThreadInventoryEvent::InventoryContentsChanged,
        workspace_id,
        &workspace_state,
    );

    assert!(inventory.finish_refresh_for_token(token, stale_snapshot, &workspace_state));
    assert!(!inventory.refreshing());
    assert!(inventory.needs_refresh());
}

#[test]
fn content_change_during_refresh_survives_worker_disconnect_failure() {
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

    let token = inventory.begin_refresh();
    inventory.apply_event(
        member_thread_inventory::MemberThreadInventoryEvent::InventoryContentsChanged,
        workspace_id,
        &workspace_state,
    );

    assert!(inventory.fail_refresh_for_token(token, "worker disconnected"));
    assert!(!inventory.refreshing());
    assert!(inventory.needs_refresh());
    assert_eq!(inventory.last_error(), Some("worker disconnected"));
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

    let token = inventory.begin_refresh();
    inventory.fail_refresh_for_token(token, "backend unavailable");

    assert!(!inventory.refreshing());
    assert!(!inventory.needs_refresh());
    assert_eq!(inventory.last_error(), Some("backend unavailable"));
}

#[test]
fn failed_refresh_preserves_last_partial_snapshot_instead_of_publishing_empty_success() {
    let workspace_id = BerylWorkspaceId::new("inventory").unwrap();
    let first = WorkspaceId::host_windows(r"C:\work\first");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&first)
        .unwrap();
    let coverage = member_thread_inventory::MemberThreadInventoryCoverage::Partial(
        member_thread_inventory::MemberThreadInventoryPartialCoverage::new()
            .with_row_coverage_truncated("page budget exhausted"),
    );
    let snapshot =
        member_thread_inventory::build_member_thread_inventory_snapshot_for_backend_threads_with_coverage(
            workspace_id.clone(),
            &workspace_state,
            member_thread_inventory::empty_groups_for_workspace_state(&workspace_state),
            vec![member_thread_inventory::MemberThreadInventoryBackendThread::new(
                RuntimeMode::HostWindows,
                summary("thread_existing", first.canonical_path(), Some("Existing"), 1, 2),
            )],
            50,
            coverage,
        );
    let mut inventory =
        member_thread_inventory::MemberThreadInventoryState::new(workspace_id, &workspace_state);
    let accepted_token = inventory.begin_refresh();
    assert!(inventory.finish_refresh_for_token(accepted_token, snapshot.clone(), &workspace_state));
    assert!(inventory.last_error().is_none());

    inventory.mark_refresh_needed();
    let failed_token = inventory.begin_refresh();
    assert!(inventory.fail_refresh_for_token(failed_token, "first page timed out"));

    assert_eq!(inventory.snapshot(), &snapshot);
    assert_eq!(inventory.last_error(), Some("first page timed out"));
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
