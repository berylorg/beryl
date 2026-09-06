use std::{
    cell::RefCell,
    num::{NonZeroU64, NonZeroUsize},
    rc::Rc,
};

use beryl_app::main_window::{
    MainWindowNoticeArbiter, MainWindowNoticeDiagnosticKey, MainWindowNoticeOverlayAllocation,
    MainWindowNoticePaintPhase, MainWindowNoticeWidget, MainWindowNoticeWidgetEvent,
    MainWindowNoticeWidgetRecord, NoticeAdmission, NoticeConditionId, NoticeContent, NoticeKind,
    NoticeRecord, NoticeRecordToken,
};
use beryl_model::WindowId;
use beryl_state::{
    InstalledThemeId, PreparedThemeAppearance, ThemeDocument, ThemeDocumentDigest,
    ThemeDocumentIdentity, ThemeDocumentRevision, ThemeManifestGeneration, ThemeParseMode,
    ThemeResolver,
};
use gpui::{
    AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement, ParentElement,
    Render, Styled, Window, WindowHandle, canvas, div, px,
};

#[path = "../theme_runtime_cases/support.rs"]
mod theme_support;

pub const ROOT: &str = "main-window-notice";
pub const DETAIL: &str = "main-window-notice-detail";
pub const CLOSE: &str = "main-window-notice-close";
pub const SAFE_TARGET: &str = "notice-safe-target";

pub struct NoticeSource {
    arbiter: MainWindowNoticeArbiter,
    token: NoticeRecordToken,
    key: MainWindowNoticeDiagnosticKey,
    allocation: MainWindowNoticeOverlayAllocation,
}

impl NoticeSource {
    pub fn new(seed: u8, content: NoticeContent) -> Self {
        let window = WindowId::from_bytes([seed; 16]);
        let mut arbiter = MainWindowNoticeArbiter::new(window);
        let token = match arbiter.admit(NoticeRecord {
            window_id: window,
            condition: NoticeConditionId::new(),
            revision: 1,
            kind: NoticeKind::Warning,
            content,
        }) {
            NoticeAdmission::Admitted(token) => token,
            outcome => panic!("notice fixture admission failed: {outcome:?}"),
        };
        Self {
            arbiter,
            token,
            key: MainWindowNoticeDiagnosticKey::from_opaque_bytes([seed; 32]),
            allocation: MainWindowNoticeOverlayAllocation::new(48., 12., 12., 620., 320.),
        }
    }

    pub fn record(&self) -> MainWindowNoticeWidgetRecord {
        MainWindowNoticeWidgetRecord::from_projection(
            self.arbiter.active().expect("fixture active notice"),
            self.key,
            self.allocation,
        )
    }

    pub fn update(&mut self, content: NoticeContent) -> MainWindowNoticeWidgetRecord {
        self.token = self
            .arbiter
            .update(&self.token, self.token.revision() + 1, content)
            .expect("fixture revision update");
        self.record()
    }

    pub fn with_allocation(mut self, allocation: MainWindowNoticeOverlayAllocation) -> Self {
        self.allocation = allocation;
        self
    }

    pub fn set_allocation(&mut self, allocation: MainWindowNoticeOverlayAllocation) {
        self.allocation = allocation;
    }
}

pub struct TestRoot {
    pub widget: Entity<MainWindowNoticeWidget>,
    pub safe_focus: FocusHandle,
    pub snapshot: Rc<RefCell<Option<gpui::test::PaintSnapshot>>>,
}

