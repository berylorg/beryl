use super::*;
use crate::app_services::{
    AppServiceConfiguration, recovery_graph::RecoveryServicePreparationError,
};
use beryl_home_store::{CommandCancellation, HomeGeneration};
use settlement::{CandidateSettlement, CandidateSettlementError};
use syndic_storage::SyndicTimestamp;

impl RunningProcessOwner {
    pub(crate) fn interrupted_exit_composer_adapters(
        &self,
        request: &impl RecoveryIdentity,
        home: beryl_model::BerylHomeId,
        generation: HomeGeneration,
        requirement: beryl_home_store::TurnStartAdmissionRequirement,
    ) -> Result<crate::app_services::recovery_composer::PreparedComposerRecoveryAdapters, String>
    {
        self.interrupted_exit_graph_retirement_result(request)?;
        let recovery = self.interrupted_exit.as_ref().unwrap();
        if recovery.session.borrow().is_none() {
            return Err("Interrupted Exit session custody is unavailable".into());
        }
        let mut settlement = recovery.settlement.borrow_mut();
        let Some(CandidateSettlement::Services(Ok(graph))) = settlement.as_mut() else {
            return Err("Interrupted Exit has no prepared service graph".into());
        };
        graph.composer_recovery_adapters(home, generation, requirement)
    }

    pub(crate) fn take_interrupted_exit_preparation_failure(
        &mut self,
        request: &impl RecoveryIdentity,
        generation: HomeGeneration,
    ) -> Result<RecoveryServicePreparationError, String> {
        self.return_interrupted_exit_preparation_home(request, generation)?;
        let recovery = self.interrupted_exit.as_mut().unwrap();
        let Some(CandidateSettlement::DisposedPreparationFailure { failure, .. }) =
            recovery.settlement.borrow_mut().take()
        else {
            unreachable!("validated preparation failure retains exclusive custody")
        };
        Ok(failure)
    }

    pub(super) fn return_interrupted_exit_preparation_home(
        &mut self,
        request: &impl RecoveryIdentity,
        generation: HomeGeneration,
    ) -> Result<(), String> {
        self.interrupted_exit_graph_retirement_result(request)?;
        let recovery = self.interrupted_exit.as_mut().unwrap();
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
        if let Some(CandidateSettlement::DisposedPreparationFailure { retired, .. }) =
            settlement.as_ref()
        {
            return if *retired == generation {
                Ok(())
            } else {
                Err("Interrupted Exit retired generation changed".into())
            };
        }
        let Some(CandidateSettlement::Services(Err(failure))) = settlement.as_mut() else {
            return Err("Interrupted Exit service preparation has no retained failure".into());
        };
        self.process
            .services
            .as_mut()
            .ok_or("The complete service owner is on a worker")?
            .return_recovery_preparation_home(generation, failure)
            .map_err(|error| error.to_string())?;
        let Some(CandidateSettlement::Services(Err(failure))) = settlement.take() else {
            unreachable!("returned preparation home retains exclusive failure custody")
        };
        *settlement = Some(CandidateSettlement::DisposedPreparationFailure {
            retired: generation,
            failure,
        });
        recovery.reopen_deadline =
            Some(std::time::Instant::now() + recovery.reopen_schedule.next_delay());
        Ok(())
    }

    pub(crate) fn prepare_interrupted_exit_services(
        owner: &Rc<RefCell<Self>>,
        request: &impl RecoveryIdentity,
        generation: HomeGeneration,
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<(), String> {
        let (mut services, session_slot, settlement_slot, original, candidate) = {
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
            let settled = match services
                .prepare_failed_claims(
                    &mut candidate.candidate,
                    &mut original,
                    &worker_cancellation,
                )
                .and_then(|()| {
                    original
                        .revalidate_candidate(&mut candidate.candidate, &candidate.session)
                        .and_then(|()| {
                            services.prepare_failed_residents(
                                &mut candidate.candidate,
                                &original,
                                at,
                                &worker_cancellation,
                            )
                        })
                }) {
                Err(error) => CandidateSettlement::Returned {
                    candidate,
                    result: Err(CandidateSettlementError::Candidate(error)),
                },
                Ok(()) => {
                    let mut storage = Some(candidate.candidate);
                    let result = services
                        .prepare_recovery_service_graph(
                            generation,
                            &mut storage,
                            configuration,
                            at,
                            &worker_cancellation,
                        )
                        .map(|mut graph| {
                            graph.retain_recovered_window(&original);
                            graph
                        });
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
            Box::new((services, original, settled))
        });
        app.spawn(async move |cx| {
            let (services, original, mut settled) = *work.await;
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
        request: &impl RecoveryIdentity,
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
            let mut services = services;
            let mut prepared = prepared;
            if let Err(error) = services.retain_cancelled_claim_editors(&mut prepared) {
                return (services, original, Err((prepared, error)));
            }
            services.retain_cancelled_failed_residents(&mut prepared);
            if let Err(error) = services.retain_cancelled_first_conversation(&mut prepared) {
                return (services, original, Err((prepared, error)));
            }
            let failure = prepared.cancel();
            (services, original, Ok(failure))
        });
        app.spawn(async move |cx| {
            let (services, original, failure) = work.await;
            retained.borrow_mut().process.services = Some(services);
            *session_slot.borrow_mut() = Some(original);
            *settlement_slot.borrow_mut() = Some(match failure {
                Ok(failure) => CandidateSettlement::Services(Err(
                    RecoveryServicePreparationError::App(failure),
                )),
                Err((prepared, _error)) => CandidateSettlement::Services(Ok(prepared)),
            });
            let _ = cx.update(|app| completed(&retained, app));
        })
        .detach();
        Ok(())
    }

    pub(crate) fn interrupted_exit_services_result(
        &self,
        request: &impl RecoveryIdentity,
    ) -> Result<(), String> {
        self.interrupted_exit_graph_retirement_result(request)?;
        let recovery = self.interrupted_exit.as_ref().unwrap();
        if let Some(services) = self.process.services.as_ref() {
            if services.has_returned_claim_graph() {
                let mut slot = recovery.settlement.borrow_mut();
                if !matches!(slot.as_ref(), None | Some(CandidateSettlement::Pending))
                    || recovery.session.borrow().is_none()
                    || recovery.resident.is_some()
                {
                    return Err(
                        "returned New Thread graph cannot replace active recovery custody".into(),
                    );
                }
                if let Some(graph) = services.take_returned_claim_graph() {
                    *slot = Some(CandidateSettlement::Services(Ok(graph)));
                }
            }
        }
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
            Some(CandidateSettlement::DisposedPreparationFailure { failure, .. }) => {
                Err(format!("{failure:?}"))
            }
            _ => Err("Interrupted Exit service preparation has not returned".into()),
        }
    }
}
