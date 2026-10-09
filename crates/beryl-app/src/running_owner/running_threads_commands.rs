use super::*;

impl RunningProcessOwner {
    pub(crate) fn admit_thread_creation(
        &self,
        invoking: beryl_model::WindowId,
        app: &App,
    ) -> Result<crate::window_acquisition::WindowSelectionLease, String> {
        let members = self
            .process
            .windows
            .shells()
            .iter()
            .filter(|shell| shell.is_published())
            .map(|shell| shell.retained_window_id(app))
            .collect::<Result<Vec<_>, _>>()?;
        self.process
            .services
            .as_ref()
            .ok_or("Thread creation services unavailable")?
            .admit_thread_creation(&members, invoking)
    }

    pub(crate) fn running_selection_pending(&self) -> bool {
        self.process
            .services
            .as_ref()
            .is_some_and(|services| services.running_selection_pending())
    }
    pub(crate) fn admit_running_selection(
        &self,
        invoking: crate::main_window::MainWindowSelectionInvocation,
        app: &App,
    ) -> Result<crate::window_acquisition::WindowSelectionLease, String> {
        let members = invoking.published_members(self.process.windows.shells(), app)?;
        self.process
            .services
            .as_ref()
            .ok_or("Running threads services unavailable")?
            .admit_running_selection(&members, invoking.window_id())
    }
    pub(crate) fn reveal_running_claim(
        owner: &Rc<RefCell<Self>>,
        claim: beryl_state::ThreadClaimRecord,
        app: &mut App,
    ) -> Result<(), String> {
        let windows = owner
            .borrow()
            .process
            .windows
            .shells()
            .iter()
            .filter(|shell| shell.is_published())
            .map(|shell| shell.window())
            .collect::<Vec<_>>();
        Self::reveal_running_claim_in_windows(&windows, claim, app)
    }

    pub(crate) fn reveal_running_claim_in_windows(
        windows: &[gpui::WindowHandle<crate::main_window::MainWindowShellRoot>],
        claim: beryl_state::ThreadClaimRecord,
        app: &mut App,
    ) -> Result<(), String> {
        if windows.len() > beryl_state::MAX_RESTORABLE_WINDOWS
            || windows
                .iter()
                .enumerate()
                .any(|(index, window)| windows[..index].contains(window))
        {
            return Err("Running thread reveal has invalid published window membership.".into());
        }
        let window = windows
            .iter()
            .copied()
            .find(|window| {
                window
                    .read(app)
                    .is_ok_and(|root| root.matches_running_claim(claim, app))
            })
            .ok_or("The thread's original window is no longer available.")?;
        window
            .update(app, |root, window, cx| {
                if !root.matches_running_claim(claim, cx) {
                    return Err("The thread's original window changed.".to_owned());
                }
                window.activate_window();
                Ok(())
            })
            .map_err(|error| error.to_string())?
    }
}
