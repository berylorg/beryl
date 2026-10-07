use super::*;
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions, HomeRecoveryCandidate,
    HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    AdmittedHostPath, DomainRevision, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicThreadId, WindowBounds, WindowDisplayState, WindowPlacement,
};
use beryl_state::{
    AvailabilitySnapshot, CreateRuntimeWithHomeRoot, InitializeThreadlessWindow, RememberedTarget,
    ReplaceWindowClaim, RootRegistration, RuntimeRegistration, UnixMillis, UpdateWindowPlacement,
};

struct Fixture {
    candidate: HomeRecoveryCandidate,
    state: BerylState,
    old_state: BerylState,
    faults: FaultController,
    old_home: BerylHomeId,
    old_generation: HomeGeneration,
    window: SessionWindowRecord,
    directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::Builder::new()
            .prefix("threadless-recovery-")
            .tempdir()
            .unwrap();
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let old_state = BerylState::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(BerylState::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        let session = old_state.session();
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(session.initialize_threadless(
                session.revision(&store).unwrap(),
                InitializeThreadlessWindow::new(WindowId::from_bytes([1; 16]), placement(0)),
            ))
            .unwrap();
        assert_committed(store.execute(command));
        let window = session
            .minimal_bootstrap(&store)
            .unwrap()
            .unwrap()
            .windows()[0]
            .clone();
        let old_home = store.home_id();
        let old_generation = store.health().generation().unwrap();
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let candidate = store.recover_same_home().unwrap();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        Self {
            candidate,
            state,
            old_state,
            faults,
            old_home,
            old_generation,
            window,
            directory,
        }
    }

    fn prepare(&mut self) -> Result<ThreadlessRecoveryWindow, String> {
        ThreadlessRecoveryWindow::prepare(
            &self.candidate.recovery_access().unwrap(),
            &self.state,
            self.old_home,
            self.old_generation,
            self.window.window_id(),
        )
    }

    fn finish(self) {
        let Self {
            candidate,
            state,
            old_state,
            directory,
            ..
        } = self;
        drop((state, old_state));
        drop(candidate.abort());
        directory.close().unwrap();
    }
}

fn placement(x: i32) -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(x, 0, 800, 600).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
}

