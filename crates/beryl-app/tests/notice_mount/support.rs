use super::*;
use beryl_app::theme_runtime::{
    AppearanceCoordinator, AppearanceCoordinatorConfig, AppearanceGeneration,
    GpuiAppearanceWindowSet, PreparedPreviewAppearance, PreviewCandidateIdentity, PreviewSource,
    PreviewSourceIdentity,
};
use gpui::{AppContext, Entity, TestAppContext, WindowHandle};
use std::num::NonZeroUsize;

pub struct Mounted {
    pub fixture: Fixture,
    pub services: Arc<MainWindowCreationServices>,
    pub owner: Entity<MainWindowCreationOwner>,
    pub window: WindowHandle<MainWindowShellRoot>,
    pub appearance: Entity<GpuiAppearanceWindowSet>,
    pub coordinator: Option<AppearanceCoordinator>,
}

pub fn mount(cx: &mut TestAppContext, seed: u8) -> Mounted {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let (fixture, services, mut coordinator, appearance, prepared) = home_support::join(
        home_support::worker(move || {
            let fixture = Fixture::new(seed);
            let (services, initial_appearance) = creation_support::services(&fixture);
            let coordinator = AppearanceCoordinator::new(
                AppearanceCoordinatorConfig::new(NonZeroUsize::new(256).unwrap()),
                initial_appearance.prepared().clone(),
            );
            let appearance = coordinator.current();
            let mut initial = fixture.begin(seed.wrapping_add(1));
            assert_eq!(
                initial.advance(&CommandCancellation::new()).unwrap(),
                MainWindowInitialComposerProgress::Activated
            );
            let prepared = initial
                .prepare(&mut config)
                .unwrap_or_else(|failure| panic!("{}", failure.error));
            let prepared = prepared
                .into_shell(
                    Box::new(config),
                    services.marker_seals.clone(),
                    MainWindowComposerSubmissionRequestSource::new(services.turn_start_requirement),
                    appearance.clone(),
                )
                .unwrap_or_else(|failure| panic!("{}", failure.error));
            (fixture, services, coordinator, appearance, prepared)
        }),
        cx,
    );
    let appearance_owner = cx.update(|app| {
        GpuiAppearanceWindowSet::new(appearance, NonZeroUsize::new(256).unwrap(), app)
    });
    let publication_target = appearance_owner.read_with(cx, |owner, _| owner.target());
    coordinator
        .attach_publication_target(publication_target)
        .expect("attach mounted appearance target");
    let owner = cx.update(|app| {
        MainWindowCreationOwner::install(
            services.clone(),
            appearance_owner.clone(),
            MainWindowCreationGate::Ready,
            app,
        )
        .unwrap()
    });
    let mut shell = cx
        .update(|app| {
            GpuiMainWindowShellHost::new(app, appearance_owner.clone()).construct_hidden(prepared)
        })
        .unwrap_or_else(|_| panic!("source hidden shell"));
    cx.update(|app| shell.attach_creation(owner.clone(), app));
    drive_until(cx, |cx| cx.update(|app| shell.ready_to_publish(app)));
    cx.update(|app| shell.publish(app).unwrap());
    let window = shell.window();
    cx.update(|app| shell.release_published_handle(app))
        .unwrap_or_else(|_| panic!("source published"));
    Mounted {
        fixture,
        services,
        owner,
        window,
        appearance: appearance_owner,
        coordinator: Some(coordinator),
    }
}

pub fn activate_second(
    mounted: &Mounted,
    cx: &mut TestAppContext,
) -> WindowHandle<MainWindowShellRoot> {
    cx.update(|app| {
        let root = mounted.window.entity(app).expect("mounted root");
        mounted
            .owner
            .update(app, |owner, cx| owner.activate(&root, cx))
            .expect("activate second window");
    });
    drive_until(cx, |cx| {
        cx.windows().len() == 2
            && mounted
                .owner
                .read_with(cx, |owner, _| owner.pending_count())
                == 0
    });
    cx.windows()
        .into_iter()
        .find(|window| *window != mounted.window.into())
        .expect("second main window")
        .downcast::<MainWindowShellRoot>()
        .expect("second shell root")
}

