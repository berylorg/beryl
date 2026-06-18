#![allow(dead_code)]

#[path = "../src/member_thread_inventory.rs"]
mod member_thread_inventory;

use beryl_model::{
    conversation::{
        ConversationThreadId, RegisteredConversationThread, SyndicConversationId,
        SyndicConversationViewId, WorkspaceConversationState,
    },
    workspace::{BerylWorkspaceId, WorkspaceId},
};
use member_thread_inventory::build_workspace_syndic_catalog_snapshot;
use syndic_storage::{
    ConversationId, ConversationRecord, HistoryState, MAX_CONVERSATION_SUMMARY_READ_LIMIT,
    ProviderRevision, StoreOpenOptions, SyndicStore, SyndicWriteBatch, ThreadViewId,
};

#[test]
fn catalog_snapshot_joins_workspace_registrations_with_syndic_summaries() {
    let dir = tempfile::tempdir().expect("temp dir should be created");
    let store =
        SyndicStore::open(dir.path(), StoreOpenOptions::default()).expect("store should open");
    let workspace_id = BerylWorkspaceId::new("workspace-alpha").unwrap();
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&execution_target)
        .unwrap();

    let parent_thread_id = ConversationThreadId::new("thread-parent");
    let child_thread_id = ConversationThreadId::new("thread-child");
    let untitled_thread_id = ConversationThreadId::new("thread-untitled");
    let missing_thread_id = ConversationThreadId::new("thread-missing-summary");
    let mismatch_thread_id = ConversationThreadId::new("thread-mismatch");
    let backend_only_thread_id = ConversationThreadId::new("thread-backend-only");

    workspace_state.remember_thread(registered_thread(
        parent_thread_id.clone(),
        &execution_target,
        "conversation-parent",
        "view-parent",
        1,
        1,
    ));
    workspace_state.remember_thread(registered_thread(
        child_thread_id.clone(),
        &execution_target,
        "conversation-child",
        "view-child",
        2,
        2,
    ));
    workspace_state
        .set_thread_manual_title(&child_thread_id, " Workspace child ", 9)
        .unwrap();
    workspace_state.remember_thread(registered_thread(
        untitled_thread_id.clone(),
        &execution_target,
        "conversation-untitled",
        "view-untitled",
        3,
        3,
    ));
    workspace_state.remember_thread(registered_thread(
        missing_thread_id.clone(),
        &execution_target,
        "conversation-missing-summary",
        "view-missing-summary",
        4,
        4,
    ));
    workspace_state.remember_thread(registered_thread(
        mismatch_thread_id.clone(),
        &execution_target,
        "conversation-mismatch-registered",
        "view-mismatch",
        5,
        5,
    ));
    workspace_state.remember_thread(RegisteredConversationThread::new(
        backend_only_thread_id.clone(),
        execution_target.clone(),
        "Old backend preview",
        6,
        6,
    ));

    store
        .commit(
            SyndicWriteBatch::new()
                .put_conversation(conversation_record(
                    "conversation-parent",
                    "view-parent",
                    Some("Syndic parent"),
                    None,
                    10,
                    40,
                ))
                .put_conversation(conversation_record(
                    "conversation-child",
                    "view-child",
                    Some("Syndic child"),
                    Some("view-parent"),
                    20,
                    60,
                ))
                .put_conversation(conversation_record(
                    "conversation-untitled",
                    "view-untitled",
                    None,
                    None,
                    30,
                    50,
                ))
                .put_conversation(conversation_record(
                    "conversation-mismatch-store",
                    "view-mismatch",
                    Some("Mismatched row"),
                    None,
                    40,
                    70,
                )),
        )
        .expect("seed conversations should persist");
    drop(store);

    let snapshot =
        build_workspace_syndic_catalog_snapshot(dir.path(), workspace_id.clone(), &workspace_state)
            .expect("snapshot should build");

    assert_eq!(snapshot.workspace_id(), &workspace_id);
    assert_eq!(snapshot.groups().len(), 1);
    let group = &snapshot.groups()[0];
    let threads = group.threads();
    assert_eq!(
        thread_ids(threads),
        vec!["thread-child", "thread-untitled", "thread-parent"]
    );

    let child = threads
        .iter()
        .find(|thread| thread.thread_id() == &child_thread_id)
        .expect("child thread should be catalog-visible");
    assert_eq!(child.title(), "Workspace child");
    assert_eq!(
        child.forked_from_id().map(ConversationThreadId::as_str),
        Some("thread-parent")
    );
    assert_eq!(child.syndic_view_id().as_str(), "view-child");

    let parent = threads
        .iter()
        .find(|thread| thread.thread_id() == &parent_thread_id)
        .expect("parent thread should be catalog-visible");
    assert_eq!(parent.title(), "Syndic parent");
    assert_eq!(parent.forked_from_id(), None);

    let untitled = threads
        .iter()
        .find(|thread| thread.thread_id() == &untitled_thread_id)
        .expect("untitled thread should be catalog-visible");
    assert_eq!(untitled.title(), "Untitled thread");

    assert!(
        !threads
            .iter()
            .any(|thread| thread.thread_id() == &missing_thread_id)
    );
    assert!(
        !threads
            .iter()
            .any(|thread| thread.thread_id() == &mismatch_thread_id)
    );
    assert!(
        !threads
            .iter()
            .any(|thread| thread.thread_id() == &backend_only_thread_id)
    );
}

