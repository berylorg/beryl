#![cfg(feature = "test-faults")]

#[path = "phase186_pending_composer_activation/support.rs"]
mod composer_support;
#[path = "phase289_main_window_shell/custody.rs"]
mod custody;
#[path = "phase289_main_window_shell/gpui.rs"]
mod gpui_cases;
#[path = "phase289_main_window_shell/support.rs"]
mod support;

use std::sync::Arc;

use beryl_app::theme_runtime::{
    AppearanceCoordinator, AppearanceCoordinatorConfig, AppearanceGeneration,
    GpuiAppearanceWindowSet,
};
use beryl_app::{
    composer_host::{ComposerHostActivationOutcome, SyndicComposerHost},
    composer_marker_seal::DraftMarkerSealService,
    main_window::{
        GpuiMainWindowShellHost, MainWindowComposerMarkerMetadataAuthority, MainWindowComposerSlot,
        MainWindowComposerSubmissionRequestSource, MainWindowConversationComposerConfig,
        MainWindowConversationComposerService, MainWindowShellHost as _,
        MainWindowShellPreparationRequest, MainWindowShellPrepared,
    },
    window_acquisition::{
        RuntimeBackedWindowAcquisition, RuntimeBackedWindowAcquisitionOutcome,
        RuntimeBackedWindowAcquisitionRequest, RuntimeBackedWindowAcquisitionService,
        RuntimeBackedWindowProcessRegistry,
    },
};
use beryl_home_store::CommandCancellation;
use beryl_model::{
    ExecutionBinding, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId, WindowBounds, WindowDisplayState, WindowPlacement,
};
use beryl_state::{BerylState, RememberedTarget};
use gpui_text_input::ensure_text_input_bindings;
use std::num::NonZeroUsize;
use syndic_storage::{DraftEditHistoryPolicyV1, SyndicTimestamp};

struct ShellFixture {
    _directory: tempfile::TempDir,
    store: Arc<beryl_home_store::HomeStore>,
    service: RuntimeBackedWindowAcquisitionService,
    process: RuntimeBackedWindowProcessRegistry,
    composer: Arc<MainWindowConversationComposerService>,
    marker_seals: DraftMarkerSealService,
    acquisition: Option<RuntimeBackedWindowAcquisition>,
    appearance: Arc<AppearanceGeneration>,
    coordinator: Option<AppearanceCoordinator>,
    faults: beryl_home_store::test_faults::FaultController,
    state: BerylState,
    storage: syndic_storage::SyndicStorage,
    target: RememberedTarget,
}

