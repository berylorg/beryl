#![cfg(feature = "test-faults")]

#[path = "pending_composer_activation/support.rs"]
mod composer_support;
#[path = "main_window_creation/support.rs"]
mod creation_support;
#[path = "main_window_restore_set/faults.rs"]
mod faults;
#[path = "main_window_shell/support.rs"]
mod home_support;
#[path = "initial_composer/support.rs"]
mod initial_support;
#[path = "main_window_restore_set/support.rs"]
mod support;

use beryl_app::main_window::*;
use beryl_app::window_acquisition::*;
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::{
    ExecutionBinding, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId, WindowBounds, WindowDisplayState, WindowId, WindowPlacement,
};
use beryl_state::{BerylState, RememberedTarget};
use initial_support::{Fixture, config, execute, native_path, placement};
use std::sync::Arc;
use support::*;
use syndic_storage::{
    DraftEditHistoryPolicyV1, DraftEditorCandidateSessionIdV1,
    DraftEditorCandidateSessionReadOutcomeV1, SyndicStorage, SyndicTimestamp,
};

#[test]
fn complete_saved_set_prepares_every_exact_editor_and_disposal_preserves_members() {
    let fixture = Fixture::new(11);
    let first = fixture.acquire(12);
    let second = fixture.acquire(13);
    let expected = [first.window_id(), second.window_id()];
    drop((first, second));
    let (work, _lifetime) = work(&fixture);
    let prepared = prepare(work);
    assert_eq!(
        prepared
            .members()
            .iter()
            .map(|m| m.window_id())
            .collect::<Vec<_>>(),
        expected
    );
    assert!(
        prepared
            .members()
            .iter()
            .all(|m| matches!(m, PreparedRestoreSetMember::Restored(_)))
    );
    prepared.revalidate().unwrap();
    let active = snapshot(&fixture);
    assert_eq!(fixture.process.main_window_occupancy(), 2);
    dispose(prepared.dispose());
    assert_eq!(snapshot(&fixture), active);
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    cleanup(fixture);
}

#[test]
fn failed_last_editor_retires_the_complete_attempt_without_deleting_saved_members() {
    let fixture = Fixture::new(21);
    drop(fixture.acquire(22));
    drop(fixture.acquire(23));
    let original = snapshot(&fixture);
    let (mut services, appearance) = creation_support::services(&fixture);
    Arc::get_mut(&mut services).unwrap().configurator_source = Arc::new(|| {
        Box::new(|selection| {
            if selection.window_id() == WindowId::from_bytes([23; 16]) {
                Err("last required editor is unavailable".to_owned())
            } else {
                config(selection)
            }
        })
    });
    let (attempt, _lifetime) = attempt(&fixture);
    let work = MainWindowRestoreSet::new(
        services,
        attempt,
        activation_source(),
        appearance,
        WindowId::from_bytes([240; 16]),
        placement(),
    )
    .unwrap();
    assert!(dispose(work).contains("last required editor"));
    let after = snapshot(&fixture);
    assert_eq!(after.windows().len(), original.windows().len());
    for (before, after) in original.windows().iter().zip(after.windows()) {
        assert_eq!(before.window_id(), after.window_id());
        assert_eq!(
            before.selected_thread().unwrap().thread_id(),
            after.selected_thread().unwrap().thread_id()
        );
        assert_eq!(before.placement(), after.placement());
    }
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    cleanup(fixture);
}

#[test]
fn empty_runtime_backed_session_uses_remembered_target_and_abandons_only_new_fallback() {
    let fixture = Fixture::new(31);
    let old = fixture.acquire(32);
    fixture.remove_session_window(old.window_id());
    drop(old);
    let before = snapshot(&fixture);
    let (work, _lifetime) = work(&fixture);
    let prepared = prepare(work);
    assert_eq!(prepared.members().len(), 1);
    assert!(matches!(
        prepared.members()[0],
        PreparedRestoreSetMember::Replacement(_)
    ));
    assert_eq!(
        snapshot(&fixture).windows()[0].remembered_target(),
        before.header().fallback()
    );
    dispose(prepared.dispose());
    assert!(snapshot(&fixture).windows().is_empty());
    assert_eq!(
        snapshot(&fixture).header().fallback(),
        before.header().fallback()
    );
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    cleanup(fixture);
}

#[test]
fn zero_runtime_startup_handles_absent_empty_and_existing_threadless_sessions() {
    for initial in [None, Some(false), Some(true)] {
        let fixture = zero_runtime(initial);
        let (work, _lifetime) = work(&fixture);
        let prepared = prepare(work);
        assert_eq!(prepared.members().len(), 1);
        assert!(matches!(
            prepared.members()[0],
            PreparedRestoreSetMember::Threadless(_)
        ));
        prepared.revalidate().unwrap();
        let before = snapshot(&fixture);
        assert!(before.windows()[0].selected_thread().is_none());
        dispose(prepared.dispose());
        assert_eq!(snapshot(&fixture), before);
        assert_eq!(fixture.process.main_window_occupancy(), 0);
        cleanup(fixture);
    }
}

#[test]
fn changed_first_member_rejects_the_prepared_set_and_cleanup_keeps_the_changed_record() {
    let fixture = Fixture::new(41);
    drop(fixture.acquire(42));
    drop(fixture.acquire(43));
    let (work, _lifetime) = work(&fixture);
    let prepared = prepare(work);
    change_placement(&fixture, WindowId::from_bytes([42; 16]));
    assert!(prepared.revalidate().is_err());
    let changed = snapshot(&fixture);
    dispose(prepared.dispose());
    assert_eq!(snapshot(&fixture), changed);
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    cleanup(fixture);
}
