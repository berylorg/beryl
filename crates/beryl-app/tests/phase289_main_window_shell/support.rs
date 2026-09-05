use super::*;
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::FaultController,
};
use beryl_model::{AdmittedHostPath, Availability, PathFlavor, WindowId};
use beryl_state::{
    AvailabilitySnapshot, CreateRuntimeWithHomeRoot, InitializeThreadlessWindow,
    RemoveSessionWindow, RootRegistration, RuntimeRegistration, UnixMillis,
};

pub fn open_home(
    seed: u8,
) -> (
    tempfile::TempDir,
    Arc<HomeStore>,
    BerylState,
    syndic_storage::SyndicStorage,
    FaultController,
) {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let state = BerylState::register(&mut store).unwrap();
    let storage = syndic_storage::SyndicStorage::register(&mut store).unwrap();
    let host_path = |path| AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap();
    let runtime = RuntimeRegistration::new(
        RuntimeId::from_bytes([seed; 16]),
        host_path(r"C:\Program Files\Codex\codex.exe"),
        RuntimeMode::host(),
        native_path(RuntimeMode::host(), r"C:\Program Files\Codex\codex.exe"),
        UnixMillis::new(1),
        AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(2)).unwrap(),
    )
    .unwrap();
    let root = RootRegistration::new(
        RootId::from_bytes([seed.wrapping_add(1); 16]),
        native_path(RuntimeMode::host(), r"C:\Work\Beryl"),
        host_path(r"C:\Work\Beryl"),
        UnixMillis::new(1),
        AvailabilitySnapshot::unknown(),
    );
    execute(
        &store,
        state.runtime_roots().create_runtime_with_home_root(
            state.runtime_roots().revision(&store).unwrap(),
            CreateRuntimeWithHomeRoot::new(runtime, root).unwrap(),
        ),
    );
    let session = state.session();
    let initial = WindowId::from_bytes([250; 16]);
    execute(
        &store,
        session.initialize_threadless(
            session.revision(&store).unwrap(),
            InitializeThreadlessWindow::new(initial, placement()),
        ),
    );
    let bootstrap = session.minimal_bootstrap(&store).unwrap().unwrap();
    let window = &bootstrap.windows()[0];
    execute(
        &store,
        session.remove_window(
            session.revision(&store).unwrap(),
            RemoveSessionWindow::new(
                bootstrap.header().revision(),
                initial,
                window.revision(),
                window.selected_thread(),
            ),
        ),
    );
    (directory, Arc::new(store), state, storage, faults)
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

pub fn worker<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> std::thread::JoinHandle<T> {
    std::thread::Builder::new()
        .name("phase289-shell-worker".to_owned())
        .stack_size(16 * 1024 * 1024)
        .spawn(work)
        .unwrap()
}

pub fn join<T: Send + 'static>(
    worker: std::thread::JoinHandle<T>,
    cx: &mut gpui::TestAppContext,
) -> T {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !worker.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "shell worker did not settle"
        );
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    worker.join().unwrap()
}
