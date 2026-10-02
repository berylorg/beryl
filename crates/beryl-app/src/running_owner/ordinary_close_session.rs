use crate::exit_session::ResumeSessionOutcome;
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeRecoveryCandidate, HomeStore, ReconciliationResolution,
};
use beryl_model::WindowId;
use beryl_state::{SessionState, SessionWindowRemovalEvidence, SessionWindowRemovalState};

#[derive(Debug)]
pub(crate) struct OrdinaryCloseSession {
    home_id: beryl_model::BerylHomeId,
    canonical_path: std::path::PathBuf,
    evidence: SessionWindowRemovalEvidence,
    original: ResumeSessionOutcome,
    restoration: Option<ResumeSessionOutcome>,
    previous_restoration: Option<ResumeSessionOutcome>,
}

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../tests/unit/ordinary_close_session.rs"]
mod tests;

fn retain_outcome(outcome: CommandOutcome) -> ResumeSessionOutcome {
    match outcome {
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
    }
}

impl OrdinaryCloseSession {
    #[cfg(test)]
    pub(crate) fn test_outcome_status(&self) -> (Option<bool>, Option<bool>) {
        (
            self.original.known_commit(),
            self.restoration
                .as_ref()
                .and_then(ResumeSessionOutcome::known_commit),
        )
    }
    pub(crate) fn restore_healthy(
        &mut self,
        home: &HomeStore,
        session: &SessionState,
    ) -> Result<beryl_state::SessionWindowRecord, String> {
        if home.home_id() != self.home_id || home.canonical_path() != self.canonical_path {
            return Err("ordinary close restoration belongs to another home".into());
        }
        Self::reconcile_healthy(&mut self.original, home)?;
        if let Some(outcome) = self.restoration.as_mut() {
            Self::reconcile_healthy(outcome, home)?;
        }
        let expected = match self.original.known_commit() {
            Some(false) if self.restoration.is_none() => SessionWindowRemovalState::Original,
            Some(true) => {
                if self
                    .restoration
                    .as_ref()
                    .and_then(ResumeSessionOutcome::known_commit)
                    != Some(true)
                {
                    if self
                        .restoration
                        .as_ref()
                        .is_some_and(|outcome| outcome.known_commit() != Some(false))
                    {
                        return Err("ordinary close restoration outcome is unproven".into());
                    }
                    if session
                        .classify_window_removal(home, &self.evidence)
                        .map_err(|e| e.to_string())?
                        != SessionWindowRemovalState::Removed
                    {
                        return Err("ordinary close restoration source conflicts".into());
                    }
                    let revision = home.home_revision().map_err(|e| e.to_string())?;
                    let domain = session.revision(home).map_err(|e| e.to_string())?;
                    let contribution = session
                        .recover_removed_window(home, domain, &self.evidence)
                        .map_err(|e| e.to_string())?;
                    let mut command = HomeCommand::new(revision);
                    command.add(contribution).map_err(|e| e.to_string())?;
                    self.previous_restoration = self.restoration.take();
                    self.restoration = Some(retain_outcome(home.execute(command)));
                    Self::reconcile_healthy(self.restoration.as_mut().unwrap(), home)?;
                    if self
                        .restoration
                        .as_ref()
                        .and_then(ResumeSessionOutcome::known_commit)
                        != Some(true)
                    {
                        return Err("ordinary close restoration did not commit".into());
                    }
                }
                SessionWindowRemovalState::Recovered
            }
            _ => return Err("ordinary close removal outcome is unproven".into()),
        };
        if session
            .classify_window_removal(home, &self.evidence)
            .map_err(|e| e.to_string())?
            != expected
        {
            return Err("ordinary close restored membership or claim conflicts".into());
        }
        session
            .minimal_bootstrap(home)
            .map_err(|e| e.to_string())?
            .ok_or("ordinary close session is unavailable")?
            .windows()
            .iter()
            .find(|window| window.window_id() == self.evidence.window().window_id())
            .cloned()
            .ok_or_else(|| "ordinary close restored window is unavailable".into())
    }

    fn reconcile_healthy(
        outcome: &mut ResumeSessionOutcome,
        home: &HomeStore,
    ) -> Result<(), String> {
        if let ResumeSessionOutcome::Indeterminate {
            handle,
            reconciliation,
            ..
        } = outcome
        {
            if reconciliation.is_none() {
                *reconciliation = Some(home.reconcile(handle));
            }
        }
        if outcome.known_commit().is_none() {
            return Err("ordinary close reconciliation remains unresolved".into());
        }
        Ok(())
    }

    pub(crate) fn removal_evidence(&self) -> &SessionWindowRemovalEvidence {
        &self.evidence
    }

    pub(crate) fn execute(
        home: &HomeStore,
        session: &SessionState,
        window: WindowId,
    ) -> Result<Self, String> {
        Self::execute_with_admission_hook(home, session, window, |_| {})
    }

    pub(crate) fn execute_with_admission_hook(
        home: &HomeStore,
        session: &SessionState,
        window: WindowId,
        before_admission: impl FnOnce(&HomeStore),
    ) -> Result<Self, String> {
        let revision = home.home_revision().map_err(|e| e.to_string())?;
        let evidence = session
            .capture_window_removal(home, window)
            .map_err(|e| e.to_string())?;
        let domain = session.revision(home).map_err(|e| e.to_string())?;
        let contribution = session
            .remove_captured_window(home, domain, &evidence)
            .map_err(|e| e.to_string())?;
        if home.home_revision().map_err(|e| e.to_string())? != revision {
            return Err("ordinary close source changed during preparation".into());
        }
        let mut command = HomeCommand::new(revision);
        command.add(contribution).map_err(|e| e.to_string())?;
        before_admission(home);
        Ok(Self {
            home_id: home.home_id(),
            canonical_path: home.canonical_path().to_owned(),
            evidence,
            original: retain_outcome(home.execute(command)),
            restoration: None,
            previous_restoration: None,
        })
    }

