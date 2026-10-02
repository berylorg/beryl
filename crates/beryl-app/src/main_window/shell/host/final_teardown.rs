use super::*;
use std::pin::Pin;

impl MainWindowShellRoot {
    pub(crate) fn mount_blocked_shutdown(
        &mut self,
        owner: std::rc::Weak<RefCell<crate::running_owner::RunningProcessOwner>>,
        cx: &mut Context<Self>,
    ) {
        self.blocked_shutdown = Some(owner);
        cx.notify();
    }

    pub(crate) fn request_blocked_quit(&self, app: &mut App) -> Result<(), String> {
        let owner = self
            .blocked_shutdown
            .as_ref()
            .and_then(std::rc::Weak::upgrade)
            .ok_or("blocked shutdown ownership is unavailable")?;
        let invoking = self
            .controller
            .as_ref()
            .ok_or("shell is unavailable")?
            .window_id();
        app.defer(move |app| {
            let _ = crate::running_owner::RunningProcessOwner::begin_blocked_quit(
                &owner, invoking, app,
            );
        });
        Ok(())
    }
}

impl MainWindowShell {
    pub(crate) fn begin_final_native_cleanup(
        &mut self,
        app: &mut App,
    ) -> Result<Pin<Box<gpui::WindowsNativeWindowDestroyed>>, String> {
        let root = self.root.read(app);
        if !self.published
            || root.startup_interaction_gated()
            || !root.shutdown_interaction_gated
            || !root.controller.as_ref().is_some_and(|controller| {
                matches!(controller.content, ShellContent::Retired { .. })
            })
            || !self.desktop_cleanup_allowed()
        {
            return Err("native cleanup requires the exact retired running shell".into());
        }
        let receipt = match self.startup_disposal.as_mut() {
            Some(admission) if admission.preserve_records && !admission.started => {
                admission.started = true;
                admission
                    .receipt
                    .take()
                    .ok_or("published native destruction receipt is unavailable")?
            }
            Some(_) => {
                return Err("published native destruction custody is already consumed".into());
            }
            None => Box::pin(
                self.window
                    .update(app, |_, window, _| {
                        window.observe_windows_native_destruction()
                    })
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())?,
            ),
        };
        if let Err(error) = self
            .window
            .update(app, |_, window, _| window.remove_window())
        {
            if let Some(admission) = self.startup_disposal.as_mut() {
                admission.receipt = Some(receipt);
            }
            return Err(format!("published window removal failed: {error}"));
        }
        Ok(receipt)
    }

    pub(crate) fn settle_final_native_cleanup(&mut self, app: &mut App) -> Result<(), String> {
        if self.appearance_registered {
            self.appearance_owner
                .update(app, |owner, _| owner.unregister(self.adapter_id))
                .map_err(|error| error.to_string())?;
            self.appearance_registered = false;
        }
        Ok(())
    }
}
