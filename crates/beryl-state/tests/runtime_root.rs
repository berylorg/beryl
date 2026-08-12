mod support;

use std::{sync::Arc, thread};

use beryl_home_store::{CommandError, CommandOutcome, CursorReadLimits, ReadError};
use beryl_model::{
    AdmittedHostPath, Availability, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
    UnavailableReason,
};
use beryl_state::{
    AddConfiguredRoot, AvailabilitySnapshot, CreateRuntimeWithHomeRoot, RootActivityUpdate,
    RootRegistration, RuntimeRegistration, RuntimeRootMutationError, SetRootAvailability,
    SetRuntimeAvailability, UnixMillis,
};
use tempfile::tempdir;

use support::{contributor_source, create_host_runtime, execute, host_runtime, open, wsl_runtime};

#[test]
fn runtime_and_non_removable_home_root_publish_atomically_and_reopen() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    let runtime_id = RuntimeId::from_bytes([1; 16]);
    let root_id = RootId::from_bytes([2; 16]);
    match execute(
        &store,
        state.runtime_roots().create_runtime_with_home_root(
            state.runtime_roots().revision(&store).unwrap(),
            host_runtime(1, 2, r"C:\Codex\codex.exe", r"C:\Users\operator"),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed runtime creation, got {outcome:?}"),
    }

    let runtime = state
        .runtime_roots()
        .runtime(&store, runtime_id)
        .unwrap()
        .unwrap();
    let root = state
        .runtime_roots()
        .root(&store, root_id)
        .unwrap()
        .unwrap();
    assert_eq!(runtime.runtime_id(), runtime_id);
    assert_eq!(runtime.environment_label(), "Host");
    assert_eq!(runtime.revision().get(), 1);
    assert!(root.non_removable());
    assert_eq!(root.runtime_id(), runtime_id);
    assert_eq!(root.revision().get(), 1);
    assert_eq!(
        state
            .runtime_roots()
            .runtime_by_executable(&store, runtime.canonical_executable())
            .unwrap()
            .unwrap(),
        runtime
    );
    assert_eq!(
        state
            .runtime_roots()
            .root_by_path(&store, runtime_id, root.canonical_path())
            .unwrap()
            .unwrap(),
        root
    );
    store.close().unwrap();

    let (reopened, state) = open(directory.path());
    assert!(
        state
            .runtime_roots()
            .runtime(&reopened, runtime_id)
            .unwrap()
            .is_some()
    );
    assert!(
        state
            .runtime_roots()
            .root(&reopened, root_id)
            .unwrap()
            .unwrap()
            .non_removable()
    );
}

#[test]
fn concurrent_duplicate_executable_commands_publish_only_one_runtime() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    let store = Arc::new(store);
    let expected_home = store.home_revision().unwrap();
    let expected_domain = state.runtime_roots().revision(&store).unwrap();
    let mut commands = Vec::new();
    for (runtime_byte, root_byte) in [(1, 2), (3, 4)] {
        let mut command = beryl_home_store::HomeCommand::new(expected_home);
        command
            .add(state.runtime_roots().create_runtime_with_home_root(
                expected_domain,
                host_runtime(
                    runtime_byte,
                    root_byte,
                    r"C:\Canonical\codex.exe",
                    &format!(r"C:\Users\operator{root_byte}"),
                ),
            ))
            .unwrap();
        commands.push(command);
    }

    let results: Vec<_> = commands
        .into_iter()
        .map(|command| {
            let store = Arc::clone(&store);
            thread::spawn(move || store.execute(command))
        })
        .map(|worker| worker.join().unwrap())
        .collect();
    let mut committed = 0;
    let mut conflicts = 0;
    for outcome in results {
        match outcome {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => committed += 1,
            CommandOutcome::NotCommitted {
                evidence: CommandError::Conflict { .. },
            } => conflicts += 1,
            outcome => panic!("unexpected concurrent runtime command outcome: {outcome:?}"),
        }
    }
    assert_eq!(committed, 1);
    assert_eq!(conflicts, 1);
    let page = state
        .runtime_roots()
        .list_runtimes(&store, None, CursorReadLimits::new(8, 1_000_000).unwrap())
        .unwrap();
    assert_eq!(page.records().len(), 1);
}

