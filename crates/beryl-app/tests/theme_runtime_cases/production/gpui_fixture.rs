use super::super::*;
use beryl_app::{
    composer_host::{ComposerHostActivationOutcome, SyndicComposerHost},
    composer_marker_seal::DraftMarkerSealService,
    main_window::{
        MainWindowComposerMarkerMetadataAuthority, MainWindowComposerSlot,
        MainWindowComposerSubmissionRequestSource, MainWindowConversationComposerConfig,
        MainWindowConversationComposerConfigurator, MainWindowConversationComposerMount,
        MainWindowConversationComposerPreparedSelection, MainWindowConversationComposerService,
    },
    theme_runtime::{
        AdapterFailureClass, AppearanceGeneration, AppearancePublicationTarget,
        AppearanceWindowAdapter, GpuiAppearancePublicationTarget, PreparedWindowAppearance,
        WindowAdapterId,
    },
};
use beryl_home_store::CommandCancellation;
use gpui::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window,
    WindowHandle, canvas, div, px,
};
use gpui_text_input::{TextInputTheme, ensure_text_input_bindings};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[path = "../../phase186_pending_composer_activation/support.rs"]
mod composer_support;

pub struct ComposerFixture {
    _directory: tempfile::TempDir,
    service: Arc<MainWindowConversationComposerService>,
    seals: DraftMarkerSealService,
}

impl ComposerFixture {
    pub fn new(seed: u8) -> Self {
        let fixture = composer_support::fixture::Fixture::new("phase294-gpui", seed);
        composer_support::seed_activation_published_draft(&fixture, fixture.selected_thread);
        let mut host = SyndicComposerHost::new(fixture.storage.clone());
        assert!(matches!(
            host.test_activate(
                &fixture.store,
                composer_support::activation(
                    fixture.selected_thread,
                    21,
                    22,
                    1,
                    composer_support::ACTIVATION_DRAFT_BYTES
                ),
                &CommandCancellation::new()
            )
            .unwrap(),
            ComposerHostActivationOutcome::Activated { .. }
        ));
        let slot = MainWindowComposerSlot::new(
            fixture.window_id,
            fixture.claims().0,
            host,
            fixture.storage.clone(),
            MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
        )
        .unwrap();
        let seals = fixture.marker_seals();
        let (directory, store, _) = fixture.into_store();
        Self {
            _directory: directory,
            service: Arc::new(MainWindowConversationComposerService::new(
                Arc::new(store),
                slot,
            )),
            seals,
        }
    }

    pub fn prepared(&self) -> MainWindowConversationComposerPreparedSelection {
        MainWindowConversationComposerMount::prepare_selected(
            self.service.clone(),
            &mut configurator(),
        )
        .unwrap()
    }
}

fn configurator() -> MainWindowConversationComposerConfigurator {
    Box::new(|selection| {
        MainWindowConversationComposerConfig::new(
            selection,
            composer_support::widget_config(
                selection.binding().range_binding(),
                selection.binding().presentation_generation(),
            ),
        )
        .map_err(|error| format!("{error:?}"))
    })
}

pub struct TestRoot {
    pub mount: Entity<MainWindowConversationComposerMount>,
    pub generation: Arc<AppearanceGeneration>,
    pub color: gpui::Rgba,
    pub snapshot: Rc<RefCell<Option<gpui::test::PaintSnapshot>>>,
}

