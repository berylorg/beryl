use super::*;
use crate::app_services::RetiredHomeRecoveryError;
use beryl_home_store::{CommandCancellation, HomeGeneration};
use settlement::CandidateSettlement;

impl RunningProcessOwner {
    pub(crate) fn abort_constructed_exit_candidate(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<(), String> {
        let (mut services, candidate, slot) = {
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
            let slot = recovery.settlement.clone();
            let mut settlement = slot.borrow_mut();
            let Some(CandidateSettlement::Constructed(Ok(candidate))) = settlement.as_ref() else {
                return Err("Interrupted Exit has no constructed storage candidate".into());
            };
            owner
                .process
                .services
                .as_ref()
                .ok_or("The complete service owner is on a worker")?
                .validate_retired_service_home_return(generation, Some(candidate.home_id()))
                .map_err(|error| error.to_string())?;
            let Some(CandidateSettlement::Constructed(Ok(candidate))) =
                settlement.replace(CandidateSettlement::Pending)
            else {
                unreachable!()
            };
            drop(settlement);
            (owner.process.services.take().unwrap(), candidate, slot)
        };
        let retained = owner.clone();
        let work = app.background_executor().spawn(async move {
            let mut home = Some(candidate.abort());
            services
                .return_retired_service_home(generation, &mut home)
                .expect("exclusive validated retired custody accepts aborted candidate");
            services
        });
        app.spawn(async move |cx| {
            let services = work.await;
            retained.borrow_mut().process.services = Some(services);
            *slot.borrow_mut() = None;
            let _ = cx.update(|app| completed(&retained, app));
        })
        .detach();
        Ok(())
    }

    pub(crate) fn construct_interrupted_exit_candidate(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        cancellation: CommandCancellation,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<(), String> {
        let (mut services, slot) = {
            let mut owner = owner.borrow_mut();
            owner.interrupted_exit_graph_retirement_result(request)?;
            if cancellation.is_cancelled() {
                return Err(RetiredHomeRecoveryError::Cancelled.to_string());
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
            let (mut services, mut result) = work.await;
            if cancellation.is_cancelled() {
                match result {
                    Ok(candidate) => {
                        services = cx
                            .background_executor()
                            .spawn(async move {
                                let mut home = Some(candidate.abort());
                                services
                                    .return_retired_service_home(generation, &mut home)
                                    .expect(
                                        "exclusive retired custody accepts cancelled candidate",
                                    );
                                services
                            })
                            .await;
                        result = Err(RetiredHomeRecoveryError::Cancelled);
                    }
                    Err(error) => result = Err(error),
                }
            }
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