#[test]
fn executable_and_root_uniqueness_have_their_exact_scopes() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    create_host_runtime(&store, &state, 1, 2, r"C:\One\codex.exe", r"C:\Shared");
    create_host_runtime(&store, &state, 3, 4, r"C:\Two\codex.exe", r"C:\Shared");
    assert_eq!(
        state
            .runtime_roots()
            .list_runtimes(&store, None, CursorReadLimits::new(8, 1_000_000).unwrap())
            .unwrap()
            .records()
            .len(),
        2
    );

    let duplicate_executable = match execute(
        &store,
        state.runtime_roots().create_runtime_with_home_root(
            state.runtime_roots().revision(&store).unwrap(),
            host_runtime(6, 7, r"C:\One\codex.exe", r"C:\Elsewhere"),
        ),
    ) {
        CommandOutcome::NotCommitted { evidence } => evidence,
        outcome => panic!("expected rejected duplicate executable command, got {outcome:?}"),
    };
    assert!(matches!(
        contributor_source::<RuntimeRootMutationError>(&duplicate_executable),
        Some(RuntimeRootMutationError::ExecutableExists { .. })
    ));

    let duplicate = RootRegistration::new(
        RootId::from_bytes([5; 16]),
        RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, r"C:\Shared")
            .unwrap(),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Shared").unwrap(),
        UnixMillis::new(30),
        AvailabilitySnapshot::unknown(),
    );
    let error = match execute(
        &store,
        state.runtime_roots().add_root(
            state.runtime_roots().revision(&store).unwrap(),
            AddConfiguredRoot::new(RuntimeId::from_bytes([1; 16]), duplicate),
        ),
    ) {
        CommandOutcome::NotCommitted { evidence } => evidence,
        outcome => panic!("expected rejected duplicate root command, got {outcome:?}"),
    };
    assert!(matches!(
        contributor_source::<RuntimeRootMutationError>(&error),
        Some(RuntimeRootMutationError::RootPathExists { .. })
    ));
}

#[test]
fn root_activity_strictly_advances_under_record_revision_control() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    create_host_runtime(
        &store,
        &state,
        1,
        2,
        r"C:\Codex\codex.exe",
        r"C:\Users\operator",
    );
    let root_id = RootId::from_bytes([2; 16]);
    let initial = state
        .runtime_roots()
        .root(&store, root_id)
        .unwrap()
        .unwrap();
    match execute(
        &store,
        state.runtime_roots().update_root_activity(
            state.runtime_roots().revision(&store).unwrap(),
            RootActivityUpdate::new(root_id, initial.revision(), UnixMillis::new(50)),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed root activity update, got {outcome:?}"),
    }
    let current = state
        .runtime_roots()
        .root(&store, root_id)
        .unwrap()
        .unwrap();
    assert_eq!(current.last_activity_at(), Some(UnixMillis::new(50)));

    let stale_revision = match execute(
        &store,
        state.runtime_roots().update_root_activity(
            state.runtime_roots().revision(&store).unwrap(),
            RootActivityUpdate::new(root_id, initial.revision(), UnixMillis::new(51)),
        ),
    ) {
        CommandOutcome::NotCommitted { evidence } => evidence,
        outcome => panic!("expected rejected stale root activity command, got {outcome:?}"),
    };
    assert!(matches!(
        contributor_source::<RuntimeRootMutationError>(&stale_revision),
        Some(RuntimeRootMutationError::RecordRevisionConflict { .. })
    ));

    let regressed = match execute(
        &store,
        state.runtime_roots().update_root_activity(
            state.runtime_roots().revision(&store).unwrap(),
            RootActivityUpdate::new(root_id, current.revision(), UnixMillis::new(50)),
        ),
    ) {
        CommandOutcome::NotCommitted { evidence } => evidence,
        outcome => panic!("expected rejected regressed root activity command, got {outcome:?}"),
    };
    assert!(matches!(
        contributor_source::<RuntimeRootMutationError>(&regressed),
        Some(RuntimeRootMutationError::RootActivityNotLater)
    ));
}

