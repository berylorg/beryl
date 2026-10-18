use super::*;
use crate::theme_runtime::{
    AdapterFailureClass, AppearancePublicationTarget, GpuiAppearancePublicationTarget,
    PreparedWindowAppearance, WindowAdapterId,
};

struct PreparedRecoveryAppearance {
    window: WindowHandle<MainWindowShellRoot>,
    owner: Entity<GpuiAppearanceWindowSet>,
    target: Arc<GpuiAppearancePublicationTarget>,
    id: WindowAdapterId,
    appearance: MainWindowShellAppearance,
    retained_handle: bool,
    metrics: sizing::ShellMetrics,
    binding_guard: Option<std::rc::Rc<dyn Fn(&mut App) -> Result<(), String>>>,
    binding_commit: Option<std::rc::Rc<dyn Fn(&mut App)>>,
}

impl MainWindowShell {
    #[cfg(test)]
    pub(crate) fn bind_interrupted_exit_appearance(
        &mut self,
        owner: &Entity<GpuiAppearanceWindowSet>,
        app: &mut App,
    ) -> Result<(), String> {
        self.require_interrupted_exit_appearance_custody()?;
        MainWindowShellRoot::register_interrupted_exit_appearance(self.window, owner, true, app)?;
        self.appearance_owner = owner.clone();
        Ok(())
    }

    pub(crate) fn require_interrupted_exit_appearance_custody(&self) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        if !self.startup_publication_allowed() {
            return Err("Recovery shell disposal is pending".into());
        }
        if !self.published || !self.appearance_registered.get() {
            return Err("Recovery shell has no published appearance custody".into());
        }
        Ok(())
    }

    pub(crate) fn begin_bind_interrupted_exit_appearance(
        &self,
        owner: &Entity<GpuiAppearanceWindowSet>,
        binding_guard: std::rc::Rc<dyn Fn(&mut App) -> Result<(), String>>,
        binding_commit: std::rc::Rc<dyn Fn(&mut App)>,
        app: &mut App,
    ) -> Result<gpui::Task<Result<(), crate::theme_runtime::AdapterRegistrationError>>, String>
    {
        self.require_interrupted_exit_appearance_custody()?;
        MainWindowShellRoot::begin_interrupted_exit_appearance_registration(
            self.window,
            owner,
            binding_guard,
            binding_commit,
            app,
        )
    }

    pub(crate) fn complete_interrupted_exit_appearance_binding(
        &mut self,
        owner: Entity<GpuiAppearanceWindowSet>,
    ) {
        self.appearance_owner = owner;
    }
}

impl MainWindowShellRoot {
    #[cfg(test)]
    pub(crate) fn bind_interrupted_exit_appearance(
        window: WindowHandle<Self>,
        owner: &Entity<GpuiAppearanceWindowSet>,
        app: &mut App,
    ) -> Result<(), String> {
        Self::register_interrupted_exit_appearance(window, owner, false, app)
    }

    #[cfg(test)]
    fn register_interrupted_exit_appearance(
        window: WindowHandle<Self>,
        owner: &Entity<GpuiAppearanceWindowSet>,
        retained_handle: bool,
        app: &mut App,
    ) -> Result<(), String> {
        let id = window
            .update(app, |_, _, cx| {
                WindowAdapterId::new(std::num::NonZeroU64::new(cx.entity_id().as_u64()).unwrap())
            })
            .map_err(|error| error.to_string())?;
        let target = owner.read(app).target();
        owner.update(app, |set, cx| {
            set.register_with(
                Box::new(
                    crate::main_window::shell::appearance::ShellAppearanceAdapter { id, window },
                ),
                |_, generation, app| {
                    Self::prepare_interrupted_exit_appearance(
                        window,
                        owner.clone(),
                        target,
                        id,
                        generation,
                        retained_handle,
                        None,
                        None,
                        app,
                    )
                },
                cx,
            )
            .map_err(|error| error.to_string())
        })
    }

    fn begin_interrupted_exit_appearance_registration(
        window: WindowHandle<Self>,
        owner: &Entity<GpuiAppearanceWindowSet>,
        binding_guard: std::rc::Rc<dyn Fn(&mut App) -> Result<(), String>>,
        binding_commit: std::rc::Rc<dyn Fn(&mut App)>,
        app: &mut App,
    ) -> Result<gpui::Task<Result<(), crate::theme_runtime::AdapterRegistrationError>>, String>
    {
        let id = window
            .update(app, |_, _, cx| {
                WindowAdapterId::new(std::num::NonZeroU64::new(cx.entity_id().as_u64()).unwrap())
            })
            .map_err(|error| error.to_string())?;
        let target = owner.read(app).target();
        let expected = target.snapshot();
        let retained_owner = owner.clone();
        Ok(GpuiAppearanceWindowSet::begin_registration_with(
            owner.clone(),
            Box::new(crate::main_window::shell::appearance::ShellAppearanceAdapter { id, window }),
            move |_, generation, app| {
                let current = target.snapshot();
                if !current.active
                    || current.epoch != expected.epoch
                    || !Arc::ptr_eq(&generation, &expected.current)
                {
                    return Err(AdapterFailureClass::Unavailable);
                }
                binding_guard(app).map_err(|_| AdapterFailureClass::Unavailable)?;
                Self::prepare_interrupted_exit_appearance(
                    window,
                    retained_owner.clone(),
                    target.clone(),
                    id,
                    generation,
                    true,
                    Some(binding_guard.clone()),
                    Some(binding_commit.clone()),
                    app,
                )
            },
            app,
        ))
    }

