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
}

impl MainWindowShellRoot {
    pub(crate) fn bind_interrupted_exit_appearance(
        window: WindowHandle<Self>,
        owner: &Entity<GpuiAppearanceWindowSet>,
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
                |_, generation, _| {
                    Ok(Box::new(PreparedRecoveryAppearance {
                        window,
                        owner: owner.clone(),
                        target,
                        id,
                        appearance: MainWindowShellAppearance::prepare(generation),
                    }))
                },
                cx,
            )
            .map_err(|error| error.to_string())
        })
    }
}

impl PreparedWindowAppearance for PreparedRecoveryAppearance {
    fn validate(&self, app: &App) -> Result<(), AdapterFailureClass> {
        self.window
            .read_with(app, |root, app| {
                let reject = AdapterFailureClass::Rejected;
                let controller = root.controller.as_ref().ok_or(reject)?;
                if !root.shutdown_interaction_gated
                    || root.startup_interaction_gated()
                    || root.appearance_release.is_none()
                    || !root.notices.publication_retired()
                {
                    return Err(reject);
                }
                let (home, generation) = match &controller.content {
                    ShellContent::Recovered { selection, .. } => (
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
                            .appearance_applicable()
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
    }
}
