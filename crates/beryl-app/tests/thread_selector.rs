use std::path::PathBuf;

use beryl_backend::ThreadSummary;
use beryl_model::{
    conversation::{
        ConversationThreadId, RegisteredConversationThread, WorkspaceConversationState,
    },
    workspace::{BerylWorkspaceId, RuntimeMode, WorkspaceId},
};
use gpui::{Bounds, point, px, size};

#[allow(dead_code)]
#[path = "../src/member_thread_inventory.rs"]
mod member_thread_inventory;

#[path = "../src/shell/column_selector.rs"]
mod column_selector;

#[path = "../src/shell/thread_selection.rs"]
mod thread_selection;

#[allow(dead_code)]
#[path = "../src/shell/thread_selector.rs"]
mod thread_selector;

use member_thread_inventory::{
    MemberThreadInventoryMemberKey, MemberThreadInventoryState,
    build_member_thread_inventory_snapshot, empty_groups_for_workspace_state,
};
use thread_selection::{ThreadSelectionRequest, exact_thread_selection_request};
use thread_selector::{
    ThreadSelectorColumnKey, ThreadSelectorProjection, ThreadSelectorSelection,
    ThreadSelectorState, thread_direct_child_count, thread_rows_for_column,
};

#[test]
fn thread_selector_opens_directly_to_threads_for_a_single_available_member() {
    let workspace_id = BerylWorkspaceId::new("thread_selector").unwrap();
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .select_runtime(RuntimeMode::HostWindows)
        .unwrap();
    let snapshot = member_thread_inventory::MemberThreadInventorySnapshot::empty_for_workspace(
        workspace_id,
        &workspace_state,
    );

    let mut selector = ThreadSelectorState::default();
    selector.open(&snapshot, None);

    assert_eq!(selector.columns().len(), 1);
    assert_eq!(
        selector.columns()[0].root_key(),
        &ThreadSelectorColumnKey::root_threads(MemberThreadInventoryMemberKey::ImplicitHome)
    );
}

#[test]
fn thread_selector_uses_member_to_thread_columns_for_multiple_members() {
    let (workspace_id, workspace_state, first, second) = workspace_with_two_members();
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![summary("thread_second", second.canonical_path())],
        50,
    );
    let first_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[0].id().clone(),
    );

    let mut selector = ThreadSelectorState::default();
    selector.open(&snapshot, None);

    assert_eq!(selector.columns().len(), 1);
    assert_eq!(
        selector.columns()[0].root_key(),
        &ThreadSelectorColumnKey::Members
    );

    assert!(selector.select_member(0, first_key.clone()));
    assert_eq!(selector.columns().len(), 2);
    assert_eq!(
        selector.columns()[0].selection(),
        Some(&ThreadSelectorSelection::Member(first_key.clone()))
    );
    assert_eq!(
        selector.columns()[1].root_key(),
        &ThreadSelectorColumnKey::root_threads(first_key)
    );
    assert_eq!(
        first.canonical_path(),
        PathBuf::from(r"C:\work\alpha").as_path()
    );
}

#[test]
fn thread_selector_omits_unavailable_explicit_members_from_inventory_columns() {
    let workspace_id = BerylWorkspaceId::new("thread_selector").unwrap();
    let available = WorkspaceId::host_windows(r"C:\work\available");
    let missing = WorkspaceId::host_windows(r"C:\work\missing");
    let mut workspace_state = WorkspaceConversationState::default();

    workspace_state
        .designate_primary_execution_target(&available)
        .unwrap();
    workspace_state.attach_execution_target(&missing).unwrap();
    let available_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[0].id().clone(),
    );
    let missing_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[1].id().clone(),
    );
    let missing_id = workspace_state.explicit_members()[1].id().clone();
    workspace_state
        .mark_explicit_member_path_not_found(&missing_id)
        .unwrap();

    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![summary("thread_available", available.canonical_path())],
        50,
    );
    let mut selector = ThreadSelectorState::default();
    selector.open(&snapshot, None);

    assert!(snapshot.group(&available_key).is_some());
    assert!(snapshot.group(&missing_key).is_none());
    assert_eq!(selector.columns().len(), 1);
    assert_eq!(
        selector.columns()[0].root_key(),
        &ThreadSelectorColumnKey::root_threads(available_key)
    );
}

#[test]
fn thread_selector_open_preselects_active_thread_path_for_multiple_members() {
    let (workspace_id, workspace_state, first, second) = workspace_with_two_members();
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary("thread_first", first.canonical_path()),
            summary("thread_second", second.canonical_path()),
        ],
        50,
    );
    let second_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[1].id().clone(),
    );
    let active_thread_id = ConversationThreadId::new("thread_second");

    let mut selector = ThreadSelectorState::default();
    selector.open(&snapshot, Some(active_thread_id.clone()));

    assert_eq!(selector.columns().len(), 2);
    assert_eq!(
        selector.columns()[0].root_key(),
        &ThreadSelectorColumnKey::Members
    );
    assert_eq!(
        selector.columns()[0].selection(),
        Some(&ThreadSelectorSelection::Member(second_key.clone()))
    );
    assert_eq!(
        selector.columns()[1].root_key(),
        &ThreadSelectorColumnKey::root_threads(second_key)
    );
    assert_eq!(
        selector.columns()[1].selection(),
        Some(&ThreadSelectorSelection::Thread(active_thread_id))
    );
}

