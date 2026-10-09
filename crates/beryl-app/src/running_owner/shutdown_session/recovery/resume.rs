use super::*;
use crate::exit_session::{
    ExitSessionValidation, InterruptedExit, InterruptedExitResume, ResumeSessionOutcome,
    ResumeSessionValidation,
};
use beryl_home_store::HomeRecoveryCandidate;
use beryl_state::SessionState;

impl RunningShutdownSession {
    pub(crate) fn accept_original_claim_candidate(
        &mut self,
        window: beryl_model::WindowId,
        committed: Option<crate::app_services::RetiredClaimCommit<'_>>,
        candidate: &mut HomeRecoveryCandidate,
        state: &beryl_state::BerylState,
    ) -> Result<(), String> {
        let Self::UnchangedRunning(running) = self else {
            return Err(
                "original thread creation requires its captured Running recovery session".into(),
            );
        };
        running.accept_original_claim_candidate(window, committed, candidate, state)
    }

    pub(crate) fn revalidate_candidate(
        &self,
        candidate: &mut HomeRecoveryCandidate,
        session: &SessionState,
    ) -> Result<(), String> {
        if let Self::UnchangedRunning(running) = self {
            return running.revalidate(candidate, session);
        }
        if let Self::RemovedWindow(close) = self {
            return close.revalidate_candidate(candidate, session);
        }
        if let Self::UnremovedWindows(windows) = self {
            return windows.validate(candidate, session);
        }
        let running = match self {
            Self::Settled(Ok(outcome)) => outcome
                .validate_candidate(candidate, session)
                .map(|state| state == ExitSessionValidation::UnchangedRunning),
            Self::Reconciled(outcome) => outcome
                .validate_candidate(candidate, session)
                .map(|state| state == ExitSessionValidation::UnchangedRunning),
            Self::Resuming(resume) => resume
                .validate_candidate(candidate, session)
                .map(|state| state == ResumeSessionValidation::ResumedRunning),
            _ => return Err("Interrupted Exit session outcome remains unproven".into()),
        }
        .map_err(|error| error.to_string())?;
        if !running {
            return Err("Interrupted Exit candidate has not resumed Running".into());
        }
        Ok(())
    }

    pub(crate) fn converge_candidate(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        session: &SessionState,
    ) -> Result<(), String> {
        if let Self::UnchangedRunning(running) = self {
            return running.converge(candidate, session);
        }
        if let Self::RemovedWindow(close) = self {
            return close.converge_candidate(candidate, session);
        }
        if let Self::UnremovedWindows(windows) = self {
            return windows.validate(candidate, session);
        }
        if !matches!(self, Self::Resuming(_)) {
            match self
                .settle_candidate(candidate, session)
                .map_err(|error| error.to_string())?
            {
                ExitSessionValidation::UnchangedRunning => return Ok(()),
                ExitSessionValidation::CommittedExit => {}
            }
            let original = match std::mem::replace(self, Self::RecoveryOwned) {
                Self::Settled(Ok(value)) => InterruptedExit::Executed(value),
                Self::Reconciled(value) => InterruptedExit::Reconciled(value),
                _ => unreachable!("validated original Exit outcome"),
            };
            *self = Self::Resuming(InterruptedExitResume::new(original));
        }
        let Self::Resuming(resume) = self else {
            unreachable!()
        };
        if resume.result_revision().is_none() {
            resume.execute(candidate, session)?;
        }
        if matches!(
            resume.outcome(),
            Some(ResumeSessionOutcome::Indeterminate {
                reconciliation: None,
                ..
            })
        ) {
            resume.reconcile(candidate)?;
        }
        match resume
            .validate_candidate(candidate, session)
            .map_err(|error| error.to_string())?
        {
            ResumeSessionValidation::ResumedRunning => Ok(()),
            ResumeSessionValidation::UnchangedExit => Err("Session resume did not commit".into()),
        }
    }
}
