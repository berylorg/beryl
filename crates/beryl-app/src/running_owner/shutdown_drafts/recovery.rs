use super::*;

#[cfg(test)]
impl RunningProcessOwner {
    pub(crate) fn test_replace_recovery_drafts(
        &mut self,
        drafts: Option<Rc<RefCell<RunningShutdownDrafts>>>,
    ) -> Option<Rc<RefCell<RunningShutdownDrafts>>> {
        std::mem::replace(&mut self.shutdown.as_mut().unwrap().drafts, drafts)
    }
}

impl RunningShutdownDrafts {
    pub(in crate::running_owner) fn validate_recovered_bindings(
        &self,
        published: &crate::main_window::PublishedMainWindowRestoreSet,
        appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        app: &mut App,
    ) -> Result<(), String> {
        if !self.prepared
            || self.driving
            || self.releasing
            || self.released
            || self.windows.is_empty()
            || self.windows.len() != published.shells().len()
        {
            return Err("Recovery draft set is incomplete or busy".into());
        }
        for shell in published.shells() {
            let mut entries = self
                .windows
                .iter()
                .filter(|(window, _)| *window == shell.window());
            let (_, draft) = entries.next().ok_or("Recovery draft is missing")?;
            if entries.next().is_some() {
                return Err("Recovery draft is duplicated".into());
            }
            shell.validate_interrupted_exit_binding(
                draft.as_ref().map_err(Clone::clone)?,
                appearance,
                app,
            )?;
        }
        Ok(())
    }

    pub(in crate::running_owner) fn require_recovery_window(
        &self,
        window: WindowHandle<MainWindowShellRoot>,
    ) -> Result<(), String> {
        if !self.prepared || self.driving || self.releasing || self.released {
            return Err("Interrupted Exit draft set is not available for binding".into());
        }
        self.windows
            .iter()
            .find(|(handle, _)| *handle == window)
            .ok_or("Recovery window is absent from retained drafts")?
            .1
            .as_ref()
            .map(|_| ())
            .map_err(Clone::clone)
    }

    pub(crate) fn adopt_recovered_threadless_shell(
        &mut self,
        root: &mut MainWindowShellRoot,
        source: &mut Option<crate::app_services::recovery_threadless::ThreadlessRecoveryWindow>,
        window: &gpui::Window,
        cx: &mut gpui::Context<MainWindowShellRoot>,
    ) -> Result<(), String> {
        if !self.prepared || self.driving || self.releasing || self.released {
            return Err("Interrupted Exit draft set is not available for adoption".into());
        }
        let (_, draft) = self
            .windows
            .iter()
            .find(|(handle, _)| gpui::AnyWindowHandle::from(*handle) == window.window_handle())
            .ok_or("Recovery window is absent from retained drafts")?;
        let draft = draft.as_ref().map_err(|error| error.clone())?;
        root.adopt_interrupted_exit_threadless_shell(draft, source, cx)?;
        self.ready = false;
        Ok(())
    }

    pub(crate) fn adopt_recovered_shell(
        &mut self,
        root: &mut MainWindowShellRoot,
        resident: gpui::EntityId,
        close: crate::main_window::MainWindowConversationComposerCloseTicket,
        preparation: &mut crate::main_window::MainWindowComposerRecoveryPreparation,
        adapters: &mut Option<
            crate::app_services::recovery_composer::PreparedComposerRecoveryAdapters,
        >,
        configurator: &mut Option<crate::main_window::MainWindowConversationComposerConfigurator>,
        current: gpui_text_input::RangePrepublicationCurrent,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<MainWindowShellRoot>,
    ) -> Result<
        (
            beryl_home_store::HomeRecoveryCandidate,
            crate::main_window::MainWindowConversationComposerCloseTicket,
        ),
        String,
    > {
        if !self.prepared || self.driving || self.releasing || self.released {
            return Err("Interrupted Exit draft set is not available for adoption".into());
        }
        let (_, draft) = self
            .windows
            .iter_mut()
            .find(|(handle, _)| gpui::AnyWindowHandle::from(*handle) == window.window_handle())
            .ok_or("Recovery window is absent from retained drafts")?;
        let draft = draft.as_mut().map_err(|error| error.clone())?;
        if draft.recovery_resident_identity() != Some((resident, close)) {
            return Err("Recovery resident differs from retained draft".into());
        }
        let adopted = root.adopt_interrupted_exit_shell(
            draft,
            preparation,
            adapters,
            configurator,
            current,
            window,
            cx,
        )?;
        self.ready = false;
        Ok(adopted)
    }

    #[cfg(test)]
    pub(crate) fn test_recovery_drafts(
        window: WindowHandle<MainWindowShellRoot>,
        draft: MainWindowShutdownDraft,
    ) -> Self {
        Self {
            windows: vec![(window, Ok(draft))],
            driving: false,
            prepared: true,
            releasing: false,
            released: false,
            ready: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn test_recovery_driving(&mut self, driving: bool) {
        self.driving = driving;
    }

    #[cfg(test)]
    pub(crate) fn test_recovery_ready(&self) -> bool {
        self.ready()
    }

    pub(in crate::running_owner) fn retire_residents(
        &mut self,
        app: &mut App,
    ) -> Result<bool, String> {
        if !self.ready() || self.released {
            return Err("Interrupted Exit draft set is not ready".into());
        }
        let mut ready = true;
        let mut failure = None;
        for (window, draft) in &mut self.windows {
            let result = match draft {
                Ok(draft) => window
                    .update(app, |root, _, cx| root.retire_shutdown_draft(draft, cx))
                    .map_err(|error| format!("Interrupted Exit window is unavailable: {error}"))
                    .and_then(|result| result),
                Err(error) => Err(error.clone()),
            };
            match result {
                Ok(retired) => ready &= retired,
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(ready),
        }
    }
}
