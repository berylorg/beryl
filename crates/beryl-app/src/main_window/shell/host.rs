use super::*;

pub struct GpuiMainWindowShellHost<'a> {
    app: &'a mut App,
    appearance_owner: Entity<GpuiAppearanceWindowSet>,
    #[cfg(feature = "test-faults")]
    reject_mount: bool,
}

impl<'a> GpuiMainWindowShellHost<'a> {
    #[must_use]
    pub fn new(app: &'a mut App, appearance_owner: Entity<GpuiAppearanceWindowSet>) -> Self {
        Self {
            app,
            appearance_owner,
            #[cfg(feature = "test-faults")]
            reject_mount: false,
        }
    }

    #[cfg(feature = "test-faults")]
    pub fn test_reject_mount_after_native(&mut self) {
        self.reject_mount = true;
    }
}

impl MainWindowShellHost for GpuiMainWindowShellHost<'_> {
    type Shell = MainWindowShell;
    type Error = String;

    fn construct_hidden(
        &mut self,
        prepared: MainWindowShellPrepared,
    ) -> Result<Self::Shell, MainWindowShellHostFailure<Self::Error>> {
        let MainWindowShellPrepared {
            acquisition,
            reservation,
            initial_composer,
            composer,
            composer_configurator,
            marker_seals,
            submission_request_source,
            appearance,
        } = prepared;
        let selection = composer.selection_identity();
        let minimum_size = composer.shell_minimum_size();
        let pending = Rc::new(RefCell::new(Some((
            MainWindowShellController {
                acquisition,
                reservation,
                initial_composer,
                appearance,
                selection,
                minimum_size,
                composer_mount: None,
            },
            (
                composer,
                composer_configurator,
                marker_seals,
                submission_request_source,
            ),
        ))));
        let root_pending = Rc::clone(&pending);
        #[cfg(feature = "test-faults")]
        let reject_mount = std::mem::take(&mut self.reject_mount);
        let window = self
            .app
            .open_window(
                WindowOptions {
                    show: false,
                    focus: false,
                    window_min_size: Some(minimum_size),
                    ..Default::default()
                },
                move |window, cx| {
                    let (mut controller, composer) = root_pending
                        .borrow_mut()
                        .take()
                        .expect("main-window shell host invokes its root constructor once");
                    let (composer, configurator, marker_seals, submission_request_source) =
                        composer;
                    let mount = || {
                        MainWindowConversationComposerMount::from_prepared_entity(
                            composer,
                            configurator,
                            marker_seals,
                            submission_request_source,
                            window,
                            cx,
                        )
                    };
                    #[cfg(feature = "test-faults")]
                    let mounted = if reject_mount {
                        Err("injected post-native composer mount rejection".to_owned())
                    } else {
                        mount()
                    };
                    #[cfg(not(feature = "test-faults"))]
                    let mounted = mount();
                    match mounted {
                        Ok(composer) => {
                            controller.composer_mount = Some(composer);
                            cx.new(|_| MainWindowShellRoot {
                                controller: Some(controller),
                                construction_error: None,
                                composer_observer: None,
                            })
                        }
                        Err(error) => cx.new(|_| MainWindowShellRoot {
                            controller: Some(controller),
                            construction_error: Some(error),
                            composer_observer: None,
                        }),
                    }
                },
            )
            .map_err(|error| {
                let (controller, composer) = pending
                    .borrow_mut()
                    .take()
                    .expect("failed native construction leaves shell preparation intact");
                MainWindowShellHostFailure::BeforeConstruction {
                    error: error.to_string(),
                    prepared: MainWindowShellPrepared {
                        acquisition: controller.acquisition,
                        reservation: controller.reservation,
                        initial_composer: controller.initial_composer,
                        composer: composer.0,
                        composer_configurator: composer.1,
                        marker_seals: composer.2,
                        submission_request_source: composer.3,
                        appearance: controller.appearance,
                    },
                }
            })?;
        let construction = window
            .update(self.app, |root, window, _| {
                if let Some(error) = root.construction_error.take() {
                    window.remove_window();
                    Err((
                        error,
                        root.controller
                            .take()
                            .expect("failed shell root retains its controller"),
                    ))
                } else {
                    Ok(())
                }
            })
            .expect("a newly constructed hidden main-window shell retains its root");
        match construction {
            Ok(()) => {
                let root = window.entity(self.app).expect("new hidden shell root");
                root.update(self.app, |root, cx| {
                    let mount = root
                        .controller
                        .as_ref()
                        .and_then(|controller| controller.composer_mount.as_ref())
                        .expect("complete composer mount");
                    let input = mount
                        .read(cx)
                        .contribution()
                        .expect("ordinary contribution")
                        .read(cx)
                        .gpui_input();
                    root.composer_observer = Some(cx.observe(&input, |_, _, cx| cx.notify()));
                });
                let adapter_id = crate::theme_runtime::WindowAdapterId::new(
                    std::num::NonZeroU64::new(root.entity_id().as_u64())
                        .expect("GPUI entity identity"),
                );
                let registration = self.appearance_owner.update(self.app, |owner, cx| {
                    owner.register(
                        Box::new(appearance::ShellAppearanceAdapter {
                            id: adapter_id,
                            window,
                        }),
                        cx,
                    )
                });
                if let Err(error) = registration {
                    let controller = window
                        .update(self.app, |root, window, _| {
                            window.remove_window();
                            root.controller.take().expect("unpublished controller")
                        })
                        .expect("new hidden shell");
                    return Err(MainWindowShellHostFailure::Construction {
                        error: error.to_string(),
                        unpublished: controller.into_unpublished(),
                    });
                }
                Ok(MainWindowShell {
                    window,
                    root,
                    appearance_owner: self.appearance_owner.clone(),
                    adapter_id,
                    published: false,
                })
            }
            Err((error, controller)) => Err(MainWindowShellHostFailure::Construction {
                error,
                unpublished: MainWindowShellUnpublished {
                    acquisition: controller.acquisition,
                    reservation: controller.reservation,
                    initial_composer: controller.initial_composer,
                },
            }),
        }
    }
}

