use super::*;

impl MainWindowCreationOwner {
    #[cfg(test)]
    pub(crate) fn test_entry_status(&self) -> Vec<String> {
        self.entries
            .values()
            .map(|entry| {
                format!(
                    "running={}, hidden={}, work={}, continuations={}, error={:?}",
                    entry.running,
                    entry.hidden.is_some(),
                    entry.work.is_some(),
                    entry.continuations,
                    entry.work.as_ref().and_then(MainWindowCreation::last_error),
                )
            })
            .collect()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_uninstall(app: &mut App) {
        assert!(
            app.windows().is_empty(),
            "test removes every native window first"
        );
        let owner = &app.global::<CreationProcessOwner>()._owner;
        assert!(
            owner.read(app).entries.is_empty(),
            "test settles every creation first"
        );
        app.remove_global::<CreationProcessOwner>();
    }

    #[cfg(feature = "test-faults")]
    pub fn test_delay_next_completion(&mut self, delay: Duration) {
        self.test_completion_delay = Some(delay);
    }

    #[cfg(feature = "test-faults")]
    pub fn test_completion_is_waiting(&self, window_id: WindowId) -> bool {
        self.entries
            .get(&window_id)
            .is_some_and(|entry| entry.test_completion_waiting)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_hold_next_publication(&mut self) {
        self.test_hold_publication = true;
    }

    #[cfg(feature = "test-faults")]
    pub fn test_hidden_window(
        &self,
        window_id: WindowId,
    ) -> Option<gpui::WindowHandle<MainWindowShellRoot>> {
        self.entries
            .get(&window_id)?
            .hidden
            .as_ref()
            .map(MainWindowShell::window)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_release_publication(&mut self, window_id: WindowId, cx: &mut Context<Self>) {
        self.entries
            .get_mut(&window_id)
            .unwrap()
            .test_publication_held = false;
        self.drive(window_id, cx);
    }
}
