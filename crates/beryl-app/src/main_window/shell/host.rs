use super::*;
use crate::main_window::{MainWindowCreationOwner, NewWindow};

mod threadless;
pub use threadless::*;
mod restored;
mod selected;
mod startup;
pub use restored::*;
#[cfg(target_os = "windows")]
mod desktop_flight;
#[cfg(target_os = "windows")]
pub use desktop_flight::*;

enum ShellContent {
    Acquired {
        custody: MainWindowShellUnpublished,
        selection: crate::main_window::MainWindowComposerSelectionIdentity,
    },
    Threadless {
        source: crate::main_window::ThreadlessWindowSource,
        reservation: RuntimeBackedWindowMainWindowReservation,
    },
    Restored {
        custody: Box<RestoredWindowShellUnpublished>,
        selection: crate::main_window::MainWindowComposerSelectionIdentity,
    },
}

struct SelectedShellPrepared {
    content: ShellContent,
    composer: MainWindowConversationComposerPreparedSelection,
    composer_configurator: MainWindowShellComposerConfigurator,
    marker_seals: DraftMarkerSealService,
    submission_request_source: MainWindowComposerSubmissionRequestSource,
    appearance: MainWindowShellAppearance,
}

enum SelectedShellHostFailure {
    BeforeConstruction {
        error: String,
        prepared: SelectedShellPrepared,
    },
    Construction {
        error: String,
        controller: MainWindowShellController,
    },
}

pub struct GpuiMainWindowShellHost<'a> {
    app: &'a mut App,
    appearance_owner: Entity<GpuiAppearanceWindowSet>,
    #[cfg(target_os = "windows")]
    prepared_placement: Option<crate::main_window::PreparedWindowsWindowPlacement>,
    #[cfg(feature = "test-faults")]
    reject_mount: bool,
}

impl<'a> GpuiMainWindowShellHost<'a> {
    #[must_use]
    pub fn new(app: &'a mut App, appearance_owner: Entity<GpuiAppearanceWindowSet>) -> Self {
        Self {
            app,
            appearance_owner,
            #[cfg(target_os = "windows")]
            prepared_placement: None,
            #[cfg(feature = "test-faults")]
            reject_mount: false,
        }
    }

    #[cfg(target_os = "windows")]
    pub fn with_prepared_placement(
        mut self,
        placement: crate::main_window::PreparedWindowsWindowPlacement,
    ) -> Self {
        self.prepared_placement = Some(placement);
        self
    }

    #[cfg(feature = "test-faults")]
    pub fn test_reject_mount_after_native(&mut self) {
        self.reject_mount = true;
    }
}

