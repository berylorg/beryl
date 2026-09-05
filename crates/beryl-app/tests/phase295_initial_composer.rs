#![cfg(feature = "test-faults")]

#[path = "phase186_pending_composer_activation/support.rs"]
mod composer_support;
#[path = "phase295_initial_composer/gpui.rs"]
mod gpui_cases;
#[path = "phase289_main_window_shell/support.rs"]
mod home_support;
#[path = "phase295_initial_composer/support.rs"]
mod support;

use beryl_app::main_window::*;
use beryl_app::window_acquisition::*;
use beryl_home_store::{CommandCancellation, HomeStore, test_faults::FaultPoint};
use beryl_model::{
    ExecutionBinding, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId, WindowBounds, WindowDisplayState, WindowId, WindowPlacement,
};
use beryl_state::{BerylState, RememberedTarget};
use std::sync::Arc;
use support::*;
use syndic_storage::{
    DraftEditHistoryPolicyV1, DraftEditorCandidateSessionIdV1,
    DraftEditorCandidateSessionReadOutcomeV1, SyndicStorage, SyndicTimestamp,
};

#[test]
fn production_activation_prepares_exact_editor_and_retires_before_window_release() {
    let fixture = Fixture::new(31);
    let mut custody = fixture.begin(32);
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    assert_eq!(
        custody.advance(&CommandCancellation::new()).unwrap(),
        MainWindowInitialComposerProgress::Activated
    );
    let draft = custody.acquisition().draft_id();
    let prepared = custody
        .prepare(&mut config)
        .unwrap_or_else(|failure| panic!("{}", failure.error));
    let (editor, custody) = prepared.into_parts();
    assert_eq!(
        editor.selection_identity().binding().candidate().draft_id(),
        draft
    );
    assert_eq!(editor.test_seed_count(), 2);
    drop(editor);
    fixture.retire_and_release(custody);
    assert_eq!(fixture.process.main_window_occupancy(), 0);
}

#[test]
fn cancellation_before_open_and_after_durable_open_preserves_exact_cleanup_owner() {
    for after_open in [false, true] {
        let fixture = Fixture::new(41);
        let mut custody = fixture.begin(42);
        let draft = custody.acquisition().draft_id();
        let cancellation = CommandCancellation::new();
        if after_open {
            let cancel = cancellation.clone();
            custody.test_arm_after_open(move |_, _| {
                cancel.cancel();
            });
        } else {
            cancellation.cancel();
        }
        assert!(custody.advance(&cancellation).is_err());
        assert_eq!(fixture.process.main_window_occupancy(), 1);
        assert!(
            matches!(
                fixture.session(draft, 42),
                DraftEditorCandidateSessionReadOutcomeV1::Active(_)
            ) == after_open
        );
        fixture.retire_and_release(custody);
        assert_eq!(fixture.process.main_window_occupancy(), 0);
    }
}

#[test]
fn failed_seed_and_failed_widget_configuration_keep_durable_candidate_until_typed_retirement() {
    for seed_failure in [false, true] {
        let fixture = Fixture::new(51);
        let mut custody = if seed_failure {
            fixture.begin_with_bad_seed(52)
        } else {
            fixture.begin(52)
        };
        if seed_failure {
            assert!(custody.advance(&CommandCancellation::new()).is_err());
        } else {
            assert_eq!(
                custody.advance(&CommandCancellation::new()).unwrap(),
                MainWindowInitialComposerProgress::Activated
            );
            let failure = custody
                .prepare(&mut |_| Err("injected widget preparation refusal".to_owned()))
                .err()
                .unwrap();
            assert_eq!(failure.error, "injected widget preparation refusal");
            custody = failure.custody;
        }
        assert!(matches!(
            fixture.session(custody.acquisition().draft_id(), 52),
            DraftEditorCandidateSessionReadOutcomeV1::Active(_)
        ));
        fixture.retire_and_release(custody);
    }
}

#[test]
fn not_committed_open_and_retirement_are_exact_retry_custody() {
    let fixture = Fixture::new(61);
    let mut custody = fixture.begin(62);
    let cancel = CommandCancellation::new();
    let at_execute = cancel.clone();
    custody.test_arm_before_open(move |_, _| {
        at_execute.cancel();
    });
    assert_eq!(
        custody.advance(&cancel).unwrap(),
        MainWindowInitialComposerProgress::Retry
    );
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    assert_eq!(
        custody.advance(&CommandCancellation::new()).unwrap(),
        MainWindowInitialComposerProgress::Activated
    );
    let cancel = CommandCancellation::new();
    cancel.cancel();
    let MainWindowInitialComposerRetirement::Pending(failure) = custody.retire(cancel) else {
        panic!("NotCommitted must retain owner")
    };
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    fixture.retire_and_release(failure.custody);
}

#[test]
fn acknowledgement_loss_during_open_and_retirement_keeps_reservation_until_exact_settlement() {
    let fixture = Fixture::new(71);
    let mut custody = fixture.begin(72);
    let faults = fixture.faults.clone();
    custody
        .test_arm_before_open(move |_, _| faults.fail_next(FaultPoint::AfterCommitBeforePersist));
    assert_eq!(
        custody.advance(&CommandCancellation::new()).unwrap(),
        MainWindowInitialComposerProgress::Pending
    );
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    assert_eq!(
        custody.advance(&CommandCancellation::new()).unwrap(),
        MainWindowInitialComposerProgress::Activated
    );
    let faults = fixture.faults.clone();
    custody.test_arm_before_retirement(move |_, _| {
        faults.fail_next(FaultPoint::AfterCommitBeforePersist)
    });
    let MainWindowInitialComposerRetirement::Pending(failure) =
        custody.retire(CommandCancellation::new())
    else {
        panic!("retirement acknowledgement loss must retain owner")
    };
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    fixture.retire_and_release(failure.custody);
}

