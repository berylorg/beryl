#[path = "window_presentation/schema.rs"]
mod schema;
mod support;

use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::WindowId;
use beryl_state::{
    ActivityPanelHeight, BerylState, PreparedWindowMutation, SessionState,
    SessionWindowRemovalState,
};
use support::session_exit::{committed, seed, snapshot};

fn height(
    home: &beryl_home_store::HomeStore,
    session: &SessionState,
    window: WindowId,
    value: f32,
) {
    committed(support::execute(
        home,
        session
            .prepare_activity_panel_height(home, window, ActivityPanelHeight::new(value).unwrap())
            .unwrap(),
    ));
}

fn command(home: &beryl_home_store::HomeStore, bundle: PreparedWindowMutation) -> HomeCommand {
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    bundle.add_to(&mut command).unwrap();
    command
}

#[test]
fn independent_window_preferences_reopen_without_changing_session_or_claims() {
    let directory = tempfile::tempdir().unwrap();
    let (home, state) = support::open(directory.path());
    let session = state.session();
    seed(&home, &session, 2);
    let before = snapshot(&home, &session);
    let a = before.windows()[0].window_id();
    let b = before.windows()[1].window_id();
    assert!(session.window_presentation(&home, a).unwrap().is_none());
    height(&home, &session, a, 144.125);
    height(&home, &session, b, 311.75);
    height(&home, &session, a, 225.5);
    assert_eq!(snapshot(&home, &session), before);
    let expected_a = session.window_presentation(&home, a).unwrap().unwrap();
    assert_eq!(expected_a.revision().get(), 2);
    let expected_b = session.window_presentation(&home, b).unwrap().unwrap();
    home.close().unwrap();
    let (home, state) = support::open(directory.path());
    assert_eq!(snapshot(&home, &state.session()), before);
    assert_eq!(
        state
            .session()
            .window_presentation(&home, a)
            .unwrap()
            .as_ref(),
        Some(&expected_a)
    );
    assert_eq!(
        state
            .session()
            .window_presentation(&home, b)
            .unwrap()
            .as_ref(),
        Some(&expected_b)
    );
    home.close().unwrap();
}

#[test]
fn retained_window_selection_exit_and_restore_preserve_its_preference() {
    let directory = tempfile::tempdir().unwrap();
    let (home, state) = support::open(directory.path());
    let session = state.session();
    seed(&home, &session, 2);
    let current = snapshot(&home, &session);
    let window = &current.windows()[0];
    let id = window.window_id();
    height(&home, &session, id, 187.875);
    let original = session.window_presentation(&home, id).unwrap();
    committed(support::execute(
        &home,
        session.replace_claim(
            session.revision(&home).unwrap(),
            beryl_state::ReplaceWindowClaim::new(
                current.header().revision(),
                id,
                window.revision(),
                window.selected_thread(),
                window.remembered_target().unwrap(),
                beryl_model::SyndicThreadId::from_bytes([91; 16]),
            ),
        ),
    ));
    assert_eq!(session.window_presentation(&home, id).unwrap(), original);
    let current = snapshot(&home, &session);
    committed(support::execute(
        &home,
        session.mark_orderly_exit(
            session.revision(&home).unwrap(),
            beryl_state::MarkOrderlyExit::new(current.header().revision()),
        ),
    ));
    assert_eq!(session.window_presentation(&home, id).unwrap(), original);
    assert!(
        session
            .prepare_activity_panel_height(&home, id, ActivityPanelHeight::new(200.0).unwrap())
            .is_err()
    );
    let current = snapshot(&home, &session);
    committed(support::execute(
        &home,
        session.begin_restore(
            session.revision(&home).unwrap(),
            beryl_state::BeginSessionRestore::new(current.header().revision()),
        ),
    ));
    assert_eq!(session.window_presentation(&home, id).unwrap(), original);
    home.close().unwrap();
}

