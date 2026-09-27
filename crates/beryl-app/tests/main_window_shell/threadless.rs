use super::*;

#[cfg(target_os = "windows")]
#[path = "threadless_construction.rs"]
mod construction;
#[cfg(target_os = "windows")]
#[path = "threadless_desktop.rs"]
mod desktop;
#[cfg(target_os = "windows")]
#[path = "threadless_disposal.rs"]
mod disposal;

#[path = "../support/window_placement.rs"]
mod placement_support;
use beryl_app::main_window::{
    RestoredWindowPreparationAttempt, RestoredWindowServiceTestLifetime,
    ThreadlessWindowShellPrepared,
};
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_state::{InitializeThreadlessWindow, PreparedThemeAppearance};

struct Fixture {
    attempt: RestoredWindowPreparationAttempt,
    lifetime: Option<RestoredWindowServiceTestLifetime>,
    process: RuntimeBackedWindowProcessRegistry,
    coordinator: Option<AppearanceCoordinator>,
    state: BerylState,
    window: beryl_model::WindowId,
    store: Arc<HomeStore>,
    _directory: tempfile::TempDir,
}

impl Fixture {
    fn new(seed: u8) -> Self {
        Self::with_placement(seed, placement())
    }

    fn with_placement(seed: u8, saved: WindowPlacement) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let storage = syndic_storage::SyndicStorage::register(&mut candidate).unwrap();
        let store = Arc::new(
            candidate
                .prepare_publication(
                    BerylState::required_domains()
                        .unwrap()
                        .merge(syndic_storage::SyndicStorage::required_domains().unwrap())
                        .unwrap(),
                )
                .unwrap()
                .publish()
                .unwrap(),
        );
        let window = beryl_model::WindowId::from_bytes([seed; 16]);
        let session = state.session();
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(session.initialize_threadless(
                session.revision(&store).unwrap(),
                InitializeThreadlessWindow::new(window, saved),
            ))
            .unwrap();
        assert!(matches!(
            store.execute(command),
            CommandOutcome::Committed { .. }
        ));
        let (attempt, lifetime) =
            RestoredWindowPreparationAttempt::new_for_test(store.clone(), session, storage)
                .unwrap();
        let coordinator = AppearanceCoordinator::new(
            AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
            PreparedThemeAppearance::fallback(
                state
                    .themes()
                    .settings_identity(beryl_model::DomainRevision::new(1).unwrap(), None),
            ),
        );
        Self {
            attempt,
            lifetime: Some(lifetime),
            process: RuntimeBackedWindowProcessRegistry::new(Default::default()),
            coordinator: Some(coordinator),
            state,
            window,
            store,
            _directory: directory,
        }
    }

    fn prepare(&self) -> Result<ThreadlessWindowShellPrepared, String> {
        let revision = self
            .state
            .session()
            .minimal_bootstrap(&self.store)
            .unwrap()
            .unwrap()
            .header()
            .revision();
        let source =
            self.attempt
                .begin_threadless(revision, self.window, self.state.runtime_roots())?;
        ThreadlessWindowShellPrepared::new(
            source,
            &self.process,
            self.coordinator.as_ref().unwrap().current(),
        )
    }

    fn owner(&mut self, cx: &mut gpui::TestAppContext) -> gpui::Entity<GpuiAppearanceWindowSet> {
        let appearance = self.coordinator.as_ref().unwrap().current();
        let owner = cx.update(|app| {
            GpuiAppearanceWindowSet::new(appearance, NonZeroUsize::new(4).unwrap(), app)
        });
        let target = owner.read_with(cx, |owner, _| owner.target());
        self.coordinator
            .as_mut()
            .unwrap()
            .attach_publication_target(target)
            .unwrap();
        owner
    }
}

