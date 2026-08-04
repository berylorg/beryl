mod support;

use beryl_home_store::{CursorReadLimits, HomeCommand};
use beryl_model::{
    AdmittedHostPath, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
};
use beryl_state::{
    AddConfiguredRoot, ApplySettings, CreateClaimedWindow, ExpectedSettingRevision,
    InitializeThreadlessWindow, MarkOrderlyExit, RemoveSessionWindow, ReplaceWindowClaim,
    RootRegistration, RuntimeRootMutationError, SessionExitIntent, SessionMutationError,
    SettingKey, SettingUpdate, SettingValue, UnixMillis,
};
use tempfile::tempdir;

use support::phase9::{
    assert_one_success_one_conflict, command, placement, race_commands, target, thread, window,
};
use support::{contributor_source, execute, host_runtime};

fn theme(value: &str) -> SettingUpdate {
    SettingUpdate::new(
        SettingKey::ActiveThemeId,
        ExpectedSettingRevision::Absent,
        SettingValue::active_theme_id(value).unwrap(),
    )
}

fn root_registration(seed: u8, path: &str) -> RootRegistration {
    RootRegistration::new(
        RootId::from_bytes([seed; 16]),
        RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, path).unwrap(),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap(),
        UnixMillis::new(50),
        beryl_state::AvailabilitySnapshot::unknown(),
    )
}

#[test]
fn different_domains_serialize_without_lost_state() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let expected_home = store.home_revision().unwrap();

    let mut settings_command = HomeCommand::new(expected_home);
    settings_command
        .add(state.settings().apply(
            state.settings().revision(&store).unwrap(),
            ApplySettings::new(vec![theme("different-domain")]).unwrap(),
        ))
        .unwrap();
    let mut runtime_command = HomeCommand::new(expected_home);
    runtime_command
        .add(state.runtime_roots().create_runtime_with_home_root(
            state.runtime_roots().revision(&store).unwrap(),
            host_runtime(1, 2, r"C:\Codex\codex.exe", r"C:\Work\ten"),
        ))
        .unwrap();

    let results = race_commands(&store, settings_command, runtime_command);
    assert_one_success_one_conflict(&results);

    if state
        .settings()
        .setting(&store, SettingKey::ActiveThemeId)
        .unwrap()
        .is_none()
    {
        execute(
            &store,
            state.settings().apply(
                state.settings().revision(&store).unwrap(),
                ApplySettings::new(vec![theme("different-domain")]).unwrap(),
            ),
        )
        .unwrap();
    }
    if state
        .runtime_roots()
        .runtime(&store, RuntimeId::from_bytes([1; 16]))
        .unwrap()
        .is_none()
    {
        execute(
            &store,
            state.runtime_roots().create_runtime_with_home_root(
                state.runtime_roots().revision(&store).unwrap(),
                host_runtime(1, 2, r"C:\Codex\codex.exe", r"C:\Work\ten"),
            ),
        )
        .unwrap();
    }

    store.close().unwrap();
    let (reopened, state) = support::open(directory.path());
    assert_eq!(
        state
            .settings()
            .setting(&reopened, SettingKey::ActiveThemeId)
            .unwrap()
            .unwrap()
            .value()
            .as_active_theme_id(),
        Some("different-domain")
    );
    assert!(
        state
            .runtime_roots()
            .runtime(&reopened, RuntimeId::from_bytes([1; 16]))
            .unwrap()
            .is_some()
    );
}

#[test]
fn runtime_executable_and_per_runtime_root_uniqueness_survive_concurrent_admission() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let expected_domain = state.runtime_roots().revision(&store).unwrap();
    let first = command(
        &store,
        state.runtime_roots().create_runtime_with_home_root(
            expected_domain,
            host_runtime(1, 11, r"C:\Canonical\codex.exe", r"C:\Users\first"),
        ),
    );
    let second = command(
        &store,
        state.runtime_roots().create_runtime_with_home_root(
            expected_domain,
            host_runtime(2, 12, r"C:\Canonical\codex.exe", r"C:\Users\second"),
        ),
    );
    let results = race_commands(&store, first, second);
    assert_one_success_one_conflict(&results);

    let first_exists = state
        .runtime_roots()
        .runtime(&store, RuntimeId::from_bytes([1; 16]))
        .unwrap()
        .is_some();
    let second_exists = state
        .runtime_roots()
        .runtime(&store, RuntimeId::from_bytes([2; 16]))
        .unwrap()
        .is_some();
    assert_ne!(first_exists, second_exists);
    let (winner, loser, loser_home_root) = if first_exists { (1, 2, 12) } else { (2, 1, 11) };
    let duplicate = execute(
        &store,
        state.runtime_roots().create_runtime_with_home_root(
            state.runtime_roots().revision(&store).unwrap(),
            host_runtime(
                loser,
                loser_home_root,
                r"C:\Canonical\codex.exe",
                r"C:\Users\retry",
            ),
        ),
    )
    .unwrap_err();
    assert!(matches!(
        contributor_source::<RuntimeRootMutationError>(&duplicate),
        Some(RuntimeRootMutationError::ExecutableExists { .. })
    ));

    let runtime_id = RuntimeId::from_bytes([winner; 16]);
    let shared_path = r"C:\Shared\concurrent";
    let expected_domain = state.runtime_roots().revision(&store).unwrap();
    let first = command(
        &store,
        state.runtime_roots().add_root(
            expected_domain,
            AddConfiguredRoot::new(runtime_id, root_registration(50, shared_path)),
        ),
    );
    let second = command(
        &store,
        state.runtime_roots().add_root(
            expected_domain,
            AddConfiguredRoot::new(runtime_id, root_registration(51, shared_path)),
        ),
    );
    let results = race_commands(&store, first, second);
    assert_one_success_one_conflict(&results);

    let canonical =
        RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, shared_path)
            .unwrap();
    let admitted_root = state
        .runtime_roots()
        .root_by_path(&store, runtime_id, &canonical)
        .unwrap()
        .unwrap();
    let loser_root = if admitted_root.root_id() == RootId::from_bytes([50; 16]) {
        51
    } else {
        50
    };
    let duplicate = execute(
        &store,
        state.runtime_roots().add_root(
            state.runtime_roots().revision(&store).unwrap(),
            AddConfiguredRoot::new(runtime_id, root_registration(loser_root, shared_path)),
        ),
    )
    .unwrap_err();
    assert!(matches!(
        contributor_source::<RuntimeRootMutationError>(&duplicate),
        Some(RuntimeRootMutationError::RootPathExists { .. })
    ));

    let limits = CursorReadLimits::new(8, 1024 * 1024).unwrap();
    assert_eq!(
        state
            .runtime_roots()
            .list_runtimes(&store, None, limits)
            .unwrap()
            .records()
            .len(),
        1
    );
    assert_eq!(
        state
            .runtime_roots()
            .list_roots(&store, runtime_id, None, limits)
            .unwrap()
            .records()
            .len(),
        2
    );

    store.close().unwrap();
    let (reopened, state) = support::open(directory.path());
    assert!(
        state
            .runtime_roots()
            .runtime(&reopened, runtime_id)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        state
            .runtime_roots()
            .root_by_path(&reopened, runtime_id, &canonical)
            .unwrap(),
        Some(admitted_root)
    );
    assert_eq!(
        state
            .runtime_roots()
            .list_roots(&reopened, runtime_id, None, limits)
            .unwrap()
            .records()
            .len(),
        2
    );
}

