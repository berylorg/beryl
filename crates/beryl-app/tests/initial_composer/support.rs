use super::*;
use beryl_home_store::{CommandOutcome, HomeCommand};

pub struct Fixture {
    pub directory: tempfile::TempDir,
    pub store: Arc<HomeStore>,
    pub state: BerylState,
    pub storage: SyndicStorage,
    pub faults: beryl_home_store::test_faults::FaultController,
    pub process: RuntimeBackedWindowProcessRegistry,
    pub service: RuntimeBackedWindowAcquisitionService,
    pub seed: u8,
}

impl Fixture {
    pub fn new(seed: u8) -> Self {
        let (directory, store, state, storage, faults) = home_support::open_home(seed);
        let process = RuntimeBackedWindowProcessRegistry::new();
        let service = RuntimeBackedWindowAcquisitionService::new(
            &process,
            store.clone(),
            state.clone(),
            storage.clone(),
        );
        Self {
            directory,
            store,
            state,
            storage,
            faults,
            process,
            service,
            seed,
        }
    }

    pub fn execution(&self) -> ExecutionBinding {
        ExecutionBinding::new(
            RuntimeId::from_bytes([self.seed; 16]),
            RootId::from_bytes([self.seed.wrapping_add(1); 16]),
            native_path(RuntimeMode::host(), r"C:\Work\Beryl"),
        )
    }