#[test]
fn host_and_wsl_paths_cannot_cross_runtime_boundaries() {
    let host_mode = RuntimeMode::host();
    let runtime = RuntimeRegistration::new(
        RuntimeId::from_bytes([1; 16]),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Codex\codex.exe").unwrap(),
        host_mode.clone(),
        RuntimeNativePath::from_admitted(host_mode, PathFlavor::Windows, r"C:\Codex\codex.exe")
            .unwrap(),
        UnixMillis::new(1),
        AvailabilitySnapshot::unknown(),
    )
    .unwrap();
    let wsl_mode = RuntimeMode::wsl("Ubuntu-24.04").unwrap();
    let root = RootRegistration::new(
        RootId::from_bytes([2; 16]),
        RuntimeNativePath::from_admitted(wsl_mode, PathFlavor::Posix, "/home/operator").unwrap(),
        AdmittedHostPath::from_admitted(
            PathFlavor::Windows,
            r"\\wsl.localhost\Ubuntu-24.04\home\operator",
        )
        .unwrap(),
        UnixMillis::new(1),
        AvailabilitySnapshot::unknown(),
    );
    assert!(matches!(
        CreateRuntimeWithHomeRoot::new(runtime, root),
        Err(RuntimeRootMutationError::RuntimeModeMismatch)
    ));

    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    match execute(
        &store,
        state.runtime_roots().create_runtime_with_home_root(
            state.runtime_roots().revision(&store).unwrap(),
            wsl_runtime(
                3,
                4,
                "Ubuntu-24.04",
                r"\\wsl.localhost\Ubuntu-24.04\usr\bin\codex",
                "/usr/bin/codex",
                r"\\wsl.localhost\Ubuntu-24.04\home\operator",
                "/home/operator",
            ),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed WSL runtime creation, got {outcome:?}"),
    }
    let runtime = state
        .runtime_roots()
        .runtime(&store, RuntimeId::from_bytes([3; 16]))
        .unwrap()
        .unwrap();
    assert_eq!(runtime.environment_label(), "Ubuntu-24.04");
}

#[test]
fn availability_updates_retain_registry_records_and_bindings() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    create_host_runtime(
        &store,
        &state,
        1,
        2,
        r"C:\Codex\codex.exe",
        r"C:\Users\operator",
    );
    let runtime_id = RuntimeId::from_bytes([1; 16]);
    let root_id = RootId::from_bytes([2; 16]);
    let runtime = state
        .runtime_roots()
        .runtime(&store, runtime_id)
        .unwrap()
        .unwrap();
    match execute(
        &store,
        state.runtime_roots().set_runtime_availability(
            state.runtime_roots().revision(&store).unwrap(),
            SetRuntimeAvailability::new(
                runtime_id,
                runtime.revision(),
                AvailabilitySnapshot::observed(
                    Availability::Unavailable(UnavailableReason::BackendUnavailable),
                    UnixMillis::new(50),
                )
                .unwrap(),
            ),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed runtime availability update, got {outcome:?}"),
    }
    let root = state
        .runtime_roots()
        .root(&store, root_id)
        .unwrap()
        .unwrap();
    match execute(
        &store,
        state.runtime_roots().set_root_availability(
            state.runtime_roots().revision(&store).unwrap(),
            SetRootAvailability::new(
                root_id,
                root.revision(),
                AvailabilitySnapshot::observed(
                    Availability::Unavailable(UnavailableReason::NotFound),
                    UnixMillis::new(51),
                )
                .unwrap(),
            ),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed root availability update, got {outcome:?}"),
    }

    assert!(
        state
            .runtime_roots()
            .runtime(&store, runtime_id)
            .unwrap()
            .is_some()
    );
    assert!(
        state
            .runtime_roots()
            .root(&store, root_id)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        state
            .runtime_roots()
            .runtime(&store, runtime_id)
            .unwrap()
            .unwrap()
            .availability()
            .availability(),
        Availability::Unavailable(UnavailableReason::BackendUnavailable)
    );
}

#[test]
fn registry_lists_obey_explicit_item_and_byte_bounds() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    for byte in 1..=3 {
        create_host_runtime(
            &store,
            &state,
            byte,
            byte + 10,
            &format!(r"C:\Codex{byte}\codex.exe"),
            &format!(r"C:\Users\operator{byte}"),
        );
    }
    let page = state
        .runtime_roots()
        .list_runtimes(&store, None, CursorReadLimits::new(1, 1_000_000).unwrap())
        .unwrap();
    assert_eq!(page.records().len(), 1);
    assert!(page.has_more());
    assert!(page.stored_bytes() > 0);
    assert!(page.decoded_bytes() > 0);

    let error = state
        .runtime_roots()
        .list_runtimes(&store, None, CursorReadLimits::new(8, 1).unwrap())
        .unwrap_err();
    assert!(matches!(error, ReadError::BoundExceeded { .. }));
}