impl ShellFixture {
    fn new(seed: u8) -> Self {
        let (directory, store, state, storage, faults) = support::open_home(seed);
        let window_id = beryl_model::WindowId::from_bytes([seed; 16]);
        let marker_seals = DraftMarkerSealService::new(
            &store,
            store.health().generation().unwrap(),
            storage.clone(),
            state.assets(),
            beryl_app::composer_marker_seal::DraftMarkerSealServiceLimits::new(
                NonZeroUsize::new(1).unwrap(),
                NonZeroUsize::new(1).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let process = RuntimeBackedWindowProcessRegistry::new();
        let service = RuntimeBackedWindowAcquisitionService::new(
            &process,
            Arc::clone(&store),
            state.clone(),
            storage.clone(),
        );
        let runtime_id = RuntimeId::from_bytes([seed; 16]);
        let root_id = RootId::from_bytes([seed.wrapping_add(1); 16]);
        let execution = ExecutionBinding::new(
            runtime_id,
            root_id,
            native_path(RuntimeMode::host(), r"C:\Work\Beryl"),
        );
        let request = RuntimeBackedWindowAcquisitionRequest::new(
            window_id,
            RememberedTarget::new(runtime_id, root_id),
            placement(),
            SyndicThreadId::from_bytes([seed.wrapping_add(40); 16]),
            SyndicDraftId::from_bytes([seed.wrapping_add(41); 16]),
            execution,
            SyndicTimestamp::from_unix_millis(u64::from(seed) + 100),
            DraftEditHistoryPolicyV1::new(65_536, 1).expect("history policy"),
        )
        .expect("acquisition request");
        let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } =
            service.acquire(request, CommandCancellation::new())
        else {
            panic!("shell fixture acquisition must commit")
        };
        let selection = state
            .session()
            .minimal_bootstrap(&store)
            .expect("session read")
            .expect("session bootstrap")
            .windows()
            .iter()
            .find(|record| record.window_id() == window_id)
            .expect("acquired window record")
            .selected_thread()
            .expect("acquired selected claim");
        let mut host = SyndicComposerHost::new(storage.clone());
        assert!(matches!(
            host.test_activate(
                &store,
                composer_support::activation(acquisition.thread_id(), 21, 22, 1, 0),
                &CommandCancellation::new(),
            )
            .expect("activate exact acquired draft"),
            ComposerHostActivationOutcome::Activated { .. }
        ));
        let slot = MainWindowComposerSlot::new(
            window_id,
            selection,
            host,
            storage.clone(),
            MainWindowComposerMarkerMetadataAuthority::new(state.assets()),
        )
        .expect("exact window composer slot");
        let composer = Arc::new(MainWindowConversationComposerService::new(
            store.clone(),
            slot,
        ));
        let theme = state.themes();
        let coordinator = AppearanceCoordinator::new(
            AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
            beryl_state::PreparedThemeAppearance::fallback(
                theme.settings_identity(beryl_model::DomainRevision::new(1).unwrap(), None),
            ),
        );
        let appearance = coordinator.current();
        Self {
            _directory: directory,
            store,
            service,
            process,
            composer,
            marker_seals,
            acquisition: Some(acquisition),
            appearance,
            coordinator: Some(coordinator),
            faults,
            state,
            storage,
            target: RememberedTarget::new(runtime_id, root_id),
        }
    }

    fn prepare(&mut self) -> MainWindowShellPrepared {
        MainWindowShellPrepared::prepare(
            &self.process,
            &self.service,
            MainWindowShellPreparationRequest::new(
                self.acquisition.take().expect("one exact acquisition"),
                self.composer.clone(),
                configurator(),
                self.marker_seals.clone(),
                submission_source(),
                self.appearance.clone(),
            ),
        )
        .unwrap_or_else(|_| panic!("exact shell preparation"))
    }

    fn owner(&mut self, cx: &mut gpui::TestAppContext) -> gpui::Entity<GpuiAppearanceWindowSet> {
        let owner = cx.update(|app| {
            GpuiAppearanceWindowSet::new(
                self.appearance.clone(),
                NonZeroUsize::new(4).unwrap(),
                app,
            )
        });
        let target = owner.read_with(cx, |owner, _| owner.target());
        self.coordinator
            .as_mut()
            .unwrap()
            .attach_publication_target(target)
            .unwrap();
        owner
    }

    fn additional(&self, seed: u8) -> MainWindowShellPrepared {
        let window_id = beryl_model::WindowId::from_bytes([seed; 16]);
        let request = RuntimeBackedWindowAcquisitionRequest::new(
            window_id,
            self.target,
            placement(),
            SyndicThreadId::from_bytes([seed.wrapping_add(40); 16]),
            SyndicDraftId::from_bytes([seed.wrapping_add(41); 16]),
            ExecutionBinding::new(
                self.target.runtime_id(),
                self.target.root_id(),
                native_path(RuntimeMode::host(), r"C:\Work\Beryl"),
            ),
            SyndicTimestamp::from_unix_millis(u64::from(seed) + 100),
            DraftEditHistoryPolicyV1::new(65536, 1).unwrap(),
        )
        .unwrap();
        let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } =
            self.service.acquire(request, CommandCancellation::new())
        else {
            panic!("second acquisition")
        };
        let bootstrap = self
            .state
            .session()
            .minimal_bootstrap(&self.store)
            .unwrap()
            .unwrap();
        let claim = bootstrap
            .windows()
            .iter()
            .find(|record| record.window_id() == window_id)
            .unwrap()
            .selected_thread()
            .unwrap();
        let mut host = SyndicComposerHost::new(self.storage.clone());
        assert!(matches!(
            host.test_activate(
                &self.store,
                composer_support::activation(
                    acquisition.thread_id(),
                    seed,
                    seed.wrapping_add(1),
                    1,
                    0
                ),
                &CommandCancellation::new()
            )
            .unwrap(),
            ComposerHostActivationOutcome::Activated { .. }
        ));
        let slot = MainWindowComposerSlot::new(
            window_id,
            claim,
            host,
            self.storage.clone(),
            MainWindowComposerMarkerMetadataAuthority::new(self.state.assets()),
        )
        .unwrap();
        let composer = Arc::new(MainWindowConversationComposerService::new(
            self.store.clone(),
            slot,
        ));
        MainWindowShellPrepared::prepare(
            &self.process,
            &self.service,
            MainWindowShellPreparationRequest::new(
                acquisition,
                composer,
                configurator(),
                self.marker_seals.clone(),
                submission_source(),
                self.appearance.clone(),
            ),
        )
        .unwrap_or_else(|_| panic!("second prepared shell"))
    }
}

fn submission_source() -> MainWindowComposerSubmissionRequestSource {
    MainWindowComposerSubmissionRequestSource::new(
        beryl_app::cas_projection::ProjectionServiceConfig::try_new(
            1,
            4,
            beryl_home_store::MinimumTurnCaptureReserve::try_new(1).expect("capture reserve"),
        )
        .expect("projection config")
        .turn_start_admission_requirement(),
    )
}

fn configurator() -> beryl_app::main_window::MainWindowShellComposerConfigurator {
    Box::new(|selection| {
        MainWindowConversationComposerConfig::new(
            selection,
            composer_support::widget_config(
                selection.binding().range_binding(),
                selection.binding().presentation_generation(),
            ),
        )
        .map_err(|error| error.to_string())
    })
}

fn placement() -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(10, 20, 900, 700).expect("window bounds"),
        WindowDisplayState::Normal,
        None,
        None,
    )
}

fn native_path(mode: RuntimeMode, path: &str) -> RuntimeNativePath {
    RuntimeNativePath::from_admitted(mode, beryl_model::PathFlavor::Windows, path)
        .expect("native path")
}

