use super::*;
use crate::app_services::{
    AppServiceConfiguration, recovery_graph::RecoveryServicePreparationError,
};
use beryl_home_store::{CommandCancellation, HomeGeneration};
use settlement::{CandidateSettlement, CandidateSettlementError};
use syndic_storage::SyndicTimestamp;

impl RunningProcessOwner {
    pub(crate) fn take_interrupted_exit_preparation_failure(
        &mut self,
        request: &RunningExitRequest,
        generation: HomeGeneration,
    ) -> Result<RecoveryServicePreparationError, String> {
        self.return_interrupted_exit_preparation_home(request, generation)?;
        let Some(CandidateSettlement::Services(Err(failure))) = self
            .interrupted_exit
            .as_ref()
            .unwrap()
            .settlement
            .borrow_mut()
            .take()
        else {
            unreachable!("validated preparation failure retains exclusive custody")
        };
        Ok(failure)
    }

    fn return_interrupted_exit_preparation_home(
        &mut self,
        request: &RunningExitRequest,
        generation: HomeGeneration,
    ) -> Result<(), String> {
        self.interrupted_exit_graph_retirement_result(request)?;
        let recovery = self.interrupted_exit.as_ref().unwrap();
        if recovery.session.borrow().is_none()
            || recovery.resident.is_some()
            || recovery
                .pending_resident_frame
                .as_ref()
                .is_some_and(|wake| wake.strong_count() != 0)
        {
            return Err("Interrupted Exit recovery custody is unavailable".into());
        }
        let mut settlement = recovery.settlement.borrow_mut();
        let Some(CandidateSettlement::Services(Err(failure))) = settlement.as_mut() else {
            return Err("Interrupted Exit service preparation has no retained failure".into());
        };
        self.process
            .services
            .as_mut()
            .ok_or("The complete service owner is on a worker")?
            .return_recovery_preparation_home(generation, failure)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn prepare_interrupted_exit_services(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<(), String> {
        let (services, session_slot, settlement_slot, original, candidate) = {
            let mut owner = owner.borrow_mut();
            owner.interrupted_exit_graph_retirement_result(request)?;
            if cancellation.is_cancelled() {
                return Err("Interrupted Exit service preparation was cancelled".into());
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
            let slot = recovery.settlement.clone();
            let mut settlement = slot.borrow_mut();
            let Some(CandidateSettlement::Returned {
                candidate,
                result: Ok(()),
            }) = settlement.as_ref()
            else {
                return Err("Interrupted Exit candidate has no successful settlement".into());
            };
            owner
                .process
                .services
                .as_ref()
                .ok_or("The complete service owner is on a worker")?
                .validate_retired_service_home_return(
                    generation,
                    Some(candidate.candidate.home_id()),
                )
                .map_err(|error| error.to_string())?;
            let original = recovery.session.borrow_mut().take().unwrap();
            let session_slot = recovery.session.clone();
            let Some(CandidateSettlement::Returned { candidate, .. }) =
                settlement.replace(CandidateSettlement::Pending)
            else {
                unreachable!()
            };
            drop(settlement);
            (
                owner.process.services.take().unwrap(),
                session_slot,
                slot,
                original,
                candidate,
            )
        };
        let retained = owner.clone();
        let worker_cancellation = cancellation.clone();
        let work = app.background_executor().spawn(async move {
            let mut original = original;
            let mut candidate = candidate;
            let settled =
                match original.revalidate_candidate(&mut candidate.candidate, &candidate.session) {
                    Err(error) => CandidateSettlement::Returned {
                        candidate,
                        result: Err(CandidateSettlementError::Candidate(error)),
                    },
                    Ok(()) => {
                        let mut storage = Some(candidate.candidate);
                        let result = services.prepare_recovery_service_graph(
                            generation,
                            &mut storage,
                            configuration,
                            at,
                            &worker_cancellation,
                        );
                        match result {
                            Err(RecoveryServicePreparationError::Refused(error)) => {
                                CandidateSettlement::Returned {
                                    candidate: InterruptedExitCandidate {
                                        candidate: storage
                                            .expect("refused preparation retains candidate"),
                                        session: candidate.session,
                                    },
                                    result: Err(CandidateSettlementError::Candidate(error)),
                                }
                            }
                            result => CandidateSettlement::Services(result),
                        }
                    }
                };
            (services, original, settled)
        });
        app.spawn(async move |cx| {
            let (services, original, mut settled) = work.await;
            if cancellation.is_cancelled() {
                if let CandidateSettlement::Services(Ok(prepared)) = settled {
                    settled = cx
                        .background_executor()
                        .spawn(async move {
                            CandidateSettlement::Services(Err(
                                RecoveryServicePreparationError::App(prepared.cancel()),
                            ))
                        })
                        .await;
                }
            }
            retained.borrow_mut().process.services = Some(services);
            *session_slot.borrow_mut() = Some(original);
            *settlement_slot.borrow_mut() = Some(settled);
            let _ = cx.update(|app| completed(&retained, app));
        })
        .detach();
        Ok(())
    }

    pub(crate) fn cancel_interrupted_exit_services(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<(), String> {
        let (services, session_slot, settlement_slot, original, prepared) = {
            let mut owner = owner.borrow_mut();
            owner.interrupted_exit_services_result(request)?;
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
            if owner.process.services.is_none() {
                return Err("The complete service owner is on a worker".into());
            }
            let session_slot = recovery.session.clone();
            let settlement_slot = recovery.settlement.clone();
            let original = session_slot.borrow_mut().take().unwrap();
            let Some(CandidateSettlement::Services(Ok(prepared))) = settlement_slot
                .borrow_mut()
                .replace(CandidateSettlement::Pending)
            else {
                unreachable!()
            };
            (
                owner.process.services.take().unwrap(),
                session_slot,
                settlement_slot,
                original,
                prepared,
            )
        };
        let retained = owner.clone();
        let work = app.background_executor().spawn(async move {
            let failure = prepared.cancel();
            (services, original, failure)
        });
        app.spawn(async move |cx| {
            let (services, original, failure) = work.await;
            retained.borrow_mut().process.services = Some(services);
            *session_slot.borrow_mut() = Some(original);
            *settlement_slot.borrow_mut() = Some(CandidateSettlement::Services(Err(
                RecoveryServicePreparationError::App(failure),
            )));
            let _ = cx.update(|app| completed(&retained, app));
        })
        .detach();
        Ok(())
    }

    pub(crate) fn interrupted_exit_services_result(
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
            Some(CandidateSettlement::Services(result)) => result
                .as_ref()
                .map(|_| ())
                .map_err(|error| format!("{error:?}")),
            _ => Err("Interrupted Exit service preparation has not returned".into()),
        }
    }
}
