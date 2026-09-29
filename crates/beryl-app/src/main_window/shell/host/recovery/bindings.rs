use super::*;
use crate::theme_runtime::{AppearancePublicationTarget, GpuiAppearancePublicationTarget};

impl MainWindowShell {
    pub(crate) fn validate_interrupted_exit_binding(
        &self,
        draft: &MainWindowShutdownDraft,
        appearance: &Entity<GpuiAppearanceWindowSet>,
        app: &mut App,
    ) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        if !self.startup_publication_allowed() {
            return Err("Recovery shell disposal is pending".into());
        }
        if !self.published || !self.appearance_registered || self.appearance_owner != *appearance {
            return Err("Recovery shell appearance ownership is incomplete".into());
        }
        let target = appearance.read(app).target();
        self.window
            .update(app, |root, _, cx| {
                root.validate_interrupted_exit_binding(draft, &target, cx)
            })
            .map_err(|error| error.to_string())?
    }
}

impl MainWindowShellRoot {
    pub(crate) fn validate_interrupted_exit_binding(
        &self,
        draft: &MainWindowShutdownDraft,
        target: &Arc<GpuiAppearancePublicationTarget>,
        cx: &Context<Self>,
    ) -> Result<(), String> {
        if draft.root != cx.entity_id()
            || draft.retirement.is_some()
            || !self.shutdown_interaction_gated
            || self.startup_interaction_gated()
            || self.appearance_release.is_none()
        {
            return Err("Recovery shell lost its exact gated draft or release custody".into());
        }
        let controller = self
            .controller
            .as_ref()
            .ok_or("Recovery shell has no controller")?;
        let snapshot = target.snapshot();
        if !snapshot.active
            || !Arc::ptr_eq(&snapshot.current, &controller.appearance.generation)
            || !self
                .notices
                .recovery_binding_current(target, controller.window_id())
        {
            return Err("Recovery shell appearance or notice binding is stale".into());
        }
        controller.validate_recovered_appearance()?;
        match (
            &controller.content,
            &controller.composer_mount,
            &draft.composer,
        ) {
            (ShellContent::RecoveredThreadless { .. }, None, None) => Ok(()),
            (
                ShellContent::Recovered { selection, .. },
                Some(mount),
                Some((expected, editor, close)),
            ) if mount == expected && close.selection() == *selection => {
                let resident = mount
                    .read(cx)
                    .contribution()
                    .ok_or("Recovery resident is missing")?;
                if resident.entity_id() != *editor
                    || !resident.read(cx).recovery_binding_current(*close)
                    || !mount.read(cx).recovery_binding_current(*close)
                {
                    return Err("Recovery resident binding is stale".into());
                }
                Ok(())
            }
            _ => Err("Recovery shell binding is incomplete".into()),
        }
    }
}
