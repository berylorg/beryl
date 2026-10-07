use super::*;
use beryl_home_store::{HomeHealthState, HomeRecoveryCandidate, test_faults::FaultController};

struct RecoveryFixture {
    _directory: tempfile::TempDir,
    candidate: HomeRecoveryCandidate,
    storage: SyndicStorage,
    faults: FaultController,
    process: RuntimeBackedWindowProcessRegistry,
}

fn fail(fixture: &Fixture) {
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    assert_eq!(fixture.store.health().state(), HomeHealthState::Failed);
}

fn recover(fixture: Fixture) -> RecoveryFixture {
    let Fixture {
        directory,
        store,
        state,
        storage,
        faults,
        process,
        service,
        ..
    } = fixture;
    let reference = Arc::downgrade(&service.home_reference());
    drop((state, storage, service));
    assert!(
        reference.upgrade().is_none(),
        "cleanup retained old service reference"
    );
    let candidate = Arc::try_unwrap(store)
        .ok()
        .unwrap()
        .recover_same_home()
        .unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    RecoveryFixture {
        _directory: directory,
        candidate,
        storage,
        faults,
        process,
    }
}

fn capture(custody: MainWindowInitialComposer) -> MainWindowInitialComposerRecovery {
    custody
        .capture_failed_recovery()
        .unwrap_or_else(|failure| panic!("{}", failure.error))
}

fn settle(
    recovery: &mut MainWindowInitialComposerRecovery,
    fixture: &mut RecoveryFixture,
) -> MainWindowInitialComposerRecoveryProgress {
    recovery
        .cleanup_mut()
        .settle(
            &fixture.storage,
            &mut fixture.candidate.recovery_access().unwrap(),
            CommandCancellation::new(),
        )
        .unwrap()
}

fn assert_disposed(
    recovery: &mut MainWindowInitialComposerRecovery,
    fixture: &mut RecoveryFixture,
    draft: SyndicDraftId,
    seed: u8,
) {
    let access = fixture.candidate.recovery_access().unwrap();
    let DraftEditorCandidateSessionReadOutcomeV1::Disposed(head) = fixture
        .storage
        .qualify_fresh_draft_editor_candidate_session_open_candidate(
            &access,
            access.home_id(),
            composer_support::fixture::operation_id(seed.wrapping_add(2)),
            recovery.cleanup_mut().test_original_opening().unwrap(),
        )
        .unwrap()
    else {
        panic!("original editor disposed")
    };
    assert_eq!(head.draft_id(), draft);
    assert_eq!(
        head.session_id(),
        DraftEditorCandidateSessionIdV1::from_bytes([seed; 16])
    );
    assert_eq!(
        head.disposal_operation_id(),
        Some(composer_support::fixture::operation_id(
            seed.wrapping_add(2)
        ))
    );
}

