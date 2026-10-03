use super::*;
use beryl_app::composer_host::{ComposerHostActivationOutcome, SyndicComposerHost};
use beryl_app::composer_marker_seal::{DraftMarkerSealService, DraftMarkerSealServiceLimits};
use beryl_app::theme_runtime::{AppearanceCoordinator, AppearanceCoordinatorConfig};
use beryl_state::{
    AvailabilitySnapshot, CreateRuntimeWithHomeRoot, RootRegistration, RuntimeRegistration,
    UnixMillis,
};
use std::num::NonZeroUsize;

pub fn worker<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> std::thread::JoinHandle<T> {
    std::thread::Builder::new()
        .name("mounted-status-worker".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(work)
        .unwrap()
}

pub fn join<T: Send + 'static>(
    work: std::thread::JoinHandle<T>,
    cx: &mut gpui::TestAppContext,
) -> T {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !work.is_finished() {
        assert!(Instant::now() < deadline, "status worker did not finish");
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(1));
    }
    work.join().unwrap()
}

pub fn draw(window: gpui::WindowHandle<MainWindowShellRoot>, cx: &mut gpui::TestAppContext) {
    for _ in 0..8 {
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();
    }
}

type Diagnostics = (
    &'static str,
    bool,
    bool,
    usize,
    Option<ExactStopFeedbackState>,
);

pub fn wait(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::TestAppContext,
    predicate: impl Fn(Diagnostics) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        draw(window, cx);
        let diagnostic = window
            .read_with(cx, |root, _| root.test_exact_status_diagnostics())
            .unwrap();
        if predicate(diagnostic) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "mounted status did not settle: {diagnostic:?}"
        );
        cx.executor().advance_clock(Duration::from_millis(250));
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub fn prepare_shell(
    fixture: &mut syndic::Fixture,
) -> (
    MainWindowShellPrepared,
    (
        RuntimeBackedWindowAcquisitionService,
        RuntimeBackedWindowProcessRegistry,
    ),
) {
    let home = fixture.home();
    let execution = syndic::execution_binding();
    let host = |path| AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap();
    let native = |path| {
        RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, path).unwrap()
    };
    let runtime = RuntimeRegistration::new(
        execution.runtime_id(),
        host(r"C:\runtime\codex.exe"),
        RuntimeMode::host(),
        native(r"C:\runtime\codex.exe"),
        UnixMillis::new(1),
        AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(2)).unwrap(),
    )
    .unwrap();
    let root = RootRegistration::new(
        execution.root_id(),
        native(EXECUTION_ROOT),
        host(EXECUTION_ROOT),
        UnixMillis::new(1),
        AvailabilitySnapshot::unknown(),
    );
    commit(
        &home,
        fixture.state.runtime_roots().create_runtime_with_home_root(
            fixture.state.runtime_roots().revision(&home).unwrap(),
            CreateRuntimeWithHomeRoot::new(runtime, root).unwrap(),
        ),
    );
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
        &home,
        session.initialize_threadless(
            session.revision(&home).unwrap(),
            beryl_state::InitializeThreadlessWindow::new(initial, placement.clone()),
        ),
    );
    let bootstrap = session.minimal_bootstrap(&home).unwrap().unwrap();
    let initial_record = &bootstrap.windows()[0];
    commit(
        &home,
        session.remove_window(
            session.revision(&home).unwrap(),
            beryl_state::RemoveSessionWindow::new(
                bootstrap.header().revision(),
                initial,
                initial_record.revision(),
                initial_record.selected_thread(),
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
        syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
    )
    .unwrap();
    let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } =
        service.acquire(request, CommandCancellation::new())
    else {
        panic!("status shell acquisition");
    };
    let selection = fixture
        .state
        .session()
        .minimal_bootstrap(&home)
        .unwrap()
        .unwrap()
        .windows()
        .iter()
        .find(|window| window.window_id() == acquisition.window_id())
        .unwrap()
        .selected_thread()
        .unwrap();
    drop(home);
    fixture.thread = acquisition.thread_id();
    let home = fixture.home();
    let mut composer = SyndicComposerHost::new(fixture.storage.clone());
    assert!(matches!(
        composer
            .test_activate(
                &home,
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
        &home,
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
    .unwrap_or_else(|failure| match failure {
        MainWindowShellPreparationFailure::Composer { error, .. } => {
            panic!("status composer preparation: {error}")
        }
        MainWindowShellPreparationFailure::Reservation { error, .. } => {
            panic!("status reservation: {error:?}")
        }
    });
    drop(home);
    (prepared, (service, process))
}

fn commit(home: &beryl_home_store::HomeStore, mutation: beryl_home_store::MutationContribution) {
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