fn assert_committed(outcome: CommandOutcome) {
    assert!(matches!(
        outcome,
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

#[test]
fn threadless_recovery_authenticates_fixed_facts_without_writes_or_home_custody() {
    let mut fixture = Fixture::new();
    let before = fixture
        .candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    let source = fixture.prepare().unwrap();
    assert_eq!(source.home_id(), fixture.old_home);
    assert_eq!(source.generation(), fixture.candidate.generation());
    assert_eq!(source.window(), &fixture.window);
    let access = fixture.candidate.recovery_access().unwrap();
    source.revalidate(&access, &fixture.state).unwrap();
    assert_eq!(access.home_revision().unwrap(), before);
    assert!(
        fixture
            .state
            .session()
            .minimal_bootstrap(&fixture.candidate.service_reference())
            .is_err()
    );
    fixture.finish();
    assert!(source.window().selected_thread().is_none());
}

#[test]
fn threadless_recovery_rejects_wrong_identity_and_stale_or_foreign_handles() {
    let mut fixture = Fixture::new();
    let mut foreign = Fixture::new();
    let source = fixture.prepare().unwrap();
    let access = fixture.candidate.recovery_access().unwrap();
    for (home, generation, window) in [
        (
            foreign.old_home,
            fixture.old_generation,
            fixture.window.window_id(),
        ),
        (
            fixture.old_home,
            access.generation(),
            fixture.window.window_id(),
        ),
        (
            fixture.old_home,
            fixture.old_generation,
            WindowId::from_bytes([2; 16]),
        ),
    ] {
        assert!(
            ThreadlessRecoveryWindow::prepare(&access, &fixture.state, home, generation, window)
                .is_err()
        );
    }
    for state in [&fixture.old_state, &foreign.state] {
        assert!(
            ThreadlessRecoveryWindow::prepare(
                &access,
                state,
                fixture.old_home,
                fixture.old_generation,
                fixture.window.window_id()
            )
            .is_err()
        );
        assert!(
            state
                .runtime_roots()
                .has_runtimes_candidate(&access)
                .is_err()
        );
    }
    assert!(
        source
            .revalidate(
                &foreign.candidate.recovery_access().unwrap(),
                &foreign.state
            )
            .is_err()
    );
    fixture.finish();
    foreign.finish();
}

#[test]
fn threadless_recovery_revalidation_rejects_changed_window_facts() {
    let mut fixture = Fixture::new();
    let source = fixture.prepare().unwrap();
    let access = fixture.candidate.recovery_access().unwrap();
    let session = fixture.state.session();
    let snapshot = session
        .minimal_bootstrap_candidate(&access)
        .unwrap()
        .unwrap();
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command
        .add(session.update_placement(
            session.revision_candidate(&access).unwrap(),
            UpdateWindowPlacement::new(
                snapshot.header().revision(),
                fixture.window.window_id(),
                fixture.window.revision(),
                placement(100),
            ),
        ))
        .unwrap();
    assert_committed(access.execute(command));
    assert!(
        source
            .revalidate(&access, &fixture.state)
            .unwrap_err()
            .contains("facts changed")
    );
    let fresh = fixture.prepare().unwrap();
    assert_eq!(fresh.window().placement(), &placement(100));
    fixture.finish();
}

#[test]
fn threadless_recovery_rejects_a_selected_window() {
    let mut fixture = Fixture::new();
    let source = fixture.prepare().unwrap();
    let access = fixture.candidate.recovery_access().unwrap();
    let session = fixture.state.session();
    let snapshot = session
        .minimal_bootstrap_candidate(&access)
        .unwrap()
        .unwrap();
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command
        .add(session.replace_claim(
            session.revision_candidate(&access).unwrap(),
            ReplaceWindowClaim::new(
                snapshot.header().revision(),
                fixture.window.window_id(),
                fixture.window.revision(),
                None,
                RememberedTarget::new(RuntimeId::from_bytes([3; 16]), RootId::from_bytes([4; 16])),
                SyndicThreadId::from_bytes([5; 16]),
            ),
        ))
        .unwrap();
    assert_committed(access.execute(command));
    assert!(source.revalidate(&access, &fixture.state).is_err());
    assert!(fixture.prepare().is_err());
    fixture.finish();
}

#[test]
fn threadless_recovery_rejects_configured_runtime_with_one_bounded_presence_read() {
    let mut fixture = Fixture::new();
    let source = fixture.prepare().unwrap();
    let host_path = |path| AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap();
    let native_path = |path| {
        RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, path).unwrap()
    };
    let runtime = RuntimeRegistration::new(
        RuntimeId::from_bytes([u8::MAX; 16]),
        host_path(r"C:\Codex\codex.exe"),
        RuntimeMode::host(),
        beryl_model::RuntimeLaunchForm::CodexCli,
        native_path(r"C:\Codex\codex.exe"),
        UnixMillis::new(1),
        AvailabilitySnapshot::unknown(),
    )
    .unwrap();
    let root = RootRegistration::new(
        RootId::from_bytes([4; 16]),
        native_path(r"C:\Work"),
        host_path(r"C:\Work"),
        UnixMillis::new(1),
        AvailabilitySnapshot::unknown(),
    );
    let access = fixture.candidate.recovery_access().unwrap();
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command
        .add(fixture.state.runtime_roots().create_runtime_with_home_root(
            DomainRevision::new(1).unwrap(),
            CreateRuntimeWithHomeRoot::new(runtime, root).unwrap(),
        ))
        .unwrap();
    assert_committed(access.execute(command));
    assert!(
        fixture
            .state
            .runtime_roots()
            .has_runtimes_candidate(&access)
            .unwrap()
    );
    assert!(source.revalidate(&access, &fixture.state).is_err());
    assert!(
        fixture
            .prepare()
            .err()
            .unwrap()
            .contains("empty runtime registry")
    );
    fixture.finish();
}

#[test]
fn threadless_recovery_preserves_candidate_read_failure() {
    let mut fixture = Fixture::new();
    let source = fixture.prepare().unwrap();
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    let access = fixture.candidate.recovery_access().unwrap();
    assert!(source.revalidate(&access, &fixture.state).is_err());
    assert!(fixture.candidate.recovery_access().is_err());
    fixture.finish();
}
