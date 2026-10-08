use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    AdmittedHostPath, Availability, ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicDraftId, SyndicThreadId, WindowBounds, WindowDisplayState, WindowId,
    WindowPlacement,
};
use beryl_state::{
    AvailabilitySnapshot, BerylState, CreateRuntimeWithHomeRoot, InitializeThreadlessWindow,
    RememberedTarget, ReplaceWindowClaim, RootRegistration, RuntimeRegistration, UnixMillis,
};
use syndic_storage::{CreateThread, DraftEditHistoryPolicyV1, SyndicStorage, SyndicTimestamp};

pub(super) struct Fixture {
    pub(super) _directory: tempfile::TempDir,
    pub(super) store: HomeStore,
    pub(super) state: BerylState,
    pub(super) syndic: SyndicStorage,
    pub(super) thread_id: SyndicThreadId,
    pub(super) runtime_id: RuntimeId,
    pub(super) root_id: RootId,
}

impl Fixture {
    pub(super) fn new(binding_path: &str) -> Self {
        let directory = tempfile::tempdir().expect("temp home");
        let store = beryl_home_store::HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .expect("open home");
        Self::with_candidate(directory, store, binding_path)
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn new_with_faults(
        binding_path: &str,
        faults: beryl_home_store::test_faults::FaultController,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = beryl_home_store::HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults,
        )
        .unwrap();
        Self::with_candidate(directory, store, binding_path)
    }

    fn with_candidate(
        directory: tempfile::TempDir,
        mut store: beryl_home_store::HomeOpenCandidate,
        binding_path: &str,
    ) -> Self {
        let state = BerylState::register(&mut store).expect("register Beryl state");
        let syndic = SyndicStorage::register(&mut store).expect("register Syndic");
        let store = store
            .prepare_publication(
                BerylState::required_domains()
                    .expect("Beryl requirements")
                    .merge(SyndicStorage::required_domains().expect("Syndic requirements"))
                    .expect("merge requirements"),
            )
            .expect("prepare home publication")
            .publish()
            .expect("publish home");
        let runtime_id = RuntimeId::from_bytes([1; 16]);
        let root_id = RootId::from_bytes([2; 16]);
        let thread_id = SyndicThreadId::from_bytes([3; 16]);
        let mode = RuntimeMode::host();
        let canonical_root = native_path(mode.clone(), r"C:\Work\Beryl");
        let runtime = RuntimeRegistration::new(
            runtime_id,
            host_path(r"C:\Program Files\Codex\codex.exe"),
            mode.clone(),
            beryl_model::RuntimeLaunchForm::CodexCli,
            native_path(mode.clone(), r"C:\Program Files\Codex\codex.exe"),
            UnixMillis::new(1),
            AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(2))
                .expect("runtime availability"),
        )
        .expect("runtime registration");
        let root = RootRegistration::new(
            root_id,
            canonical_root,
            host_path(r"C:\Work\Beryl"),
            UnixMillis::new(1),
            AvailabilitySnapshot::unknown(),
        );
        let create_runtime =
            CreateRuntimeWithHomeRoot::new(runtime, root).expect("runtime and root agree");
        let create_thread = CreateThread::ordinary(
            thread_id,
            SyndicDraftId::from_bytes([4; 16]),
            ExecutionBinding::new(runtime_id, root_id, native_path(mode, binding_path)),
            SyndicTimestamp::from_unix_millis(3),
            DraftEditHistoryPolicyV1::new(65_536, 1).expect("history policy"),
        );
        let mut command = HomeCommand::new(store.home_revision().expect("home revision"));
        command
            .add(
                state.runtime_roots().create_runtime_with_home_root(
                    state
                        .runtime_roots()
                        .revision(&store)
                        .expect("runtime/root revision"),
                    create_runtime,
                ),
            )
            .expect("add runtime/root creation");
        command
            .add(syndic.create_thread(
                syndic.revision(&store).expect("Syndic revision"),
                create_thread,
            ))
            .expect("add thread creation");
        match store.execute(command) {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            CommandOutcome::NotCommitted { evidence } => {
                panic!("create sources unexpectedly not committed: {evidence:?}")
            }
            outcome @ CommandOutcome::Committed {
                later_failure: Some(_),
                ..
            } => panic!("create sources committed with later failure: {outcome:?}"),
            outcome @ CommandOutcome::Indeterminate { .. } => {
                panic!("create sources indeterminate: {outcome:?}")
            }
        }
        Self {
            _directory: directory,
            store,
            state,
            syndic,
            thread_id,
            runtime_id,
            root_id,
        }
    }

    pub(super) fn claim_thread(&self) -> WindowId {
        let session = self.state.session();
        let window_id = WindowId::from_bytes([5; 16]);
        let placement = WindowPlacement::new(
            WindowBounds::new(0, 0, 900, 700).expect("window bounds"),
            WindowDisplayState::Normal,
            None,
            None,
        );
        execute_contribution(
            &self.store,
            session.initialize_threadless(
                session.revision(&self.store).expect("session revision"),
                InitializeThreadlessWindow::new(window_id, placement),
            ),
        );
        let initial = session
            .minimal_bootstrap(&self.store)
            .expect("read session")
            .expect("initialized session");
        execute_contribution(
            &self.store,
            session.replace_claim(
                session.revision(&self.store).expect("session revision"),
                ReplaceWindowClaim::new(
                    initial.header().revision(),
                    window_id,
                    initial.windows()[0].revision(),
                    None,
                    RememberedTarget::new(self.runtime_id, self.root_id),
                    self.thread_id,
                ),
            ),
        );
        window_id
    }
}

pub(super) fn host_path(value: &str) -> AdmittedHostPath {
    AdmittedHostPath::from_admitted(PathFlavor::Windows, value).expect("admitted host path")
}

pub(super) fn native_path(mode: RuntimeMode, value: &str) -> RuntimeNativePath {
    RuntimeNativePath::from_admitted(mode, PathFlavor::Windows, value)
        .expect("admitted runtime-native path")
}

pub(super) fn execute_contribution(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) {
    let mut command = HomeCommand::new(store.home_revision().expect("home revision"));
    command.add(contribution).expect("add contribution");
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        CommandOutcome::NotCommitted { evidence } => {
            panic!("execute contribution unexpectedly not committed: {evidence:?}")
        }
        outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("execute contribution committed with later failure: {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("execute contribution indeterminate: {outcome:?}")
        }
    }
}
