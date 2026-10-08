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
        #[cfg(target_os = "windows")]
        if !self.nonfinal_native_recovery_allowed() {
            return Err("Recovery shell native destruction is unresolved".into());
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
    pub(crate) fn prepare_interrupted_exit_mount(
        &mut self,
        draft: &MainWindowShutdownDraft,
        target: &Arc<GpuiAppearancePublicationTarget>,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        self.validate_interrupted_exit_binding(draft, target, cx)?;
        let Some((mount, _, close)) = &draft.composer else {
            return Ok(true);
        };
        mount.update(cx, |mount, cx| {
            mount.prepare_interrupted_exit_mount(*close, cx)
        })
    }

    pub(crate) fn release_interrupted_exit_mount(
        &mut self,
        draft: &MainWindowShutdownDraft,
        target: &Arc<GpuiAppearancePublicationTarget>,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        self.validate_interrupted_exit_binding(draft, target, cx)?;
        let Some((mount, _, close)) = &draft.composer else {
            return Ok(true);
        };
        mount.update(cx, |mount, cx| {
            mount.release_interrupted_exit_mount(*close, cx)
        })
    }

    pub(crate) fn release_interrupted_exit_draft(
        &mut self,
        draft: &MainWindowShutdownDraft,
        target: &Arc<GpuiAppearancePublicationTarget>,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        self.validate_interrupted_exit_binding(draft, target, cx)?;
        if let Some((mount, _, close)) = &draft.composer {
            match mount.update(cx, |mount, cx| {
                mount.release_interrupted_exit_draft(*close, cx)
            })? {
                crate::main_window::MainWindowConversationComposerCloseRelease::Pending => {
                    return Ok(false);
                }
                crate::main_window::MainWindowConversationComposerCloseRelease::Released => {}
                crate::main_window::MainWindowConversationComposerCloseRelease::Stale => {
                    return Err("Recovery draft close ticket changed".into());
                }
            }
        }
        let drained = draft.advance_original_prepublication_cleanup(
            self.controller
                .as_ref()
                .ok_or("Recovered cleanup controller is missing")?
                .window_id(),
        )?;
        #[cfg(all(test, feature = "test-faults"))]
        {
            self.running_threads
                .fixture_creation_page_release_acknowledgements =
                draft.accepted_prepublication_page_release_acknowledgements()?;
        }
        Ok(drained)
    }

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
                ShellContent::Selected { selection, .. },
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
                mount
                    .read(cx)
                    .validate_fresh_recovery_protection(*close, cx)?;
                Ok(())
            }
            _ => Err("Recovery shell binding is incomplete".into()),
        }
    }
}