pub struct MainWindowShellController {
    acquisition: RuntimeBackedWindowAcquisition,
    reservation: RuntimeBackedWindowMainWindowReservation,
    initial_composer: Option<Box<InitialComposerCandidate>>,
    pub(super) appearance: MainWindowShellAppearance,
    selection: crate::main_window::MainWindowComposerSelectionIdentity,
    minimum_size: gpui::Size<gpui::Pixels>,
    pub(super) composer_mount: Option<Entity<MainWindowConversationComposerMount>>,
}

impl MainWindowShellController {
    pub fn minimum_size(&self) -> gpui::Size<gpui::Pixels> {
        self.minimum_size
    }
    #[must_use]
    pub const fn window_id(&self) -> beryl_model::WindowId {
        self.acquisition.window_id()
    }

    #[must_use]
    pub fn appearance(&self) -> &Arc<AppearanceGeneration> {
        &self.appearance.generation
    }

    #[must_use]
    pub fn composer_mount(&self) -> Option<Entity<MainWindowConversationComposerMount>> {
        self.composer_mount.clone()
    }

    #[must_use]
    fn into_unpublished(self) -> MainWindowShellUnpublished {
        MainWindowShellUnpublished {
            acquisition: self.acquisition,
            reservation: self.reservation,
            initial_composer: self.initial_composer,
        }
    }
}

pub struct MainWindowShell {
    window: WindowHandle<MainWindowShellRoot>,
    root: Entity<MainWindowShellRoot>,
    appearance_owner: Entity<GpuiAppearanceWindowSet>,
    adapter_id: crate::theme_runtime::WindowAdapterId,
    published: bool,
}

impl MainWindowShell {
    #[must_use]
    pub const fn window(&self) -> WindowHandle<MainWindowShellRoot> {
        self.window
    }

