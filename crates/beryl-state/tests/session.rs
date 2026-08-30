mod support;

use beryl_home_store::{CommandOutcome, HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{
    MonitorHint, MonitorId, RootId, RuntimeId, SyndicThreadId, WindowBounds, WindowDisplayState,
    WindowId, WindowPlacement,
};
use beryl_state::{
    ActivateRestoringClaim, BeginSessionRestore, BerylStateBootstrap, BerylStateRegistrationError,
    CreateClaimedWindow, InitializeThreadlessWindow, MarkOrderlyExit, RememberedTarget,
    RemoveSessionWindow, ReplaceWindowClaim, SESSION_HEADER_V1_BYTES, SESSION_WINDOW_V1_BYTES,
    SessionExitIntent, SessionMutationError, SessionState, UpdateWindowPlacement,
};
use tempfile::tempdir;

use support::{contributor_source, execute};

fn placement(seed: i32) -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(seed, seed + 1, 900, 700).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
}

fn target(runtime: u8, root: u8) -> RememberedTarget {
    RememberedTarget::new(
        RuntimeId::from_bytes([runtime; 16]),
        RootId::from_bytes([root; 16]),
    )
}

fn bootstrap(store: &HomeStore, session: &SessionState) -> beryl_state::MinimalSessionBootstrap {
    session.clone().minimal_bootstrap(store).unwrap().unwrap()
}

