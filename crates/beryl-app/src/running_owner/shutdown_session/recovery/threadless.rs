use super::*;
use crate::{
    app_services::recovery_threadless::ThreadlessRecoveryWindow, main_window::MainWindowShellRoot,
};

impl RunningProcessOwner {
    pub(crate) fn attach_interrupted_exit_threadless(
        &mut self,
        request: &RunningExitRequest,
        candidate: &InterruptedExitCandidate,
        root: &mut MainWindowShellRoot,
        source: &mut Option<ThreadlessRecoveryWindow>,
        window: &gpui::Window,
        cx: &mut gpui::Context<MainWindowShellRoot>,
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
        let facts = source
            .as_ref()
            .ok_or("Threadless recovery source is unavailable")?;
        if candidate.candidate.home_id() != facts.home_id()
            || candidate.candidate.generation() != facts.generation()
        {
            return Err("Threadless recovery candidate identity changed".into());
        }
        let drafts = self
            .shutdown
            .as_ref()
            .and_then(|attempt| attempt.drafts.as_ref())
            .ok_or("Interrupted Exit drafts are unavailable")?;
        let mut drafts = drafts
            .try_borrow_mut()
            .map_err(|_| "Interrupted Exit drafts are busy")?;
        drafts.adopt_recovered_threadless_shell(root, source, window, cx)
    }

    #[cfg(test)]
    pub(crate) fn test_take_retired_recovery_home(&mut self) -> beryl_home_store::HomeStore {
        self.process
            .services
            .as_mut()
            .unwrap()
            .test_retired_service_home()
            .unwrap()
    }
}
