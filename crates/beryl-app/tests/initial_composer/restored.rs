use super::*;
use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_state::{BeginSessionRestore, MinimalSessionBootstrap};

fn snapshot(fixture: &Fixture) -> MinimalSessionBootstrap {
    fixture
        .state
        .session()
        .minimal_bootstrap(&fixture.store)
        .unwrap()
        .unwrap()
}

fn begin_restore(fixture: &Fixture) {
    let session = fixture.state.session();
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    command
        .add(session.begin_restore(
            session.revision(&fixture.store).unwrap(),
            BeginSessionRestore::new(snapshot(fixture).header().revision()),
        ))
        .unwrap();
    assert!(matches!(
        fixture.store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

fn attempt(
    fixture: &Fixture,
) -> (
    RestoredWindowPreparationAttempt,
    RestoredWindowServiceTestLifetime,
) {
    RestoredWindowPreparationAttempt::new_for_test(
        fixture.store.clone(),
        fixture.state.session(),
        fixture.storage.clone(),
    )
    .unwrap()
}

fn begin(
    fixture: &Fixture,
    attempt: &RestoredWindowPreparationAttempt,
    seed: u8,
) -> RestoredWindowComposer {
    let snapshot = snapshot(fixture);
    let window = &snapshot.windows()[0];
    attempt
        .begin(
            snapshot.header().revision(),
            window.window_id(),
            composer_support::activation(
                window.selected_thread().unwrap().thread_id(),
                seed,
                seed.wrapping_add(1),
                1,
                0,
            ),
            composer_support::fixture::operation_id(seed.wrapping_add(2)),
            MainWindowComposerMarkerMetadataAuthority::new(fixture.state.assets()),
        )
        .unwrap()
}

fn retire(custody: RestoredWindowComposer) {
    match custody.retire(CommandCancellation::new()) {
        RestoredWindowComposerRetirement::Retired => {}
        RestoredWindowComposerRetirement::Pending(failure) => panic!("{}", failure.error),
    }
}

#[test]
fn restored_editor_preparation_and_retirement_preserve_saved_window_and_thread() {
    let fixture = Fixture::new(131);
    let acquired = fixture.acquire(132);
    let draft = acquired.draft_id();
    drop(acquired);
    begin_restore(&fixture);
    let before = snapshot(&fixture);
    let (attempt, _service) = attempt(&fixture);
    let mut custody = begin(&fixture, &attempt, 133);
    assert_eq!(
        custody
            .advance(&attempt, &CommandCancellation::new())
            .unwrap(),
        MainWindowInitialComposerProgress::Activated
    );
    let prepared = custody
        .prepare(&attempt, &mut config)
        .unwrap_or_else(|failure| panic!("{}", failure.error));
    assert_eq!(
        prepared
            .selection_identity()
            .binding()
            .candidate()
            .draft_id(),
        draft
    );
    assert!(matches!(
        prepared.retire(CommandCancellation::new()),
        RestoredWindowComposerRetirement::Retired
    ));
    assert_eq!(snapshot(&fixture), before);
    assert!(matches!(
        fixture.session(draft, 133),
        DraftEditorCandidateSessionReadOutcomeV1::Disposed(_)
    ));
    assert_eq!(fixture.process.main_window_occupancy(), 0);
}

#[test]
fn foreign_attempt_and_changed_session_reject_without_losing_editor_cleanup() {
    let fixture = Fixture::new(141);
    drop(fixture.acquire(142));
    begin_restore(&fixture);
    let (owner, _service) = attempt(&fixture);
    let (foreign, _foreign_service) = attempt(&fixture);
    let mut custody = begin(&fixture, &owner, 143);
    assert!(
        custody
            .advance(&foreign, &CommandCancellation::new())
            .is_err()
    );
    assert_eq!(
        custody
            .advance(&owner, &CommandCancellation::new())
            .unwrap(),
        MainWindowInitialComposerProgress::Activated
    );
    begin_restore(&fixture);
    let before = snapshot(&fixture);
    assert!(
        custody
            .advance(&owner, &CommandCancellation::new())
            .is_err()
    );
    retire(custody);
    assert_eq!(snapshot(&fixture), before);
}

#[test]
fn restored_editor_uncertain_open_and_retirement_keep_original_custody() {
    let fixture = Fixture::new(151);
    drop(fixture.acquire(152));
    begin_restore(&fixture);
    let before = snapshot(&fixture);
    let (owner, _service) = attempt(&fixture);
    let mut custody = begin(&fixture, &owner, 153);
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    assert_eq!(
        custody
            .advance(&owner, &CommandCancellation::new())
            .unwrap(),
        MainWindowInitialComposerProgress::Pending
    );
    assert_eq!(
        custody
            .advance(&owner, &CommandCancellation::new())
            .unwrap(),
        MainWindowInitialComposerProgress::Activated
    );
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let RestoredWindowComposerRetirement::Pending(failure) =
        custody.retire(CommandCancellation::new())
    else {
        panic!("uncertain retirement must retain custody")
    };
    retire(failure.custody);
    assert_eq!(snapshot(&fixture), before);
    assert!(fixture.store.pending_reconciliations().is_empty());
}

#[test]
fn cancelled_restore_and_failed_configuration_preserve_all_durable_members() {
    for after_open in [false, true] {
        let fixture = Fixture::new(161);
        drop(fixture.acquire(162));
        begin_restore(&fixture);
        let before = snapshot(&fixture);
        let (owner, _service) = attempt(&fixture);
        let mut custody = begin(&fixture, &owner, 163);
        if after_open {
            custody
                .advance(&owner, &CommandCancellation::new())
                .unwrap();
            let failure = custody
                .prepare(&owner, &mut |_| Err("configuration failed".to_owned()))
                .err()
                .unwrap();
            retire(failure.custody);
        } else {
            let cancel = CommandCancellation::new();
            cancel.cancel();
            assert!(custody.advance(&owner, &cancel).is_err());
            retire(custody);
        }
        assert_eq!(snapshot(&fixture), before);
    }
}

#[test]
fn retired_service_rejects_preparation_but_preserves_cleanup_custody() {
    let fixture = Fixture::new(171);
    drop(fixture.acquire(172));
    begin_restore(&fixture);
    let before = snapshot(&fixture);
    let (owner, service) = attempt(&fixture);
    let mut custody = begin(&fixture, &owner, 173);
    custody
        .advance(&owner, &CommandCancellation::new())
        .unwrap();
    drop(service);
    assert!(
        custody
            .advance(&owner, &CommandCancellation::new())
            .is_err()
    );
    let failure = custody.prepare(&owner, &mut config).err().unwrap();
    retire(failure.custody);
    assert_eq!(snapshot(&fixture), before);
}

#[test]
fn external_claim_activation_is_not_adopted_by_an_old_restore_editor() {
    let fixture = Fixture::new(181);
    drop(fixture.acquire(182));
    begin_restore(&fixture);
    let (owner, _service) = attempt(&fixture);
    let mut custody = begin(&fixture, &owner, 183);
    custody
        .advance(&owner, &CommandCancellation::new())
        .unwrap();
    let before = snapshot(&fixture);
    let window = &before.windows()[0];
    let session = fixture.state.session();
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    command
        .add(session.activate_restoring_claim(
            session.revision(&fixture.store).unwrap(),
            beryl_state::ActivateRestoringClaim::new(
                before.header().revision(),
                window.window_id(),
                window.revision(),
                window.selected_thread().unwrap(),
            ),
        ))
        .unwrap();
    assert!(matches!(
        fixture.store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let active = snapshot(&fixture);
    assert!(
        custody
            .advance(&owner, &CommandCancellation::new())
            .is_err()
    );
    retire(custody);
    assert_eq!(snapshot(&fixture), active);
    let window = &active.windows()[0];
    assert!(
        owner
            .begin(
                active.header().revision(),
                window.window_id(),
                composer_support::activation(
                    window.selected_thread().unwrap().thread_id(),
                    184,
                    185,
                    1,
                    0
                ),
                composer_support::fixture::operation_id(186),
                MainWindowComposerMarkerMetadataAuthority::new(fixture.state.assets())
            )
            .is_err()
    );
}

#[test]
fn foreign_domain_handles_cannot_create_a_restored_preparation_service() {
    let first = Fixture::new(191);
    let foreign = Fixture::new(201);
    assert!(
        RestoredWindowPreparationAttempt::new_for_test(
            first.store.clone(),
            foreign.state.session(),
            first.storage.clone()
        )
        .is_err()
    );
    assert!(
        RestoredWindowPreparationAttempt::new_for_test(
            first.store.clone(),
            first.state.session(),
            foreign.storage.clone()
        )
        .is_err()
    );
}
