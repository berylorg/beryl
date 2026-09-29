use super::*;
use crate::theme_runtime::{AppearancePublicationTarget, GpuiAppearanceWindowSet};

impl RunningProcessOwner {
    #[cfg(test)]
    pub(crate) fn test_process_appearance(&self) -> gpui::Entity<GpuiAppearanceWindowSet> {
        self.process.appearance.clone()
    }

    pub(crate) fn bind_interrupted_exit_appearance(
        &mut self,
        request: &RunningExitRequest,
        candidate: &InterruptedExitCandidate,
        window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
        appearance: &gpui::Entity<GpuiAppearanceWindowSet>,
        app: &mut App,
    ) -> Result<(), String> {
        self.interrupted_exit_graph_retirement_result(request)?;
        let recovery = self.interrupted_exit.as_ref().unwrap();
        if recovery.resident.is_some()
            || recovery.settlement.borrow().is_some()
            || recovery.session.borrow().is_none()
            || recovery
                .pending_resident_frame
                .as_ref()
                .is_some_and(|wake| wake.strong_count() != 0)
        {
            return Err("Interrupted Exit recovery custody is unavailable".into());
        }
        let snapshot = appearance.read(app).target().snapshot();
        let home = snapshot.current.prepared().home();
        if !snapshot.active
            || home.home_id() != candidate.candidate.home_id()
            || home.home_generation() != candidate.candidate.generation()
        {
            return Err("Recovery appearance candidate identity changed".into());
        }
        let drafts = self
            .shutdown
            .as_ref()
            .and_then(|attempt| attempt.drafts.as_ref())
            .ok_or("Interrupted Exit drafts are unavailable")?;
        let drafts = drafts
            .try_borrow_mut()
            .map_err(|_| "Interrupted Exit drafts are busy")?;
        drafts.require_recovery_window(window)?;
        self.process
            .windows
            .bind_interrupted_exit_appearance(window, appearance, app)
    }
}