#[cfg(target_os = "windows")]
#[gpui::test]
fn startup_geometry_is_required_and_bound_before_threadless_allocation(
    cx: &mut gpui::TestAppContext,
) {
    for mode in ["missing", "foreign", "changed"] {
        let (mut fixture, prepared) = support::join(
            support::worker(|| {
                let fixture = Fixture::new(141);
                let prepared = fixture.prepare().unwrap();
                (fixture, prepared)
            }),
            cx,
        );
        let owner = fixture.owner(cx);
        let native = match mode {
            "missing" => None,
            "foreign" => Some(placement_support::prepare(
                beryl_model::WindowId::from_bytes([142; 16]),
                prepared.placement(),
            )),
            _ => Some(placement_support::prepare(
                prepared.window_id(),
                &WindowPlacement::new(
                    WindowBounds::new(321, 123, 640, 480).unwrap(),
                    WindowDisplayState::Maximized,
                    None,
                    None,
                ),
            )),
        };
        let before = cx.update(|app| app.windows().len());
        assert!(
            cx.update(|app| {
                let host = GpuiMainWindowShellHost::new(app, owner);
                match native {
                    Some(native) => host.with_prepared_placement(native),
                    None => host,
                }
                .construct_threadless_hidden(prepared)
            })
            .is_err()
        );
        assert_eq!(cx.update(|app| app.windows().len()), before);
        assert_eq!(fixture.process.main_window_occupancy(), 0);
    }
}

#[cfg(target_os = "windows")]
#[gpui::test]
fn threadless_hidden_construction_uses_prepared_normal_and_maximized_geometry(
    cx: &mut gpui::TestAppContext,
) {
    for state in [WindowDisplayState::Normal, WindowDisplayState::Maximized] {
        let (mut fixture, prepared) = support::join(
            support::worker(move || {
                let fixture = Fixture::with_placement(
                    143,
                    WindowPlacement::new(
                        WindowBounds::new(80, 60, 800, 600).unwrap(),
                        state,
                        None,
                        None,
                    ),
                );
                let prepared = fixture.prepare().unwrap();
                (fixture, prepared)
            }),
            cx,
        );
        let owner = fixture.owner(cx);
        let native = placement_support::prepare(prepared.window_id(), prepared.placement());
        let expected = native.gpui_window_bounds().unwrap();
        let shell = cx
            .update(|app| {
                GpuiMainWindowShellHost::new(app, owner)
                    .with_prepared_placement(native)
                    .construct_threadless_hidden(prepared)
            })
            .unwrap();
        shell
            .window()
            .update(cx, |_, window, _| {
                assert_eq!(window.window_bounds(), expected)
            })
            .unwrap();
        assert!(!cx.window_visibility(shell.window().into()).is_visible);
        cx.update(|app| shell.close_threadless_before_publication(app))
            .unwrap_or_else(|_| panic!("dispose placed threadless shell"));
        assert_eq!(fixture.process.main_window_occupancy(), 0);
    }
}

#[test]
fn threadless_preparation_rejects_foreign_appearance_without_reserving_or_writing() {
    let fixture = Fixture::new(125);
    let foreign = Fixture::new(126);
    let before = fixture.store.home_revision().unwrap();
    let revision = fixture
        .state
        .session()
        .minimal_bootstrap(&fixture.store)
        .unwrap()
        .unwrap()
        .header()
        .revision();
    let source = fixture
        .attempt
        .begin_threadless(revision, fixture.window, fixture.state.runtime_roots())
        .unwrap();
    assert!(
        ThreadlessWindowShellPrepared::new(
            source,
            &fixture.process,
            foreign.coordinator.as_ref().unwrap().current()
        )
        .is_err()
    );
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    assert_eq!(fixture.store.home_revision().unwrap(), before);
}

