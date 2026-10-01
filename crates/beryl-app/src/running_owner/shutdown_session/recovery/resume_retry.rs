use super::*;
use crate::exit_session::ResumeSessionOutcome;
use settlement::{CandidateSettlement, CandidateSettlementError};

pub(super) enum ResumeRetry {
    Command,
    Reconciliation,
}

impl RunningProcessOwner {
    pub(super) fn carried_interrupted_exit_resume_retry(
        &self,
        request: &RunningExitRequest,
    ) -> Result<Option<ResumeRetry>, String> {
        self.interrupted_exit_graph_retirement_result(request)?;
        let recovery = self.interrupted_exit.as_ref().unwrap();
        let session = recovery.session.borrow();
        let Some(session) = session.as_ref() else {
            return Err("Interrupted Exit resume is on a worker".into());
        };
        let RunningShutdownSession::Resuming(resume) = session else {
            return Ok(None);
        };
        let retry = if resume
            .outcome()
            .and_then(ResumeSessionOutcome::known_commit)
            == Some(false)
        {
            Some(ResumeRetry::Command)
        } else if resume.can_retry_reconciliation() {
            Some(ResumeRetry::Reconciliation)
        } else {
            None
        };
        if retry.is_some() && recovery.previous_resume.borrow().is_some() {
            return Err("Interrupted Exit resume retry custody is unavailable".into());
        }
        Ok(retry)
    }

    pub(crate) fn retry_interrupted_exit_resume(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<(), String> {
        Self::retry_interrupted_exit_resume_pass(
            owner,
            request,
            ResumeRetry::Command,
            app,
            completed,
        )
    }

    pub(crate) fn retry_interrupted_exit_resume_reconciliation(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<(), String> {
        Self::retry_interrupted_exit_resume_pass(
            owner,
            request,
            ResumeRetry::Reconciliation,
            app,
            completed,
        )
    }

    pub(super) fn retry_interrupted_exit_resume_pass(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        action: ResumeRetry,
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
            match action {
                ResumeRetry::Command
                    if resume
                        .outcome()
                        .and_then(ResumeSessionOutcome::known_commit)
                        != Some(false) =>
                {
                    return Err("Interrupted Exit resume noncommit is unproven".into());
                }
                ResumeRetry::Reconciliation if !resume.can_retry_reconciliation() => {
                    return Err(
                        "Interrupted Exit resume reconciliation retry is unavailable".into(),
                    );
                }
                _ => {}
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
                match action {
                    ResumeRetry::Command => {
                        resume.retry(&mut candidate.candidate, &candidate.session, &mut previous)?
                    }
                    ResumeRetry::Reconciliation => {
                        resume.retry_reconciliation(&mut candidate.candidate)?
                    }
                }
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

    pub(crate) fn take_previous_interrupted_exit_resume_reconciliation(
        &mut self,
        request: &RunningExitRequest,
    ) -> Result<beryl_home_store::ReconciliationFailure, String> {
        self.interrupted_exit_graph_retirement_result(request)?;
        let recovery = self.interrupted_exit.as_ref().unwrap();
        let mut session = recovery.session.borrow_mut();
        let Some(RunningShutdownSession::Resuming(resume)) = session.as_mut() else {
            return Err("Interrupted Exit resume is unavailable".into());
        };
        resume
            .take_previous_reconciliation()
            .ok_or_else(|| "No previous resume reconciliation failure is retained".into())
    }
}