impl Render for TestRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.snapshot.clone();
        div()
            .size_full()
            .flex()
            .bg(self.color)
            .child(self.mount.clone())
            .child(
                canvas(
                    |_, window, _| {
                        let text =
                            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-[]";
                        let glyphs = window.text_system().shape_line(
                            text.into(),
                            px(12.),
                            &[gpui::TextRun {
                                len: text.len(),
                                font: gpui::font(".SystemUIFont"),
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
                                gpui::point(gpui::DevicePixels(0), gpui::DevicePixels(-6)),
                                gpui::size(gpui::DevicePixels(4), gpui::DevicePixels(6)),
                            ),
                        );
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

pub fn open_root(
    fixture: &ComposerFixture,
    prepared: MainWindowConversationComposerPreparedSelection,
    generation: Arc<AppearanceGeneration>,
    cx: &mut gpui::TestAppContext,
) -> WindowHandle<TestRoot> {
    cx.update(ensure_text_input_bindings);
    cx.update(|app| {
        app.open_window(gpui::WindowOptions::default(), |window, app| {
            let source = MainWindowComposerSubmissionRequestSource::new(
                beryl_app::cas_projection::ProjectionServiceConfig::try_new(
                    1,
                    4,
                    beryl_home_store::MinimumTurnCaptureReserve::try_new(1).unwrap(),
                )
                .unwrap()
                .turn_start_admission_requirement(),
            );
            let mount = app.new(|mount_cx| {
                MainWindowConversationComposerMount::from_prepared(
                    prepared,
                    configurator(),
                    fixture.seals.clone(),
                    source,
                    window,
                    mount_cx,
                )
                .unwrap()
            });
            app.new(|_| TestRoot {
                mount,
                generation,
                color: gpui::rgb(0),
                snapshot: Rc::new(RefCell::new(None)),
            })
        })
        .unwrap()
    })
}

pub fn color(generation: &AppearanceGeneration) -> gpui::Rgba {
    let style = generation
        .prepared()
        .appearance()
        .roles()
        .iter()
        .find(|(id, _)| id.as_str() == "app.window")
        .unwrap()
        .1;
    let beryl_state::ThemeValue::Color(value) = style
        .property(beryl_state::ThemePropertyId::Background)
        .unwrap()
    else {
        panic!("window background")
    };
    let [r, g, b] = value.rgb();
    gpui::rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
}

#[derive(Clone, Default)]
pub struct AdapterControls {
    pub reject: Arc<AtomicBool>,
    pub retire_during_prepare: Arc<AtomicBool>,
    pub retire_during_commit: Arc<AtomicBool>,
    pub close_during_prepare: Rc<RefCell<Option<WindowHandle<TestRoot>>>>,
}

pub struct TestAdapter {
    pub id: WindowAdapterId,
    pub window: WindowHandle<TestRoot>,
    pub target: Arc<GpuiAppearancePublicationTarget>,
    pub controls: AdapterControls,
}

struct Prepared {
    window: WindowHandle<TestRoot>,
    generation: Arc<AppearanceGeneration>,
    target: Arc<GpuiAppearancePublicationTarget>,
    controls: AdapterControls,
}

impl PreparedWindowAppearance for Prepared {
    fn validate(&self, app: &App) -> Result<(), AdapterFailureClass> {
        self.window
            .read_with(app, |root, app| {
                root.mount.read(app).selected_first_presentable(app)
            })
            .map_err(|_| AdapterFailureClass::Unavailable)
            .and_then(|mounted| {
                if mounted {
                    Ok(())
                } else {
                    Err(AdapterFailureClass::Unavailable)
                }
            })
    }

    fn commit(self: Box<Self>, app: &mut App) {
        let _ = self.target.snapshot();
        if self.controls.retire_during_commit.load(Ordering::SeqCst) {
            self.target.retire();
        }
        let window = self.window;
        window
            .update(app, |root, _, app| {
                let color = color(&self.generation);
                let owner = root.mount.read(app).contribution().unwrap();
                let input = owner.read(app).gpui_input();
                input
                    .update(app, |input, input_cx| {
                        input.set_appearance(
                            TextInputTheme {
                                text: Some(color.into()),
                                placeholder: color.into(),
                                ..Default::default()
                            },
                            gpui_scrollbar::ScrollbarStyle::default(),
                            input_cx,
                        )
                    })
                    .unwrap();
                root.color = color;
                root.generation = self.generation;
                app.notify();
            })
            .unwrap();
    }
}

impl AppearanceWindowAdapter for TestAdapter {
    fn id(&self) -> WindowAdapterId {
        self.id
    }

    fn prepare(
        &self,
        generation: Arc<AppearanceGeneration>,
        app: &mut App,
    ) -> Result<Box<dyn PreparedWindowAppearance>, AdapterFailureClass> {
        let _ = self.target.snapshot();
        if self.controls.retire_during_prepare.load(Ordering::SeqCst) {
            self.target.retire();
        }
        if let Some(window) = self.controls.close_during_prepare.borrow_mut().take() {
            window
                .update(app, |_, window, _| window.remove_window())
                .unwrap();
        }
        if self.controls.reject.load(Ordering::SeqCst) {
            return Err(AdapterFailureClass::Rejected);
        }
        Ok(Box::new(Prepared {
            window: self.window,
            generation,
            target: self.target.clone(),
            controls: self.controls.clone(),
        }))
    }
}

pub fn on_worker<T: Send + 'static>(
    cx: &mut gpui::TestAppContext,
    work: impl FnOnce() -> T + Send + 'static,
) -> T {
    let worker = std::thread::Builder::new()
        .name("phase294-appearance-worker".to_owned())
        .stack_size(16 * 1024 * 1024)
        .spawn(work)
        .unwrap();
    join_worker(cx, worker)
}

pub fn join_worker<T: Send + 'static>(
    cx: &mut gpui::TestAppContext,
    worker: std::thread::JoinHandle<T>,
) -> T {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while !worker.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "publication worker did not settle"
        );
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(1));
    }
    worker.join().unwrap()
}

pub fn draw(window: WindowHandle<TestRoot>, cx: &mut gpui::TestAppContext) {
    for _ in 0..32 {
        cx.run_until_parked();
        cx.update(|app| {
            app.update_window(window.into(), |_, window, app| window.draw(app).clear())
                .unwrap()
        });
    }
}
