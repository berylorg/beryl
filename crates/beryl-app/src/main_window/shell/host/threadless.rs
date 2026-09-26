use super::*;
use crate::main_window::ThreadlessWindowSource;

pub struct ThreadlessWindowShellPrepared {
    source: ThreadlessWindowSource,
    reservation: RuntimeBackedWindowMainWindowReservation,
    appearance: MainWindowShellAppearance,
}

impl ThreadlessWindowShellPrepared {
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
        use crate::theme_runtime::AppearancePublicationTarget;
        prepared.source.validate_lifetime()?;
        let publication = self.appearance_owner.read(self.app).target();
        let snapshot = publication.snapshot();
        if !snapshot.active || !Arc::ptr_eq(&snapshot.current, &prepared.appearance.generation) {
            return Err("threadless shell appearance is no longer current".to_owned());
        }
        let minimum_size = gpui::size(px(160.), px(64.));
        let mut options = WindowOptions {
            show: false,
            focus: false,
            ..Default::default()
        };
        #[cfg(target_os = "windows")]
        self.apply_prepared_placement(
            &mut options,
            prepared.window_id(),
            prepared.placement(),
            true,
        )?;
        options.window_min_size = Some(minimum_size);
        let pending = Rc::new(RefCell::new(Some(MainWindowShellController {
            content: ShellContent::Threadless {
                source: prepared.source,
                reservation: prepared.reservation,
            },
            appearance: prepared.appearance,
            minimum_size,
            composer_mount: None,
        })));
        let root_pending = pending.clone();
        let window = self
            .app
            .open_window(options, move |window, cx| {
                let controller = root_pending
                    .borrow_mut()
                    .take()
                    .expect("one threadless root construction");
                cx.new(|cx| MainWindowShellRoot::new(controller, None, publication, window, cx))
            })
            .map_err(|error| error.to_string())?;
        let root = window.entity(self.app).expect("new hidden threadless root");
        let adapter_id = crate::theme_runtime::WindowAdapterId::new(
            std::num::NonZeroU64::new(root.entity_id().as_u64()).expect("GPUI entity identity"),
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
            let _ = window.update(self.app, |root, window, cx| {
                root.retire_notices(window, cx);
                root.controller.take();
                window.remove_window();
            });
            return Err(error.to_string());
        }
        Ok(MainWindowShell {
            window,
            root,
            appearance_owner: self.appearance_owner.clone(),
            adapter_id,
            published: false,
            #[cfg(target_os = "windows")]
            startup_disposal: None,
            #[cfg(target_os = "windows")]
            desktop_placement: None,
            #[cfg(all(target_os = "windows", feature = "test-faults"))]
            desktop_worker_gate: None,
        })
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