#[test]
fn exact_absence_presence_session_and_foreign_sources_refuse_stale_height_writes() {
    let directory = tempfile::tempdir().unwrap();
    let (home, state) = support::open(directory.path());
    let session = state.session();
    seed(&home, &session, 2);
    let before = snapshot(&home, &session);
    let window = before.windows()[0].window_id();
    let absent = session
        .prepare_activity_panel_height(&home, window, ActivityPanelHeight::new(200.0).unwrap())
        .unwrap();
    height(&home, &session, window, 250.0);
    let revision = home.home_revision().unwrap();
    assert!(matches!(
        home.execute(command(&home, absent)),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(home.home_revision().unwrap(), revision);
    let present = session
        .prepare_activity_panel_height(&home, window, ActivityPanelHeight::new(210.0).unwrap())
        .unwrap();
    let evidence = session.capture_window_removal(&home, window).unwrap();
    committed(support::execute(
        &home,
        session
            .remove_captured_window(&home, session.revision(&home).unwrap(), &evidence)
            .unwrap(),
    ));
    assert!(matches!(
        home.execute(command(&home, present)),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(
        session
            .prepare_activity_panel_height(&home, window, ActivityPanelHeight::new(100.0).unwrap())
            .is_err()
    );
    assert!(
        session
            .window_presentation(&home, window)
            .unwrap()
            .is_none()
    );
    let foreign = tempfile::tempdir().unwrap();
    let (other, _) = support::open(foreign.path());
    assert!(
        session
            .prepare_activity_panel_height(&other, window, ActivityPanelHeight::new(100.0).unwrap())
            .is_err()
    );
    other.close().unwrap();
    home.close().unwrap();
}

#[test]
fn captured_absence_cannot_remove_a_new_preference_or_change_session_membership() {
    let directory = tempfile::tempdir().unwrap();
    let (home, state) = support::open(directory.path());
    let session = state.session();
    seed(&home, &session, 1);
    let before = snapshot(&home, &session);
    let window = before.windows()[0].window_id();
    let evidence = session.capture_window_removal(&home, window).unwrap();
    let removal = session
        .remove_captured_window(&home, session.revision(&home).unwrap(), &evidence)
        .unwrap();
    height(&home, &session, window, 204.0);
    let revision = home.home_revision().unwrap();
    assert_eq!(
        session.classify_window_removal(&home, &evidence).unwrap(),
        SessionWindowRemovalState::Collision
    );
    assert!(matches!(
        home.execute(command(&home, removal)),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(snapshot(&home, &session), before);
    assert_eq!(home.home_revision().unwrap(), revision);
    assert_eq!(
        session
            .window_presentation(&home, window)
            .unwrap()
            .unwrap()
            .activity_panel_height()
            .get(),
        204.0
    );
    home.close().unwrap();
}

#[test]
fn present_preferences_are_atomically_removed_and_recovered_with_renewed_revision() {
    for candidate in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (home, state) = support::open(directory.path());
        let session = state.session();
        seed(&home, &session, 2);
        let before = snapshot(&home, &session);
        let a = before.windows()[0].window_id();
        let b = before.windows()[1].window_id();
        height(&home, &session, a, 212.125);
        height(&home, &session, b, 196.5);
        let original = session.window_presentation(&home, a).unwrap().unwrap();
        let unaffected = session.window_presentation(&home, b).unwrap();
        let evidence = session.capture_window_removal(&home, a).unwrap();
        assert_eq!(evidence.presentation(), Some(&original));
        committed(support::execute(
            &home,
            session
                .remove_captured_window(&home, session.revision(&home).unwrap(), &evidence)
                .unwrap(),
        ));
        assert!(session.window_presentation(&home, a).unwrap().is_none());
        if candidate {
            home.inject_retained_maintenance_terminal();
            assert!(BerylState::reacquire(&home).is_err());
            let mut recovery = home.recover_same_home().unwrap();
            let fresh = BerylState::reacquire_candidate(&recovery)
                .unwrap()
                .session();
            let access = recovery.recovery_access().unwrap();
            assert_eq!(
                fresh
                    .classify_window_removal_candidate(&access, &evidence)
                    .unwrap(),
                SessionWindowRemovalState::Removed
            );
            let restored = fresh
                .recover_removed_window_candidate(
                    &access,
                    fresh.revision_candidate(&access).unwrap(),
                    &evidence,
                )
                .unwrap();
            let mut command = HomeCommand::new(access.home_revision().unwrap());
            restored.add_to(&mut command).unwrap();
            committed(access.execute(command));
            assert_eq!(
                fresh
                    .classify_window_removal_candidate(&access, &evidence)
                    .unwrap(),
                SessionWindowRemovalState::Recovered
            );
            let value = fresh
                .window_presentation_candidate(&access, a)
                .unwrap()
                .unwrap();
            assert_eq!(
                value.activity_panel_height(),
                original.activity_panel_height()
            );
            assert_eq!(value.revision().get(), original.revision().get() + 1);
            assert_eq!(
                fresh.window_presentation_candidate(&access, b).unwrap(),
                unaffected
            );
            drop(access);
            recovery.publish().unwrap().close().unwrap();
        } else {
            committed(support::execute(
                &home,
                session
                    .recover_removed_window(&home, session.revision(&home).unwrap(), &evidence)
                    .unwrap(),
            ));
            assert_eq!(
                session.classify_window_removal(&home, &evidence).unwrap(),
                SessionWindowRemovalState::Recovered
            );
            let value = session.window_presentation(&home, a).unwrap().unwrap();
            assert_eq!(
                value.activity_panel_height(),
                original.activity_panel_height()
            );
            assert_eq!(value.revision().get(), original.revision().get() + 1);
            assert_eq!(session.window_presentation(&home, b).unwrap(), unaffected);
            home.close().unwrap();
        }
    }
}

#[test]
fn ambiguous_height_write_reconciles_original_attempt_before_candidate_republication() {
    for point in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
        FaultPoint::AfterPersist,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let (home, state) =
            support::state_fixture::open_with_faults(directory.path(), faults.clone());
        let session = state.session();
        seed(&home, &session, 1);
        let window = snapshot(&home, &session).windows()[0].window_id();
        let update = session
            .prepare_activity_panel_height(&home, window, ActivityPanelHeight::new(202.25).unwrap())
            .unwrap();
        let command = command(&home, update);
        let before = home.home_revision().unwrap();
        faults.fail_next(point);
        let outcome = home.execute(command);
        let handle = match (point, outcome) {
            (FaultPoint::BeforeCommit, CommandOutcome::NotCommitted { .. }) => None,
            (
                FaultPoint::AfterPersist,
                CommandOutcome::Committed {
                    receipt,
                    later_failure: Some(beryl_home_store::CommandError::Persistence { .. }),
                    local_finalization: Some(_),
                },
            ) => {
                assert_eq!(receipt.home_revision(), before.checked_next().unwrap());
                None
            }
            (
                FaultPoint::AfterCommitBeforePersist,
                CommandOutcome::Indeterminate { reconciliation, .. },
            ) => Some(reconciliation.install_and_handle()),
            (_, outcome) => panic!("unexpected height outcome: {outcome:?}"),
        };
        if home.health().state() == beryl_home_store::HomeHealthState::Healthy {
            home.inject_retained_maintenance_terminal();
            assert!(BerylState::reacquire(&home).is_err());
        }
        let mut recovery = home.recover_same_home().unwrap();
        let fresh = BerylState::reacquire_candidate(&recovery)
            .unwrap()
            .session();
        let access = recovery.recovery_access().unwrap();
        if let Some(handle) = handle {
            assert!(matches!(
                access.reconcile(&handle).unwrap(),
                beryl_home_store::ReconciliationResolution::ExactNew { .. }
            ));
            assert!(access.pending_reconciliations().is_empty());
        }
        let value = fresh
            .window_presentation_candidate(&access, window)
            .unwrap();
        if point == FaultPoint::BeforeCommit {
            assert!(value.is_none());
        } else {
            assert_eq!(
                value.unwrap().activity_panel_height().get().to_bits(),
                202.25f32.to_bits()
            );
        }
        drop(access);
        recovery.publish().unwrap().close().unwrap();
    }
}
