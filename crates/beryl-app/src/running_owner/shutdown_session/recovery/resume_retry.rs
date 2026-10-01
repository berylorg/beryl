use super::*;
use crate::exit_session::ResumeSessionOutcome;
use settlement::{CandidateSettlement, CandidateSettlementError};

impl RunningProcessOwner {
    pub(crate) fn retry_interrupted_exit_resume(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<(), String> {
        let (session_slot, settlement_slot, previous_slot, mut original, mut candidate) = {
            let owner = owner.borrow();
            owner.interrupted_exit_graph_retirement_result(request)?;
            let recovery = owner.interrupted_exit.as_ref().unwrap();
            if recovery.resident.is_some()
                || recovery
                    .pending_resident_frame
                    .as_ref()
                    .is_some_and(|wake| wake.strong_count() != 0)
                || recovery.previous_resume.borrow().is_some()
            {
                return Err("Interrupted Exit resume retry custody is unavailable".into());
            }
            let mut session = recovery.session.borrow_mut();
            let Some(RunningShutdownSession::Resuming(resume)) = session.as_ref() else {
                return Err("Interrupted Exit resume is unavailable".into());
            };
            if resume
                .outcome()
                .and_then(ResumeSessionOutcome::known_commit)
                != Some(false)
            {
                return Err("Interrupted Exit resume noncommit is unproven".into());
            }
            let mut settlement = recovery.settlement.borrow_mut();
            if !matches!(
                settlement.as_ref(),
                Some(CandidateSettlement::Returned { result: Err(_), .. })
            ) {
                return Err("Interrupted Exit has no failed candidate settlement".into());
            }
            let Some(CandidateSettlement::Returned { candidate, .. }) =
                settlement.replace(CandidateSettlement::Pending)
            else {
                unreachable!()
            };
            (
                recovery.session.clone(),
                recovery.settlement.clone(),
                recovery.previous_resume.clone(),
                session.take().unwrap(),
                candidate,
            )
        };
        let retained = owner.clone();
        let work = app.background_executor().spawn(async move {
            let mut previous = None;
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let RunningShutdownSession::Resuming(resume) = &mut original else {
                    unreachable!()
                };
                resume.retry(&mut candidate.candidate, &candidate.session, &mut previous)?;
                original.converge_candidate(&mut candidate.candidate, &candidate.session)
            }))
            .unwrap_or_else(|_| Err("Interrupted Exit resume retry unwound".into()))
            .map_err(CandidateSettlementError::Candidate);
            (original, candidate, previous, result)
        });
        app.spawn(async move |cx| {
            let (original, candidate, previous, result) = work.await;
            *session_slot.borrow_mut() = Some(original);
            *previous_slot.borrow_mut() = previous;
            *settlement_slot.borrow_mut() =
                Some(CandidateSettlement::Returned { candidate, result });
            let _ = cx.update(|app| completed(&retained, app));
        })
        .detach();
        Ok(())
    }

    pub(crate) fn take_previous_interrupted_exit_resume(
        &mut self,
        request: &RunningExitRequest,
    ) -> Result<ResumeSessionOutcome, String> {
        self.interrupted_exit_graph_retirement_result(request)?;
        let recovery = self.interrupted_exit.as_ref().unwrap();
        if recovery.session.borrow().is_none() {
            return Err("Interrupted Exit resume is on a worker".into());
        }
        recovery
            .previous_resume
            .borrow_mut()
            .take()
            .ok_or_else(|| "No previous resume outcome is retained".into())
    }
}
