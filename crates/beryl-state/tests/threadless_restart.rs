mod support;

use beryl_home_store::{
    CommandOutcome, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    ReconciliationResolution,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    RootId, RuntimeId, SessionRevision, SyndicThreadId, WindowBounds, WindowDisplayState, WindowId,
    WindowPlacement,
};
use beryl_state::{
    BeginSessionRestore, BerylState, InitializeThreadlessWindow, MinimalSessionBootstrap,
    RememberedTarget, RemoveSessionWindow, ReplaceWindowClaim, SessionExitIntent,
    SessionMutationError, SessionState,
};

fn placement() -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(0, 0, 800, 600).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
}

fn snapshot(home: &HomeStore, session: &SessionState) -> MinimalSessionBootstrap {
    session.minimal_bootstrap(home).unwrap().unwrap()
}

fn committed(outcome: CommandOutcome) {
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "{outcome:?}"
    );
}

fn initialize(home: &HomeStore, session: &SessionState) {
    committed(support::execute(
        home,
        session.initialize_threadless(
            session.revision(home).unwrap(),
            InitializeThreadlessWindow::new(WindowId::from_bytes([1; 16]), placement()),
        ),
    ));
}

fn rejection(outcome: &CommandOutcome) -> Option<&SessionMutationError> {
    let CommandOutcome::NotCommitted { evidence } = outcome else {
        panic!("expected rejection, got {outcome:?}");
    };
    support::contributor_source(evidence)
}

fn remove_only_window(home: &HomeStore, session: &SessionState) -> SessionRevision {
    let current = snapshot(home, session);
    let window = &current.windows()[0];
    committed(support::execute(
        home,
        session.remove_window(
            session.revision(home).unwrap(),
            RemoveSessionWindow::new(
                current.header().revision(),
                window.window_id(),
                window.revision(),
                window.selected_thread(),
            ),
        ),
    ));
    let current = snapshot(home, session);
    assert!(current.windows().is_empty());
    current.header().revision()
}

fn restore(home: &HomeStore, session: &SessionState, expected: SessionRevision) -> CommandOutcome {
    support::execute(
        home,
        session.initialize_threadless(
            session.revision(home).unwrap(),
            InitializeThreadlessWindow::for_empty_session(
                expected,
                WindowId::from_bytes([2; 16]),
                placement(),
            ),
        ),
    )
}

#[test]
fn reopened_empty_session_initializes_one_threadless_window_without_resetting_revision() {
    let directory = tempfile::tempdir().unwrap();
    let (home, state) = support::open(directory.path());
    initialize(&home, &state.session());
    let expected = remove_only_window(&home, &state.session());
    home.close().unwrap();
    let (home, state) = support::open(directory.path());
    let session = state.session();
    committed(restore(&home, &session, expected));
    let current = snapshot(&home, &session);
    assert_eq!(
        current.header().revision(),
        expected.checked_next().unwrap()
    );
    assert_eq!(current.header().exit_intent(), SessionExitIntent::Running);
    assert_eq!(current.header().fallback(), None);
    assert_eq!(current.windows().len(), 1);
    let window = &current.windows()[0];
    assert_eq!(window.window_id(), WindowId::from_bytes([2; 16]));
    assert_eq!(window.selected_thread(), None);
    assert_eq!(window.remembered_target(), None);
    assert_eq!(window.placement(), &placement());
    assert_eq!(window.revision().get(), 1);
    let repeat = restore(&home, &session, current.header().revision());
    assert!(matches!(
        rejection(&repeat),
        Some(SessionMutationError::InvalidCurrentState(_))
    ));
    assert_eq!(
        snapshot(&home, &session).header().revision(),
        current.header().revision()
    );
}