#[gpui::test]
fn threadless_publication_keeps_no_editor_and_releases_native_reservation_on_close(
    cx: &mut gpui::TestAppContext,
) {
    let (mut fixture, prepared) = support::join(
        support::worker(|| {
            let fixture = Fixture::new(123);
            let prepared = fixture.prepare().unwrap();
            (fixture, prepared)
        }),
        cx,
    );
    let owner = fixture.owner(cx);
    let native_placement = placement_support::prepare(prepared.window_id(), prepared.placement());
    let mut shell = cx.update(|app| {
        placement_support::attach(
            GpuiMainWindowShellHost::new(app, owner.clone()),
            native_placement,
        )
        .construct_threadless_hidden(prepared)
        .unwrap()
    });
    draw(shell.window(), cx);
    cx.update(|app| shell.publish(app)).unwrap();
    let window = shell.window();
    assert!(cx.window_visibility(window.into()).is_visible);
    let shell = cx
        .update(|app| shell.close_threadless_before_publication(app))
        .err()
        .expect("published shell no longer admits prepublication disposal");
    cx.update(|app| shell.release_published_handle(app))
        .unwrap_or_else(|_| panic!("publish handoff"));
    window
        .update(cx, |root, window, _| {
            assert!(root.controller().unwrap().composer_mount().is_none());
            window.remove_window();
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    assert_eq!(
        owner.read_with(cx, |owner, _| {
            use beryl_app::theme_runtime::AppearancePublicationTarget;
            owner.target().snapshot().count
        }),
        0
    );
}

#[gpui::test]
fn retired_threadless_preparation_releases_reservation_without_native_mount(
    cx: &mut gpui::TestAppContext,
) {
    let (mut fixture, prepared) = support::join(
        support::worker(|| {
            let fixture = Fixture::new(124);
            let prepared = fixture.prepare().unwrap();
            (fixture, prepared)
        }),
        cx,
    );
    let owner = fixture.owner(cx);
    drop(fixture.lifetime.take());
    let native_placement = placement_support::prepare(prepared.window_id(), prepared.placement());
    assert!(
        cx.update(|app| placement_support::attach(
            GpuiMainWindowShellHost::new(app, owner.clone()),
            native_placement
        )
        .construct_threadless_hidden(prepared))
            .is_err()
    );
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    assert_eq!(
        owner.read_with(cx, |owner, _| {
            use beryl_app::theme_runtime::AppearancePublicationTarget;
            owner.target().snapshot().count
        }),
        0
    );
}

#[gpui::test]
fn threadless_hidden_shell_has_no_editor_and_disposal_preserves_saved_member(
    cx: &mut gpui::TestAppContext,
) {
    let (mut fixture, prepared, before) = support::join(
        support::worker(|| {
            let fixture = Fixture::new(121);
            let prepared = fixture.prepare().unwrap();
            assert!(fixture.prepare().is_err());
            let before = fixture.store.home_revision().unwrap();
            (fixture, prepared, before)
        }),
        cx,
    );
    let owner = fixture.owner(cx);
    let native_placement = placement_support::prepare(prepared.window_id(), prepared.placement());
    let shell = cx.update(|app| {
        placement_support::attach(
            GpuiMainWindowShellHost::new(app, owner.clone()),
            native_placement,
        )
        .construct_threadless_hidden(prepared)
        .unwrap()
    });
    assert!(!cx.window_visibility(shell.window().into()).is_visible);
    shell
        .window()
        .update(cx, |root, window, cx| {
            let before = root.new_window_disabled_reason(cx);
            let exit_before = root.test_exit_presentation();
            assert!(root.test_begin_shutdown_draft(window, cx).is_err());
            root.test_set_shutdown_interaction_gated(true, cx).unwrap();
            let draft = root.test_begin_shutdown_draft(window, cx).unwrap();
            assert!(draft.test_ticket().is_none());
            assert_eq!(
                root.test_advance_shutdown_draft(&draft, window, cx)
                    .unwrap(),
                beryl_app::main_window::MainWindowShutdownDraftAdvance::Threadless
            );
            assert_eq!(
                root.test_exit_presentation(),
                (
                    "Exiting…",
                    "Application Exit is waiting for active work and durable state."
                )
            );
            assert_eq!(
                root.new_window_disabled_reason(cx).as_deref(),
                Some("Application Exit is waiting for active work and durable state.")
            );
            root.test_set_shutdown_interaction_gated(false, cx).unwrap();
            assert_eq!(root.new_window_disabled_reason(cx), before);
            assert_eq!(root.test_exit_presentation(), exit_before);
        })
        .unwrap();
    assert!(cx.update(|app| shell.ready_to_publish(app)));
    let minimum = shell
        .window()
        .read_with(cx, |root, _| {
            let controller = root.controller().unwrap();
            assert!(controller.is_threadless());
            assert!(controller.acquisition().is_none());
            assert!(controller.composer_mount().is_none());
            assert_eq!(controller.window_id(), fixture.window);
            assert_eq!(controller.placement(), &placement());
            controller.minimum_size()
        })
        .unwrap();
    cx.simulate_window_resize(shell.window().into(), minimum);
    draw(shell.window(), cx);
    let mut visual = gpui::VisualTestContext::from_window(shell.window().into(), cx);
    assert!(
        visual
            .debug_bounds("main-window-user-input-panel")
            .is_none()
    );
    assert!(visual.debug_bounds("conversation-composer-root").is_none());
    assert!(visual.debug_bounds("main-window-new-window").is_some());
    let shell = cx
        .update(|app| shell.close_before_publication(app))
        .err()
        .expect("threadless cannot enter acquired abandonment");
    cx.update(|app| shell.close_threadless_before_publication(app))
        .unwrap_or_else(|_| panic!("threadless disposal"));
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    assert_eq!(
        owner.read_with(cx, |owner, _| {
            use beryl_app::theme_runtime::AppearancePublicationTarget;
            owner.target().snapshot().count
        }),
        0
    );
    support::join(
        support::worker(move || {
            assert_eq!(fixture.store.home_revision().unwrap(), before);
            let saved = fixture
                .state
                .session()
                .minimal_bootstrap(&fixture.store)
                .unwrap()
                .unwrap();
            assert_eq!(saved.windows().len(), 1);
            assert_eq!(saved.windows()[0].window_id(), fixture.window);
            assert!(saved.windows()[0].selected_thread().is_none());
        }),
        cx,
    );
}

#[gpui::test]
fn threadless_shell_adopts_appearance_without_composer_and_retired_attempt_blocks_publication(
    cx: &mut gpui::TestAppContext,
) {
    let (mut fixture, prepared) = support::join(
        support::worker(|| {
            let fixture = Fixture::new(122);
            let prepared = fixture.prepare().unwrap();
            (fixture, prepared)
        }),
        cx,
    );
    let owner = fixture.owner(cx);
    let native_placement = placement_support::prepare(prepared.window_id(), prepared.placement());
    let shell = cx.update(|app| {
        placement_support::attach(GpuiMainWindowShellHost::new(app, owner), native_placement)
            .construct_threadless_hidden(prepared)
            .unwrap()
    });
    let mut coordinator = fixture.coordinator.take().unwrap();
    coordinator = support::join(
        support::worker(move || {
            use beryl_app::theme_runtime::{
                PreparedPreviewAppearance, PreviewCandidateIdentity, PreviewSource,
                PreviewSourceIdentity,
            };
            let request = coordinator
                .begin_preview(
                    PreviewSource::DynamicTool(PreviewSourceIdentity::try_new(71).unwrap()),
                    PreviewCandidateIdentity::Digest(beryl_state::ThemeDocumentDigest::from_bytes(
                        [71; 32],
                    )),
                )
                .unwrap();
            let candidate = request.candidate().clone();
            coordinator
                .publish_preview(
                    request,
                    PreparedPreviewAppearance::new(
                        candidate,
                        coordinator.current().prepared().clone(),
                    ),
                )
                .unwrap();
            coordinator
        }),
        cx,
    );
    draw(shell.window(), cx);
    shell
        .window()
        .read_with(cx, |root, _| {
            assert!(Arc::ptr_eq(
                root.controller().unwrap().appearance(),
                &coordinator.current()
            ))
        })
        .unwrap();
    assert!(cx.update(|app| shell.ready_to_publish(app)));
    drop(fixture.lifetime.take());
    assert!(!cx.update(|app| shell.ready_to_publish(app)));
    cx.update(|app| shell.close_threadless_before_publication(app))
        .unwrap_or_else(|_| panic!("retired source still allows cleanup"));
    assert_eq!(fixture.process.main_window_occupancy(), 0);
}
