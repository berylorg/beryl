use super::*;
use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, test_faults::FaultController,
};

pub fn snapshot(fixture: &Fixture) -> beryl_state::MinimalSessionBootstrap {
    fixture
        .state
        .session()
        .minimal_bootstrap(&fixture.store)
        .unwrap()
        .unwrap()
}

pub fn attempt(
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

pub fn activation_source() -> RestoredWindowActivationSource {
    Arc::new(|window| {
        let seed = window.window_id().as_bytes()[0];
        Ok((
            composer_support::activation(
                window.selected_thread().unwrap().thread_id(),
                seed,
                seed.wrapping_add(1),
                1,
                0,
            ),
            composer_support::fixture::operation_id(seed.wrapping_add(2)),
        ))
    })
}

pub fn work(fixture: &Fixture) -> (MainWindowRestoreSet, RestoredWindowServiceTestLifetime) {
    let (services, appearance) = creation_support::services(fixture);
    let (attempt, lifetime) = attempt(fixture);
    (
        MainWindowRestoreSet::new(
            services,
            attempt,
            activation_source(),
            appearance,
            WindowId::from_bytes([240; 16]),
            placement(),
        )
        .unwrap(),
        lifetime,
    )
}

pub fn prepare(mut work: MainWindowRestoreSet) -> PreparedMainWindowRestoreSet {
    for _ in 0..1024 {
        match work.advance() {
            MainWindowRestoreSetOutcome::Pending(pending) => work = pending,
            MainWindowRestoreSetOutcome::Retained { reason, .. } => {
                panic!("unexpected retained command: {reason:?}")
            }
            MainWindowRestoreSetOutcome::Prepared(prepared) => return prepared,
            MainWindowRestoreSetOutcome::Failed { error } => panic!("restore failed: {error}"),
        }
    }
    panic!(
        "restore did not prepare: {:?}, {:?}",
        work.last_error(),
        work.cleanup_error()
    );
}

pub fn dispose(mut work: MainWindowRestoreSet) -> String {
    for _ in 0..1024 {
        match work.advance() {
            MainWindowRestoreSetOutcome::Pending(pending) => work = pending,
            MainWindowRestoreSetOutcome::Retained { reason, .. } => {
                panic!("unexpected retained command: {reason:?}")
            }
            MainWindowRestoreSetOutcome::Prepared(_) => {
                panic!("failed attempt exposed a prepared subset")
            }
            MainWindowRestoreSetOutcome::Failed { error } => return error,
        }
    }
    panic!(
        "restore did not dispose: {:?}, {:?}",
        work.last_error(),
        work.cleanup_error()
    );
}

pub fn change_placement(fixture: &Fixture, id: WindowId) {
    let before = snapshot(fixture);
    let window = before
        .windows()
        .iter()
        .find(|w| w.window_id() == id)
        .unwrap();
    execute(
        &fixture.store,
        fixture.state.session().update_placement(
            fixture.state.session().revision(&fixture.store).unwrap(),
            beryl_state::UpdateWindowPlacement::new(
                before.header().revision(),
                id,
                window.revision(),
                WindowPlacement::new(
                    WindowBounds::new(40, 50, 800, 600).unwrap(),
                    WindowDisplayState::Normal,
                    None,
                    None,
                ),
            ),
        ),
    );
}

pub fn zero_runtime(initial: Option<bool>) -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = Arc::new(
        candidate
            .prepare_publication(
                BerylState::required_domains()
                    .unwrap()
                    .merge(SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap(),
    );
    let process = RuntimeBackedWindowProcessRegistry::new(Default::default());
    let service = RuntimeBackedWindowAcquisitionService::new(
        &process,
        Arc::new(store.service_reference()),
        state.clone(),
        storage.clone(),
    );
    let fixture = Fixture {
        directory,
        store,
        state,
        storage,
        faults,
        process,
        service,
        seed: 51,
    };
    if let Some(retain) = initial {
        let window = WindowId::from_bytes([52; 16]);
        execute(
            &fixture.store,
            fixture.state.session().initialize_threadless(
                fixture.state.session().revision(&fixture.store).unwrap(),
                beryl_state::InitializeThreadlessWindow::new(window, placement()),
            ),
        );
        if !retain {
            fixture.remove_session_window(window);
        }
    }
    fixture
}

pub fn cleanup(fixture: Fixture) {
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
    drop((service, process, faults, storage, state, store));
    directory.close().unwrap();
}