#[test]
fn missing_and_existing_headers_cannot_be_substituted_for_each_other() {
    let directory = tempfile::tempdir().unwrap();
    let (home, state) = support::open(directory.path());
    let session = state.session();
    let missing = restore(&home, &session, SessionRevision::new(1).unwrap());
    assert!(matches!(
        rejection(&missing),
        Some(SessionMutationError::NotInitialized)
    ));
    assert!(session.minimal_bootstrap(&home).unwrap().is_none());
    initialize(&home, &session);
    remove_only_window(&home, &session);
    let before = snapshot(&home, &session);
    let existing = support::execute(
        &home,
        session.initialize_threadless(
            session.revision(&home).unwrap(),
            InitializeThreadlessWindow::new(WindowId::from_bytes([2; 16]), placement()),
        ),
    );
    assert!(matches!(
        rejection(&existing),
        Some(SessionMutationError::AlreadyInitialized)
    ));
    assert_eq!(
        snapshot(&home, &session).header().revision(),
        before.header().revision()
    );
}

#[test]
fn changed_empty_header_rejects_the_original_initialization_request() {
    let directory = tempfile::tempdir().unwrap();
    let (home, state) = support::open(directory.path());
    let session = state.session();
    initialize(&home, &session);
    let expected = remove_only_window(&home, &session);
    committed(support::execute(
        &home,
        session.begin_restore(
            session.revision(&home).unwrap(),
            BeginSessionRestore::new(expected),
        ),
    ));
    let outcome = restore(&home, &session, expected);
    assert!(matches!(
        rejection(&outcome),
        Some(SessionMutationError::SessionRevisionConflict { .. })
    ));
    let current = snapshot(&home, &session);
    assert!(current.windows().is_empty());
    committed(restore(&home, &session, current.header().revision()));
}

#[test]
fn nonempty_or_runtime_backed_sessions_cannot_be_replaced_by_threadless_initialization() {
    let directory = tempfile::tempdir().unwrap();
    let (home, state) = support::open(directory.path());
    let session = state.session();
    initialize(&home, &session);
    let before = snapshot(&home, &session);
    let outcome = restore(&home, &session, before.header().revision());
    assert!(matches!(
        rejection(&outcome),
        Some(SessionMutationError::InvalidCurrentState(_))
    ));
    let window = &before.windows()[0];
    let target = RememberedTarget::new(RuntimeId::from_bytes([3; 16]), RootId::from_bytes([4; 16]));
    committed(support::execute(
        &home,
        session.replace_claim(
            session.revision(&home).unwrap(),
            ReplaceWindowClaim::new(
                before.header().revision(),
                window.window_id(),
                window.revision(),
                None,
                target,
                SyndicThreadId::from_bytes([5; 16]),
            ),
        ),
    ));
    let expected = remove_only_window(&home, &session);
    let outcome = restore(&home, &session, expected);
    assert!(matches!(
        rejection(&outcome),
        Some(SessionMutationError::InvalidCurrentState(_))
    ));
    let after = snapshot(&home, &session);
    assert_eq!(after.header().fallback(), Some(target));
    assert_eq!(after.header().revision(), expected);
    assert!(after.windows().is_empty());
}

#[test]
fn uncertain_initialization_retains_and_settles_the_original_command_scope() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let home = candidate
        .prepare_publication(BerylState::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let session = state.session();
    initialize(&home, &session);
    let expected = remove_only_window(&home, &session);
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let outcome = restore(&home, &session, expected);
    let CommandOutcome::Indeterminate { reconciliation, .. } = outcome else {
        panic!("{outcome:?}");
    };
    let reconciliation = reconciliation.install_and_handle();
    assert_eq!(home.pending_reconciliations().len(), 1);
    assert!(matches!(
        home.retry_reconciliation(&reconciliation).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert!(home.pending_reconciliations().is_empty());
    let after = snapshot(&home, &session);
    assert_eq!(after.header().revision(), expected.checked_next().unwrap());
    assert_eq!(after.windows().len(), 1);
    home.close().unwrap();
}
