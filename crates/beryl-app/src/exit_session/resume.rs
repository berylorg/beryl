use super::{
    ExitSessionExecution, ExitSessionPublication, ExitSessionReconciled, ExitSessionValidation,
};
use beryl_home_store::{
    CommandError, CommandOutcome, CommitReceipt, CommittedLocalFinalization, HomeCommand,
    HomeRecoveryCandidate, ReconciliationFailure, ReconciliationHandle, ReconciliationResolution,
};
use beryl_model::SessionRevision;
use beryl_state::{ResumeSessionAfterExit, SessionState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResumeSessionValidation {
    UnchangedExit,
    ResumedRunning,
}

#[derive(Debug)]
pub(crate) enum InterruptedExit {
    Executed(ExitSessionExecution),
    Reconciled(ExitSessionReconciled),
}

impl InterruptedExit {
    pub(crate) fn publication(&self) -> &ExitSessionPublication {
        match self {
            Self::Executed(value) => value.publication(),
            Self::Reconciled(value) => value.publication(),
        }
    }
}

#[derive(Debug)]
pub(crate) enum ResumeSessionOutcome {
    NotCommitted {
        evidence: CommandError,
    },
    Committed {
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
        local_finalization: Option<CommittedLocalFinalization>,
    },
    Indeterminate {
        original_failure: CommandError,
        handle: ReconciliationHandle,
        reconciliation: Option<Result<ReconciliationResolution, ReconciliationFailure>>,
    },
}

impl ResumeSessionOutcome {
    pub(crate) fn known_commit(&self) -> Option<bool> {
        match self {
            Self::NotCommitted { .. } => Some(false),
            Self::Committed { .. } => Some(true),
            Self::Indeterminate {
                reconciliation: Some(Ok(ReconciliationResolution::ExactOld)),
                ..
            } => Some(false),
            Self::Indeterminate {
                reconciliation: Some(Ok(ReconciliationResolution::ExactNew { .. })),
                ..
            } => Some(true),
            _ => None,
        }
    }
}

#[derive(Debug)]
#[must_use]
pub(crate) struct InterruptedExitResume {
    exit: InterruptedExit,
    result_revision: Option<SessionRevision>,
    outcome: Option<ResumeSessionOutcome>,
}

impl InterruptedExitResume {
    pub(crate) fn new(exit: InterruptedExit) -> Self {
        Self {
            exit,
            result_revision: None,
            outcome: None,
        }
    }

    pub(crate) fn exit(&self) -> &InterruptedExit {
        &self.exit
    }

    pub(crate) fn outcome(&self) -> Option<&ResumeSessionOutcome> {
        self.outcome.as_ref()
    }

    pub(crate) fn result_revision(&self) -> Option<SessionRevision> {
        self.result_revision
    }

    pub(crate) fn validate_candidate(
        &self,
        candidate: &mut HomeRecoveryCandidate,
        session: &SessionState,
    ) -> Result<ResumeSessionValidation, super::ExitSessionValidationError> {
        use super::ExitSessionValidationError as Error;
        use beryl_state::SessionExitIntent;
        let result_revision = self.result_revision.ok_or(Error::Unproven)?;
        let committed = self
            .outcome
            .as_ref()
            .and_then(ResumeSessionOutcome::known_commit)
            .ok_or(Error::Unproven)?;
        let evidence = self.exit.publication();
        let (intent, revision, validated) = if committed {
            (
                SessionExitIntent::Running,
                result_revision,
                ResumeSessionValidation::ResumedRunning,
            )
        } else {
            (
                SessionExitIntent::OrderlyExit,
                evidence.result_session_revision,
                ResumeSessionValidation::UnchangedExit,
            )
        };
        super::validation::validate_published(evidence, intent, revision, candidate, session)?;
        Ok(validated)
    }

    pub(crate) fn execute(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        session: &SessionState,
    ) -> Result<(), String> {
        if self.result_revision.is_some() {
            return Err("Session resume has already been attempted".into());
        }
        self.execute_prepared(candidate, session, None)
    }

    pub(crate) fn retry(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        session: &SessionState,
        previous: &mut Option<ResumeSessionOutcome>,
    ) -> Result<(), String> {
        if previous.is_some() {
            return Err("Previous resume outcome custody is occupied".into());
        }
        self.execute_prepared(candidate, session, Some(previous))
    }

    fn execute_prepared(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        session: &SessionState,
        previous: Option<&mut Option<ResumeSessionOutcome>>,
    ) -> Result<(), String> {
        let home_revision = candidate
            .recovery_access()
            .map_err(|e| e.to_string())?
            .home_revision()
            .map_err(|e| e.to_string())?;
        if previous.is_some() {
            if self
                .validate_candidate(candidate, session)
                .map_err(|e| e.to_string())?
                != ResumeSessionValidation::UnchangedExit
            {
                return Err("Committed session resume cannot be repeated".into());
            }
        } else {
            let validated = match &self.exit {
                InterruptedExit::Executed(value) => value.validate_candidate(candidate, session),
                InterruptedExit::Reconciled(value) => value.validate_candidate(candidate, session),
            }
            .map_err(|e| e.to_string())?;
            if validated != ExitSessionValidation::CommittedExit {
                return Err("Noncommitted Exit needs no session resume".into());
            }
        }
        let publication = self.exit.publication();
        let result_revision = publication
            .result_session_revision
            .checked_next()
            .map_err(|_| "Session resume revision overflow")?;
        let access = candidate.recovery_access().map_err(|e| e.to_string())?;
        let domain_revision = session
            .revision_candidate(&access)
            .map_err(|e| e.to_string())?;
        let mutation = ResumeSessionAfterExit::new(
            publication.result_session_revision,
            publication
                .result_windows
                .iter()
                .map(|(id, revision, _)| (*id, *revision))
                .collect(),
        )
        .map_err(|e| e.to_string())?;
        let mut command = HomeCommand::new(home_revision);
        command
            .add(session.resume_after_exit(domain_revision, mutation))
            .map_err(|e| e.to_string())?;
        if access.home_revision().map_err(|e| e.to_string())? != home_revision {
            return Err("Home changed during session resume preparation".into());
        }
        if let Some(previous) = previous {
            *previous = self.outcome.take();
        }
        self.result_revision = Some(result_revision);
        self.outcome = Some(match access.execute(command) {
            CommandOutcome::NotCommitted { evidence } => {
                ResumeSessionOutcome::NotCommitted { evidence }
            }
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            } => ResumeSessionOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            },
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => ResumeSessionOutcome::Indeterminate {
                original_failure: failure,
                handle: reconciliation.install_and_handle(),
                reconciliation: None,
            },
        });
        Ok(())
    }

    pub(crate) fn reconcile(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
    ) -> Result<(), String> {
        let Some(ResumeSessionOutcome::Indeterminate {
            handle,
            reconciliation,
            ..
        }) = &mut self.outcome
        else {
            return Err("Session resume has no pending reconciliation".into());
        };
        if matches!(reconciliation, Some(Ok(_))) {
            return Err("Session resume reconciliation is already settled".into());
        }
        let access = candidate.recovery_access().map_err(|e| e.to_string())?;
        *reconciliation = Some(access.reconcile(handle));
        Ok(())
    }
}
