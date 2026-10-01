use super::*;
use beryl_home_store::HomeGeneration;
use settlement::{CandidateSettlement, CandidateSettlementError};

impl RunningProcessOwner {
    pub(crate) async fn dispose_and_take_interrupted_exit_candidate_failure(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        cx: &mut gpui::AsyncApp,
    ) -> Result<CandidateSettlementError, String> {
        Self::dispose_interrupted_exit_candidate_failure(owner, request, generation, cx).await?;
        owner
            .borrow_mut()
            .take_interrupted_exit_candidate_failure(request)
    }

    pub(super) async fn dispose_interrupted_exit_candidate_failure(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        cx: &mut gpui::AsyncApp,
    ) -> Result<(), String> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            Self::dispose_failed_interrupted_exit_candidate(
                owner,
                request,
                generation,
                app,
                move |_, _| {
                    let _ = sender.send(());
                },
            )
        })
        .map_err(|error| error.to_string())??;
        receiver
            .await
            .map_err(|_| "Interrupted Exit candidate disposal delivery is unavailable".into())
    }

    pub(crate) fn take_interrupted_exit_candidate_failure(
        &mut self,
        request: &RunningExitRequest,
    ) -> Result<CandidateSettlementError, String> {
        self.interrupted_exit_graph_retirement_result(request)?;
        let recovery = self.interrupted_exit.as_ref().unwrap();
        if recovery.session.borrow().is_none() || self.process.services.is_none() {
            return Err("Interrupted Exit recovery custody is unavailable".into());
        }
        let mut settlement = recovery.settlement.borrow_mut();
        if !matches!(
            settlement.as_ref(),
            Some(CandidateSettlement::DisposedFailure(_))
        ) {
            return Err("Interrupted Exit has no disposed candidate failure".into());
        }
        let Some(CandidateSettlement::DisposedFailure(failure)) = settlement.take() else {
            unreachable!("validated disposed failure retains exclusive custody")
        };
        Ok(failure)
    }

    pub(crate) fn dispose_failed_interrupted_exit_candidate(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<(), String> {
        Self::dispose_settled_interrupted_exit_candidate(
            owner, request, generation, false, app, completed,
        )
    }

    pub(super) fn dispose_settled_interrupted_exit_candidate(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        cancelled: bool,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<(), String> {
        let (mut services, candidate, failure, slot) = {
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
            let Some(CandidateSettlement::Returned { candidate, result }) = settlement.as_ref()
            else {
                return Err("Interrupted Exit has no failed settled candidate".into());
            };
            if result.is_ok() && !cancelled {
                return Err("Interrupted Exit has no failed settled candidate".into());
            }
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
            let Some(CandidateSettlement::Returned { candidate, result }) =
                settlement.replace(CandidateSettlement::Pending)
            else {
                unreachable!()
            };
            drop(settlement);
            (
                owner.process.services.take().unwrap(),
                candidate,
                result.err().unwrap_or(CandidateSettlementError::Cancelled),
                slot,
            )
        };
        let retained = owner.clone();
        let work = app.background_executor().spawn(async move {
            drop(candidate.session);
            let mut home = Some(candidate.candidate.abort());
            services
                .return_retired_service_home(generation, &mut home)
                .expect("exclusive validated retired custody accepts disposed candidate");
            (services, failure)
        });
        app.spawn(async move |cx| {
            let (services, failure) = work.await;
            {
                let mut owner = retained.borrow_mut();
                owner.process.services = Some(services);
                let recovery = owner.interrupted_exit.as_mut().unwrap();
                recovery.reopen_deadline =
                    Some(std::time::Instant::now() + recovery.reopen_schedule.next_delay());
            }
            *slot.borrow_mut() = Some(CandidateSettlement::DisposedFailure(failure));
            let _ = cx.update(|app| completed(&retained, app));
        })
        .detach();
        Ok(())
    }
}
