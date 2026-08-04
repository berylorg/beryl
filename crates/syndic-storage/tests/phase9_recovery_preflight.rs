#![cfg(feature = "test-faults")]

#[path = "phase9_recovery_projection/support.rs"]
mod support;

use syndic_storage::*;

use support::{Builder, TestHome, open};

#[test]
fn empty_current_path_is_native_fresh_without_model_metadata_or_state_change() {
    let home = TestHome::new("phase9-recovery-current-empty");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let builder = Builder::new(&store, storage, 41);
    let selected = builder.selected_path();
    let before_revision = storage.revision(&store).unwrap();
    let before_draft = storage
        .current_draft(&store, builder.thread(), support::point_limit())
        .unwrap()
        .unwrap();
    let request =
        RecoveryProjectionRequest::for_current_selected_path(builder.thread(), selected, None);
    assert_eq!(
        request.scope(),
        RecoveryProjectionScope::CurrentSelectedPath
    );

    let RecoveryAssembly::NativeEmptyPrefix {
        thread_id,
        selected_path,
        source_revision,
    } = storage
        .prepare_recovery_projection(&store, request)
        .unwrap()
    else {
        panic!("an empty current path must select native Fresh")
    };

    assert_eq!(thread_id, builder.thread());
    assert_eq!(selected_path, selected);
    assert_eq!(selected_path.tail(), None);
    assert_eq!(source_revision, before_revision);
    assert_eq!(storage.revision(&store).unwrap(), before_revision);
    assert_eq!(
        storage
            .current_draft(&store, builder.thread(), support::point_limit())
            .unwrap()
            .unwrap(),
        before_draft
    );
}

#[test]
fn pre_admission_projection_equals_later_pending_parent_projection_without_state_change() {
    let home = TestHome::new("phase9-recovery-current-pending-equivalence");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut builder = Builder::new(&store, storage, 42);
    let completed = builder.submit_text("retained user");
    builder.complete_with_assistant(
        completed,
        AssistantMessagePhase::FinalAnswer,
        "retained assistant",
    );
    builder.update_draft_text("draft that must survive preflight");

    let current_selected = builder.selected_path();
    let before_revision = storage.revision(&store).unwrap();
    let before_draft = storage
        .current_draft(&store, builder.thread(), support::point_limit())
        .unwrap()
        .unwrap();
    let before_binding = storage
        .current_binding(&store, builder.thread(), support::point_limit())
        .unwrap()
        .unwrap();
    let before_gate = storage
        .input_gate(&store, builder.thread(), support::point_limit())
        .unwrap()
        .unwrap();
    let request = RecoveryProjectionRequest::for_current_selected_path(
        builder.thread(),
        current_selected,
        Some(100_000),
    );
    assert_eq!(
        request.scope(),
        RecoveryProjectionScope::CurrentSelectedPath
    );
    let RecoveryAssembly::Ready(pre_admission) = storage
        .prepare_recovery_projection(&store, request)
        .unwrap()
    else {
        panic!("a nonempty recovery-complete current path must be ready")
    };
    let pre_admission_items = replay(&storage, &store, pre_admission);

    assert_eq!(storage.revision(&store).unwrap(), before_revision);
    assert_eq!(
        storage
            .current_draft(&store, builder.thread(), support::point_limit())
            .unwrap()
            .unwrap(),
        before_draft
    );
    assert_eq!(
        storage
            .current_binding(&store, builder.thread(), support::point_limit())
            .unwrap()
            .unwrap(),
        before_binding
    );
    assert_eq!(
        storage
            .input_gate(&store, builder.thread(), support::point_limit())
            .unwrap()
            .unwrap(),
        before_gate
    );

    let pending = builder.submit_text("draft that must survive preflight");
    let pending_selected = builder.selected_path();
    let request = RecoveryProjectionRequest::for_pending_selected_turn_parent(
        builder.thread(),
        pending_selected,
        Some(100_000),
    );
    assert_eq!(
        request.scope(),
        RecoveryProjectionScope::PendingSelectedTurnParent
    );
    let RecoveryAssembly::Ready(restarted) = storage
        .prepare_recovery_projection(&store, request)
        .unwrap()
    else {
        panic!("the later pending parent path must be ready")
    };

    assert_eq!(pre_admission.version(), restarted.version());
    assert_eq!(
        pre_admission_items,
        vec![
            (
                RecoveryItemSequenceRole::UserInputText,
                "retained user".to_owned(),
            ),
            (
                RecoveryItemSequenceRole::AssistantOutputText,
                "retained assistant".to_owned(),
            ),
        ]
    );
    assert_eq!(pre_admission_items, replay(&storage, &store, restarted),);
    assert_eq!(pre_admission.item_count(), restarted.item_count());
    assert_eq!(pre_admission.utf8_bytes(), restarted.utf8_bytes());
    assert_eq!(pre_admission.sequence_digest(), restarted.sequence_digest());
    assert_eq!(
        pre_admission.represented_prefix().tail(),
        Some(completed.turn)
    );
    assert_eq!(restarted.represented_prefix().tail(), Some(completed.turn));
    assert_ne!(restarted.represented_prefix().tail(), Some(pending.turn));
    assert_eq!(
        pre_admission.represented_prefix().digest(),
        restarted.represented_prefix().digest()
    );
    assert_ne!(pre_admission.selected_path(), restarted.selected_path());
    assert_ne!(pre_admission.source_revision(), restarted.source_revision());
}

#[test]
fn cursor_read_rejects_a_changed_source_revision_before_emitting_text() {
    let home = TestHome::new("phase9-recovery-cursor-revision");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut builder = Builder::new(&store, storage, 43);
    let completed = builder.submit_text("retained user");
    builder.complete_without_assistant(completed, TurnTerminalOutcome::Complete);
    let RecoveryAssembly::Ready(projection) = storage
        .prepare_recovery_projection(
            &store,
            RecoveryProjectionRequest::for_current_selected_path(
                builder.thread(),
                builder.selected_path(),
                Some(100_000),
            ),
        )
        .unwrap()
    else {
        panic!("the completed prefix must be recoverable")
    };
    let mut cursor = storage.open_recovery_cursor(&store, projection).unwrap();

    builder.update_draft_text("unrelated newer draft revision");

    let pool = support::recovery_page_pool(RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES);
    assert!(matches!(
        storage.read_recovery_cursor_page(
            &store,
            &mut cursor,
            pool.try_lease().unwrap(),
            RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES,
        ),
        Err(RecoveryProjectionError::ConcurrentChange)
    ));
    assert!(matches!(
        storage.open_recovery_cursor(&store, projection),
        Err(RecoveryProjectionError::ConcurrentChange)
    ));
}

fn replay(
    storage: &SyndicStorage,
    store: &beryl_home_store::HomeStore,
    projection: RecoveryProjection,
) -> Vec<(RecoveryItemSequenceRole, String)> {
    let mut cursor = storage.open_recovery_cursor(store, projection).unwrap();
    let pool = support::recovery_page_pool(RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES);
    let mut page_lease = pool.try_lease().unwrap();
    let mut items: Vec<(RecoveryItemSequenceRole, String)> = Vec::new();
    loop {
        let Some(page) = storage
            .read_recovery_cursor_page(
                store,
                &mut cursor,
                page_lease,
                RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES,
            )
            .unwrap()
        else {
            break;
        };
        if page.item_offset() == 0 {
            items.push((page.role(), String::new()));
        }
        items.last_mut().unwrap().1.push_str(page.text());
        page_lease = page.into_page_lease();
    }
    items
}
