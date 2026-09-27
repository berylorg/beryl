use super::*;
use crate::startup_owner::RunningExitRequest;

pub(in crate::running_owner) struct InterruptedExitRecovery {
    request: Rc<()>,
    session: Option<RunningShutdownSession>,
}

impl RunningProcessOwner {
    pub(in crate::running_owner) fn retain_reported_exit_failure(
        &mut self,
        request: &RunningExitRequest,
    ) {
        if self.interrupted_exit.is_some()
            || !self.process.commands.is_active(request)
            || self.shutdown_session().is_none()
        {
            return;
        }
        self.interrupted_exit = Some(InterruptedExitRecovery {
            request: request.identity(),
            session: None,
        });
    }

    pub(crate) fn retain_interrupted_exit_session(
        &mut self,
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
        if recovery.session.is_some() {
            return Err("Interrupted Exit session is already retained".into());
        }
        if !matches!(
            self.shutdown_session(),
            Some(
                RunningShutdownSession::Settled(_)
                    | RunningShutdownSession::Reconciled(_)
                    | RunningShutdownSession::Unwound
            )
        ) {
            return Err("Interrupted Exit session work has not settled".into());
        }
        let session = self
            .shutdown
            .as_mut()
            .unwrap()
            .session
            .replace(RunningShutdownSession::RecoveryOwned);
        self.interrupted_exit.as_mut().unwrap().session = session;
        Ok(())
    }

    pub(crate) fn interrupted_exit_session(&self) -> Option<&RunningShutdownSession> {
        self.interrupted_exit.as_ref()?.session.as_ref()
    }
}

impl RunningShutdownSession {
    pub(crate) fn settle_candidate(
        &mut self,
        candidate: &mut beryl_home_store::HomeRecoveryCandidate,
        session: &beryl_state::SessionState,
    ) -> Result<
        crate::exit_session::ExitSessionValidation,
        crate::exit_session::ExitSessionValidationError,
    > {
        match self {
            Self::Settled(Ok(ExitSessionExecution::Indeterminate(pending)))
            | Self::Reconciled(ExitSessionReconciled::Pending {
                reconciliation: pending,
                ..
            }) => pending.settle_candidate(candidate, session),
            Self::Settled(Ok(outcome)) => outcome.validate_candidate(candidate, session),
            Self::Reconciled(outcome) => outcome.validate_candidate(candidate, session),
            _ => Err(crate::exit_session::ExitSessionValidationError::Unproven),
        }
    }
}
