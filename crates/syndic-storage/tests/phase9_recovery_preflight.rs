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
    assert_eq!(pre_admission.items(), restarted.items());
    assert_eq!(
        pre_admission.items(),
        &[
            RecoveryItem::UserInputText("retained user".into()),
            RecoveryItem::AssistantOutputText("retained assistant".into()),
        ]
    );
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
