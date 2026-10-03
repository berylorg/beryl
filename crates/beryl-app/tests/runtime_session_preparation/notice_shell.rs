use super::*;
use beryl_app::cas_projection::{
    MinimumTurnCaptureReserve, ProjectionServiceConfig, SubmissionExecutionWake,
};
use beryl_app::composer_host::{ComposerHostActivationOutcome, SyndicComposerHost};
use beryl_app::composer_marker_seal::{DraftMarkerSealService, DraftMarkerSealServiceLimits};
use beryl_app::main_window::*;
use beryl_app::theme_runtime::{
    AppearanceCoordinator, AppearanceCoordinatorConfig, GpuiAppearanceWindowSet,
};
use beryl_app::window_acquisition::*;
use beryl_home_store::{CommandCancellation, HomeStore, MutationContribution};
use beryl_model::{DomainRevision, WindowBounds, WindowDisplayState, WindowId, WindowPlacement};
use gpui::AppContext;
use std::num::NonZeroUsize;

pub(super) fn commit(home: &HomeStore, mutation: MutationContribution) {
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(mutation).unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

pub(super) fn prepare(
    fixture: &Fixture,
) -> (
    MainWindowShellPrepared,
    (
        RuntimeBackedWindowAcquisitionService,
        RuntimeBackedWindowProcessRegistry,
    ),
) {
    let live = fixture.service().live_home_command().unwrap();
    let home = live.home();
    let execution = binding(fixture, 1);
    let process = RuntimeBackedWindowProcessRegistry::new(Default::default());
    let service = RuntimeBackedWindowAcquisitionService::new(
        &process,
        Arc::new(home.service_reference()),
        fixture.state.clone(),
        fixture.storage.clone(),
    );
    let placement = WindowPlacement::new(
        WindowBounds::new(0, 0, 900, 700).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    );
    let session = fixture.state.session();
    let initial = WindowId::from_bytes([250; 16]);
    commit(
        home,
        session.initialize_threadless(
            session.revision(home).unwrap(),
            beryl_state::InitializeThreadlessWindow::new(initial, placement.clone()),
        ),
    );
    let bootstrap = session.minimal_bootstrap(home).unwrap().unwrap();
    let record = &bootstrap.windows()[0];
    commit(
        home,
        session.remove_window(
            session.revision(home).unwrap(),
            beryl_state::RemoveSessionWindow::new(
                bootstrap.header().revision(),
                initial,
                record.revision(),
                record.selected_thread(),
            ),
        ),
    );
    let request = RuntimeBackedWindowAcquisitionRequest::new(
        WindowId::from_bytes([190; 16]),
        beryl_state::RememberedTarget::new(execution.runtime_id(), execution.root_id()),
        placement,
        SyndicThreadId::from_bytes([191; 16]),
        SyndicDraftId::from_bytes([192; 16]),
        execution,
        SyndicTimestamp::from_unix_millis(1),
        DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
    )
    .unwrap();
    let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } =
        service.acquire(request, CommandCancellation::new())
    else {
        panic!("runtime notice shell acquisition");
    };
    let bootstrap = session.minimal_bootstrap(home).unwrap().unwrap();
    let selection = bootstrap
        .windows()
        .iter()
        .find(|record| record.window_id() == acquisition.window_id())
        .unwrap()
        .selected_thread()
        .unwrap();
    let mut composer = SyndicComposerHost::new(fixture.storage.clone());
    assert!(matches!(
        composer
            .test_activate(
                home,
                composer_support::activation(acquisition.thread_id(), 21, 22, 1, 0),
                &CommandCancellation::new()
            )
            .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let slot = MainWindowComposerSlot::new(
        acquisition.window_id(),
        selection,
        composer,
        fixture.storage.clone(),
        MainWindowComposerMarkerMetadataAuthority::new(fixture.state.assets()),
    )
    .unwrap();
    let composer = Arc::new(MainWindowConversationComposerService::new(
        home.service_reference(),
        slot,
    ));
    let seals = DraftMarkerSealService::test_new(
        home,
        home.health().generation().unwrap(),
        fixture.storage.clone(),
        fixture.state.assets(),
        DraftMarkerSealServiceLimits::new(
            NonZeroUsize::new(1).unwrap(),
            NonZeroUsize::new(1).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let appearance = AppearanceCoordinator::new(
        AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
        beryl_state::PreparedThemeAppearance::fallback(
            fixture
                .state
                .themes()
                .settings_identity(DomainRevision::new(1).unwrap(), None),
        ),
    )
    .current();
    let prepared = MainWindowShellPrepared::prepare(
        &process,
        &service,
        MainWindowShellPreparationRequest::new(
            acquisition,
            composer,
            Box::new(|selection| {
                MainWindowConversationComposerConfig::new(
                    selection,
                    composer_support::widget_config(
                        selection.binding().range_binding(),
                        selection.binding().presentation_generation(),
                    ),
                )
                .map_err(|error| error.to_string())
            }),
            seals,
            MainWindowComposerSubmissionRequestSource::new(
                SubmissionExecutionWake::storage_only_for_test(),
                ProjectionServiceConfig::try_new(
                    1,
                    4,
                    MinimumTurnCaptureReserve::try_new(1).unwrap(),
                )
                .unwrap()
                .turn_start_admission_requirement(),
            ),
            appearance,
        ),
    )
    .unwrap_or_else(|_| panic!("runtime notice shell preparation"));
    (prepared, (service, process))
}

pub(super) fn mount(
    prepared: MainWindowShellPrepared,
    cx: &mut gpui::TestAppContext,
) -> MainWindowShell {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let appearance = cx.update(|app| {
        GpuiAppearanceWindowSet::new(
            prepared.appearance().clone(),
            NonZeroUsize::new(4).unwrap(),
            app,
        )
    });
    cx.update(|app| {
        GpuiMainWindowShellHost::new(app, appearance)
            .construct_hidden(prepared)
            .unwrap_or_else(|_| panic!("runtime notice shell mount"))
    })
}

pub(super) fn tick(window: gpui::WindowHandle<MainWindowShellRoot>, cx: &mut gpui::TestAppContext) {
    cx.executor().advance_clock(Duration::from_millis(250));
    for _ in 0..8 {
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, app| window.draw(app).clear())
            .unwrap();
    }
    thread::sleep(Duration::from_millis(1));
}

pub(super) fn wait(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::TestAppContext,
    predicate: impl Fn(&MainWindowShellRoot, &gpui::App) -> bool,
) {
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        tick(window, cx);
        if window
            .read_with(cx, |root, app| predicate(root, app))
            .unwrap()
        {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "runtime notice did not converge"
        );
    }
}