#[test]
fn minimal_bootstrap_is_session_only_and_accepts_fixed_all_zero_identity_shapes() {
    let directory = tempdir().unwrap();
    let mut store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let state = BerylStateBootstrap::register(&mut store).unwrap();
    assert!(state.session().minimal_bootstrap(&store).unwrap().is_none());

    let monitor = MonitorHint::new(
        MonitorId::new("m".repeat(MonitorId::MAX_BYTES)).unwrap(),
        WindowBounds::new(-100, -50, 1920, 1080).unwrap(),
    );
    let placement = WindowPlacement::new(
        WindowBounds::new(0, 0, 1200, 800).unwrap(),
        WindowDisplayState::Maximized,
        Some(monitor),
        Some(beryl_model::VirtualDesktopId::from_bytes([0; 16])),
    );
    let window_id = WindowId::from_bytes([0; 16]);
    match execute(
        &store,
        state.session().initialize_threadless(
            state.session().revision(&store).unwrap(),
            InitializeThreadlessWindow::new(window_id, placement.clone()),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session command, got {outcome:?}"),
    }

    let snapshot = bootstrap(&store, &state.session());
    assert_eq!(snapshot.header().revision().get(), 1);
    assert_eq!(snapshot.header().exit_intent(), SessionExitIntent::Running);
    assert_eq!(snapshot.header().fallback(), None);
    assert_eq!(snapshot.windows().len(), 1);
    assert_eq!(snapshot.windows()[0].window_id(), window_id);
    assert_eq!(snapshot.windows()[0].placement(), &placement);
    assert_eq!(snapshot.windows()[0].remembered_target(), None);
    assert_eq!(snapshot.windows()[0].selected_thread(), None);
    assert_eq!(SESSION_HEADER_V1_BYTES, 6_188);
    assert_eq!(SESSION_WINDOW_V1_BYTES, 655);

    let _complete = state.complete(&mut store).unwrap();
    store.close().unwrap();
    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let reopened_state = BerylStateBootstrap::register(&mut reopened).unwrap();
    assert_eq!(
        bootstrap(&reopened, &reopened_state.session()).windows()[0].placement(),
        &placement
    );
}

#[test]
fn claim_replacement_window_updates_and_final_removal_preserve_exact_revisions_and_fallback() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let session = state.session();
    let first_window = WindowId::from_bytes([1; 16]);
    match execute(
        &store,
        session.initialize_threadless(
            session.revision(&store).unwrap(),
            InitializeThreadlessWindow::new(first_window, placement(1)),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session command, got {outcome:?}"),
    }

    let initial = bootstrap(&store, &session);
    match execute(
        &store,
        session.replace_claim(
            session.revision(&store).unwrap(),
            ReplaceWindowClaim::new(
                initial.header().revision(),
                first_window,
                initial.windows()[0].revision(),
                None,
                target(1, 2),
                SyndicThreadId::from_bytes([10; 16]),
            ),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session command, got {outcome:?}"),
    }
    let established = bootstrap(&store, &session);
    let first_selection = established.windows()[0].selected_thread().unwrap();
    assert_eq!(established.header().revision().get(), 2);
    assert_eq!(established.windows()[0].revision().get(), 2);
    assert_eq!(first_selection.generation().get(), 2);
    assert_eq!(first_selection.revision().get(), 1);
    assert_eq!(established.header().fallback(), Some(target(1, 2)));

    let second_window = WindowId::from_bytes([2; 16]);
    match execute(
        &store,
        session.create_claimed_window(
            session.revision(&store).unwrap(),
            CreateClaimedWindow::new(
                established.header().revision(),
                second_window,
                target(3, 4),
                SyndicThreadId::from_bytes([20; 16]),
                placement(2),
            ),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session command, got {outcome:?}"),
    }
    let created = bootstrap(&store, &session);
    assert_eq!(created.header().revision().get(), 3);
    assert_eq!(created.header().fallback(), Some(target(3, 4)));
    assert_eq!(
        created.windows()[0].selected_thread(),
        Some(first_selection)
    );
    assert_eq!(created.windows()[1].revision().get(), 1);

    match execute(
        &store,
        session.update_placement(
            session.revision(&store).unwrap(),
            UpdateWindowPlacement::new(
                created.header().revision(),
                first_window,
                created.windows()[0].revision(),
                placement(9),
            ),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session command, got {outcome:?}"),
    }
    let moved = bootstrap(&store, &session);
    assert_eq!(moved.header().revision().get(), 4);
    assert_eq!(moved.windows()[0].revision().get(), 3);
    assert_eq!(moved.windows()[0].selected_thread(), Some(first_selection));

    match execute(
        &store,
        session.replace_claim(
            session.revision(&store).unwrap(),
            ReplaceWindowClaim::new(
                moved.header().revision(),
                first_window,
                moved.windows()[0].revision(),
                moved.windows()[0].selected_thread(),
                target(5, 6),
                SyndicThreadId::from_bytes([30; 16]),
            ),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session command, got {outcome:?}"),
    }
    let replaced = bootstrap(&store, &session);
    let replacement = replaced.windows()[0].selected_thread().unwrap();
    assert_eq!(replaced.header().revision().get(), 5);
    assert_eq!(replaced.windows()[0].revision().get(), 4);
    assert_eq!(replacement.generation().get(), 5);
    assert_eq!(replacement.revision().get(), 2);

    match execute(
        &store,
        session.remove_window(
            session.revision(&store).unwrap(),
            RemoveSessionWindow::new(
                replaced.header().revision(),
                second_window,
                replaced.windows()[1].revision(),
                replaced.windows()[1].selected_thread(),
            ),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session command, got {outcome:?}"),
    }
    let one = bootstrap(&store, &session);
    match execute(
        &store,
        session.remove_window(
            session.revision(&store).unwrap(),
            RemoveSessionWindow::new(
                one.header().revision(),
                first_window,
                one.windows()[0].revision(),
                one.windows()[0].selected_thread(),
            ),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session command, got {outcome:?}"),
    }
    let empty = bootstrap(&store, &session);
    assert!(empty.windows().is_empty());
    assert_eq!(empty.header().revision().get(), 7);
    assert_eq!(empty.header().fallback(), Some(target(5, 6)));
}

#[test]
fn begin_restore_is_generation_atomic_and_activation_advances_only_changed_records() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let session = state.session();
    let window_id = WindowId::from_bytes([1; 16]);
    match execute(
        &store,
        session.initialize_threadless(
            session.revision(&store).unwrap(),
            InitializeThreadlessWindow::new(window_id, placement(1)),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session command, got {outcome:?}"),
    }
    let initial = bootstrap(&store, &session);
    match execute(
        &store,
        session.replace_claim(
            session.revision(&store).unwrap(),
            ReplaceWindowClaim::new(
                initial.header().revision(),
                window_id,
                initial.windows()[0].revision(),
                None,
                target(1, 2),
                SyndicThreadId::from_bytes([9; 16]),
            ),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session command, got {outcome:?}"),
    }
    let active = bootstrap(&store, &session);
    match execute(
        &store,
        session.mark_orderly_exit(
            session.revision(&store).unwrap(),
            MarkOrderlyExit::new(active.header().revision()),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session command, got {outcome:?}"),
    }
    let exited = bootstrap(&store, &session);
    let active_hook = exited.windows()[0].selected_thread().unwrap();
    assert_eq!(
        exited.header().exit_intent(),
        SessionExitIntent::OrderlyExit
    );
    assert_eq!(active_hook.revision().get(), 1);

    match execute(
        &store,
        session.begin_restore(
            session.revision(&store).unwrap(),
            BeginSessionRestore::new(exited.header().revision()),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session command, got {outcome:?}"),
    }
    let restoring = bootstrap(&store, &session);
    let restoring_hook = restoring.windows()[0].selected_thread().unwrap();
    assert_eq!(restoring.header().revision().get(), 4);
    assert_eq!(restoring.header().exit_intent(), SessionExitIntent::Running);
    assert_eq!(restoring.windows()[0].revision().get(), 3);
    assert_eq!(restoring_hook.generation().get(), 4);
    assert_eq!(restoring_hook.revision().get(), 2);

    match execute(
        &store,
        session.begin_restore(
            session.revision(&store).unwrap(),
            BeginSessionRestore::new(restoring.header().revision()),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session command, got {outcome:?}"),
    }
    let repeated = bootstrap(&store, &session);
    assert_eq!(repeated.header().revision().get(), 5);
    assert_eq!(
        repeated.windows()[0].revision(),
        restoring.windows()[0].revision()
    );
    assert_eq!(
        repeated.windows()[0].selected_thread(),
        Some(restoring_hook)
    );

    match execute(
        &store,
        session.activate_restoring_claim(
            session.revision(&store).unwrap(),
            ActivateRestoringClaim::new(
                repeated.header().revision(),
                window_id,
                repeated.windows()[0].revision(),
                restoring_hook,
            ),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session command, got {outcome:?}"),
    }
    let activated = bootstrap(&store, &session);
    let hook = activated.windows()[0].selected_thread().unwrap();
    assert_eq!(activated.header().revision().get(), 6);
    assert_eq!(activated.windows()[0].revision().get(), 4);
    assert_eq!(hook.generation().get(), 6);
    assert_eq!(hook.revision().get(), 3);
    assert_eq!(activated.header().fallback(), Some(target(1, 2)));
}

#[test]
fn exclusive_claims_and_exact_record_expectations_reject_stale_or_noop_commands() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let session = state.session();
    let window_id = WindowId::from_bytes([1; 16]);
    let thread_id = SyndicThreadId::from_bytes([7; 16]);
    match execute(
        &store,
        session.initialize_threadless(
            session.revision(&store).unwrap(),
            InitializeThreadlessWindow::new(window_id, placement(1)),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session command, got {outcome:?}"),
    }
    let initial = bootstrap(&store, &session);
    match execute(
        &store,
        session.replace_claim(
            session.revision(&store).unwrap(),
            ReplaceWindowClaim::new(
                initial.header().revision(),
                window_id,
                initial.windows()[0].revision(),
                None,
                target(1, 2),
                thread_id,
            ),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session command, got {outcome:?}"),
    }
    let current = bootstrap(&store, &session);

    let duplicate = match execute(
        &store,
        session.create_claimed_window(
            session.revision(&store).unwrap(),
            CreateClaimedWindow::new(
                current.header().revision(),
                WindowId::from_bytes([2; 16]),
                target(3, 4),
                thread_id,
                placement(2),
            ),
        ),
    ) {
        CommandOutcome::NotCommitted { evidence } => evidence,
        outcome => panic!("expected rejected duplicate session command, got {outcome:?}"),
    };
    assert!(matches!(
        contributor_source::<SessionMutationError>(&duplicate),
        Some(SessionMutationError::ThreadAlreadyClaimed { .. })
    ));

    let unchanged = match execute(
        &store,
        session.update_placement(
            session.revision(&store).unwrap(),
            UpdateWindowPlacement::new(
                current.header().revision(),
                window_id,
                current.windows()[0].revision(),
                current.windows()[0].placement().clone(),
            ),
        ),
    ) {
        CommandOutcome::NotCommitted { evidence } => evidence,
        outcome => panic!("expected rejected unchanged placement command, got {outcome:?}"),
    };
    assert!(matches!(
        contributor_source::<SessionMutationError>(&unchanged),
        Some(SessionMutationError::PlacementUnchanged { .. })
    ));

    let stale_claim = match execute(
        &store,
        session.replace_claim(
            session.revision(&store).unwrap(),
            ReplaceWindowClaim::new(
                current.header().revision(),
                window_id,
                current.windows()[0].revision(),
                None,
                target(5, 6),
                SyndicThreadId::from_bytes([8; 16]),
            ),
        ),
    ) {
        CommandOutcome::NotCommitted { evidence } => evidence,
        outcome => panic!("expected rejected stale claim command, got {outcome:?}"),
    };
    assert!(matches!(
        contributor_source::<SessionMutationError>(&stale_claim),
        Some(SessionMutationError::ClaimExpectationConflict { .. })
    ));

    match execute(
        &store,
        session.mark_orderly_exit(
            session.revision(&store).unwrap(),
            MarkOrderlyExit::new(current.header().revision()),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session command, got {outcome:?}"),
    }
    let exited = bootstrap(&store, &session);
    let blocked = match execute(
        &store,
        session.update_placement(
            session.revision(&store).unwrap(),
            UpdateWindowPlacement::new(
                exited.header().revision(),
                window_id,
                exited.windows()[0].revision(),
                placement(3),
            ),
        ),
    ) {
        CommandOutcome::NotCommitted { evidence } => evidence,
        outcome => panic!("expected rejected orderly-exit command, got {outcome:?}"),
    };
    assert!(matches!(
        contributor_source::<SessionMutationError>(&blocked),
        Some(SessionMutationError::OrderlyExitInProgress)
    ));
}

#[test]
fn session_bootstrap_cannot_complete_against_a_different_home() {
    let first_directory = tempdir().unwrap();
    let second_directory = tempdir().unwrap();
    let mut first = HomeStore::open(HomeOpenOptions::new(
        first_directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let bootstrap = BerylStateBootstrap::register(&mut first).unwrap();
    let mut second = HomeStore::open(HomeOpenOptions::new(
        second_directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let error = match bootstrap.complete(&mut second) {
        Ok(_) => panic!("session bootstrap unexpectedly crossed Beryl homes"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        BerylStateRegistrationError::BootstrapHomeMismatch { .. }
    ));
}