#[test]
fn failed_open_classification_captures_committed_opening_without_old_runtime_references() {
    let fixture = Fixture::new(161);
    let mut custody = fixture.begin(162);
    let draft = custody.acquisition().draft_id();
    let faults = fixture.faults.clone();
    custody.test_arm_before_open_classification(move |store, _| {
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
    });
    assert!(custody.advance(&CommandCancellation::new()).is_err());
    let recovery = capture(custody);
    let mut recovery = recovery
        .into_unpublished()
        .err()
        .expect("cleanup is not disposal");
    let mut fixture = recover(fixture);
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    assert_eq!(
        settle(&mut recovery, &mut fixture),
        MainWindowInitialComposerRecoveryProgress::Complete
    );
    assert_disposed(&mut recovery, &mut fixture, draft, 162);
    let revision = fixture
        .candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    assert_eq!(
        settle(&mut recovery, &mut fixture),
        MainWindowInitialComposerRecoveryProgress::Complete
    );
    assert_eq!(
        fixture
            .candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
    let unpublished = recovery.into_unpublished().ok().unwrap();
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    drop(unpublished);
    assert_eq!(fixture.process.main_window_occupancy(), 0);
}

#[test]
fn unopened_and_exact_noncommitted_openings_complete_without_creating_editor() {
    for attempted in [false, true] {
        let fixture = Fixture::new(163);
        let mut custody = fixture.begin(164);
        if attempted {
            let cancel = CommandCancellation::new();
            let execute_cancel = cancel.clone();
            custody.test_arm_before_open(move |_, _| execute_cancel.cancel());
            assert_eq!(
                custody.advance(&cancel).unwrap(),
                MainWindowInitialComposerProgress::Retry
            );
        }
        fail(&fixture);
        let mut recovery = capture(custody);
        let mut fixture = recover(fixture);
        let revision = fixture
            .candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap();
        assert_eq!(
            settle(&mut recovery, &mut fixture),
            MainWindowInitialComposerRecoveryProgress::Complete
        );
        let access = fixture.candidate.recovery_access().unwrap();
        if attempted {
            assert!(matches!(
                fixture
                    .storage
                    .qualify_fresh_draft_editor_candidate_session_open_candidate(
                        &access,
                        access.home_id(),
                        composer_support::fixture::operation_id(166),
                        recovery.cleanup_mut().test_original_opening().unwrap(),
                    )
                    .unwrap(),
                DraftEditorCandidateSessionReadOutcomeV1::Absent
            ));
        } else {
            assert!(recovery.cleanup_mut().test_original_opening().is_none());
        }
        assert_eq!(access.home_revision().unwrap(), revision);
    }
}

#[test]
fn activated_unmounted_host_retires_before_cleanup_handoff() {
    let fixture = Fixture::new(165);
    let mut custody = fixture.begin(166);
    let draft = custody.acquisition().draft_id();
    assert_eq!(
        custody.advance(&CommandCancellation::new()).unwrap(),
        MainWindowInitialComposerProgress::Activated
    );
    fail(&fixture);
    let mut recovery = capture(custody);
    let mut fixture = recover(fixture);
    assert_eq!(
        settle(&mut recovery, &mut fixture),
        MainWindowInitialComposerRecoveryProgress::Complete
    );
    assert_disposed(&mut recovery, &mut fixture, draft, 166);
}

#[test]
fn healthy_capture_rejects_with_original_candidate_and_reservation() {
    let fixture = Fixture::new(167);
    let custody = fixture.begin(168);
    let failure = custody.capture_failed_recovery().err().unwrap();
    assert!(failure.error.contains("failed home generation"));
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    fixture.retire_and_release(failure.custody);
}

#[test]
fn prepared_service_capture_refusal_preserves_original_owner_and_live_widget_custody() {
    let fixture = Fixture::new(169);
    let mut custody = fixture.begin(170);
    custody.advance(&CommandCancellation::new()).unwrap();
    let (editor, custody) = custody
        .prepare(&mut config)
        .unwrap_or_else(|failure| panic!("{}", failure.error))
        .into_parts();
    fail(&fixture);
    let failure = custody.capture_failed_recovery().err().unwrap();
    assert!(failure.error.contains("prepared service"));
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    assert_eq!(
        editor.selection_identity().binding().candidate().draft_id(),
        failure.custody.acquisition().draft_id()
    );
    drop((editor, failure));
}

#[test]
fn original_ambiguous_opening_is_settled_before_fresh_disposal() {
    let fixture = Fixture::new(171);
    let mut custody = fixture.begin(172);
    let draft = custody.acquisition().draft_id();
    let faults = fixture.faults.clone();
    custody
        .test_arm_before_open(move |_, _| faults.fail_next(FaultPoint::AfterCommitBeforePersist));
    assert_eq!(
        custody.advance(&CommandCancellation::new()).unwrap(),
        MainWindowInitialComposerProgress::Pending
    );
    fail(&fixture);
    let mut recovery = capture(custody);
    let mut fixture = recover(fixture);
    assert_eq!(
        settle(&mut recovery, &mut fixture),
        MainWindowInitialComposerRecoveryProgress::Complete
    );
    assert_disposed(&mut recovery, &mut fixture, draft, 172);
    assert!(
        fixture
            .candidate
            .recovery_access()
            .unwrap()
            .pending_reconciliations()
            .is_empty()
    );
}

#[test]
fn original_ambiguous_disposal_is_settled_without_repeating_committed_write() {
    let fixture = Fixture::new(173);
    let mut custody = fixture.begin(174);
    let draft = custody.acquisition().draft_id();
    custody.advance(&CommandCancellation::new()).unwrap();
    let faults = fixture.faults.clone();
    custody.test_arm_before_retirement(move |_, _| {
        faults.fail_next(FaultPoint::AfterCommitBeforePersist)
    });
    let MainWindowInitialComposerRetirement::Pending(failure) =
        custody.retire(CommandCancellation::new())
    else {
        panic!("original disposal retained")
    };
    fail(&fixture);
    let mut recovery = capture(failure.custody);
    let mut fixture = recover(fixture);
    let revision = fixture
        .candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    assert_eq!(
        settle(&mut recovery, &mut fixture),
        MainWindowInitialComposerRecoveryProgress::Complete
    );
    assert_disposed(&mut recovery, &mut fixture, draft, 174);
    assert_eq!(
        fixture
            .candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
}

#[test]
fn recovery_cancellation_retains_exact_cleanup_and_original_reservation() {
    let fixture = Fixture::new(175);
    let mut custody = fixture.begin(176);
    let draft = custody.acquisition().draft_id();
    custody.advance(&CommandCancellation::new()).unwrap();
    fail(&fixture);
    let mut recovery = capture(custody);
    let mut fixture = recover(fixture);
    let cancel = CommandCancellation::new();
    cancel.cancel();
    assert_eq!(
        recovery
            .cleanup_mut()
            .settle(
                &fixture.storage,
                &mut fixture.candidate.recovery_access().unwrap(),
                cancel
            )
            .unwrap(),
        MainWindowInitialComposerRecoveryProgress::Pending
    );
    let mut recovery = recovery.into_unpublished().err().unwrap();
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    assert_eq!(
        settle(&mut recovery, &mut fixture),
        MainWindowInitialComposerRecoveryProgress::Complete
    );
    assert_disposed(&mut recovery, &mut fixture, draft, 176);
}

#[test]
fn foreign_recovery_candidate_refuses_cleanup_without_consuming_original_intent() {
    let fixture = Fixture::new(177);
    let mut custody = fixture.begin(178);
    custody.advance(&CommandCancellation::new()).unwrap();
    fail(&fixture);
    let mut recovery = capture(custody);
    let foreign = Fixture::new(177);
    fail(&foreign);
    let mut foreign = recover(foreign);
    let revision = foreign
        .candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    assert!(
        recovery
            .cleanup_mut()
            .settle(
                &foreign.storage,
                &mut foreign.candidate.recovery_access().unwrap(),
                CommandCancellation::new()
            )
            .unwrap_err()
            .contains("same home")
    );
    assert_eq!(
        foreign
            .candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
    let mut fixture = recover(fixture);
    assert_eq!(
        settle(&mut recovery, &mut fixture),
        MainWindowInitialComposerRecoveryProgress::Complete
    );
}

#[test]
fn fresh_disposal_ambiguity_preserves_separate_outcome_and_reconciles_without_duplicate() {
    let fixture = Fixture::new(179);
    let mut custody = fixture.begin(180);
    let draft = custody.acquisition().draft_id();
    custody.advance(&CommandCancellation::new()).unwrap();
    fail(&fixture);
    let mut recovery = capture(custody);
    let mut fixture = recover(fixture);
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    assert_eq!(
        settle(&mut recovery, &mut fixture),
        MainWindowInitialComposerRecoveryProgress::Pending
    );
    let revision = fixture
        .candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    assert_eq!(
        settle(&mut recovery, &mut fixture),
        MainWindowInitialComposerRecoveryProgress::Complete
    );
    assert_disposed(&mut recovery, &mut fixture, draft, 180);
    assert_eq!(
        fixture
            .candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
}

#[test]
fn completed_cleanup_is_authenticated_again_after_recovery_candidate_failure() {
    let fixture = Fixture::new(181);
    let mut custody = fixture.begin(182);
    let draft = custody.acquisition().draft_id();
    custody.advance(&CommandCancellation::new()).unwrap();
    fail(&fixture);
    let mut recovery = capture(custody);
    let mut fixture = recover(fixture);
    assert_eq!(
        settle(&mut recovery, &mut fixture),
        MainWindowInitialComposerRecoveryProgress::Complete
    );
    drop(fixture.storage);
    let failed = fixture.candidate.abort();
    fixture.candidate = failed.recover_same_home().unwrap();
    fixture.storage = SyndicStorage::reacquire_candidate(&fixture.candidate).unwrap();
    let revision = fixture
        .candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    assert_eq!(
        settle(&mut recovery, &mut fixture),
        MainWindowInitialComposerRecoveryProgress::Complete
    );
    assert_disposed(&mut recovery, &mut fixture, draft, 182);
    assert_eq!(
        fixture
            .candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
}

#[test]
fn exact_noncommitted_recovery_disposal_retries_only_original_cleanup() {
    let fixture = Fixture::new(183);
    let mut custody = fixture.begin(184);
    let draft = custody.acquisition().draft_id();
    custody.advance(&CommandCancellation::new()).unwrap();
    fail(&fixture);
    let mut recovery = capture(custody);
    let mut fixture = recover(fixture);
    fixture.faults.fail_next(FaultPoint::BeforeCommit);
    assert_eq!(
        settle(&mut recovery, &mut fixture),
        MainWindowInitialComposerRecoveryProgress::Pending
    );
    let mut recovery = recovery.into_unpublished().err().unwrap();
    drop(fixture.storage);
    let failed = fixture.candidate.abort();
    fixture.candidate = failed.recover_same_home().unwrap();
    fixture.storage = SyndicStorage::reacquire_candidate(&fixture.candidate).unwrap();
    assert_eq!(
        settle(&mut recovery, &mut fixture),
        MainWindowInitialComposerRecoveryProgress::Complete
    );
    assert_disposed(&mut recovery, &mut fixture, draft, 184);
}

#[test]
fn conflicting_disposal_identity_is_terminal_and_keeps_cleanup_fenced() {
    let fixture = Fixture::new(185);
    let mut custody = fixture.begin(186);
    let draft = custody.acquisition().draft_id();
    custody.advance(&CommandCancellation::new()).unwrap();
    let DraftEditorCandidateSessionReadOutcomeV1::Active(head) = fixture.session(draft, 186) else {
        panic!("original active editor")
    };
    let prepared = fixture
        .storage
        .prepare_abandon_fresh_draft_editor_candidate_session(
            &fixture.store,
            syndic_storage::DraftEditorCandidateSessionDisposeRequestV1::new(
                draft,
                head.session_id(),
                composer_support::fixture::operation_id(195),
                head.session_generation(),
                syndic_storage::DraftRootHistoryPairV1::new(
                    head.newest_root(),
                    head.newest_history(),
                ),
            ),
        )
        .unwrap();
    execute(
        &fixture.store,
        fixture
            .storage
            .abandon_fresh_draft_editor_candidate_session(
                fixture.storage.revision(&fixture.store).unwrap(),
                prepared,
            ),
    );
    fail(&fixture);
    let mut recovery = capture(custody);
    let mut fixture = recover(fixture);
    let revision = fixture
        .candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    for _ in 0..2 {
        assert!(
            matches!(settle(&mut recovery, &mut fixture), MainWindowInitialComposerRecoveryProgress::Unavailable(reason) if reason.contains("original intent is not exact"))
        );
    }
    assert_eq!(
        fixture
            .candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
    let recovery = recovery.into_unpublished().err().unwrap();
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    drop(recovery);
    assert_eq!(fixture.process.main_window_occupancy(), 0);
}
