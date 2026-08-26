use std::num::{NonZeroU64, NonZeroUsize};

use beryl_app::{
    composer_host::{
        ComposerHostActivationOutcome, ComposerHostActivationRequest, SyndicComposerHost,
    },
    composer_marker_seal::{DraftMarkerSealService, DraftMarkerSealServiceLimits},
};
use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion,
    HomeStore, MutationContribution, test_faults::FaultController,
};
use beryl_model::{
    AdmittedHostPath, Availability, ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicDraftId, SyndicThreadId, WindowBounds, WindowDisplayState, WindowId,
    WindowPlacement,
};
use beryl_state::{
    AvailabilitySnapshot, BerylState, CreateRuntimeWithHomeRoot, InitializeThreadlessWindow,
    RememberedTarget, ReplaceWindowClaim, RootRegistration, RuntimeRegistration, UnixMillis,
    WindowClaimSelection,
};
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, DraftEditorCandidateSessionIdV1,
    DraftPieceOperationIdV1, SyndicCurrentDraft, SyndicPointReadLimit, SyndicStorage,
    SyndicTimestamp,
};

pub struct Fixture {
    _directory: tempfile::TempDir,
    pub store: HomeStore,
    state: BerylState,
    pub storage: SyndicStorage,
    pub window_id: WindowId,
    pub selected_thread: SyndicThreadId,
    pub target_thread: SyndicThreadId,
    runtime_id: RuntimeId,
    root_id: RootId,
    pub faults: FaultController,
}

impl Fixture {
    pub fn new(name: &str, seed: u8) -> Self {
        Self::with_history_budget(name, seed, 65_536)
    }