#[test]
fn thread_selector_reconcile_preselects_active_thread_when_refresh_adds_it() {
    let (workspace_id, workspace_state, _, second) = workspace_with_two_members();
    let active_thread_id = ConversationThreadId::new("thread_second");
    let initial_snapshot = build_member_thread_inventory_snapshot(
        workspace_id.clone(),
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        Vec::new(),
        50,
    );
    let refreshed_snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![summary("thread_second", second.canonical_path())],
        60,
    );
    let second_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[1].id().clone(),
    );

    let mut selector = ThreadSelectorState::default();
    selector.open(&initial_snapshot, Some(active_thread_id.clone()));
    selector.reconcile_snapshot(&refreshed_snapshot);

    assert_eq!(
        selector.columns()[0].selection(),
        Some(&ThreadSelectorSelection::Member(second_key.clone()))
    );
    assert_eq!(
        selector.columns()[1].root_key(),
        &ThreadSelectorColumnKey::root_threads(second_key)
    );
    assert_eq!(
        selector.columns()[1].selection(),
        Some(&ThreadSelectorSelection::Thread(active_thread_id))
    );
}

#[test]
fn thread_selector_snapshot_orders_thread_rows_by_latest_update_first() {
    let (workspace_id, workspace_state, first) = workspace_with_single_member();
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary_with_updated("thread_old", first.canonical_path(), 10),
            summary_with_updated("thread_new", first.canonical_path(), 30),
            summary_with_updated("thread_middle", first.canonical_path(), 20),
        ],
        50,
    );

    let thread_ids = snapshot.groups()[0]
        .threads()
        .iter()
        .map(|thread| thread.thread_id().as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        thread_ids,
        vec!["thread_new", "thread_middle", "thread_old"]
    );
}

#[test]
fn thread_selector_tracks_selected_and_active_thread_rows_separately() {
    let (workspace_id, workspace_state, first, _) = workspace_with_two_members();
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary("thread_a", first.canonical_path()),
            summary("thread_b", first.canonical_path()),
        ],
        50,
    );
    let member_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[0].id().clone(),
    );
    let thread_a = ConversationThreadId::new("thread_a");
    let thread_b = ConversationThreadId::new("thread_b");

    let mut selector = ThreadSelectorState::default();
    selector.open(&snapshot, Some(thread_b.clone()));
    selector.select_member(0, member_key);
    selector.select_thread(1, thread_a.clone());

    assert_eq!(
        selector.thread_row_state(1, &thread_a),
        thread_selector::ThreadSelectorThreadRowState {
            selected: true,
            active: false,
        }
    );
    assert_eq!(
        selector.thread_row_state(1, &thread_b),
        thread_selector::ThreadSelectorThreadRowState {
            selected: false,
            active: true,
        }
    );
}

#[test]
fn thread_selector_close_and_dismissal_keep_anchor_clicks_inside() {
    let workspace_id = BerylWorkspaceId::new("thread_selector").unwrap();
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .select_runtime(RuntimeMode::HostWindows)
        .unwrap();
    let snapshot = member_thread_inventory::MemberThreadInventorySnapshot::empty_for_workspace(
        workspace_id,
        &workspace_state,
    );
    let mut selector = ThreadSelectorState::default();

    assert!(!selector.is_open());
    assert!(selector.toggle(&snapshot, None));
    assert!(selector.is_open());
    assert!(!selector.toggle(&snapshot, None));
    assert!(!selector.is_open());

    selector.open(&snapshot, None);
    selector.set_anchor_bounds(Some(Bounds::new(
        point(px(100.0), px(40.0)),
        size(px(260.0), px(32.0)),
    )));
    selector.set_popup_bounds(Some(Bounds::new(
        point(px(100.0), px(76.0)),
        size(px(360.0), px(280.0)),
    )));

    assert!(!selector.should_dismiss_for_mouse_down(point(px(120.0), px(52.0))));
    assert!(!selector.should_dismiss_for_mouse_down(point(px(140.0), px(96.0))));
    assert!(selector.should_dismiss_for_mouse_down(point(px(40.0), px(96.0))));
}

#[test]
fn thread_selector_uses_stale_snapshot_while_inventory_refresh_is_pending_or_failed() {
    let (workspace_id, workspace_state, first, _) = workspace_with_two_members();
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id.clone(),
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![summary("thread_a", first.canonical_path())],
        50,
    );
    let mut inventory = MemberThreadInventoryState::new(workspace_id, &workspace_state);

    inventory.finish_refresh(snapshot.clone(), &workspace_state);
    inventory.begin_refresh();

    let mut selector = ThreadSelectorState::default();
    selector.open(inventory.snapshot(), None);

    assert_eq!(inventory.snapshot(), &snapshot);
    assert!(inventory.refreshing());
    assert_eq!(
        selector.columns()[0].root_key(),
        &ThreadSelectorColumnKey::Members
    );

    inventory.fail_refresh("backend unavailable");

    assert_eq!(inventory.snapshot(), &snapshot);
    assert_eq!(inventory.last_error(), Some("backend unavailable"));
}

