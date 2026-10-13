use super::*;

impl RunningProcessOwner {
    #[cfg(test)]
    pub(crate) fn test_diagnostic_exit_idle(&self) -> bool {
        self.confirmation.is_none()
            && self.shutdown.is_none()
            && self.progress.is_none()
            && !self.observing_initial_work
            && !self.exit_requested()
    }

    pub(crate) fn diagnostic_main_window(
        &self,
        app: &App,
    ) -> Result<gpui::WindowHandle<crate::main_window::MainWindowShellRoot>, &'static str> {
        let shells = self.process.windows.shells();
        let active_index = match shells {
            [_] => None,
            [] => return Err("The diagnostic child has no registered main window."),
            _ => {
                let active = app
                    .active_window()
                    .ok_or("The diagnostic child main window is ambiguous.")?;
                shells
                    .iter()
                    .position(|shell| shell.window().window_id() == active.window_id())
            }
        };
        let index = registered_window_index(shells.len(), active_index)
            .ok_or("The active diagnostic window is not a registered main window.")?;
        let window = shells[index].window();
        let root = window
            .read(app)
            .map_err(|_| "The diagnostic main window is unavailable.")?;
        if root.controller().is_none() {
            return Err("The diagnostic main window has no current controller.");
        }
        Ok(window)
    }
}

fn registered_window_index(count: usize, active: Option<usize>) -> Option<usize> {
    match count {
        0 => None,
        1 => Some(0),
        _ => active.filter(|index| *index < count),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/diagnostic_window_selection.rs"]
mod tests;