    #[must_use]
    pub const fn is_published(&self) -> bool {
        self.published
    }

    pub fn publish(&mut self, app: &mut App) -> Result<(), String> {
        if self.published {
            return Ok(());
        }
        use crate::theme_runtime::AppearancePublicationTarget;
        let appearance = self.appearance_owner.read(app).target().snapshot();
        let presentable = self
            .window
            .read_with(app, |root, app| {
                root.controller.as_ref().is_some_and(|controller| {
                    appearance.active
                        && Arc::ptr_eq(&appearance.current, &controller.appearance.generation)
                        && controller.composer_mount.as_ref().is_some_and(|mount| {
                            let mount = mount.read(app);
                            mount.selected_first_presentable(app)
                                && mount.contribution().is_some_and(|composer| {
                                    composer.read(app).selection_identity() == controller.selection
                                })
                        })
                })
            })
            .map_err(|error| error.to_string())?;
        if !presentable {
            return Err("hidden main-window composer is not first-presentable".to_owned());
        }
        self.window
            .update(app, |_, window, cx| window.publish(cx))
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        self.published = true;
        Ok(())
    }

    pub fn close_before_publication(
        self,
        app: &mut App,
    ) -> Result<MainWindowShellUnpublished, MainWindowShell> {
        if self.published {
            return Err(self);
        }
        self.appearance_owner
            .update(app, |owner, _| owner.unregister(self.adapter_id))
            .expect("bounded window-set epoch");
        let _ = self
            .window
            .update(app, |_, window, _| window.remove_window());
        let controller = self
            .root
            .update(app, |root, _| {
                root.controller
                    .take()
                    .expect("unpublished shell retains its controller")
            })
            .into_unpublished();
        Ok(controller)
    }
}

pub struct MainWindowShellRoot {
    pub(super) controller: Option<MainWindowShellController>,
    construction_error: Option<String>,
    composer_observer: Option<gpui::Subscription>,
}

impl MainWindowShellRoot {
    #[must_use]
    pub fn controller(&self) -> Option<&MainWindowShellController> {
        self.controller.as_ref()
    }
}

impl Render for MainWindowShellRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(controller) = self.controller.as_ref() else {
            return div().id("main-window-shell-empty").into_any_element();
        };
        let appearance = &controller.appearance;
        let composer = controller.composer_mount();
        let content_height = composer
            .as_ref()
            .and_then(|mount| mount.read(cx).contribution())
            .and_then(|composer| {
                composer
                    .read(cx)
                    .gpui_input()
                    .read(cx)
                    .surface()
                    .map(|surface| surface.content_height())
            })
            .unwrap_or(px(0.));
        let minimum_panel_height = controller.minimum_size.height * 0.5;
        let composer_height = (content_height + px(22.))
            .max(minimum_panel_height)
            .min((window.viewport_size().height * 0.5).max(minimum_panel_height));
        div()
            .id("main-window-shell")
            .size_full()
            .flex()
            .flex_col()
            .bg(appearance.background)
            .child(
                div()
                    .id("main-window-toolbar")
                    .h(px(0.))
                    .flex_none()
                    .bg(appearance.toolbar),
            )
            .child(
                div()
                    .id("main-window-conversation-body")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id("main-window-transcript-region")
                            .debug_selector(|| "main-window-transcript-region".to_owned())
                            .flex_1()
                            .min_h_0(),
                    )
                    .child(
                        div()
                            .id("main-window-user-input-panel")
                            .debug_selector(|| "main-window-user-input-panel".to_owned())
                            .flex()
                            .h(composer_height)
                            .min_h(minimum_panel_height)
                            .px(px(12.))
                            .py(px(10.))
                            .border_1()
                            .flex_none()
                            .bg(appearance.input_panel)
                            .border_color(appearance.separator)
                            .children(composer),
                    ),
            )
            .child(
                div()
                    .id("main-window-status-line")
                    .h(px(0.))
                    .flex_none()
                    .bg(appearance.status),
            )
            .into_any_element()
    }
}