#[test]
fn thread_selector_reconciles_columns_when_a_refreshed_snapshot_drops_a_member() {
    let (workspace_id, workspace_state, _, second) = workspace_with_two_members();
    let initial_snapshot = build_member_thread_inventory_snapshot(
        workspace_id.clone(),
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![summary("thread_second", second.canonical_path())],
        50,
    );
    let first_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[0].id().clone(),
    );
    let second_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[1].id().clone(),
    );
    let mut selector = ThreadSelectorState::default();

    selector.open(&initial_snapshot, None);
    selector.select_member(0, second_key.clone());
    assert_eq!(
        selector.columns()[1].root_key(),
        &ThreadSelectorColumnKey::root_threads(second_key)
    );

    let mut reduced_state = WorkspaceConversationState::default();
    reduced_state
        .designate_primary_execution_target(&WorkspaceId::host_windows(r"C:\work\alpha"))
        .unwrap();
    let reduced_snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &reduced_state,
        empty_groups_for_workspace_state(&reduced_state),
        Vec::new(),
        60,
    );

    selector.reconcile_snapshot(&reduced_snapshot);

    assert_eq!(selector.columns().len(), 1);
    assert_eq!(
        selector.columns()[0].root_key(),
        &ThreadSelectorColumnKey::root_threads(first_key)
    );
}

#[test]
fn thread_selector_selected_thread_builds_exact_activation_request() {
    let (workspace_id, workspace_state, first, _) = workspace_with_two_members();
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![summary("thread_a", first.canonical_path())],
        50,
    );
    let member_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[0].id().clone(),
    );
    let mut selector = ThreadSelectorState::default();

    selector.open(&snapshot, None);
    selector.select_member(0, member_key);
    selector.select_thread(1, ConversationThreadId::new("thread_a"));

    let target = selector
        .selected_activation_target()
        .expect("selected thread should resolve from the latest snapshot");
    let request = exact_thread_selection_request(&target.thread_id, &target.label);

    assert_eq!(target.execution_target, first);
    assert_eq!(
        request,
        ThreadSelectionRequest::Exact {
            thread_id: "thread_a".to_string(),
            label: "Thread thread_a".to_string(),
        }
    );
}

#[test]
fn thread_selector_activation_label_reflects_live_backend_name_update() {
    let (workspace_id, mut workspace_state, first) = workspace_with_single_member();
    let thread_id = ConversationThreadId::new("thread_a");
    workspace_state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        first.clone(),
        "thread_a preview",
        None,
        1,
        2,
    ));
    workspace_state
        .set_thread_generated_title_if_absent(&thread_id, "Generated title", 3)
        .unwrap();
    let mut snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![summary_without_name("thread_a", first.canonical_path())],
        50,
    );
    let mut selector = ThreadSelectorState::default();

    selector.open(&snapshot, Some(thread_id.clone()));
    assert_eq!(
        selector
            .selected_activation_target()
            .expect("active thread should be selected")
            .label,
        "Generated title"
    );

    workspace_state
        .set_thread_backend_name(&thread_id, Some("Backend title".to_string()))
        .unwrap();
    assert!(snapshot.update_thread_backend_name(
        &workspace_state,
        &thread_id,
        Some("Backend title"),
    ));
    selector.reconcile_snapshot(&snapshot);

    assert_eq!(
        selector
            .selected_activation_target()
            .expect("updated thread should remain selected")
            .label,
        "Backend title"
    );
}

#[test]
fn thread_selector_activation_label_uses_reconciled_refreshed_inventory_title() {
    let (workspace_id, mut workspace_state, first) = workspace_with_single_member();
    let thread_id = ConversationThreadId::new("thread_a");
    workspace_state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        first.clone(),
        "thread_a preview",
        None,
        1,
        2,
    ));
    let stale_snapshot = build_member_thread_inventory_snapshot(
        workspace_id.clone(),
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![summary_without_name("thread_a", first.canonical_path())],
        50,
    );
    assert_eq!(
        stale_snapshot.groups()[0].threads()[0].title(),
        "Untitled thread"
    );

    workspace_state
        .set_thread_generated_title_if_absent(&thread_id, "Generated title", 3)
        .unwrap();
    let mut inventory = MemberThreadInventoryState::new(workspace_id, &workspace_state);
    inventory.finish_refresh(stale_snapshot, &workspace_state);
    let mut selector = ThreadSelectorState::default();

    selector.open(inventory.snapshot(), Some(thread_id.clone()));

    assert_eq!(
        selector
            .selected_activation_target()
            .expect("active thread should be selected")
            .label,
        "Generated title"
    );
}