    pub fn with_history_budget(name: &str, seed: u8, history_byte_budget: u64) -> Self {
        let directory = tempfile::Builder::new()
            .prefix(&format!("phase177-{name}-"))
            .tempdir()
            .unwrap();
        let faults = FaultController::new();
        let mut store = HomeStore::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let state = BerylState::register(&mut store).unwrap();
        let storage = SyndicStorage::register(&mut store).unwrap();
        let runtime_id = RuntimeId::from_bytes([seed; 16]);
        let root_id = RootId::from_bytes([seed.wrapping_add(1); 16]);
        let selected_thread = SyndicThreadId::from_bytes([seed.wrapping_add(2); 16]);
        let target_thread = SyndicThreadId::from_bytes([seed.wrapping_add(3); 16]);
        let mode = RuntimeMode::host();
        let runtime = RuntimeRegistration::new(
            runtime_id,
            host_path(r"C:\Program Files\Codex\codex.exe"),
            mode.clone(),
            native_path(mode.clone(), r"C:\Program Files\Codex\codex.exe"),
            UnixMillis::new(1),
            AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(2)).unwrap(),
        )
        .unwrap();
        let root = RootRegistration::new(
            root_id,
            native_path(mode.clone(), r"C:\Work\Beryl"),
            host_path(r"C:\Work\Beryl"),
            UnixMillis::new(1),
            AvailabilitySnapshot::unknown(),
        );
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(state.runtime_roots().create_runtime_with_home_root(
                state.runtime_roots().revision(&store).unwrap(),
                CreateRuntimeWithHomeRoot::new(runtime, root).unwrap(),
            ))
            .unwrap();
        committed(store.execute(command));
        for (thread, draft, path) in [
            (selected_thread, seed.wrapping_add(4), r"C:\Work\Beryl"),
            (target_thread, seed.wrapping_add(5), r"C:\Work\Beryl\target"),
        ] {
            execute(
                &store,
                storage.create_thread(
                    storage.revision(&store).unwrap(),
                    CreateThread::ordinary(
                        thread,
                        SyndicDraftId::from_bytes([draft; 16]),
                        ExecutionBinding::new(runtime_id, root_id, native_path(mode.clone(), path)),
                        SyndicTimestamp::from_unix_millis(3),
                        DraftEditHistoryPolicyV1::new(history_byte_budget, 1).unwrap(),
                    ),
                ),
            );
        }
        Self {
            _directory: directory,
            store,
            state,
            storage,
            window_id: WindowId::from_bytes([seed.wrapping_add(6); 16]),
            selected_thread,
            target_thread,
            runtime_id,
            root_id,
            faults,
        }
    }

    pub fn claims(&self) -> (WindowClaimSelection, WindowClaimSelection) {
        let first_selected = self.claim(self.window_id, self.selected_thread);
        let target = self.replace_claim(self.window_id, first_selected, self.target_thread);
        let selected = self.replace_claim(self.window_id, target, self.selected_thread);
        (selected, target)
    }

    pub fn recover_same_home(self) -> (tempfile::TempDir, HomeStore, SyndicStorage) {
        let recovery = self.store.recover_same_home().unwrap();
        let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        (self._directory, recovery.publish(), storage)
    }

    pub fn into_store(self) -> (tempfile::TempDir, HomeStore, SyndicStorage) {
        (self._directory, self.store, self.storage)
    }

    pub fn activated_host(
        &self,
        thread: SyndicThreadId,
        session: u8,
        operation: u8,
        presentation: u64,
    ) -> SyndicComposerHost {
        let mut host = SyndicComposerHost::new(self.storage);
        assert!(matches!(
            host.test_activate(
                &self.store,
                activation(thread, session, operation, presentation),
                &CommandCancellation::new(),
            )
            .unwrap(),
            ComposerHostActivationOutcome::Activated { .. }
        ));
        host
    }

    pub fn current_draft(&self, thread: SyndicThreadId) -> SyndicCurrentDraft {
        self.storage
            .current_draft(
                &self.store,
                thread,
                SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .unwrap()
    }

    pub fn marker_seals(&self) -> DraftMarkerSealService {
        DraftMarkerSealService::new(
            &self.store,
            self.store.health().generation().unwrap(),
            self.storage,
            self.state.assets(),
            DraftMarkerSealServiceLimits::new(
                NonZeroUsize::new(1).unwrap(),
                NonZeroUsize::new(1).unwrap(),
            )
            .unwrap(),
        )
        .unwrap()
    }

    pub fn assets(&self) -> beryl_state::AssetState {
        self.state.assets()
    }

    fn claim(&self, window: WindowId, thread: SyndicThreadId) -> WindowClaimSelection {
        let session = self.state.session();
        execute(
            &self.store,
            session.initialize_threadless(
                session.revision(&self.store).unwrap(),
                InitializeThreadlessWindow::new(window, placement()),
            ),
        );
        let initial = session.minimal_bootstrap(&self.store).unwrap().unwrap();
        let record = initial
            .windows()
            .iter()
            .find(|record| record.window_id() == window)
            .unwrap();
        execute(
            &self.store,
            session.replace_claim(
                session.revision(&self.store).unwrap(),
                ReplaceWindowClaim::new(
                    initial.header().revision(),
                    window,
                    record.revision(),
                    None,
                    RememberedTarget::new(self.runtime_id, self.root_id),
                    thread,
                ),
            ),
        );
        session
            .minimal_bootstrap(&self.store)
            .unwrap()
            .unwrap()
            .windows()
            .iter()
            .find(|record| record.window_id() == window)
            .unwrap()
            .selected_thread()
            .unwrap()
    }

    fn replace_claim(
        &self,
        window: WindowId,
        expected: WindowClaimSelection,
        thread: SyndicThreadId,
    ) -> WindowClaimSelection {
        let session = self.state.session();
        let initial = session.minimal_bootstrap(&self.store).unwrap().unwrap();
        let record = initial
            .windows()
            .iter()
            .find(|record| record.window_id() == window)
            .unwrap();
        execute(
            &self.store,
            session.replace_claim(
                session.revision(&self.store).unwrap(),
                ReplaceWindowClaim::new(
                    initial.header().revision(),
                    window,
                    record.revision(),
                    Some(expected),
                    RememberedTarget::new(self.runtime_id, self.root_id),
                    thread,
                ),
            ),
        );
        session
            .minimal_bootstrap(&self.store)
            .unwrap()
            .unwrap()
            .windows()[0]
            .selected_thread()
            .unwrap()
    }
}

pub fn activation(
    thread: SyndicThreadId,
    session: u8,
    operation: u8,
    presentation: u64,
) -> ComposerHostActivationRequest {
    ComposerHostActivationRequest::new(
        thread,
        DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
        operation_id(operation),
        NonZeroU64::new(presentation).unwrap(),
        None,
        Box::new([]),
    )
}

pub fn operation_id(seed: u8) -> DraftPieceOperationIdV1 {
    DraftPieceOperationIdV1::from_bytes([seed; 16])
}

fn execute(store: &HomeStore, contribution: MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    committed(store.execute(command));
}

fn committed(outcome: CommandOutcome) {
    match outcome {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        other => panic!("command did not commit cleanly: {other:?}"),
    }
}

fn placement() -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(0, 0, 900, 700).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
}

fn host_path(value: &str) -> AdmittedHostPath {
    AdmittedHostPath::from_admitted(PathFlavor::Windows, value).unwrap()
}

fn native_path(mode: RuntimeMode, value: &str) -> RuntimeNativePath {
    RuntimeNativePath::from_admitted(mode, PathFlavor::Windows, value).unwrap()
}
