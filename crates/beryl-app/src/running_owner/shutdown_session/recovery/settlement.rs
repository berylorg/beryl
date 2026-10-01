use super::*;
use beryl_home_store::HomeRecoveryCandidate;
use beryl_state::SessionState;

pub(crate) struct InterruptedExitCandidate {
    pub(crate) candidate: HomeRecoveryCandidate,
    pub(crate) session: SessionState,
}

pub(super) enum CandidateSettlement {
    Pending,
    Published,
    DisposedFailure(CandidateSettlementError),
    Constructed(Result<HomeRecoveryCandidate, crate::app_services::RetiredHomeRecoveryError>),
    Services(
        Result<
            crate::app_services::recovery_graph::PreparedRecoveryServiceGraph,
            crate::app_services::recovery_graph::RecoveryServicePreparationError,
        >,
    ),
    Returned {
        candidate: InterruptedExitCandidate,
        result: Result<(), CandidateSettlementError>,
    },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CandidateSettlementError {
    #[error("{0}")]
    Candidate(String),
    #[error(transparent)]
    ProcessWork(#[from] crate::app_services::RetiredProcessWorkError),
}

impl RunningProcessOwner {
    pub(crate) fn settle_interrupted_exit_candidate(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        candidate: &mut Option<InterruptedExitCandidate>,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<(), String> {
        Self::settle_interrupted_exit_candidate_with(
            owner,
            request,
            candidate,
            app,
            completed,
            || {},
        )
    }

    fn settle_interrupted_exit_candidate_with(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        candidate: &mut Option<InterruptedExitCandidate>,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
        before_settle: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        let (session_slot, settlement_slot, original, candidate) = {
            let owner = owner.borrow();
            let recovery = owner
                .interrupted_exit
                .as_ref()
                .ok_or("No reported failed Exit")?;
            if !Rc::ptr_eq(&recovery.request, &request.identity())
                || !owner.process.commands.is_active(request)
            {
                return Err("Interrupted Exit request changed".into());
            }
            if recovery.settlement.borrow().is_some() || recovery.resident.is_some() {
                return Err("Interrupted Exit candidate settlement is already retained".into());
            }
            owner.interrupted_exit_graph_retirement_result(request)?;
            if recovery.session.borrow().is_none() || candidate.is_none() {
                return Err("Interrupted Exit candidate or original outcome is unavailable".into());
            }
            let original = recovery.session.borrow_mut().take().unwrap();
            *recovery.settlement.borrow_mut() = Some(CandidateSettlement::Pending);
            (
                recovery.session.clone(),
                recovery.settlement.clone(),
                original,
                candidate.take().unwrap(),
            )
        };
        Self::run_interrupted_exit_candidate_pass(
            owner,
            session_slot,
            settlement_slot,
            original,
            candidate,
            None,
            app,
            completed,
            move |original, candidate, _| {
                before_settle();
                original
                    .converge_candidate(&mut candidate.candidate, &candidate.session)
                    .map_err(CandidateSettlementError::Candidate)
            },
        );
        Ok(())
    }

    pub(crate) fn revalidate_interrupted_exit_candidate(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<(), String> {
        Self::revalidate_interrupted_exit_candidate_with(owner, request, app, completed, |_| {})
    }

    fn revalidate_interrupted_exit_candidate_with(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
        before_validate: impl FnOnce(&mut InterruptedExitCandidate) + Send + 'static,
    ) -> Result<(), String> {
        let (session_slot, settlement_slot, original, candidate) = {
            let owner = owner.borrow();
            owner.interrupted_exit_graph_retirement_result(request)?;
            let recovery = owner.interrupted_exit.as_ref().unwrap();
            if recovery.resident.is_some()
                || recovery.session.borrow().is_none()
                || recovery
                    .pending_resident_frame
                    .as_ref()
                    .is_some_and(|wake| wake.strong_count() != 0)
            {
                return Err("Interrupted Exit recovery custody is unavailable".into());
            }
            let mut settlement = recovery.settlement.borrow_mut();
            if !matches!(
                settlement.as_ref(),
                Some(CandidateSettlement::Returned { result: Ok(()), .. })
            ) {
                return Err("Interrupted Exit candidate has no successful settlement".into());
            }
            let Some(CandidateSettlement::Returned { candidate, .. }) =
                settlement.replace(CandidateSettlement::Pending)
            else {
                unreachable!()
            };
            let original = recovery.session.borrow_mut().take().unwrap();
            (
                recovery.session.clone(),
                recovery.settlement.clone(),
                original,
                candidate,
            )
        };
        Self::run_interrupted_exit_candidate_pass(
            owner,
            session_slot,
            settlement_slot,
            original,
            candidate,
            None,
            app,
            completed,
            move |original, candidate, _| {
                before_validate(candidate);
                original
                    .revalidate_candidate(&mut candidate.candidate, &candidate.session)
                    .map_err(CandidateSettlementError::Candidate)
            },
        );
        Ok(())
    }

    pub(super) fn run_interrupted_exit_candidate_pass(
        owner: &Rc<RefCell<Self>>,
        session_slot: Rc<RefCell<Option<RunningShutdownSession>>>,
        settlement_slot: Rc<RefCell<Option<CandidateSettlement>>>,
        mut original: RunningShutdownSession,
        mut candidate: InterruptedExitCandidate,
        services: Option<crate::app_services::ProcessServiceOwner>,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
        operation: impl FnOnce(
            &mut RunningShutdownSession,
            &mut InterruptedExitCandidate,
            Option<&crate::app_services::ProcessServiceOwner>,
        ) -> Result<(), CandidateSettlementError>
        + Send
        + 'static,
    ) {
        let retained = owner.clone();
        let work = app.background_executor().spawn(async move {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                operation(&mut original, &mut candidate, services.as_ref())
            }))
            .unwrap_or_else(|_| {
                Err(CandidateSettlementError::Candidate(
                    "Interrupted Exit candidate settlement unwound".into(),
                ))
            });
            (original, candidate, services, result)
        });
        app.spawn(async move |cx| {
            let (original, candidate, services, result) = work.await;
            if let Some(services) = services {
                retained.borrow_mut().process.services = Some(services);
            }
            *session_slot.borrow_mut() = Some(original);
            *settlement_slot.borrow_mut() =
                Some(CandidateSettlement::Returned { candidate, result });
            let _ = cx.update(|app| completed(&retained, app));
        })
        .detach();
    }

    #[cfg(test)]
    pub(crate) fn test_revalidate_interrupted_exit_candidate(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
        before_validate: impl FnOnce(&mut InterruptedExitCandidate) + Send + 'static,
    ) -> Result<(), String> {
        Self::revalidate_interrupted_exit_candidate_with(
            owner,
            request,
            app,
            completed,
            before_validate,
        )
    }

    pub(crate) fn interrupted_exit_candidate_result(
        &self,
        request: &RunningExitRequest,
    ) -> Result<(), String> {
        let recovery = self
            .interrupted_exit
            .as_ref()
            .ok_or("No reported failed Exit")?;
        if !Rc::ptr_eq(&recovery.request, &request.identity())
            || !self.process.commands.is_active(request)
        {
            return Err("Interrupted Exit request changed".into());
        }
        match recovery.settlement.borrow().as_ref() {
            Some(CandidateSettlement::DisposedFailure(error)) => Err(error.to_string()),
            Some(CandidateSettlement::Returned { result, .. }) => {
                result.as_ref().map(|_| ()).map_err(ToString::to_string)
            }
            _ => Err("Interrupted Exit candidate settlement has not returned".into()),
        }
    }

    #[cfg(test)]
    pub(crate) fn test_replace_interrupted_exit_request(&mut self, request: &RunningExitRequest) {
        self.interrupted_exit.as_mut().unwrap().request = request.identity();
    }

    #[cfg(test)]
    pub(crate) fn test_take_interrupted_exit_candidate(&self) -> InterruptedExitCandidate {
        match self
            .interrupted_exit
            .as_ref()
            .unwrap()
            .settlement
            .borrow_mut()
            .take()
            .unwrap()
        {
            CandidateSettlement::Returned { candidate, .. } => candidate,
            _ => panic!("candidate settlement unavailable"),
        }
    }

    #[cfg(test)]
    pub(crate) fn test_settle_interrupted_exit_candidate(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        candidate: &mut Option<InterruptedExitCandidate>,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
        before_settle: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        Self::settle_interrupted_exit_candidate_with(
            owner,
            request,
            candidate,
            app,
            completed,
            before_settle,
        )
    }
}