struct FailingHiddenHost;

fn draw(
    window: gpui::WindowHandle<beryl_app::main_window::MainWindowShellRoot>,
    cx: &mut gpui::TestAppContext,
) {
    for _ in 0..32 {
        cx.run_until_parked();
        cx.update(|app| {
            gpui::AppContext::update_window(app, window.into(), |_, window, app| {
                window.draw(app).clear()
            })
            .unwrap()
        });
    }
}

impl beryl_app::main_window::MainWindowShellHost for FailingHiddenHost {
    type Shell = ();
    type Error = &'static str;

    fn construct_hidden(
        &mut self,
        prepared: MainWindowShellPrepared,
    ) -> Result<(), beryl_app::main_window::MainWindowShellHostFailure<Self::Error>> {
        Err(
            beryl_app::main_window::MainWindowShellHostFailure::BeforeConstruction {
                error: "injected construction failure",
                prepared,
            },
        )
    }
}

#[gpui::test]
fn hidden_shell_prepares_exact_editor_and_appearance_then_publishes_once(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let mut fixture = ShellFixture::new(31);
    let window_id = fixture.acquisition.as_ref().unwrap().window_id();
    let owner = fixture.owner(cx);
    let mut shell = cx.update(|app| {
        GpuiMainWindowShellHost::new(app, owner)
            .construct_hidden(fixture.prepare())
            .unwrap_or_else(|_| panic!("hidden shell construction"))
    });
    let visibility = cx.window_visibility(shell.window().into());
    assert!(!visibility.is_visible);
    assert_eq!(visibility.visibility_change_count, 0);
    shell
        .window()
        .read_with(cx, |root, _| {
            let controller = root.controller().expect("window-local controller");
            assert_eq!(controller.window_id(), window_id);
            assert!(Arc::ptr_eq(controller.appearance(), &fixture.appearance));
            assert!(controller.composer_mount().is_some());
        })
        .expect("hidden root");
    draw(shell.window(), cx);
    cx.update(|app| shell.publish(app))
        .expect("first publication");
    let published = cx.window_visibility(shell.window().into());
    assert!(published.is_visible);
    assert_eq!(published.visibility_change_count, 1);
    cx.update(|app| shell.publish(app))
        .expect("idempotent publication");
    assert_eq!(
        cx.window_visibility(shell.window().into())
            .visibility_change_count,
        1
    );
}

#[test]
fn injected_construction_failure_and_prepublication_close_retain_exact_reservation_until_abandoned()
{
    let mut failed_fixture = ShellFixture::new(61);
    let failed_id = failed_fixture.acquisition.as_ref().unwrap().window_id();
    let failed = FailingHiddenHost.construct_hidden(failed_fixture.prepare());
    let Err(failed) = failed else {
        panic!("injected host must reject construction")
    };
    let unpublished = failed.into_unpublished();
    assert_eq!(unpublished.window_id(), failed_id);
    assert_eq!(failed_fixture.process.main_window_occupancy(), 1);
    let abandonment = match unpublished
        .prepare_abandonment(&failed_fixture.service, CommandCancellation::new())
    {
        beryl_app::main_window::MainWindowShellAbandonmentPreparationOutcome::ExactAcquired {
            abandonment,
        } => abandonment,
        _ => panic!("exact acquired abandonment custody"),
    };
    assert_eq!(failed_fixture.process.main_window_occupancy(), 1);
    assert!(matches!(
        abandonment.abandon(&failed_fixture.service, CommandCancellation::new()),
        beryl_app::main_window::MainWindowShellAbandonmentOutcome::Committed { .. }
    ));
    assert_eq!(failed_fixture.process.main_window_occupancy(), 0);
}

#[gpui::test]
fn close_before_publication_keeps_reservation_for_exact_abandonment(cx: &mut gpui::TestAppContext) {
    cx.update(ensure_text_input_bindings);
    let mut fixture = ShellFixture::new(91);
    let process = fixture.process.clone();
    let owner = fixture.owner(cx);
    let shell = cx.update(|app| {
        GpuiMainWindowShellHost::new(app, owner)
            .construct_hidden(fixture.prepare())
            .unwrap_or_else(|_| panic!("hidden shell construction"))
    });
    assert_eq!(process.main_window_occupancy(), 1);
    let unpublished = cx
        .update(|app| shell.close_before_publication(app))
        .unwrap_or_else(|_| panic!("prepublication close"));
    assert_eq!(process.main_window_occupancy(), 1);
    let abandonment = match unpublished
        .prepare_abandonment(&fixture.service, CommandCancellation::new())
    {
        beryl_app::main_window::MainWindowShellAbandonmentPreparationOutcome::ExactAcquired {
            abandonment,
        } => abandonment,
        _ => panic!("exact acquired abandonment custody"),
    };
    assert!(matches!(
        abandonment.abandon(&fixture.service, CommandCancellation::new()),
        beryl_app::main_window::MainWindowShellAbandonmentOutcome::Committed { .. }
    ));
    assert_eq!(process.main_window_occupancy(), 0);
}
