use super::*;
use beryl_home_store::HomeGeneration;
use settlement::CandidateSettlement;

impl RunningProcessOwner {
    pub(crate) fn construct_interrupted_exit_candidate(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<(), String> {
        let (mut services, slot) = {
            let mut owner = owner.borrow_mut();
            owner.interrupted_exit_graph_retirement_result(request)?;
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
            if !matches!(
                recovery.settlement.borrow().as_ref(),
                None | Some(CandidateSettlement::Constructed(Err(_)))
            ) {
                return Err("Interrupted Exit candidate work is already retained".into());
            }
            let slot = recovery.settlement.clone();
            let services = owner
                .process
                .services
                .take()
                .ok_or("The complete service owner is on a worker")?;
            *slot.borrow_mut() = Some(CandidateSettlement::Pending);
            (services, slot)
        };
        let retained = owner.clone();
        let work = app.background_executor().spawn(async move {
            let result = services.recover_retired_service_home(generation);
            (services, result)
        });
        app.spawn(async move |cx| {
            let (services, result) = work.await;
            retained.borrow_mut().process.services = Some(services);
            *slot.borrow_mut() = Some(CandidateSettlement::Constructed(result));
            let _ = cx.update(|app| completed(&retained, app));
        })
        .detach();
        Ok(())
    }

    pub(crate) fn interrupted_exit_construction_result(
        &self,
        request: &RunningExitRequest,
    ) -> Result<(), String> {
        self.interrupted_exit_graph_retirement_result(request)?;
        match self
            .interrupted_exit
            .as_ref()
            .unwrap()
            .settlement
            .borrow()
            .as_ref()
        {
            Some(CandidateSettlement::Constructed(result)) => {
                result.as_ref().map(|_| ()).map_err(ToString::to_string)
            }
            _ => Err("Interrupted Exit candidate construction has not returned".into()),
        }
    }

    #[cfg(test)]
    pub(crate) fn test_take_constructed_exit_candidate(
        &self,
    ) -> beryl_home_store::HomeRecoveryCandidate {
        match self
            .interrupted_exit
            .as_ref()
            .unwrap()
            .settlement
            .borrow_mut()
            .take()
            .unwrap()
        {
            CandidateSettlement::Constructed(Ok(candidate)) => candidate,
            _ => panic!("constructed candidate unavailable"),
        }
    }
}
