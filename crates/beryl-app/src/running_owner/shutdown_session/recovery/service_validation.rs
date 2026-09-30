use super::*;
use beryl_home_store::{CommandCancellation, HomeGeneration};
use settlement::CandidateSettlement;

impl RunningProcessOwner {
    pub(crate) fn revalidate_interrupted_exit_services(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        home: beryl_model::BerylHomeId,
        generation: HomeGeneration,
        cancellation: CommandCancellation,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, Result<(), String>, &mut App) + 'static,
    ) -> Result<(), String> {
        let (session_slot, settlement_slot, validation_slot, original, mut graph) = {
            let owner = owner.borrow();
            owner.interrupted_exit_services_result(request)?;
            if cancellation.is_cancelled() {
                return Err("Interrupted Exit session validation was cancelled".into());
            }
            let recovery = owner.interrupted_exit.as_ref().unwrap();
            if recovery.session.borrow().is_none()
                || recovery.resident.is_some()
                || recovery
                    .pending_resident_frame
                    .as_ref()
                    .is_some_and(|wake| wake.strong_count() != 0)
            {
                return Err("Interrupted Exit recovery custody is unavailable".into());
            }
            let mut slot = recovery.settlement.borrow_mut();
            let Some(CandidateSettlement::Services(Ok(graph))) = slot.as_mut() else {
                unreachable!("validated prepared recovery services")
            };
            if !graph.matches_candidate(home, generation) {
                return Err("session validation belongs to another recovery candidate".into());
            }
            let Some(CandidateSettlement::Services(Ok(graph))) =
                slot.replace(CandidateSettlement::Pending)
            else {
                unreachable!()
            };
            let original = recovery.session.borrow_mut().take().unwrap();
            *recovery.service_validation.borrow_mut() = None;
            (
                recovery.session.clone(),
                recovery.settlement.clone(),
                recovery.service_validation.clone(),
                original,
                graph,
            )
        };
        let identity = request.identity();
        let retained = owner.clone();
        let work = app.background_executor().spawn(async move {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                graph.revalidate_interrupted_exit_session(home, generation, &original)
            }))
            .unwrap_or_else(|_| Err("Interrupted Exit session validation unwound".into()));
            (graph, original, result)
        });
        app.spawn(async move |cx| {
            let (graph, original, mut result) = work.await;
            *session_slot.borrow_mut() = Some(original);
            *settlement_slot.borrow_mut() = Some(CandidateSettlement::Services(Ok(graph)));
            {
                let owner = retained.borrow();
                if !owner.process.commands.is_active_identity(&identity)
                    || !owner
                        .interrupted_exit
                        .as_ref()
                        .is_some_and(|recovery| Rc::ptr_eq(&recovery.request, &identity))
                {
                    result = Err("Interrupted Exit request changed".into());
                } else if cancellation.is_cancelled() {
                    result = Err("Interrupted Exit session validation was cancelled".into());
                }
            }
            *validation_slot.borrow_mut() = Some(result.clone());
            let _ = cx.update(|app| completed(&retained, result, app));
        })
        .detach();
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn test_interrupted_exit_service_validation(&self) -> Option<Result<(), String>> {
        self.interrupted_exit
            .as_ref()?
            .service_validation
            .borrow()
            .clone()
    }
}