impl Render for TestRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.snapshot.clone();
        div()
            .size_full()
            .relative()
            .child(
                div()
                    .id(SAFE_TARGET)
                    .debug_selector(|| SAFE_TARGET.to_owned())
                    .absolute()
                    .left(px(4.))
                    .bottom(px(4.))
                    .track_focus(&self.safe_focus)
                    .tab_stop(true),
            )
            .child(self.widget.clone())
            .child(
                canvas(
                    |_, window, _| {
                        let text =
                            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-[]×é漢字";
                        for (size, weight) in [
                            (13., 400.),
                            (13., 500.),
                            (13., 650.),
                            (14., 400.),
                            (29., 700.),
                        ] {
                            let mut font = gpui::font("Inter");
                            font.weight = gpui::FontWeight(weight);
                            let glyphs = window.text_system().shape_line(
                                text.into(),
                                px(size),
                                &[gpui::TextRun {
                                    len: text.len(),
                                    font,
                                    color: gpui::black(),
                                    background_color: None,
                                    underline: None,
                                    strikethrough: None,
                                }],
                                None,
                            );
                            window.text_system().set_glyph_raster_bounds_for_test(
                                &glyphs,
                                window.scale_factor(),
                                gpui::Bounds::new(
                                    gpui::point(
                                        gpui::DevicePixels(0),
                                        gpui::DevicePixels(-(size as i32)),
                                    ),
                                    gpui::size(
                                        gpui::DevicePixels((size / 2.) as i32),
                                        gpui::DevicePixels(size as i32),
                                    ),
                                ),
                            );
                        }
                    },
                    move |_, _, window, _| {
                        *snapshot.borrow_mut() = Some(window.paint_snapshot_for_test())
                    },
                )
                .absolute()
                .size_full(),
            )
    }
}

pub struct Mounted {
    pub window: WindowHandle<TestRoot>,
    pub root: Entity<TestRoot>,
    pub widget: Entity<MainWindowNoticeWidget>,
    pub events: Rc<RefCell<Vec<MainWindowNoticeWidgetEvent>>>,
    _theme: theme_support::StateFixture,
    _coordinator: theme_support::TestCoordinator,
}

impl Mounted {
    pub fn new(cx: &mut gpui::TestAppContext) -> Self {
        let theme = theme_support::StateFixture::new();
        let coordinator = theme_support::coordinator(&theme, 1);
        Self::with_theme(theme, coordinator, cx)
    }

    pub fn with_theme_document(cx: &mut gpui::TestAppContext, bytes: &[u8]) -> Self {
        let theme = theme_support::StateFixture::new();
        let active = InstalledThemeId::new("notice-test").expect("test theme id");
        let document = ThemeDocument::parse_bytes(bytes, ThemeParseMode::StrictCandidate)
            .expect("valid notice test theme");
        let settings = theme
            .service
            .settings_identity(beryl_model::DomainRevision::new(1).expect("revision"), None);
        let identity = ThemeDocumentIdentity::new(
            theme.service.manifest(ThemeManifestGeneration::INITIAL),
            active.clone(),
            ThemeDocumentRevision::new(NonZeroU64::new(1).expect("revision")),
            bytes.len() as u64,
            ThemeDocumentDigest::of_bytes(bytes),
        );
        let prepared = PreparedThemeAppearance::installed(
            settings,
            &active,
            identity,
            ThemeResolver::new(document.definition())
                .expect("resolved notice test theme")
                .resolve(),
        )
        .expect("prepared notice test theme");
        let coordinator = theme_support::TestCoordinator::new(
            beryl_app::theme_runtime::AppearanceCoordinator::new(
                beryl_app::theme_runtime::AppearanceCoordinatorConfig::new(
                    NonZeroUsize::new(1).expect("capacity"),
                ),
                prepared,
            ),
        );
        Self::with_theme(theme, coordinator, cx)
    }

    fn with_theme(
        theme: theme_support::StateFixture,
        coordinator: theme_support::TestCoordinator,
        cx: &mut gpui::TestAppContext,
    ) -> Self {
        let appearance = coordinator.current();
        let events = Rc::new(RefCell::new(Vec::new()));
        let events_for_widget = events.clone();
        let (window, root, widget) = cx.update(|app| {
            let mut root = None;
            let mut widget = None;
            let window = app
                .open_window(gpui::WindowOptions::default(), |_window, app| {
                    let safe_focus = app.focus_handle();
                    let widget_events = events_for_widget.clone();
                    let notice = app.new(|notice_cx| {
                        MainWindowNoticeWidget::new(
                            appearance.clone(),
                            safe_focus.clone(),
                            move |event| widget_events.borrow_mut().push(event),
                            notice_cx,
                        )
                    });
                    widget = Some(notice.clone());
                    let root_entity = app.new(|_| TestRoot {
                        widget: notice,
                        safe_focus,
                        snapshot: Rc::new(RefCell::new(None)),
                    });
                    root = Some(root_entity.clone());
                    root_entity
                })
                .expect("open notice test window");
            (
                window,
                root.expect("test root"),
                widget.expect("notice widget"),
            )
        });
        let mounted = Self {
            window,
            root,
            widget,
            events,
            _theme: theme,
            _coordinator: coordinator,
        };
        mounted.draw(cx);
        mounted
    }