#[test]
fn thread_selector_single_member_opens_fork_column_from_selected_root() {
    let (workspace_id, workspace_state, first) = workspace_with_single_member();
    let member_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[0].id().clone(),
    );
    let parent_id = ConversationThreadId::new("thread_parent");
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary_with_updated("thread_parent", first.canonical_path(), 10),
            summary_with_fork_parent_and_updated(
                "thread_child",
                first.canonical_path(),
                "thread_parent",
                20,
            ),
        ],
        50,
    );
    let root_column = ThreadSelectorColumnKey::root_threads(member_key.clone());

    let mut selector = ThreadSelectorState::default();
    selector.open(&snapshot, None);

    assert_eq!(
        column_thread_ids(&snapshot, &root_column),
        vec!["thread_parent"]
    );
    assert!(selector.select_thread(0, parent_id.clone()));
    assert_eq!(selector.columns().len(), 2);
    assert_eq!(
        selector.columns()[1].root_key(),
        &ThreadSelectorColumnKey::Threads {
            member_key,
            parent_thread_id: Some(parent_id),
        }
    );
    assert_eq!(
        column_thread_ids(&snapshot, selector.columns()[1].root_key()),
        vec!["thread_child"]
    );
}

#[test]
fn thread_selector_multi_member_opens_member_root_and_fork_columns() {
    let (workspace_id, workspace_state, first, second) = workspace_with_two_members();
    let first_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[0].id().clone(),
    );
    let parent_id = ConversationThreadId::new("thread_parent");
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary_with_updated("thread_parent", first.canonical_path(), 10),
            summary_with_fork_parent_and_updated(
                "thread_child",
                first.canonical_path(),
                "thread_parent",
                20,
            ),
            summary("thread_other_member", second.canonical_path()),
        ],
        50,
    );

    let mut selector = ThreadSelectorState::default();
    selector.open(&snapshot, None);

    assert!(selector.select_member(0, first_key.clone()));
    assert!(selector.select_thread(1, parent_id.clone()));
    assert_eq!(selector.columns().len(), 3);
    assert_eq!(
        selector.columns()[1].root_key(),
        &ThreadSelectorColumnKey::root_threads(first_key.clone())
    );
    assert_eq!(
        selector.columns()[2].root_key(),
        &ThreadSelectorColumnKey::Threads {
            member_key: first_key,
            parent_thread_id: Some(parent_id),
        }
    );
}

#[test]
fn thread_selector_open_preselects_active_deep_descendant_path() {
    let (workspace_id, workspace_state, first) = workspace_with_single_member();
    let member_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[0].id().clone(),
    );
    let root_id = ConversationThreadId::new("thread_root");
    let child_id = ConversationThreadId::new("thread_child");
    let leaf_id = ConversationThreadId::new("thread_leaf");
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary_with_updated("thread_root", first.canonical_path(), 10),
            summary_with_fork_parent_and_updated(
                "thread_child",
                first.canonical_path(),
                "thread_root",
                20,
            ),
            summary_with_fork_parent_and_updated(
                "thread_leaf",
                first.canonical_path(),
                "thread_child",
                30,
            ),
        ],
        50,
    );

    let mut selector = ThreadSelectorState::default();
    selector.open(&snapshot, Some(leaf_id.clone()));

    assert_eq!(selector.columns().len(), 3);
    assert_eq!(
        selector.columns()[0].root_key(),
        &ThreadSelectorColumnKey::root_threads(member_key.clone())
    );
    assert_eq!(
        selector.columns()[0].selection(),
        Some(&ThreadSelectorSelection::Thread(root_id.clone()))
    );
    assert_eq!(
        selector.columns()[1].root_key(),
        &ThreadSelectorColumnKey::Threads {
            member_key: member_key.clone(),
            parent_thread_id: Some(root_id),
        }
    );
    assert_eq!(
        selector.columns()[1].selection(),
        Some(&ThreadSelectorSelection::Thread(child_id.clone()))
    );
    assert_eq!(
        selector.columns()[2].root_key(),
        &ThreadSelectorColumnKey::Threads {
            member_key,
            parent_thread_id: Some(child_id),
        }
    );
    assert_eq!(
        selector.columns()[2].selection(),
        Some(&ThreadSelectorSelection::Thread(leaf_id))
    );
}

#[test]
fn thread_selector_treats_missing_and_cross_member_parents_as_roots() {
    let (workspace_id, workspace_state, first, second) = workspace_with_two_members();
    let first_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[0].id().clone(),
    );
    let second_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[1].id().clone(),
    );
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary_with_updated("thread_parent", first.canonical_path(), 10),
            summary_with_fork_parent_and_updated(
                "thread_missing_parent",
                first.canonical_path(),
                "thread_absent",
                20,
            ),
            summary_with_fork_parent_and_updated(
                "thread_cross_member_child",
                second.canonical_path(),
                "thread_parent",
                30,
            ),
        ],
        50,
    );

    assert_eq!(
        column_thread_ids(&snapshot, &ThreadSelectorColumnKey::root_threads(first_key)),
        vec!["thread_missing_parent", "thread_parent"]
    );
    assert_eq!(
        column_thread_ids(
            &snapshot,
            &ThreadSelectorColumnKey::root_threads(second_key)
        ),
        vec!["thread_cross_member_child"]
    );
}