    fn prepare_interrupted_exit_appearance(
        window: WindowHandle<Self>,
        owner: Entity<GpuiAppearanceWindowSet>,
        target: Arc<GpuiAppearancePublicationTarget>,
        id: WindowAdapterId,
        generation: Arc<AppearanceGeneration>,
        retained_handle: bool,
        binding_guard: Option<std::rc::Rc<dyn Fn(&mut App) -> Result<(), String>>>,
        binding_commit: Option<std::rc::Rc<dyn Fn(&mut App)>>,
        app: &mut App,
    ) -> Result<Box<dyn PreparedWindowAppearance>, AdapterFailureClass> {
        let metrics = window
            .read_with(app, |root, app| {
                let controller = root
                    .controller
                    .as_ref()
                    .ok_or(AdapterFailureClass::Unavailable)?;
                root.candidate_metrics(
                    &generation,
                    controller.metrics.panel,
                    controller.metrics.lineage,
                    app,
                )
                .map_err(|_| AdapterFailureClass::Unavailable)
            })
            .map_err(|_| AdapterFailureClass::Unavailable)??;
        Ok(Box::new(PreparedRecoveryAppearance {
            window,
            owner,
            target,
            id,
            appearance: MainWindowShellAppearance::prepare(generation),
            retained_handle,
            metrics,
            binding_guard,
            binding_commit,
        }))
    }
}

impl PreparedWindowAppearance for PreparedRecoveryAppearance {
    fn prepare_native(
        &mut self,
        app: &mut App,
    ) -> Result<Option<gpui::Task<Result<(), AdapterFailureClass>>>, AdapterFailureClass> {
        let ready = self
            .window
            .update(app, |root, window, cx| {
                root.prepare_appearance_minimum(self.metrics.clone(), window, cx)
            })
            .map_err(|_| AdapterFailureClass::Unavailable)?
            .map_err(|_| AdapterFailureClass::Unavailable)?;
        if self
            .window
            .read_with(app, |root, _| root.minimum_current(&self.metrics))
            .map_err(|_| AdapterFailureClass::Unavailable)?
        {
            return Ok(None);
        }
        Ok(Some(app.spawn(async move |_| {
            ready
                .await
                .map_err(|_| AdapterFailureClass::ApplicationFailed)
        })))
    }

    fn validate(&self, app: &mut App) -> Result<(), AdapterFailureClass> {
        if let Some(guard) = &self.binding_guard {
            guard(app).map_err(|_| AdapterFailureClass::Unavailable)?;
        }
        self.window
            .update(app, |root, window, app| {
                let reject = AdapterFailureClass::Rejected;
                let controller = root.controller.as_ref().ok_or(reject)?;
                if !root.minimum_current(&self.metrics)
                    || window.is_minimized().is_none()
                    || !sizing::contains(window.viewport_size(), self.metrics.minimum())
                    || (self.metrics != controller.metrics && window.is_minimized() != Some(false))
                    || root
                        .candidate_metrics(
                            &self.appearance.generation,
                            controller.metrics.panel,
                            controller.metrics.lineage,
                            app,
                        )
                        .map_err(|_| AdapterFailureClass::Unavailable)?
                        != self.metrics
                {
                    return Err(AdapterFailureClass::Unavailable);
                }
                if !root.shutdown_interaction_gated
                    || root.startup_interaction_gated()
                    || (!self.retained_handle && root.appearance_release.is_none())
                    || !root.notices.publication_retired()
                {
                    return Err(reject);
                }
                let (home, generation) = match &controller.content {
                    ShellContent::Selected { selection, .. } => (
                        selection.binding().home_id(),
                        selection.binding().home_generation(),
                    ),
                    ShellContent::RecoveredThreadless { source, .. } => {
                        (source.home_id(), source.generation())
                    }
                    _ => return Err(reject),
                };
                let next = self.appearance.generation.prepared().home();
                let previous = controller.appearance.generation.prepared().home();
                let target = self.target.snapshot();
                if next.home_id() != home
                    || next.home_generation() != generation
                    || previous.home_id() != home
                    || previous.home_generation() == generation
                    || !target.active
                    || !Arc::ptr_eq(&target.current, &self.appearance.generation)
                {
                    return Err(reject);
                }
                match controller.composer_mount.as_ref() {
                    None if controller.is_threadless() => Ok(()),
                    Some(mount) if !controller.is_threadless() => {
                        let resident = mount.read(app).contribution().ok_or(reject)?;
                        resident
                            .read(app)
                            .appearance_applicable(app)
                            .then_some(())
                            .ok_or(reject)
                    }
                    _ => Err(reject),
                }
            })
            .map_err(|_| AdapterFailureClass::Unavailable)?
    }

    fn commit(self: Box<Self>, app: &mut App) {
        let window = self.window;
        let binding_commit = self.binding_commit.clone();
        window
            .update(app, |root, window, cx| {
                let controller = root.controller.as_mut().expect("validated recovery shell");
                if let Some(mount) = controller.composer_mount.as_ref() {
                    mount
                        .update(cx, |mount, cx| {
                            mount.apply_appearance(
                                self.appearance.text.clone(),
                                self.appearance.scrollbar,
                                cx,
                            )
                        })
                        .expect("validated recovered editor accepts appearance");
                }
                controller.appearance = self.appearance;
                root.commit_metrics(self.metrics, window, cx);
                root.appearance_registration.set(true);
                root.replace_recovered_notices(self.target, window, cx);
                let owner = self.owner;
                let id = self.id;
                root.appearance_release = Some(cx.on_release(move |root, app| {
                    let _ = owner.update(app, |owner, _| owner.unregister(id));
                    if let Some(creation) = root.creation.as_ref().and_then(|owner| owner.upgrade())
                    {
                        creation.update(app, |_, cx| cx.notify());
                    }
                }));
                cx.notify();
            })
            .expect("validated recovery window");
        if let Some(commit) = binding_commit {
            commit(app);
        }
    }
}