    pub fn acquire(&self, seed: u8) -> RuntimeBackedWindowAcquisition {
        let request = RuntimeBackedWindowAcquisitionRequest::new(
            WindowId::from_bytes([seed; 16]),
            RememberedTarget::new(
                RuntimeId::from_bytes([self.seed; 16]),
                RootId::from_bytes([self.seed.wrapping_add(1); 16]),
            ),
            placement(),
            SyndicThreadId::from_bytes([seed.wrapping_add(40); 16]),
            SyndicDraftId::from_bytes([seed.wrapping_add(41); 16]),
            self.execution(),
            SyndicTimestamp::from_unix_millis(u64::from(seed) + 100),
            DraftEditHistoryPolicyV1::new(65536, 1).unwrap(),
        )
        .unwrap();
        let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } =
            self.service.acquire(request, CommandCancellation::new())
        else {
            panic!("acquire fixture window")
        };
        acquisition
    }

    pub fn claim(&self, window: WindowId) -> beryl_state::WindowClaimSelection {
        self.state
            .session()
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

    pub fn begin(&self, seed: u8) -> MainWindowInitialComposer {
        self.begin_kind(seed, false)
    }
    pub fn begin_with_bad_seed(&self, seed: u8) -> MainWindowInitialComposer {
        self.begin_kind(seed, true)
    }
    fn begin_kind(&self, seed: u8, bad: bool) -> MainWindowInitialComposer {
        let acquisition = self.acquire(seed);
        let reservation = self
            .process
            .reserve_main_window(acquisition.window_id())
            .unwrap();
        self.from_acquired(acquisition, reservation, seed, bad)
    }

    pub fn from_acquired(
        &self,
        acquisition: RuntimeBackedWindowAcquisition,
        reservation: RuntimeBackedWindowMainWindowReservation,
        seed: u8,
        bad: bool,
    ) -> MainWindowInitialComposer {
        let claim = self.claim(acquisition.window_id());
        let request = if bad {
            use beryl_app::composer_host::*;
            ComposerHostActivationRequest::new(
                acquisition.thread_id(),
                DraftEditorCandidateSessionIdV1::from_bytes([seed; 16]),
                composer_support::fixture::operation_id(seed.wrapping_add(1)),
                std::num::NonZeroU64::new(1).unwrap(),
                None,
                vec![ComposerHostInitialDemand::Text {
                    request_id: ComposerHostRequestId::new(std::num::NonZeroU64::new(1).unwrap()),
                    purpose: ComposerHostRequestPurpose::Geometry,
                    demand: syndic_storage::DraftPieceTextDemandV1::Forward(u64::MAX),
                    max_bytes: 4,
                }]
                .into_boxed_slice(),
            )
        } else {
            composer_support::activation(acquisition.thread_id(), seed, seed.wrapping_add(1), 1, 0)
        };
        MainWindowInitialComposer::new(
            acquisition,
            reservation,
            self.service.clone(),
            self.store.clone(),
            self.storage.clone(),
            claim,
            request,
            composer_support::fixture::operation_id(seed.wrapping_add(2)),
            MainWindowComposerMarkerMetadataAuthority::new(self.state.assets()),
        )
        .unwrap_or_else(|failure| panic!("{}", failure.error))
    }

    pub fn session(
        &self,
        draft: SyndicDraftId,
        seed: u8,
    ) -> DraftEditorCandidateSessionReadOutcomeV1 {
        self.storage
            .draft_editor_candidate_session(
                &self.store,
                draft,
                DraftEditorCandidateSessionIdV1::from_bytes([seed; 16]),
            )
            .unwrap()
    }

    pub fn retire_and_release(&self, custody: MainWindowInitialComposer) {
        let unpublished = match custody.retire(CommandCancellation::new()) {
            MainWindowInitialComposerRetirement::Retired(unpublished) => unpublished,
            MainWindowInitialComposerRetirement::Pending(failure) => {
                panic!("candidate retirement must settle: {}", failure.error)
            }
        };
        assert_eq!(self.process.main_window_occupancy(), 1);
        let MainWindowShellAbandonmentPreparationOutcome::ExactAcquired { abandonment } =
            unpublished.prepare_abandonment(&self.service, CommandCancellation::new())
        else {
            panic!("window cleanup exact")
        };
        assert!(matches!(
            abandonment.abandon(&self.service, CommandCancellation::new()),
            MainWindowShellAbandonmentOutcome::Committed { .. }
        ));
    }

    pub fn remove_session_window(&self, window: WindowId) {
        let session = self.state.session();
        let bootstrap = session.minimal_bootstrap(&self.store).unwrap().unwrap();
        let record = bootstrap
            .windows()
            .iter()
            .find(|record| record.window_id() == window)
            .unwrap();
        execute(
            &self.store,
            session.remove_window(
                session.revision(&self.store).unwrap(),
                beryl_state::RemoveSessionWindow::new(
                    bootstrap.header().revision(),
                    window,
                    record.revision(),
                    record.selected_thread(),
                ),
            ),
        );
    }

    pub fn seed_pristine(&self, seed: u8) -> SyndicThreadId {
        let thread = SyndicThreadId::from_bytes([seed; 16]);
        execute(
            &self.store,
            self.storage.create_thread(
                self.storage.revision(&self.store).unwrap(),
                syndic_storage::CreateThread::ordinary(
                    thread,
                    SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                    self.execution(),
                    SyndicTimestamp::from_unix_millis(1),
                    DraftEditHistoryPolicyV1::new(65536, 1).unwrap(),
                ),
            ),
        );
        let beryl_app::catalog_projection::ThreadCatalogProjectionPreparation::Publish(command) =
            beryl_app::catalog_projection::prepare_thread_catalog_projection(
                &self.store,
                &self.storage,
                &self.state,
                thread,
            )
            .unwrap()
        else {
            panic!("catalog projection")
        };
        assert!(matches!(
            self.store.execute(command),
            CommandOutcome::Committed { .. }
        ));
        thread
    }
}

pub fn config(
    selection: MainWindowComposerSelectionIdentity,
) -> Result<MainWindowConversationComposerConfig, String> {
    MainWindowConversationComposerConfig::new(
        selection,
        composer_support::widget_config(
            selection.binding().range_binding(),
            selection.binding().presentation_generation(),
        ),
    )
    .map_err(|error| error.to_string())
}

pub fn placement() -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(10, 20, 900, 700).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
}
pub fn native_path(mode: RuntimeMode, path: &str) -> RuntimeNativePath {
    RuntimeNativePath::from_admitted(mode, beryl_model::PathFlavor::Windows, path).unwrap()
}

pub fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed { .. }
    ));
}