#[test]
fn thread_selector_counts_only_direct_visible_child_forks() {
    let (workspace_id, workspace_state, first) = workspace_with_single_member();
    let member_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[0].id().clone(),
    );
    let root_id = ConversationThreadId::new("thread_root");
    let child_a_id = ConversationThreadId::new("thread_child_a");
    let child_b_id = ConversationThreadId::new("thread_child_b");
    let grandchild_id = ConversationThreadId::new("thread_grandchild");
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary_with_updated("thread_root", first.canonical_path(), 10),
            summary_with_fork_parent_and_updated(
                "thread_child_a",
                first.canonical_path(),
                "thread_root",
                20,
            ),
            summary_with_fork_parent_and_updated(
                "thread_child_b",
                first.canonical_path(),
                "thread_root",
                30,
            ),
            summary_with_fork_parent_and_updated(
                "thread_grandchild",
                first.canonical_path(),
                "thread_child_a",
                40,
            ),
        ],
        50,
    );

    assert_eq!(
        thread_direct_child_count(&snapshot, &member_key, &root_id),
        2
    );
    assert_eq!(
        thread_direct_child_count(&snapshot, &member_key, &child_a_id),
        1
    );
    assert_eq!(
        thread_direct_child_count(&snapshot, &member_key, &child_b_id),
        0
    );
    assert_eq!(
        thread_direct_child_count(&snapshot, &member_key, &grandchild_id),
        0
    );
}

#[test]
fn thread_selector_child_counts_ignore_non_visible_parent_relationships() {
    let (workspace_id, workspace_state, first, second) = workspace_with_two_members();
    let first_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[0].id().clone(),
    );
    let second_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[1].id().clone(),
    );
    let first_parent_id = ConversationThreadId::new("thread_parent");
    let second_parent_id = ConversationThreadId::new("thread_second_parent");
    let missing_parent_child_id = ConversationThreadId::new("thread_missing_parent_child");
    let cross_member_child_id = ConversationThreadId::new("thread_cross_member_child");
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary_with_updated("thread_parent", first.canonical_path(), 10),
            summary_with_fork_parent_and_updated(
                "thread_missing_parent_child",
                first.canonical_path(),
                "thread_absent",
                20,
            ),
            summary_with_updated("thread_second_parent", second.canonical_path(), 30),
            summary_with_fork_parent_and_updated(
                "thread_cross_member_child",
                second.canonical_path(),
                "thread_parent",
                40,
            ),
        ],
        50,
    );

    assert_eq!(
        thread_direct_child_count(&snapshot, &first_key, &first_parent_id),
        0
    );
    assert_eq!(
        thread_direct_child_count(&snapshot, &first_key, &missing_parent_child_id),
        0
    );
    assert_eq!(
        thread_direct_child_count(&snapshot, &second_key, &second_parent_id),
        0
    );
    assert_eq!(
        thread_direct_child_count(&snapshot, &second_key, &cross_member_child_id),
        0
    );
}

#[test]
fn thread_selector_child_counts_ignore_invalid_cyclic_lineage() {
    let (workspace_id, workspace_state, first) = workspace_with_single_member();
    let member_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[0].id().clone(),
    );
    let thread_a_id = ConversationThreadId::new("thread_a");
    let thread_b_id = ConversationThreadId::new("thread_b");
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary_with_fork_parent_and_updated(
                "thread_a",
                first.canonical_path(),
                "thread_b",
                10,
            ),
            summary_with_fork_parent_and_updated(
                "thread_b",
                first.canonical_path(),
                "thread_a",
                20,
            ),
        ],
        50,
    );

    assert_eq!(
        thread_direct_child_count(&snapshot, &member_key, &thread_a_id),
        0
    );
    assert_eq!(
        thread_direct_child_count(&snapshot, &member_key, &thread_b_id),
        0
    );
}

#[test]
fn thread_selector_duplicate_title_forks_activate_by_thread_id() {
    let (workspace_id, workspace_state, first) = workspace_with_single_member();
    let parent_id = ConversationThreadId::new("thread_parent");
    let child_b_id = ConversationThreadId::new("thread_child_b");
    let mut child_a = summary_with_fork_parent_and_updated(
        "thread_child_a",
        first.canonical_path(),
        "thread_parent",
        20,
    );
    child_a.name = Some("Same title".to_string());
    let mut child_b = summary_with_fork_parent_and_updated(
        "thread_child_b",
        first.canonical_path(),
        "thread_parent",
        30,
    );
    child_b.name = Some("Same title".to_string());
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary_with_updated("thread_parent", first.canonical_path(), 10),
            child_a,
            child_b,
        ],
        50,
    );

    let mut selector = ThreadSelectorState::default();
    selector.open(&snapshot, None);
    selector.select_thread(0, parent_id);
    selector.select_thread(1, child_b_id.clone());

    let target = selector
        .selected_activation_target()
        .expect("selected duplicate-title child should resolve");
    assert_eq!(target.thread_id, child_b_id);
    assert_eq!(target.label, "Same title");
}

#[test]
fn thread_selector_branch_row_with_children_activates_exact_selected_thread() {
    let (workspace_id, workspace_state, first) = workspace_with_single_member();
    let parent_id = ConversationThreadId::new("thread_parent");
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary_with_updated("thread_parent", first.canonical_path(), 10),
            summary_with_fork_parent_and_updated(
                "thread_child",
                first.canonical_path(),
                "thread_parent",
                20,
            ),
        ],
        50,
    );

    let mut selector = ThreadSelectorState::default();
    selector.open(&snapshot, None);
    selector.select_thread(0, parent_id.clone());

    assert_eq!(selector.columns().len(), 2);
    let target = selector
        .selected_activation_target()
        .expect("selected parent should remain the activation target");
    assert_eq!(target.thread_id, parent_id);
}