    pub fn replace(
        &self,
        record: Option<MainWindowNoticeWidgetRecord>,
        cx: &mut gpui::TestAppContext,
    ) {
        let widget = self.widget.clone();
        self.window
            .update(cx, |_, window, app| {
                widget.update(app, |widget, widget_cx| {
                    widget.replace(record, window, widget_cx)
                })
            })
            .expect("replace notice record");
        self.draw(cx);
    }

    pub fn inert(&self, inert: bool, cx: &mut gpui::TestAppContext) {
        let widget = self.widget.clone();
        self.window
            .update(cx, |_, window, app| {
                widget.update(app, |widget, widget_cx| {
                    widget.set_inert(inert, window, widget_cx)
                })
            })
            .expect("set notice inert");
        self.draw(cx);
    }

    pub fn set_theme_document(&self, bytes: &[u8], cx: &mut gpui::TestAppContext) {
        let theme = theme_support::StateFixture::new();
        let active = InstalledThemeId::new("notice-test-update").expect("test theme id");
        let document = ThemeDocument::parse_bytes(bytes, ThemeParseMode::StrictCandidate)
            .expect("valid notice update theme");
        let settings = theme
            .service
            .settings_identity(beryl_model::DomainRevision::new(1).expect("revision"), None);
        let identity = ThemeDocumentIdentity::new(
            theme.service.manifest(ThemeManifestGeneration::INITIAL),
            active.clone(),
            ThemeDocumentRevision::new(NonZeroU64::new(1).expect("revision")),
            bytes.len() as u64,
            ThemeDocumentDigest::of_bytes(bytes),
        );
        let prepared = PreparedThemeAppearance::installed(
            settings,
            &active,
            identity,
            ThemeResolver::new(document.definition())
                .expect("resolved notice update theme")
                .resolve(),
        )
        .expect("prepared notice update theme");
        let appearance = theme_support::TestCoordinator::new(
            beryl_app::theme_runtime::AppearanceCoordinator::new(
                beryl_app::theme_runtime::AppearanceCoordinatorConfig::new(
                    NonZeroUsize::new(1).expect("capacity"),
                ),
                prepared,
            ),
        )
        .current();
        let widget = self.widget.clone();
        self.window
            .update(cx, |_, window, app| {
                widget.update(app, |widget, widget_cx| {
                    widget.set_appearance(appearance, window, widget_cx)
                })
            })
            .expect("set notice appearance");
        self.draw(cx);
    }

    pub fn set_paint_phase(
        &self,
        phase: MainWindowNoticePaintPhase,
        cx: &mut gpui::TestAppContext,
    ) {
        let widget = self.widget.clone();
        widget.update(cx, |widget, widget_cx| {
            widget.set_paint_phase(phase, widget_cx)
        });
        self.draw(cx);
    }

    pub fn diagnostics(
        &self,
        cx: &mut gpui::TestAppContext,
    ) -> beryl_app::main_window::MainWindowNoticeWidgetDiagnostics {
        self.window
            .update(cx, |_, window, app| {
                self.widget.read(app).diagnostics(window)
            })
            .expect("notice diagnostics")
    }

    pub fn command_selector(&self, index: usize, cx: &mut gpui::TestAppContext) -> &'static str {
        let diagnostics = self.diagnostics(cx);
        Box::leak(
            format!(
                "main-window-notice-command-{}-{}-{index}",
                diagnostics.widget_instance_id, diagnostics.replacements
            )
            .into_boxed_str(),
        )
    }

    pub fn draw(&self, cx: &mut gpui::TestAppContext) {
        for _ in 0..8 {
            cx.run_until_parked();
            cx.update(|app| {
                app.update_window(self.window.into(), |_, window, app| {
                    window.draw(app).clear()
                })
                .expect("draw notice test window")
            });
        }
    }

    pub fn paint(&self, cx: &mut gpui::TestAppContext) -> gpui::test::PaintSnapshot {
        self.root.read_with(cx, |root, _| {
            root.snapshot.borrow().clone().expect("paint snapshot")
        })
    }
}
