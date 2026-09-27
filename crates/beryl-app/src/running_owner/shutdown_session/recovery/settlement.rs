use super::*;
use crate::exit_session::ExitSessionValidation;
use beryl_home_store::HomeRecoveryCandidate;
use beryl_state::SessionState;

pub(crate) struct InterruptedExitCandidate {
    pub(crate) candidate: HomeRecoveryCandidate,
    pub(crate) session: SessionState,
}

pub(super) enum CandidateSettlement {
    Pending,
    Returned {
        candidate: InterruptedExitCandidate,
        result: Result<ExitSessionValidation, String>,
    },
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
        let (session_slot, settlement_slot, mut original, mut candidate) = {
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
            if recovery.settlement.borrow().is_some() {
                return Err("Interrupted Exit candidate settlement is already retained".into());
            }
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
        let retained = owner.clone();
        let work = app.background_executor().spawn(async move {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                before_settle();
                original
                    .settle_candidate(&mut candidate.candidate, &candidate.session)
                    .map_err(|error| format!("{error:?}"))
            }))
            .unwrap_or_else(|_| Err("Interrupted Exit candidate settlement unwound".into()));
            (original, candidate, result)
        });
        app.spawn(async move |cx| {
            let (original, candidate, result) = work.await;
            *session_slot.borrow_mut() = Some(original);
            *settlement_slot.borrow_mut() =
                Some(CandidateSettlement::Returned { candidate, result });
            let _ = cx.update(|app| completed(&retained, app));
        })
        .detach();
        Ok(())
    }

    pub(crate) fn interrupted_exit_candidate_result(
        &self,
        request: &RunningExitRequest,
    ) -> Result<ExitSessionValidation, String> {
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
            Some(CandidateSettlement::Returned { result, .. }) => result.clone(),
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
            CandidateSettlement::Pending => panic!("candidate still in worker"),
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