#[test]
fn thread_selector_reselecting_parent_clears_child_selection_and_reports_change() {
    let (workspace_id, workspace_state, first) = workspace_with_single_member();
    let parent_id = ConversationThreadId::new("thread_parent");
    let child_id = ConversationThreadId::new("thread_child");
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary_with_updated("thread_parent", first.canonical_path(), 10),
            summary_with_fork_parent_and_updated(
                "thread_child",
                first.canonical_path(),
                "thread_parent",
                20,
            ),
        ],
        50,
    );

    let mut selector = ThreadSelectorState::default();
    selector.open(&snapshot, None);
    assert!(selector.select_thread(0, parent_id.clone()));
    assert!(selector.select_thread(1, child_id.clone()));

    assert!(selector.select_thread(0, parent_id.clone()));

    assert_eq!(selector.columns().len(), 2);
    assert_eq!(
        selector.columns()[0].selection(),
        Some(&ThreadSelectorSelection::Thread(parent_id.clone()))
    );
    assert_eq!(selector.columns()[1].selection(), None);

    let target = selector
        .selected_activation_target()
        .expect("reselected parent should resolve");
    assert_eq!(target.thread_id, parent_id);
}

#[test]
fn thread_selector_sorts_roots_by_newest_activity_in_branch_subtree() {
    let (workspace_id, workspace_state, first) = workspace_with_single_member();
    let member_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[0].id().clone(),
    );
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary_with_updated("thread_old_root", first.canonical_path(), 1),
            summary_with_fork_parent_and_updated(
                "thread_recent_child",
                first.canonical_path(),
                "thread_old_root",
                100,
            ),
            summary_with_updated("thread_new_root", first.canonical_path(), 50),
        ],
        50,
    );

    assert_eq!(
        column_thread_ids(
            &snapshot,
            &ThreadSelectorColumnKey::root_threads(member_key)
        ),
        vec!["thread_old_root", "thread_new_root"]
    );
}

#[test]
fn thread_selector_projection_matches_current_rows_counts_and_paths() {
    let (workspace_id, workspace_state, first, second) = workspace_with_two_members();
    let first_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[0].id().clone(),
    );
    let second_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[1].id().clone(),
    );
    let parent_id = ConversationThreadId::new("thread_parent");
    let child_id = ConversationThreadId::new("thread_child");
    let missing_parent_id = ConversationThreadId::new("thread_missing_parent");
    let cross_member_child_id = ConversationThreadId::new("thread_cross_member_child");
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary_with_updated("thread_parent", first.canonical_path(), 10),
            summary_with_fork_parent_and_updated(
                "thread_child",
                first.canonical_path(),
                "thread_parent",
                20,
            ),
            summary_with_fork_parent_and_updated(
                "thread_grandchild",
                first.canonical_path(),
                "thread_child",
                30,
            ),
            summary_with_fork_parent_and_updated(
                "thread_missing_parent",
                first.canonical_path(),
                "thread_absent",
                40,
            ),
            summary_with_fork_parent_and_updated(
                "thread_cross_member_child",
                second.canonical_path(),
                "thread_parent",
                50,
            ),
        ],
        50,
    );
    let projection = ThreadSelectorProjection::new(&snapshot);
    let first_root_column = ThreadSelectorColumnKey::root_threads(first_key.clone());
    let first_child_column = ThreadSelectorColumnKey::Threads {
        member_key: first_key.clone(),
        parent_thread_id: Some(parent_id.clone()),
    };
    let second_root_column = ThreadSelectorColumnKey::root_threads(second_key.clone());

    assert_eq!(
        projection_column_thread_ids(&projection, &first_root_column),
        column_thread_ids(&snapshot, &first_root_column)
    );
    assert_eq!(
        projection_column_thread_ids(&projection, &first_child_column),
        column_thread_ids(&snapshot, &first_child_column)
    );
    assert_eq!(
        projection_column_thread_ids(&projection, &second_root_column),
        column_thread_ids(&snapshot, &second_root_column)
    );
    assert_eq!(
        projection.direct_child_count(&first_key, &parent_id),
        thread_direct_child_count(&snapshot, &first_key, &parent_id)
    );
    assert_eq!(
        projection.direct_child_count(&first_key, &missing_parent_id),
        thread_direct_child_count(&snapshot, &first_key, &missing_parent_id)
    );
    assert_eq!(
        projection.direct_child_count(&second_key, &cross_member_child_id),
        thread_direct_child_count(&snapshot, &second_key, &cross_member_child_id)
    );
    assert!(projection.thread_exists_in_column(&first_child_column, &child_id));
    assert_eq!(
        projection.member_key_for_thread(&child_id),
        Some(first_key.clone())
    );
    assert_eq!(
        projection.thread_path_for_thread(&first_key, &child_id),
        Some(vec![parent_id, child_id])
    );
}