#[test]
fn catalog_snapshot_uses_generated_title_before_syndic_title() {
    let dir = tempfile::tempdir().expect("temp dir should be created");
    let store =
        SyndicStore::open(dir.path(), StoreOpenOptions::default()).expect("store should open");
    let workspace_id = BerylWorkspaceId::new("workspace-beta").unwrap();
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread-generated-title");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    workspace_state.remember_thread(registered_thread(
        thread_id.clone(),
        &execution_target,
        "conversation-generated-title",
        "view-generated-title",
        1,
        1,
    ));
    workspace_state
        .set_thread_generated_title_if_absent(&thread_id, " Generated title ", 11)
        .unwrap();

    store
        .commit(
            SyndicWriteBatch::new().put_conversation(conversation_record(
                "conversation-generated-title",
                "view-generated-title",
                Some("Syndic title"),
                None,
                10,
                10,
            )),
        )
        .expect("seed conversation should persist");
    drop(store);

    let snapshot =
        build_workspace_syndic_catalog_snapshot(dir.path(), workspace_id, &workspace_state)
            .expect("snapshot should build");
    let title = snapshot.groups()[0].threads()[0].title();

    assert_eq!(title, "Generated title");
}

#[test]
fn catalog_snapshot_limits_after_syndic_history_ordering() {
    let dir = tempfile::tempdir().expect("temp dir should be created");
    let store =
        SyndicStore::open(dir.path(), StoreOpenOptions::default()).expect("store should open");
    let workspace_id = BerylWorkspaceId::new("workspace-gamma").unwrap();
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let newest_thread_id = ConversationThreadId::new("thread-history-newest");
    let mut workspace_state = WorkspaceConversationState::default();
    workspace_state
        .designate_primary_execution_target(&execution_target)
        .unwrap();

    let mut batch = SyndicWriteBatch::new();
    for index in 0..=MAX_CONVERSATION_SUMMARY_READ_LIMIT {
        let is_newest = index == MAX_CONVERSATION_SUMMARY_READ_LIMIT;
        let thread_id = if is_newest {
            newest_thread_id.clone()
        } else {
            ConversationThreadId::new(format!("thread-{index:04}"))
        };
        let conversation_id = if is_newest {
            "conversation-history-newest".to_string()
        } else {
            format!("conversation-{index:04}")
        };
        let view_id = if is_newest {
            "view-history-newest".to_string()
        } else {
            format!("view-{index:04}")
        };
        let workspace_updated_at = if is_newest {
            0
        } else {
            (MAX_CONVERSATION_SUMMARY_READ_LIMIT - index + 1) as i64
        };
        let syndic_updated_at = if is_newest { 1_000_000 } else { index as u64 };
        workspace_state.remember_thread(registered_thread(
            thread_id,
            &execution_target,
            &conversation_id,
            &view_id,
            workspace_updated_at,
            workspace_updated_at,
        ));
        batch = batch.put_conversation(conversation_record(
            &conversation_id,
            &view_id,
            None,
            None,
            syndic_updated_at,
            syndic_updated_at,
        ));
    }
    store
        .commit(batch)
        .expect("seed conversations should persist");
    drop(store);

    let snapshot =
        build_workspace_syndic_catalog_snapshot(dir.path(), workspace_id, &workspace_state)
            .expect("snapshot should build");
    let threads = snapshot.groups()[0].threads();

    assert_eq!(threads.len(), MAX_CONVERSATION_SUMMARY_READ_LIMIT);
    assert_eq!(threads[0].thread_id(), &newest_thread_id);
    assert!(
        threads
            .iter()
            .any(|thread| thread.thread_id() == &newest_thread_id)
    );
}

fn registered_thread(
    thread_id: ConversationThreadId,
    execution_target: &WorkspaceId,
    conversation_id: &str,
    view_id: &str,
    created_at_millis: i64,
    updated_at_millis: i64,
) -> RegisteredConversationThread {
    RegisteredConversationThread::new(
        thread_id,
        execution_target.clone(),
        "",
        created_at_millis,
        updated_at_millis,
    )
    .with_syndic_view_registration(
        SyndicConversationId::new(conversation_id),
        SyndicConversationViewId::new(view_id),
    )
}

fn conversation_record(
    conversation_id: &str,
    view_id: &str,
    title: Option<&str>,
    parent_view_id: Option<&str>,
    created_at_ms: u64,
    updated_at_ms: u64,
) -> ConversationRecord {
    ConversationRecord {
        id: ConversationId::from(conversation_id),
        view_id: ThreadViewId::from(view_id),
        parent_view_id: parent_view_id.map(ThreadViewId::from),
        branch_source_turn_id: None,
        title: title.map(str::to_string),
        created_at_ms,
        updated_at_ms,
        current_revision: ProviderRevision(1),
        source: None,
        history_state: HistoryState::Complete,
    }
}

fn thread_ids(threads: &[member_thread_inventory::MemberThreadInventoryThread]) -> Vec<&str> {
    threads
        .iter()
        .map(|thread| thread.thread_id().as_str())
        .collect()
}