impl GpuiMainWindowShellHost<'_> {
    #[cfg(target_os = "windows")]
    fn apply_prepared_placement(
        &self,
        options: &mut WindowOptions,
        window_id: beryl_model::WindowId,
        saved: &beryl_model::WindowPlacement,
        required: bool,
    ) -> Result<(), String> {
        let Some(placement) = &self.prepared_placement else {
            return if required {
                Err("startup shell construction requires prepared window placement".to_owned())
            } else {
                Ok(())
            };
        };
        placement
            .validate_binding(window_id, saved)
            .map_err(|error| error.to_string())?;
        let bounds = placement
            .gpui_window_bounds()
            .map_err(|error| error.to_string())?;
        options.window_bounds = Some(bounds);
        options.windows_outer_bounds_monitor = Some(placement.monitor());
        Ok(())
    }

    fn construct_selected_hidden(
        &mut self,
        prepared: SelectedShellPrepared,
    ) -> Result<MainWindowShell, SelectedShellHostFailure> {
        let mut options = WindowOptions {
            show: false,
            focus: false,
            ..Default::default()
        };
        #[cfg(target_os = "windows")]
        {
            let (window_id, saved, required) = match &prepared.content {
                ShellContent::Acquired { custody, .. } => {
                    (custody.window_id(), custody.acquisition.placement(), false)
                }
                ShellContent::Restored { custody, .. } => {
                    (custody.window_id(), custody.composer.placement(), true)
                }
                ShellContent::Threadless { source, .. } => {
                    (source.window_id(), source.placement(), true)
                }
            };
            if let Err(error) =
                self.apply_prepared_placement(&mut options, window_id, saved, required)
            {
                return Err(SelectedShellHostFailure::BeforeConstruction { error, prepared });
            }
        }
        let SelectedShellPrepared {
            content,
            composer,
            composer_configurator,
            marker_seals,
            submission_request_source,
            appearance,
        } = prepared;
        let minimum_size = composer.shell_minimum_size();
        options.window_min_size = Some(minimum_size);
        let pending = Rc::new(RefCell::new(Some((
            MainWindowShellController {
                content,
                appearance,
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
        let publication = self.appearance_owner.read(self.app).target();
        #[cfg(feature = "test-faults")]
        let reject_mount = std::mem::take(&mut self.reject_mount);
        let window = self
            .app
            .open_window(options, move |window, cx| {
                let (mut controller, composer) = root_pending
                    .borrow_mut()
                    .take()
                    .expect("main-window shell host invokes its root constructor once");
                let (composer, configurator, marker_seals, submission_request_source) = composer;
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
                        cx.new(|cx| {
                            MainWindowShellRoot::new(controller, None, publication, window, cx)
                        })
                    }
                    Err(error) => cx.new(|cx| {
                        MainWindowShellRoot::new(controller, Some(error), publication, window, cx)
                    }),
                }
            })
            .map_err(|error| {
                let (controller, composer) = pending
                    .borrow_mut()
                    .take()
                    .expect("failed native construction leaves shell preparation intact");
                SelectedShellHostFailure::BeforeConstruction {
                    error: error.to_string(),
                    prepared: SelectedShellPrepared {
                        content: controller.content,
                        composer: composer.0,
                        composer_configurator: composer.1,
                        marker_seals: composer.2,
                        submission_request_source: composer.3,
                        appearance: controller.appearance,
                    },
                }
            })?;
        let construction = window
            .update(self.app, |root, window, cx| {
                if let Some(error) = root.construction_error.take() {
                    root.retire_notices(window, cx);
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
                        .update(self.app, |root, window, cx| {
                            root.retire_notices(window, cx);
                            window.remove_window();
                            root.controller.take().expect("unpublished controller")
                        })
                        .expect("new hidden shell");
                    return Err(SelectedShellHostFailure::Construction {
                        error: error.to_string(),
                        controller,
                    });
                }
                Ok(MainWindowShell {
                    window,
                    root,
                    appearance_owner: self.appearance_owner.clone(),
                    adapter_id,
                    published: false,
                    #[cfg(target_os = "windows")]
                    desktop_placement: None,
                    #[cfg(all(target_os = "windows", feature = "test-faults"))]
                    desktop_worker_gate: None,
                })
            }
            Err((error, controller)) => {
                Err(SelectedShellHostFailure::Construction { error, controller })
            }
        }
    }
}

pub struct MainWindowShellController {
    content: ShellContent,
    pub(super) appearance: MainWindowShellAppearance,
    minimum_size: gpui::Size<gpui::Pixels>,
    pub(super) composer_mount: Option<Entity<MainWindowConversationComposerMount>>,
}

impl MainWindowShellController {
    pub fn acquisition(&self) -> Option<&RuntimeBackedWindowAcquisition> {
        match &self.content {
            ShellContent::Acquired { custody, .. } => Some(&custody.acquisition),
            ShellContent::Threadless { .. } | ShellContent::Restored { .. } => None,
        }
    }

    pub fn is_threadless(&self) -> bool {
        matches!(self.content, ShellContent::Threadless { .. })
    }

    pub fn placement(&self) -> &beryl_model::WindowPlacement {
        match &self.content {
            ShellContent::Acquired { custody, .. } => custody.acquisition.placement(),
            ShellContent::Threadless { source, .. } => source.placement(),
            ShellContent::Restored { custody, .. } => custody.composer.placement(),
        }
    }

    pub fn minimum_size(&self) -> gpui::Size<gpui::Pixels> {
        self.minimum_size
    }
    #[must_use]
    pub fn window_id(&self) -> beryl_model::WindowId {
        match &self.content {
            ShellContent::Acquired { custody, .. } => custody.window_id(),
            ShellContent::Threadless { source, .. } => source.window_id(),
            ShellContent::Restored { custody, .. } => custody.composer.window_id(),
        }
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
        match self.content {
            ShellContent::Acquired { custody, .. } => custody,
            ShellContent::Threadless { .. } | ShellContent::Restored { .. } => {
                unreachable!("only acquired shells have acquisition abandonment")
            }
        }
    }
}

pub struct MainWindowShell {
    window: WindowHandle<MainWindowShellRoot>,
    root: Entity<MainWindowShellRoot>,
    appearance_owner: Entity<GpuiAppearanceWindowSet>,
    adapter_id: crate::theme_runtime::WindowAdapterId,
    published: bool,
    #[cfg(target_os = "windows")]
    desktop_placement: Option<desktop_flight::ShellDesktopPlacementAdmission>,
    #[cfg(all(target_os = "windows", feature = "test-faults"))]
    desktop_worker_gate: Option<std::sync::mpsc::Receiver<()>>,
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
        #[cfg(target_os = "windows")]
        if !self.desktop_publication_allowed() {
            return Err(
                "main-window desktop placement is pending, rejected or cancelled".to_owned(),
            );
        }
        if !self.ready_to_publish(app) {
            return Err("hidden main-window composer is not first-presentable".to_owned());
        }
        self.window
            .update(app, |_, window, cx| window.publish(cx))
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        self.published = true;
        Ok(())
    }

    pub fn ready_to_publish(&self, app: &mut App) -> bool {
        #[cfg(target_os = "windows")]
        if !self.desktop_publication_allowed() || !self.desktop_native_publication_allowed(app) {
            return false;
        }
        use crate::theme_runtime::AppearancePublicationTarget;
        let appearance = self.appearance_owner.read(app).target().snapshot();
        self.window
            .read_with(app, |root, app| {
                root.controller.as_ref().is_some_and(|controller| {
                    root.startup_interaction_ready(app)
                        && appearance.active
                        && Arc::ptr_eq(&appearance.current, &controller.appearance.generation)
                        && match &controller.content {
                            ShellContent::Threadless { source, .. } => {
                                source.validate_lifetime().is_ok()
                                    && controller.composer_mount.is_none()
                            }
                            ShellContent::Acquired { selection, .. }
                            | ShellContent::Restored { selection, .. } => {
                                if let ShellContent::Restored { custody, .. } = &controller.content
                                {
                                    if custody.composer.validate_shell_lifetime().is_err() {
                                        return false;
                                    }
                                }
                                controller.composer_mount.as_ref().is_some_and(|mount| {
                                    let mount = mount.read(app);
                                    mount.selected_first_presentable(app)
                                        && mount.contribution().is_some_and(|composer| {
                                            composer.read(app).selection_identity() == *selection
                                        })
                                })
                            }
                        }
                })
            })
            .unwrap_or(false)
    }

    pub fn attach_creation(&self, owner: Entity<MainWindowCreationOwner>, app: &mut App) {
        self.root.update(app, |root, cx| {
            root.creation = Some(owner.downgrade());
            root.creation_observer = Some(cx.observe(&owner, |_, _, cx| cx.notify()));
            cx.notify();
        });
    }

    pub fn release_published_handle(self, app: &mut App) -> Result<(), Self> {
        if !self.published || self.root.read(app).startup_interaction_gated() {
            return Err(self);
        }
        let owner = self.appearance_owner.clone();
        let adapter_id = self.adapter_id;
        self.root.update(app, |root, cx| {
            root.appearance_release = Some(cx.on_release(move |root, app| {
                let _ = owner.update(app, |owner, _| owner.unregister(adapter_id));
                if let Some(creation) = root.creation.as_ref().and_then(|owner| owner.upgrade()) {
                    creation.update(app, |_, cx| cx.notify());
                }
            }));
        });
        Ok(())
    }

    pub fn close_before_publication(
        self,
        app: &mut App,
    ) -> Result<MainWindowShellUnpublished, MainWindowShell> {
        #[cfg(target_os = "windows")]
        if !self.desktop_cleanup_allowed() {
            return Err(self);
        }
        if self.published
            || self
                .root
                .read(app)
                .controller
                .as_ref()
                .is_none_or(|controller| {
                    !matches!(controller.content, ShellContent::Acquired { .. })
                })
        {
            return Err(self);
        }
        self.appearance_owner
            .update(app, |owner, _| owner.unregister(self.adapter_id))
            .expect("bounded window-set epoch");
        let _ = self.window.update(app, |root, window, cx| {
            root.retire_notices(window, cx);
            window.remove_window();
        });
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
    startup_interaction: Option<Rc<std::cell::Cell<bool>>>,
    pub(super) controller: Option<MainWindowShellController>,
    construction_error: Option<String>,
    composer_observer: Option<gpui::Subscription>,
    pub(super) creation: Option<gpui::WeakEntity<MainWindowCreationOwner>>,
    creation_observer: Option<gpui::Subscription>,
    appearance_release: Option<gpui::Subscription>,
    command_focus: gpui::FocusHandle,
    pub(super) shell_focus: gpui::FocusHandle,
    pub(super) notices: notices::MainWindowShellNotices,
}

mod root;