#[test]
fn thread_selector_projection_treats_self_parent_as_root_row() {
    let (workspace_id, workspace_state, first) = workspace_with_single_member();
    let member_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[0].id().clone(),
    );
    let self_parent_id = ConversationThreadId::new("thread_self_parent");
    let child_id = ConversationThreadId::new("thread_child");
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary_with_fork_parent_and_updated(
                "thread_self_parent",
                first.canonical_path(),
                "thread_self_parent",
                10,
            ),
            summary_with_fork_parent_and_updated(
                "thread_child",
                first.canonical_path(),
                "thread_self_parent",
                20,
            ),
        ],
        50,
    );
    let projection = ThreadSelectorProjection::new(&snapshot);
    let root_column = ThreadSelectorColumnKey::root_threads(member_key.clone());
    let child_column = ThreadSelectorColumnKey::Threads {
        member_key: member_key.clone(),
        parent_thread_id: Some(self_parent_id.clone()),
    };

    assert_eq!(
        projection_column_thread_ids(&projection, &root_column),
        vec!["thread_self_parent"]
    );
    assert_eq!(
        projection_column_thread_ids(&projection, &child_column),
        vec!["thread_child"]
    );
    assert_eq!(
        projection.thread_path_for_thread(&member_key, &child_id),
        Some(vec![self_parent_id.clone(), child_id])
    );
    assert_eq!(
        projection.direct_child_count(&member_key, &self_parent_id),
        1
    );
}

#[test]
fn thread_selector_projection_uses_full_sort_tie_breaks() {
    let (workspace_id, workspace_state, first) = workspace_with_single_member();
    let member_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[0].id().clone(),
    );
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary_with_created_and_updated("thread_by_id_b", first.canonical_path(), 3, 30),
            summary_with_fork_parent_created_and_updated(
                "thread_by_id_b_child",
                first.canonical_path(),
                "thread_by_id_b",
                1,
                100,
            ),
            summary_with_created_and_updated("thread_by_id_a", first.canonical_path(), 3, 30),
            summary_with_fork_parent_created_and_updated(
                "thread_by_id_a_child",
                first.canonical_path(),
                "thread_by_id_a",
                1,
                100,
            ),
            summary_with_created_and_updated("thread_created_high", first.canonical_path(), 4, 30),
            summary_with_fork_parent_created_and_updated(
                "thread_created_high_child",
                first.canonical_path(),
                "thread_created_high",
                1,
                100,
            ),
            summary_with_created_and_updated("thread_updated_high", first.canonical_path(), 1, 40),
            summary_with_fork_parent_created_and_updated(
                "thread_updated_high_child",
                first.canonical_path(),
                "thread_updated_high",
                1,
                100,
            ),
        ],
        50,
    );
    let projection = ThreadSelectorProjection::new(&snapshot);

    assert_eq!(
        projection_column_thread_ids(
            &projection,
            &ThreadSelectorColumnKey::root_threads(member_key)
        ),
        vec![
            "thread_updated_high",
            "thread_created_high",
            "thread_by_id_a",
            "thread_by_id_b",
        ]
    );
}

#[test]
fn thread_selector_projection_handles_large_broad_and_deep_inventory() {
    let (workspace_id, workspace_state, first) = workspace_with_single_member();
    let member_key = MemberThreadInventoryMemberKey::Explicit(
        workspace_state.explicit_members()[0].id().clone(),
    );
    let mut summaries = Vec::new();
    summaries.push(summary_with_updated(
        "thread_chain_root",
        first.canonical_path(),
        1,
    ));
    for index in 1..=90 {
        let id = format!("thread_chain_{index:03}");
        let parent_id = if index == 1 {
            "thread_chain_root".to_string()
        } else {
            format!("thread_chain_{:03}", index - 1)
        };
        summaries.push(summary_with_fork_parent_and_updated(
            &id,
            first.canonical_path(),
            &parent_id,
            1000 + index,
        ));
    }
    for root_index in 0..60 {
        let root_id = format!("thread_broad_root_{root_index:03}");
        summaries.push(summary_with_updated(
            &root_id,
            first.canonical_path(),
            100 + root_index,
        ));
        for child_index in 0..2 {
            let child_id = format!("thread_broad_root_{root_index:03}_child_{child_index}");
            summaries.push(summary_with_fork_parent_and_updated(
                &child_id,
                first.canonical_path(),
                &root_id,
                200 + root_index + child_index,
            ));
        }
    }
    let snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        summaries,
        50,
    );
    let projection = ThreadSelectorProjection::new(&snapshot);
    let root_column = ThreadSelectorColumnKey::root_threads(member_key.clone());
    let chain_root_id = ConversationThreadId::new("thread_chain_root");
    let chain_leaf_id = ConversationThreadId::new("thread_chain_090");

    let root_rows = projection_column_thread_ids(&projection, &root_column);
    assert_eq!(root_rows.len(), 61);
    assert_eq!(
        root_rows.first().map(String::as_str),
        Some("thread_chain_root")
    );
    assert_eq!(
        projection.direct_child_count(&member_key, &chain_root_id),
        1
    );
    assert_eq!(
        projection.direct_child_count(&member_key, &chain_leaf_id),
        0
    );
    assert_eq!(
        projection
            .thread_path_for_thread(&member_key, &chain_leaf_id)
            .expect("deep chain leaf should have a path")
            .len(),
        91
    );
    assert_eq!(
        projection.direct_child_count(
            &member_key,
            &ConversationThreadId::new("thread_broad_root_000")
        ),
        2
    );
}

