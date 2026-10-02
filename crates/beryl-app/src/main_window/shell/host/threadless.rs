use super::*;
use crate::main_window::ThreadlessWindowSource;

pub struct ThreadlessWindowShellPrepared {
    source: ThreadlessWindowSource,
    reservation: RuntimeBackedWindowMainWindowReservation,
    appearance: MainWindowShellAppearance,
}

pub(super) struct ThreadlessShellBeforeFailure {
    pub(super) error: String,
    pub(super) prepared: ThreadlessWindowShellPrepared,
}

impl ThreadlessWindowShellPrepared {
    #[cfg(target_os = "windows")]
    pub(in crate::main_window) fn native_validation(
        &self,
    ) -> crate::main_window::restoration::NativeMemberValidation {
        self.source.native_validation()
    }

    pub fn new(
        source: ThreadlessWindowSource,
        registry: &RuntimeBackedWindowProcessRegistry,
        appearance: Arc<AppearanceGeneration>,
    ) -> Result<Self, String> {
        source.revalidate()?;
        let home = appearance.prepared().home();
        if source.home_id() != home.home_id() || source.home_generation() != home.home_generation()
        {
            return Err(
                "threadless shell appearance belongs to another home generation".to_owned(),
            );
        }
        let reservation = registry
            .reserve_main_window(source.window_id())
            .map_err(|error| format!("threadless shell reservation rejected: {error:?}"))?;
        Ok(Self {
            source,
            reservation,
            appearance: MainWindowShellAppearance::prepare(appearance),
        })
    }

    pub fn window_id(&self) -> beryl_model::WindowId {
        self.source.window_id()
    }

    pub fn placement(&self) -> &beryl_model::WindowPlacement {
        self.source.placement()
    }

    pub fn revalidate(&self) -> Result<(), String> {
        self.source.revalidate()
    }
}

impl GpuiMainWindowShellHost<'_> {
    pub fn construct_threadless_hidden(
        &mut self,
        prepared: ThreadlessWindowShellPrepared,
    ) -> Result<MainWindowShell, String> {
        let construction = self.construction(false);
        let shell = self
            .allocate_threadless_hidden(prepared, construction)
            .map_err(|failure| failure.error)?;
        if let Some(error) = shell.root.read(self.app).construction_error.clone() {
            let _ = shell.window.update(self.app, |root, window, cx| {
                root.retire_notices(window, cx);
                root.controller.take();
                window.remove_window();
            });
            return Err(error);
        }
        Ok(shell)
    }

    pub(super) fn allocate_threadless_hidden(
        &mut self,
        prepared: ThreadlessWindowShellPrepared,
        construction: construction::ShellConstruction,
    ) -> Result<MainWindowShell, ThreadlessShellBeforeFailure> {
        use crate::theme_runtime::AppearancePublicationTarget;
        if let Err(error) = prepared.source.validate_lifetime() {
            return Err(ThreadlessShellBeforeFailure { error, prepared });
        }
        let publication = self.appearance_owner.read(self.app).target();
        let snapshot = publication.snapshot();
        if !snapshot.active || !Arc::ptr_eq(&snapshot.current, &prepared.appearance.generation) {
            return Err(ThreadlessShellBeforeFailure {
                error: "threadless shell appearance is no longer current".to_owned(),
                prepared,
            });
        }
        let minimum_size = gpui::size(px(160.), px(64.));
        let mut options = WindowOptions {
            show: false,
            focus: false,
            ..Default::default()
        };
        #[cfg(target_os = "windows")]
        if let Err(error) = self.apply_prepared_placement(
            &mut options,
            prepared.window_id(),
            prepared.placement(),
            true,
        ) {
            return Err(ThreadlessShellBeforeFailure { error, prepared });
        }
        options.window_min_size = Some(minimum_size);
        let pending = Rc::new(RefCell::new(Some(MainWindowShellController {
            identity: Rc::new(()),
            content: ShellContent::Threadless {
                source: prepared.source,
                reservation: prepared.reservation,
            },
            appearance: prepared.appearance,
            minimum_size,
            composer_mount: None,
        })));
        let root_pending = pending.clone();
        #[cfg(target_os = "windows")]
        let admission = Rc::new(RefCell::new(None));
        #[cfg(target_os = "windows")]
        let root_admission = admission.clone();
        let window = self
            .app
            .open_window(options, move |window, cx| {
                #[cfg(target_os = "windows")]
                let error = construction.begin_native(window, cx, &root_admission);
                #[cfg(not(target_os = "windows"))]
                let error = None;
                let controller = root_pending
                    .borrow_mut()
                    .take()
                    .expect("one threadless root construction");
                cx.new(|cx| {
                    let mut root =
                        MainWindowShellRoot::new(controller, error, publication, window, cx);
                    if construction.startup {
                        let gate = root.initialize_startup_interaction(window, cx, true);
                        #[cfg(target_os = "windows")]
                        if let Some(admission) = root_admission.borrow_mut().as_mut() {
                            admission.gate_proven = gate.is_ok();
                        }
                        if let Err(error) = gate {
                            root.construction_error.get_or_insert(error);
                        }
                    }
                    root
                })
            })
            .map_err(|error| {
                let controller = pending
                    .borrow_mut()
                    .take()
                    .expect("failed allocation retains threadless preparation");
                let ShellContent::Threadless {
                    source,
                    reservation,
                } = controller.content
                else {
                    unreachable!("threadless allocation preserves kind")
                };
                ThreadlessShellBeforeFailure {
                    error: error.to_string(),
                    prepared: ThreadlessWindowShellPrepared {
                        source,
                        reservation,
                        appearance: controller.appearance,
                    },
                }
            })?;
        Ok(self.complete_hidden_shell(
            window,
            construction,
            #[cfg(target_os = "windows")]
            admission.borrow_mut().take(),
        ))
    }
}

impl MainWindowShell {
    pub fn close_threadless_before_publication(self, app: &mut App) -> Result<(), Self> {
        #[cfg(target_os = "windows")]
        if !self.desktop_cleanup_allowed() || self.startup_disposal.is_some() {
            return Err(self);
        }
        if self.published
            || !self
                .root
                .read(app)
                .controller
                .as_ref()
                .is_some_and(|controller| controller.is_threadless())
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
        self.root.update(app, |root, _| {
            root.controller.take();
        });
        Ok(())
    }
}
