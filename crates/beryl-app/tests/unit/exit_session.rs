use super::*;
use beryl_home_store::{
    CommandOutcome, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, MutationContribution,
};
use beryl_model::{RootId, RuntimeId, SyndicThreadId, WindowBounds, WindowDisplayState};
use beryl_state::{
    BerylState, CreateClaimedWindow, InitializeThreadlessWindow, RememberedTarget,
    ReplaceWindowClaim, UpdateWindowPlacement,
};

fn placement(seed: i32) -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(seed, -seed, 900, 700).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
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

fn execute(home: &HomeStore, contribution: MutationContribution) {
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    committed(home.execute(command));
}

fn open(count: usize) -> (tempfile::TempDir, HomeStore, SessionState) {
    open_with_faults(count, FaultController::new())
}

fn open_with_faults(
    count: usize,
    faults: FaultController,
) -> (tempfile::TempDir, HomeStore, SessionState) {
    let directory = tempfile::tempdir().unwrap();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let home = candidate
        .prepare_publication(BerylState::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let session = state.session();
    if count != 0 {
        let first = WindowId::from_bytes(0u128.to_be_bytes());
        execute(
            &home,
            session.initialize_threadless(
                session.revision(&home).unwrap(),
                InitializeThreadlessWindow::new(first, placement(0)),
            ),
        );
        let target =
            RememberedTarget::new(RuntimeId::from_bytes([1; 16]), RootId::from_bytes([2; 16]));
        if count > 1 {
            let snapshot = session.minimal_bootstrap(&home).unwrap().unwrap();
            execute(
                &home,
                session.replace_claim(
                    session.revision(&home).unwrap(),
                    ReplaceWindowClaim::new(
                        snapshot.header().revision(),
                        first,
                        snapshot.windows()[0].revision(),
                        None,
                        target,
                        SyndicThreadId::from_bytes(0u128.to_be_bytes()),
                    ),
                ),
            );
        }
        for ordinal in 1..count {
            let snapshot = session.minimal_bootstrap(&home).unwrap().unwrap();
            execute(
                &home,
                session.create_claimed_window(
                    session.revision(&home).unwrap(),
                    CreateClaimedWindow::new(
                        snapshot.header().revision(),
                        WindowId::from_bytes((ordinal as u128).to_be_bytes()),
                        target,
                        SyndicThreadId::from_bytes((ordinal as u128).to_be_bytes()),
                        placement(ordinal as i32),
                    ),
                ),
            );
        }
    }
    (directory, home, session)
}

fn placements(count: usize) -> Vec<(WindowId, WindowPlacement)> {
    (0..count)
        .rev()
        .map(|ordinal| {
            (
                WindowId::from_bytes((ordinal as u128).to_be_bytes()),
                placement(1000 + ordinal as i32),
            )
        })
        .collect()
}

#[test]
fn exit_session_preparation_binds_shuffled_complete_set_without_writing() {
    for count in [1, 3, MAX_RESTORABLE_WINDOWS] {
        let (_directory, home, session) = open(count);
        let before = session.minimal_bootstrap(&home).unwrap().unwrap();
        let revision = home.home_revision().unwrap();
        let claims = before
            .windows()
            .iter()
            .map(|window| {
                session
                    .window_claim_catalog_source(&home, window.window_id())
                    .unwrap()
                    .claim()
            })
            .collect::<Vec<_>>();
        let command = prepare_exit_session_command(&home, &session, placements(count)).unwrap();
        assert_eq!(home.home_revision().unwrap(), revision);
        assert_eq!(session.minimal_bootstrap(&home).unwrap().unwrap(), before);
        assert_eq!(command.publication.source, before);
        assert_eq!(command.publication.configured_home, home.configured_path());
        let publication = command.publication;
        committed(home.execute(command.command));
        let after = session.minimal_bootstrap(&home).unwrap().unwrap();
        assert_eq!(after.header().exit_intent(), SessionExitIntent::OrderlyExit);
        assert_eq!(
            publication.result_session_revision,
            after.header().revision()
        );
        assert_eq!(
            publication.result_windows,
            after
                .windows()
                .iter()
                .map(|w| (w.window_id(), w.revision(), w.placement().clone()))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            after.header().revision(),
            before.header().revision().checked_next().unwrap()
        );
        assert_eq!(after.header().fallback(), before.header().fallback());
        for (ordinal, (old, new)) in before.windows().iter().zip(after.windows()).enumerate() {
            assert_eq!(new.window_id(), old.window_id());
            assert_eq!(new.placement(), &placement(1000 + ordinal as i32));
            assert_eq!(new.revision().get(), old.revision().get() + 1);
            assert_eq!(new.selected_thread(), old.selected_thread());
            assert_eq!(new.remembered_target(), old.remembered_target());
            assert_eq!(
                session
                    .window_claim_catalog_source(&home, new.window_id())
                    .unwrap()
                    .claim(),
                claims[ordinal]
            );
        }
        assert!(matches!(
            prepare_exit_session_command(&home, &session, placements(count)),
            Err(ExitSessionPreparationError::NotRunning)
        ));
        assert_eq!(session.minimal_bootstrap(&home).unwrap().unwrap(), after);
        home.close().unwrap();
    }
}

#[test]
fn exit_session_preparation_rejects_invalid_membership_without_writing() {
    let (_directory, home, session) = open(3);
    let before = session.minimal_bootstrap(&home).unwrap();
    let revision = home.home_revision().unwrap();
    let mut duplicate = placements(3);
    duplicate[0].0 = duplicate[1].0;
    let mut foreign = placements(3);
    foreign[0].0 = WindowId::from_bytes([255; 16]);
    for input in [
        vec![],
        placements(2),
        placements(4),
        duplicate,
        foreign,
        placements(MAX_RESTORABLE_WINDOWS + 1),
    ] {
        assert!(matches!(
            prepare_exit_session_command(&home, &session, input),
            Err(ExitSessionPreparationError::WindowSet)
        ));
        assert_eq!(home.home_revision().unwrap(), revision);
        assert_eq!(session.minimal_bootstrap(&home).unwrap(), before);
    }
    home.close().unwrap();
}

#[test]
fn exit_session_preparation_refuses_absent_session() {
    let (_directory, home, session) = open(0);
    let revision = home.home_revision().unwrap();
    assert!(matches!(
        prepare_exit_session_command(&home, &session, placements(1)),
        Err(ExitSessionPreparationError::MissingSession)
    ));
    assert_eq!(home.home_revision().unwrap(), revision);
    home.close().unwrap();
}

#[test]
fn exit_session_preparation_preserves_original_revision_at_writer() {
    let (_directory, home, session) = open(1);
    let before = session.minimal_bootstrap(&home).unwrap().unwrap();
    let command = prepare_exit_session_command(&home, &session, placements(1)).unwrap();
    execute(
        &home,
        session.update_placement(
            session.revision(&home).unwrap(),
            UpdateWindowPlacement::new(
                before.header().revision(),
                before.windows()[0].window_id(),
                before.windows()[0].revision(),
                placement(55),
            ),
        ),
    );
    let changed = session.minimal_bootstrap(&home).unwrap();
    assert!(matches!(
        home.execute(command.command),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(session.minimal_bootstrap(&home).unwrap(), changed);
    home.close().unwrap();
}

#[test]
fn exit_session_preparation_rejects_foreign_home_handle() {
    let (_first_directory, first, session) = open(1);
    let (_second_directory, second, second_session) = open(1);
    let before = second_session.minimal_bootstrap(&second).unwrap();
    assert!(matches!(
        prepare_exit_session_command(&second, &session, placements(1)),
        Err(ExitSessionPreparationError::Read(_))
    ));
    assert_eq!(second_session.minimal_bootstrap(&second).unwrap(), before);
    first.close().unwrap();
    second.close().unwrap();
}

#[test]
fn exit_session_preparation_abandonment_has_no_effect() {
    let (_directory, home, session) = open(1);
    let before = session.minimal_bootstrap(&home).unwrap();
    let revision = home.home_revision().unwrap();
    drop(prepare_exit_session_command(&home, &session, placements(1)).unwrap());
    assert_eq!(home.home_revision().unwrap(), revision);
    assert_eq!(session.minimal_bootstrap(&home).unwrap(), before);
    home.close().unwrap();
}