#[test]
fn thread_selector_reconcile_prunes_invalid_fork_path_without_substitution() {
    let (workspace_id, workspace_state, first) = workspace_with_single_member();
    let parent_id = ConversationThreadId::new("thread_parent");
    let child_id = ConversationThreadId::new("thread_child");
    let initial_snapshot = build_member_thread_inventory_snapshot(
        workspace_id.clone(),
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary_with_updated("thread_parent", first.canonical_path(), 10),
            summary_with_fork_parent_and_updated(
                "thread_child",
                first.canonical_path(),
                "thread_parent",
                20,
            ),
        ],
        50,
    );
    let refreshed_snapshot = build_member_thread_inventory_snapshot(
        workspace_id,
        &workspace_state,
        empty_groups_for_workspace_state(&workspace_state),
        vec![
            summary_with_fork_parent_and_updated(
                "thread_child",
                first.canonical_path(),
                "thread_parent",
                20,
            ),
            summary_with_updated("thread_other", first.canonical_path(), 30),
        ],
        60,
    );

    let mut selector = ThreadSelectorState::default();
    selector.open(&initial_snapshot, None);
    selector.select_thread(0, parent_id);
    selector.select_thread(1, child_id);

    selector.reconcile_snapshot(&refreshed_snapshot);

    assert_eq!(selector.columns().len(), 1);
    assert_eq!(selector.columns()[0].selection(), None);
}

fn workspace_with_two_members() -> (
    BerylWorkspaceId,
    WorkspaceConversationState,
    WorkspaceId,
    WorkspaceId,
) {
    let workspace_id = BerylWorkspaceId::new("thread_selector").unwrap();
    let first = WorkspaceId::host_windows(r"C:\work\alpha");
    let second = WorkspaceId::host_windows(r"C:\work\beta");
    let mut workspace_state = WorkspaceConversationState::default();

    workspace_state
        .designate_primary_execution_target(&first)
        .unwrap();
    workspace_state.attach_execution_target(&second).unwrap();

    (workspace_id, workspace_state, first, second)
}

fn workspace_with_single_member() -> (BerylWorkspaceId, WorkspaceConversationState, WorkspaceId) {
    let workspace_id = BerylWorkspaceId::new("thread_selector").unwrap();
    let first = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut workspace_state = WorkspaceConversationState::default();

    workspace_state
        .designate_primary_execution_target(&first)
        .unwrap();

    (workspace_id, workspace_state, first)
}

fn summary(id: &str, cwd: &std::path::Path) -> ThreadSummary {
    summary_with_updated(id, cwd, 2)
}

fn summary_with_fork_parent_and_updated(
    id: &str,
    cwd: &std::path::Path,
    parent_id: &str,
    updated_at: i64,
) -> ThreadSummary {
    let mut summary = summary_with_updated(id, cwd, updated_at);
    summary.forked_from_id = Some(parent_id.to_string());
    summary
}

fn summary_with_fork_parent_created_and_updated(
    id: &str,
    cwd: &std::path::Path,
    parent_id: &str,
    created_at: i64,
    updated_at: i64,
) -> ThreadSummary {
    let mut summary = summary_with_created_and_updated(id, cwd, created_at, updated_at);
    summary.forked_from_id = Some(parent_id.to_string());
    summary
}

fn summary_without_name(id: &str, cwd: &std::path::Path) -> ThreadSummary {
    let mut summary = summary_with_updated(id, cwd, 2);
    summary.name = None;
    summary
}

fn summary_with_updated(id: &str, cwd: &std::path::Path, updated_at: i64) -> ThreadSummary {
    summary_with_created_and_updated(id, cwd, 1, updated_at)
}

fn summary_with_created_and_updated(
    id: &str,
    cwd: &std::path::Path,
    created_at: i64,
    updated_at: i64,
) -> ThreadSummary {
    ThreadSummary {
        id: id.to_string(),
        forked_from_id: None,
        cwd: cwd.to_path_buf(),
        preview: format!("{id} preview"),
        name: Some(format!("Thread {id}")),
        agent_nickname: None,
        path: None,
        created_at,
        updated_at,
        model_provider: "test".to_string(),
        ephemeral: false,
    }
}

fn column_thread_ids(
    snapshot: &member_thread_inventory::MemberThreadInventorySnapshot,
    column_key: &ThreadSelectorColumnKey,
) -> Vec<String> {
    thread_rows_for_column(snapshot, column_key)
        .into_iter()
        .map(|thread| thread.thread_id().as_str().to_string())
        .collect()
}

fn projection_column_thread_ids(
    projection: &ThreadSelectorProjection,
    column_key: &ThreadSelectorColumnKey,
) -> Vec<String> {
    projection
        .thread_rows_for_column(column_key)
        .into_iter()
        .map(|thread| thread.thread_id().as_str().to_string())
        .collect()
}
