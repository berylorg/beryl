use super::*;
use crate::{
    app_services::recovery_threadless::ThreadlessRecoveryWindow, main_window::MainWindowShellRoot,
};

impl RunningProcessOwner {
    pub(crate) fn prepare_interrupted_exit_threadless_window(
        owner: &Rc<RefCell<Self>>,
        request: &impl RecoveryIdentity,
        retired_home: beryl_model::BerylHomeId,
        retired_generation: beryl_home_store::HomeGeneration,
        window: beryl_model::WindowId,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, Result<ThreadlessRecoveryWindow, String>, &mut App)
        + 'static,
    ) -> Result<(), String> {
        let (slot, mut graph) = {
            let retained = owner.borrow();
            retained.interrupted_exit_services_result(request)?;
            let recovery = retained.interrupted_exit.as_ref().unwrap();
            if recovery.session.borrow().is_none()
                || recovery.resident.is_some()
                || recovery
                    .pending_resident_frame
                    .as_ref()
                    .is_some_and(|wake| wake.strong_count() != 0)
            {
                return Err("Interrupted Exit recovery custody is unavailable".into());
            }
            let slot = recovery.settlement.clone();
            let Some(settlement::CandidateSettlement::Services(Ok(graph))) = slot
                .borrow_mut()
                .replace(settlement::CandidateSettlement::Pending)
            else {
                unreachable!()
            };
            (slot, graph)
        };
        let identity = request.identity();
        let retained = owner.clone();
        let work = app.background_executor().spawn(async move {
            let result = graph.threadless_recovery_window(retired_home, retired_generation, window);
            (graph, result)
        });
        app.spawn(async move |cx| {
            let (graph, mut result) = work.await;
            *slot.borrow_mut() = Some(settlement::CandidateSettlement::Services(Ok(graph)));
            {
                let owner = retained.borrow();
                if !owner.active_recovery_identity(&identity)
                    || !owner
                        .interrupted_exit
                        .as_ref()
                        .is_some_and(|recovery| Rc::ptr_eq(&recovery.request, &identity))
                {
                    result = Err("Interrupted Exit request changed".into());
                }
            }
            let _ = cx.update(|app| completed(&retained, result, app));
        })
        .detach();
        Ok(())
    }
    pub(crate) fn attach_interrupted_exit_threadless(
        &mut self,
        request: &impl RecoveryIdentity,
        root: &mut MainWindowShellRoot,
        source: &mut Option<ThreadlessRecoveryWindow>,
        window: &gpui::Window,
        cx: &mut gpui::Context<MainWindowShellRoot>,
    ) -> Result<(), String> {
        self.interrupted_exit_services_result(request)?;
        let recovery = self.interrupted_exit.as_ref().unwrap();
        if recovery.resident.is_some()
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
        let mut slot = recovery.settlement.borrow_mut();
        let Some(settlement::CandidateSettlement::Services(Ok(graph))) = slot.as_mut() else {
            unreachable!()
        };
        if !graph.matches_candidate(facts.home_id(), facts.generation()) {
            return Err("Threadless recovery candidate identity changed".into());
        }
        drop(slot);
        let drafts = self.recovery_drafts()?;
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
