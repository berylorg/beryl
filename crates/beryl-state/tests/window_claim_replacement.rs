mod support;

use beryl_home_store::CommandOutcome;
use beryl_model::{
    RootId, RuntimeId, SyndicThreadId, WindowBounds, WindowDisplayState, WindowId, WindowPlacement,
};
use beryl_state::{
    CreateClaimedWindow, InitializeThreadlessWindow, RememberedTarget, UpdateWindowPlacement,
    WindowClaimReplacementPreparation, WindowClaimReplacementState,
};
use support::execute;

fn placement() -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(0, 0, 900, 700).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
}

fn target() -> RememberedTarget {
    RememberedTarget::new(RuntimeId::from_bytes([1; 16]), RootId::from_bytes([2; 16]))
}

#[test]
fn prepared_replacement_derives_writer_selection_and_audits_both_claim_copies() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let session = state.session();
    let window = WindowId::from_bytes([1; 16]);
    let first = SyndicThreadId::from_bytes([1; 16]);
    let second = SyndicThreadId::from_bytes([2; 16]);
    assert!(matches!(
        execute(
            &store,
            session.initialize_threadless(
                session.revision(&store).unwrap(),
                InitializeThreadlessWindow::new(window, placement())
            )
        ),
        CommandOutcome::Committed { .. }
    ));
    let WindowClaimReplacementPreparation::Prepared(initial) = session
        .prepare_window_claim_replacement(&store, window, None, target(), first)
        .unwrap()
    else {
        panic!("initial preparation")
    };
    assert_eq!(
        session
            .classify_window_claim_replacement(&store, &initial)
            .unwrap(),
        WindowClaimReplacementState::Original
    );
    assert!(matches!(
        execute(&store, initial.contribution(&session, &store).unwrap()),
        CommandOutcome::Committed { .. }
    ));
    assert_eq!(
        session
            .classify_window_claim_replacement(&store, &initial)
            .unwrap(),
        WindowClaimReplacementState::Committed
    );
    let WindowClaimReplacementPreparation::Prepared(replacement) = session
        .prepare_window_claim_replacement(
            &store,
            window,
            Some(initial.future_selection()),
            target(),
            second,
        )
        .unwrap()
    else {
        panic!("replacement preparation")
    };
    assert_eq!(replacement.prior_claim(), Some(initial.future_claim()));
    assert_eq!(
        replacement.future_selection().revision(),
        initial
            .future_selection()
            .revision()
            .checked_next()
            .unwrap()
    );
    assert_eq!(
        replacement.future_selection().generation(),
        initial
            .future_selection()
            .generation()
            .checked_next()
            .unwrap()
    );
    assert!(matches!(
        execute(&store, replacement.contribution(&session, &store).unwrap()),
        CommandOutcome::Committed { .. }
    ));
    assert_eq!(
        session
            .classify_window_claim_replacement(&store, &replacement)
            .unwrap(),
        WindowClaimReplacementState::Committed
    );
    assert_eq!(
        session
            .window_claim_catalog_source(&store, window)
            .unwrap()
            .claim(),
        Some(replacement.future_claim())
    );
    assert_eq!(
        session
            .thread_claim_catalog_source(&store, first)
            .unwrap()
            .claim(),
        None
    );
    assert!(
        matches!(session.prepare_window_claim_replacement(&store, window, Some(replacement.future_selection()), target(), second).unwrap(), WindowClaimReplacementPreparation::Current { claim, window: current } if claim == replacement.future_claim() && &current == replacement.future_window())
    );
    assert!(
        session
            .prepare_window_claim_replacement(
                &store,
                window,
                Some(initial.future_selection()),
                target(),
                first
            )
            .is_err()
    );
}

#[test]
fn preparation_proves_elsewhere_and_fences_foreign_home_and_session_drift() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let session = state.session();
    let first_window = WindowId::from_bytes([1; 16]);
    let second_window = WindowId::from_bytes([2; 16]);
    let first = SyndicThreadId::from_bytes([1; 16]);
    let second = SyndicThreadId::from_bytes([2; 16]);
    assert!(matches!(
        execute(
            &store,
            session.initialize_threadless(
                session.revision(&store).unwrap(),
                InitializeThreadlessWindow::new(first_window, placement())
            )
        ),
        CommandOutcome::Committed { .. }
    ));
    let WindowClaimReplacementPreparation::Prepared(initial) = session
        .prepare_window_claim_replacement(&store, first_window, None, target(), first)
        .unwrap()
    else {
        panic!("initial preparation")
    };
    assert!(matches!(
        execute(&store, initial.contribution(&session, &store).unwrap()),
        CommandOutcome::Committed { .. }
    ));
    let bootstrap = session.minimal_bootstrap(&store).unwrap().unwrap();
    assert!(matches!(
        execute(
            &store,
            session.create_claimed_window(
                session.revision(&store).unwrap(),
                CreateClaimedWindow::new(
                    bootstrap.header().revision(),
                    second_window,
                    target(),
                    second,
                    placement()
                )
            )
        ),
        CommandOutcome::Committed { .. }
    ));
    let prior = session
        .capture_window_removal(&store, first_window)
        .unwrap();
    assert!(
        matches!(session.prepare_window_claim_replacement(&store, first_window, prior.window().selected_thread(), target(), second).unwrap(), WindowClaimReplacementPreparation::ClaimedElsewhere { claim } if claim.window_id() == second_window)
    );
    let WindowClaimReplacementPreparation::Prepared(prepared) = session
        .prepare_window_claim_replacement(
            &store,
            first_window,
            prior.window().selected_thread(),
            target(),
            SyndicThreadId::from_bytes([3; 16]),
        )
        .unwrap()
    else {
        panic!("replacement preparation")
    };
    let foreign_directory = tempfile::tempdir().unwrap();
    let (foreign, foreign_state) = support::open(foreign_directory.path());
    assert!(
        prepared
            .contribution(&foreign_state.session(), &foreign)
            .is_err()
    );
    assert!(
        session
            .classify_window_claim_replacement(&foreign, &prepared)
            .is_err()
    );
    assert!(matches!(
        execute(
            &store,
            session.update_placement(
                session.revision(&store).unwrap(),
                UpdateWindowPlacement::new(
                    prior.header().revision(),
                    first_window,
                    prior.window().revision(),
                    WindowPlacement::new(
                        WindowBounds::new(20, 0, 900, 700).unwrap(),
                        WindowDisplayState::Normal,
                        None,
                        None
                    )
                )
            )
        ),
        CommandOutcome::Committed { .. }
    ));
    assert!(prepared.contribution(&session, &store).is_err());
    assert_eq!(
        session
            .classify_window_claim_replacement(&store, &prepared)
            .unwrap(),
        WindowClaimReplacementState::Collision
    );
}
