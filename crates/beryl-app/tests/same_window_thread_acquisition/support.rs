use beryl_app::{
    RunningThreadActivation, RunningThreadActivationError, RunningThreadActivationOutcome,
    RunningThreadActivationPreparation,
};
use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions,
    HomeSchemaVersion, HomeStore, MutationContribution,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    AdmittedHostPath, Availability, ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicDraftId, SyndicThreadId, WindowBounds, WindowDisplayState, WindowId,
    WindowPlacement,
};
use beryl_state::{
    AvailabilitySnapshot, BerylState, CatalogClaimKind, CatalogClaimSummary, CatalogPointReadLimit,
    CreateRuntimeWithHomeRoot, InitializeThreadlessWindow, MarkCatalogRowStale, RememberedTarget,
    RootRegistration, RuntimeRegistration, UnixMillis, UpdateWindowPlacement, WindowClaimSelection,
};
use syndic_storage::{CreateThread, DraftEditHistoryPolicyV1, SyndicStorage, SyndicTimestamp};

fn thread(seed: u8) -> SyndicThreadId {
    SyndicThreadId::from_bytes([seed; 16])
}
fn native(path: &str) -> RuntimeNativePath {
    RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, path).unwrap()
}
fn host(path: &str) -> AdmittedHostPath {
    AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap()
}
fn placement() -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(0, 0, 900, 700).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
}
fn execute(store: &HomeStore, contribution: MutationContribution) {
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

struct Fixture {
    store: HomeStore,
    state: BerylState,
    syndic: SyndicStorage,
    faults: FaultController,
    window: WindowId,
    target: RememberedTarget,
    _directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        println!(
            "owned running-thread activation fixture: {}",
            directory.path().display()
        );
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let syndic = SyndicStorage::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(
                BerylState::required_domains()
                    .unwrap()
                    .merge(SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        let target =
            RememberedTarget::new(RuntimeId::from_bytes([1; 16]), RootId::from_bytes([2; 16]));
        execute(
            &store,
            state.runtime_roots().create_runtime_with_home_root(
                state.runtime_roots().revision(&store).unwrap(),
                CreateRuntimeWithHomeRoot::new(
                    RuntimeRegistration::new(
                        target.runtime_id(),
                        host(r"C:\Codex\codex.exe"),
                        RuntimeMode::host(),
                        beryl_model::RuntimeLaunchForm::CodexCli,
                        native(r"C:\Codex\codex.exe"),
                        UnixMillis::new(1),
                        AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(2))
                            .unwrap(),
                    )
                    .unwrap(),
                    RootRegistration::new(
                        target.root_id(),
                        native(r"C:\Work\Beryl"),
                        host(r"C:\Work\Beryl"),
                        UnixMillis::new(1),
                        AvailabilitySnapshot::unknown(),
                    ),
                )
                .unwrap(),
            ),
        );
        for seed in [3, 4] {
            execute(
                &store,
                syndic.create_thread(
                    syndic.revision(&store).unwrap(),
                    CreateThread::ordinary(
                        thread(seed),
                        SyndicDraftId::from_bytes([seed; 16]),
                        ExecutionBinding::new(
                            target.runtime_id(),
                            target.root_id(),
                            native(r"C:\Work\Beryl"),
                        ),
                        SyndicTimestamp::from_unix_millis(u64::from(seed)),
                        DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                    ),
                ),
            );
        }
        let window = WindowId::from_bytes([5; 16]);
        execute(
            &store,
            state.session().initialize_threadless(
                state.session().revision(&store).unwrap(),
                InitializeThreadlessWindow::new(window, placement()),
            ),
        );
        Self {
            store,
            state,
            syndic,
            faults,
            window,
            target,
            _directory: directory,
        }
    }

    fn prepare(&self, selected: Option<WindowClaimSelection>, seed: u8) -> RunningThreadActivation {
        let preparation = RunningThreadActivation::prepare(
            &self.store,
            &self.state,
            &self.syndic,
            self.window,
            selected,
            self.target,
            thread(seed),
        )
        .unwrap();
        let RunningThreadActivationPreparation::Prepared(prepared) = preparation else {
            panic!("expected prepared, got {preparation:?}")
        };
        prepared
    }

    fn commit(
        &self,
        prepared: RunningThreadActivation,
    ) -> beryl_app::RunningThreadActivationCommit {
        let outcome = prepared.commit(
            &self.store,
            &self.state,
            &self.syndic,
            CommandCancellation::new(),
        );
        let RunningThreadActivationOutcome::Settled(settled) = outcome else {
            panic!("expected settled, got {outcome:?}")
        };
        settled
    }
}