#[test]
fn foreign_home_admission_returns_original_acquisition_and_reservation_without_opening() {
    let source = Fixture::new(81);
    let foreign = Fixture::new(81);
    let acquisition = source.acquire(82);
    let reservation = source
        .process
        .reserve_main_window(acquisition.window_id())
        .unwrap();
    let claim = source.claim(acquisition.window_id());
    let request = composer_support::activation(acquisition.thread_id(), 82, 83, 1, 0);
    let result = MainWindowInitialComposer::new(
        acquisition,
        reservation,
        foreign.service.clone(),
        foreign.store.clone(),
        foreign.storage.clone(),
        claim,
        request,
        composer_support::fixture::operation_id(84),
        MainWindowComposerMarkerMetadataAuthority::new(foreign.state.assets()),
    );
    let failure = result.err().unwrap();
    assert_eq!(source.process.main_window_occupancy(), 1);
    assert!(matches!(
        source.session(failure.acquisition.draft_id(), 82),
        DraftEditorCandidateSessionReadOutcomeV1::Absent
    ));
    let custody = source.from_acquired(failure.acquisition, failure.reservation, 82, false);
    source.retire_and_release(custody);
}

#[test]
fn stale_claim_after_open_rejects_prepared_transfer_but_retires_only_its_candidate() {
    let fixture = Fixture::new(91);
    let mut custody = fixture.begin(92);
    assert_eq!(
        custody.advance(&CommandCancellation::new()).unwrap(),
        MainWindowInitialComposerProgress::Activated
    );
    let window = custody.acquisition().window_id();
    fixture.remove_session_window(window);
    let failure = custody.prepare(&mut config).err().unwrap();
    assert!(failure.error.contains("claim"), "{}", failure.error);
    let draft = failure.custody.acquisition().draft_id();
    let MainWindowInitialComposerRetirement::Retired(unpublished) =
        failure.custody.retire(CommandCancellation::new())
    else {
        panic!("exact candidate can retire after source drift")
    };
    assert!(matches!(
        fixture.session(draft, 92),
        DraftEditorCandidateSessionReadOutcomeV1::Disposed(_)
    ));
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    drop(unpublished);
}

#[test]
fn reused_thread_survives_repeated_candidate_and_window_retirement() {
    let fixture = Fixture::new(101);
    let thread = fixture.seed_pristine(111);
    for seed in 112..116 {
        let mut custody = fixture.begin(seed);
        assert_eq!(custody.acquisition().thread_id(), thread);
        assert_eq!(
            custody.acquisition().disposition(),
            RuntimeBackedWindowAcquisitionDisposition::Reused
        );
        assert_eq!(
            custody.advance(&CommandCancellation::new()).unwrap(),
            MainWindowInitialComposerProgress::Activated
        );
        fixture.retire_and_release(custody);
        assert_eq!(fixture.process.main_window_occupancy(), 0);
        assert!(matches!(
            fixture
                .storage
                .audit_pristine_thread(&fixture.store, thread, &fixture.execution())
                .unwrap(),
            syndic_storage::PristineThreadAudit::Exact(_)
        ));
    }
}

#[test]
fn disposed_open_classification_stays_terminal_across_repeated_activation_attempts() {
    let fixture = Fixture::new(131);
    let mut custody = fixture.begin(132);
    let draft = custody.acquisition().draft_id();
    custody.test_arm_before_open_classification(move |store, storage| {
        let DraftEditorCandidateSessionReadOutcomeV1::Active(head) = storage
            .draft_editor_candidate_session(
                store,
                draft,
                DraftEditorCandidateSessionIdV1::from_bytes([132; 16]),
            )
            .unwrap()
        else {
            panic!("opened candidate")
        };
        let prepared = storage
            .prepare_abandon_fresh_draft_editor_candidate_session(
                store,
                syndic_storage::DraftEditorCandidateSessionDisposeRequestV1::new(
                    draft,
                    head.session_id(),
                    composer_support::fixture::operation_id(139),
                    head.session_generation(),
                    syndic_storage::DraftRootHistoryPairV1::new(
                        head.newest_root(),
                        head.newest_history(),
                    ),
                ),
            )
            .unwrap();
        execute(
            store,
            storage.abandon_fresh_draft_editor_candidate_session(
                storage.revision(store).unwrap(),
                prepared,
            ),
        );
    });
    assert!(
        custody
            .advance(&CommandCancellation::new())
            .unwrap_err()
            .contains("disposed")
    );
    for _ in 0..3 {
        assert!(custody.advance(&CommandCancellation::new()).is_err());
    }
    let failure = custody.prepare(&mut config).err().unwrap();
    assert!(matches!(
        fixture.session(draft, 132),
        DraftEditorCandidateSessionReadOutcomeV1::Disposed(_)
    ));
    fixture.retire_and_release(failure.custody);
}