#[test]
fn session_claim_conflicts_and_close_vs_exit_publish_one_coherent_generation() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let session = state.session();
    let initial_window = window(1);
    execute(
        &store,
        session.initialize_threadless(
            session.revision(&store).unwrap(),
            InitializeThreadlessWindow::new(initial_window, placement(1)),
        ),
    )
    .unwrap();

    let threadless = session.minimal_bootstrap(&store).unwrap().unwrap();
    execute(
        &store,
        session.replace_claim(
            session.revision(&store).unwrap(),
            ReplaceWindowClaim::new(
                threadless.header().revision(),
                initial_window,
                threadless.windows()[0].revision(),
                None,
                target(1, 11),
                thread(80),
            ),
        ),
    )
    .unwrap();
    let initial = session.minimal_bootstrap(&store).unwrap().unwrap();
    let claimed_thread = thread(90);
    let expected_domain = session.revision(&store).unwrap();
    let first = command(
        &store,
        session.create_claimed_window(
            expected_domain,
            CreateClaimedWindow::new(
                initial.header().revision(),
                window(2),
                target(2, 12),
                claimed_thread,
                placement(2),
            ),
        ),
    );
    let second = command(
        &store,
        session.create_claimed_window(
            expected_domain,
            CreateClaimedWindow::new(
                initial.header().revision(),
                window(3),
                target(3, 13),
                claimed_thread,
                placement(3),
            ),
        ),
    );
    let results = race_commands(&store, first, second);
    assert_one_success_one_conflict(&results);

    let claimed = session.minimal_bootstrap(&store).unwrap().unwrap();
    assert_eq!(claimed.windows().len(), 2);
    let missing_window = if claimed
        .windows()
        .iter()
        .any(|record| record.window_id() == window(2))
    {
        window(3)
    } else {
        window(2)
    };
    let duplicate = execute(
        &store,
        session.create_claimed_window(
            session.revision(&store).unwrap(),
            CreateClaimedWindow::new(
                claimed.header().revision(),
                missing_window,
                target(4, 14),
                claimed_thread,
                placement(4),
            ),
        ),
    )
    .unwrap_err();
    assert!(matches!(
        contributor_source::<SessionMutationError>(&duplicate),
        Some(SessionMutationError::ThreadAlreadyClaimed { .. })
    ));

    let before_close = session.minimal_bootstrap(&store).unwrap().unwrap();
    let initial_record = before_close
        .windows()
        .iter()
        .find(|record| record.window_id() == initial_window)
        .unwrap();
    let expected_domain = session.revision(&store).unwrap();
    let close = command(
        &store,
        session.remove_window(
            expected_domain,
            RemoveSessionWindow::new(
                before_close.header().revision(),
                initial_window,
                initial_record.revision(),
                initial_record.selected_thread(),
            ),
        ),
    );
    let exit = command(
        &store,
        session.mark_orderly_exit(
            expected_domain,
            MarkOrderlyExit::new(before_close.header().revision()),
        ),
    );
    let results = race_commands(&store, close, exit);
    assert_one_success_one_conflict(&results);

    let final_session = session.minimal_bootstrap(&store).unwrap().unwrap();
    match final_session.header().exit_intent() {
        SessionExitIntent::Running => assert!(
            final_session
                .windows()
                .iter()
                .all(|record| record.window_id() != initial_window)
        ),
        SessionExitIntent::OrderlyExit => assert!(
            final_session
                .windows()
                .iter()
                .any(|record| record.window_id() == initial_window)
        ),
    }
    let expected_header = final_session.header().clone();
    let expected_windows = final_session.windows().to_vec();

    store.close().unwrap();
    let (reopened, state) = support::open(directory.path());
    let reopened_session = state
        .session()
        .minimal_bootstrap(&reopened)
        .unwrap()
        .unwrap();
    assert_eq!(reopened_session.header(), &expected_header);
    assert_eq!(reopened_session.windows(), expected_windows.as_slice());
}