    pub(crate) fn validate_healthy_noncommit(
        &self,
        home: &HomeStore,
        session: &SessionState,
    ) -> Result<(), String> {
        if home.home_id() != self.home_id
            || home.canonical_path() != self.canonical_path
            || home.health().state() != beryl_home_store::HomeHealthState::Healthy
            || self.original.known_commit() != Some(false)
            || self.restoration.is_some()
        {
            return Err("ordinary close does not have a healthy proven noncommit".into());
        }
        let current = session
            .capture_window_removal(home, self.evidence.window().window_id())
            .map_err(|e| e.to_string())?;
        if current.window() != self.evidence.window() || current.claim() != self.evidence.claim() {
            return Err("ordinary close affected window or paired claim changed".into());
        }
        Ok(())
    }

    pub(crate) fn is_proven_noncommit(&self) -> bool {
        self.original.known_commit() == Some(false) && self.restoration.is_none()
    }

    pub(crate) fn require_ready(
        &self,
        home: &HomeStore,
        session: &SessionState,
    ) -> Result<(), String> {
        let ResumeSessionOutcome::Committed {
            receipt,
            later_failure: None,
            local_finalization: None,
        } = &self.original
        else {
            return Err("ordinary close removal is not proven healthy and committed".into());
        };
        session
            .committed_revision(home, receipt)
            .map_err(|e| e.to_string())?
            .ok_or("ordinary close receipt does not affect the session")?;
        Ok(())
    }

    fn reconcile(
        outcome: &mut ResumeSessionOutcome,
        candidate: &mut HomeRecoveryCandidate,
    ) -> Result<(), String> {
        if let ResumeSessionOutcome::Indeterminate {
            handle,
            reconciliation,
            ..
        } = outcome
        {
            if !matches!(reconciliation, Some(Ok(_))) {
                let access = candidate.recovery_access().map_err(|e| e.to_string())?;
                *reconciliation = Some(if reconciliation.is_some() {
                    access.retry_reconciliation(handle)
                } else {
                    access.reconcile(handle)
                });
            }
            if !matches!(
                reconciliation,
                Some(Ok(
                    ReconciliationResolution::ExactOld | ReconciliationResolution::ExactNew { .. }
                ))
            ) {
                return Err(format!(
                    "ordinary close command outcome remains unproven: {reconciliation:?}"
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn revalidate_candidate(
        &self,
        candidate: &mut HomeRecoveryCandidate,
        session: &SessionState,
    ) -> Result<(), String> {
        let expected = match self.original.known_commit() {
            Some(false) if self.restoration.is_none() => SessionWindowRemovalState::Original,
            Some(true)
                if self
                    .restoration
                    .as_ref()
                    .and_then(ResumeSessionOutcome::known_commit)
                    == Some(true) =>
            {
                SessionWindowRemovalState::Recovered
            }
            _ => return Err("cancelled ordinary close has not restored durable membership".into()),
        };
        let access = candidate.recovery_access().map_err(|e| e.to_string())?;
        if session
            .classify_window_removal_candidate(&access, &self.evidence)
            .map_err(|e| e.to_string())?
            != expected
        {
            return Err("ordinary close candidate membership or claim conflicts".into());
        }
        Ok(())
    }

    pub(crate) fn converge_candidate(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        session: &SessionState,
    ) -> Result<(), String> {
        Self::reconcile(&mut self.original, candidate)?;
        if self.original.known_commit() == Some(false) {
            return self.revalidate_candidate(candidate, session);
        }
        if self.original.known_commit() != Some(true) {
            return Err("ordinary close removal outcome is unproven".into());
        }
        if let Some(outcome) = &mut self.restoration {
            Self::reconcile(outcome, candidate)?;
            if outcome.known_commit() == Some(true) {
                return self.revalidate_candidate(candidate, session);
            }
            if outcome.known_commit() != Some(false) {
                return Err("ordinary close restoration outcome is unproven".into());
            }
        }
        let access = candidate.recovery_access().map_err(|e| e.to_string())?;
        let revision = access.home_revision().map_err(|e| e.to_string())?;
        let domain = session
            .revision_candidate(&access)
            .map_err(|e| e.to_string())?;
        let contribution = session
            .recover_removed_window_candidate(&access, domain, &self.evidence)
            .map_err(|e| e.to_string())?;
        if access.home_revision().map_err(|e| e.to_string())? != revision {
            return Err("ordinary close restoration source changed".into());
        }
        let mut command = HomeCommand::new(revision);
        command.add(contribution).map_err(|e| e.to_string())?;
        self.previous_restoration = self.restoration.take();
        self.restoration = Some(retain_outcome(access.execute(command)));
        Self::reconcile(self.restoration.as_mut().unwrap(), candidate)?;
        self.revalidate_candidate(candidate, session)
    }

    pub(crate) fn restored_evidence(&self) -> Option<&SessionWindowRemovalEvidence> {
        (self.original.known_commit() == Some(true)
            && self
                .restoration
                .as_ref()
                .and_then(ResumeSessionOutcome::known_commit)
                == Some(true))
        .then_some(&self.evidence)
    }
}
