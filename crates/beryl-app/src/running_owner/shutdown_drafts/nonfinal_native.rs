use super::*;

impl RunningShutdownDrafts {
    pub(in crate::running_owner) fn release_restored_native_close(
        &mut self,
        window: WindowHandle<MainWindowShellRoot>,
        app: &mut App,
    ) -> Result<bool, String> {
        if self.windows.len() != 1 || self.driving || self.detached_preparing {
            return Err("restored nonfinal draft release correspondence changed".into());
        }
        let draft = self
            .windows
            .iter()
            .find(|(captured, _)| *captured == window)
            .ok_or("restored nonfinal draft is unavailable")?
            .1
            .as_ref()
            .map_err(|e| e.clone())?;
        let released = match window
            .update(app, |root, window, cx| {
                root.release_shutdown_draft(draft, window, cx)
            })
            .map_err(|e| e.to_string())??
        {
            MainWindowShutdownDraftRelease::Released => true,
            MainWindowShutdownDraftRelease::Pending => false,
        };
        self.releasing = true;
        self.released = released;
        Ok(released)
    }
    pub(in crate::running_owner) fn retained_native_close_service(
        &self,
        window: WindowHandle<MainWindowShellRoot>,
    ) -> Result<
        Option<(
            std::sync::Arc<crate::main_window::MainWindowConversationComposerService>,
            crate::main_window::MainWindowConversationComposerCloseTicket,
        )>,
        String,
    > {
        let draft = self
            .windows
            .iter()
            .find(|(captured, _)| *captured == window)
            .ok_or("nonfinal native recovery draft is unavailable")?
            .1
            .as_ref()
            .map_err(|e| e.clone())?;
        draft.retained_native_close_service()
    }

    pub(in crate::running_owner) fn reattach_nonfinal_native(
        &mut self,
        window: WindowHandle<MainWindowShellRoot>,
        restored: Option<(
            &beryl_state::SessionWindowRecord,
            Option<crate::main_window::MainWindowConversationComposerCloseTicket>,
        )>,
        app: &mut App,
    ) -> Result<(), String> {
        let draft = self
            .windows
            .iter_mut()
            .find(|(captured, _)| *captured == window)
            .ok_or("nonfinal native recovery draft is unavailable")?
            .1
            .as_mut()
            .map_err(|e| e.clone())?;
        window
            .update(app, |root, _, cx| {
                root.reattach_nonfinal_native_draft(draft, restored, cx)
            })
            .map_err(|e| e.to_string())??;
        self.detached_prepared = false;
        Ok(())
    }

    pub(in crate::running_owner) fn retire_destroyed_nonfinal(
        &mut self,
        shell: &mut crate::main_window::MainWindowShell,
        app: &mut App,
    ) -> Result<bool, String> {
        let window = shell.window();
        let draft = self
            .windows
            .iter_mut()
            .find(|(captured, _)| *captured == window)
            .ok_or("destroyed nonfinal draft is unavailable")?
            .1
            .as_mut()
            .map_err(|e| e.clone())?;
        shell.retire_destroyed_nonfinal_draft(draft, app)
    }

    pub(in crate::running_owner) fn reattach_pre_native_close(
        &mut self,
        shell: &mut crate::main_window::MainWindowShell,
        request: &Rc<()>,
        restored: Option<(
            &beryl_state::SessionWindowRecord,
            Option<crate::main_window::MainWindowConversationComposerCloseTicket>,
        )>,
        app: &mut App,
    ) -> Result<(), String> {
        let draft = self
            .windows
            .iter_mut()
            .find(|(window, _)| *window == shell.window())
            .ok_or("pre-native recovery draft is unavailable")?
            .1
            .as_mut()
            .map_err(|error| error.clone())?;
        shell.reattach_pre_native_close_draft(request, draft, restored, app)?;
        self.detached_prepared = false;
        Ok(())
    }
}