pub fn ingress(
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut TestAppContext,
) -> MainWindowNoticeIngress {
    window
        .update(cx, |root, window, root_cx| {
            root.notice_ingress(window, root_cx)
        })
        .expect("notice ingress")
}

pub fn window_id(window: WindowHandle<MainWindowShellRoot>, cx: &TestAppContext) -> WindowId {
    window
        .read_with(cx, |root, _| {
            root.controller().expect("controller").window_id()
        })
        .expect("window id")
}

pub fn visible_token(
    window: WindowHandle<MainWindowShellRoot>,
    cx: &TestAppContext,
) -> NoticeVisibleToken {
    window
        .read_with(cx, |root, _| {
            root.notice_projection()
                .expect("active notice")
                .token
                .clone()
        })
        .expect("visible token")
}

#[derive(Debug, Eq, PartialEq)]
pub struct ShellGeometry {
    pub transcript: gpui::Bounds<gpui::Pixels>,
    pub composer: gpui::Bounds<gpui::Pixels>,
}

pub fn shell_geometry(
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut TestAppContext,
) -> ShellGeometry {
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    ShellGeometry {
        transcript: visual
            .debug_bounds("main-window-transcript-region")
            .expect("transcript"),
        composer: visual
            .debug_bounds("main-window-user-input-panel")
            .expect("composer"),
    }
}

pub fn widget_diagnostics(
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut TestAppContext,
) -> MainWindowNoticeWidgetDiagnostics {
    window
        .update(cx, |root, window, root_cx| {
            root.notice_widget().read(root_cx).diagnostics(window)
        })
        .expect("notice widget diagnostics")
}

pub fn notice_bounds(
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut TestAppContext,
) -> Option<gpui::Bounds<gpui::Pixels>> {
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.debug_bounds("main-window-notice")
}

pub fn command_selector(
    window: WindowHandle<MainWindowShellRoot>,
    index: usize,
    cx: &mut TestAppContext,
) -> &'static str {
    let diagnostics = widget_diagnostics(window, cx);
    Box::leak(
        format!(
            "main-window-notice-command-{}-{}-{index}",
            diagnostics.widget_instance_id, diagnostics.replacements
        )
        .into_boxed_str(),
    )
}

pub fn publish_preview(
    mounted: &mut Mounted,
    source: u64,
    cx: &mut TestAppContext,
) -> Arc<AppearanceGeneration> {
    let mut coordinator = mounted.coordinator.take().expect("appearance coordinator");
    let coordinator = home_support::join(
        home_support::worker(move || {
            let request = coordinator
                .begin_preview(
                    PreviewSource::DynamicTool(PreviewSourceIdentity::try_new(source).unwrap()),
                    PreviewCandidateIdentity::Digest(beryl_state::ThemeDocumentDigest::from_bytes(
                        [source as u8; 32],
                    )),
                )
                .expect("preview request");
            let candidate = request.candidate().clone();
            coordinator
                .publish_preview(
                    request,
                    PreparedPreviewAppearance::new(
                        candidate,
                        coordinator.current().prepared().clone(),
                    ),
                )
                .expect("publish preview");
            coordinator
        }),
        cx,
    );
    let current = coordinator.current();
    mounted.coordinator = Some(coordinator);
    current
}

pub fn draw(cx: &mut TestAppContext) {
    creation_support::draw(cx);
}

pub fn drive_until(cx: &mut TestAppContext, ready: impl FnMut(&mut TestAppContext) -> bool) {
    creation_support::drive_until(cx, ready);
}

pub fn finish(mounted: Mounted, cx: &mut TestAppContext) {
    let Mounted {
        fixture,
        services,
        owner,
        appearance,
        coordinator,
        ..
    } = mounted;
    cx.update(|app| owner.update(app, |owner, cx| owner.fence(cx)));
    drive_until(cx, |cx| {
        owner.read_with(cx, |owner, _| owner.pending_count()) == 0
    });
    for window in cx.windows() {
        cx.update(|app| {
            app.update_window(window, |_, window, _| window.remove_window())
                .unwrap()
        });
    }
    draw(cx);
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    cx.update(MainWindowCreationOwner::test_uninstall);
    drop((owner, services, appearance, coordinator));
    for _ in 0..32 {
        draw(cx);
    }
    home_support::join(
        home_support::worker(move || creation_support::cleanup(fixture)),
        cx,
    );
}
