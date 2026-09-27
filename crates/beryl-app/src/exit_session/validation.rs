use super::{ExitSessionExecution, ExitSessionPublication, ExitSessionReconciled};
use beryl_home_store::HomeRecoveryCandidate;
use beryl_state::{SessionExitIntent, SessionState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExitSessionValidation {
    UnchangedRunning,
    CommittedExit,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ExitSessionValidationError {
    #[error("Exit session outcome remains unproven")]
    Unproven,
    #[error("Exit session evidence belongs to a different configured home")]
    ForeignHome,
    #[error("Exit session state no longer matches retained evidence")]
    Changed,
    #[error("Exit session recovery read failed: {0}")]
    Read(String),
}

impl ExitSessionExecution {
    pub(crate) fn validate_candidate(
        &self,
        candidate: &mut HomeRecoveryCandidate,
        session: &SessionState,
    ) -> Result<ExitSessionValidation, ExitSessionValidationError> {
        let committed = match self {
            Self::NotCommitted { .. } => false,
            Self::Committed { .. } => true,
            Self::Indeterminate(_) => return Err(ExitSessionValidationError::Unproven),
        };
        validate(self.publication(), committed, candidate, session)
    }
}

impl ExitSessionReconciled {
    pub(crate) fn validate_candidate(
        &self,
        candidate: &mut HomeRecoveryCandidate,
        session: &SessionState,
    ) -> Result<ExitSessionValidation, ExitSessionValidationError> {
        let committed = match self {
            Self::ExactOld { .. } => false,
            Self::ExactNew { .. } => true,
            Self::Pending { .. } | Self::Blocked { .. } => {
                return Err(ExitSessionValidationError::Unproven);
            }
        };
        validate(self.publication(), committed, candidate, session)
    }
}

fn validate(
    evidence: &ExitSessionPublication,
    committed: bool,
    candidate: &mut HomeRecoveryCandidate,
    session: &SessionState,
) -> Result<ExitSessionValidation, ExitSessionValidationError> {
    if committed {
        validate_published(
            evidence,
            SessionExitIntent::OrderlyExit,
            evidence.result_session_revision,
            candidate,
            session,
        )?;
        return Ok(ExitSessionValidation::CommittedExit);
    }
    let snapshot = read_candidate(evidence, candidate, session)?;
    if snapshot == evidence.source {
        Ok(ExitSessionValidation::UnchangedRunning)
    } else {
        Err(ExitSessionValidationError::Changed)
    }
}

fn read_candidate(
    evidence: &ExitSessionPublication,
    candidate: &mut HomeRecoveryCandidate,
    session: &SessionState,
) -> Result<beryl_state::MinimalSessionBootstrap, ExitSessionValidationError> {
    use ExitSessionValidationError as Error;
    if candidate.service_reference().configured_path() != evidence.configured_home {
        return Err(Error::ForeignHome);
    }
    let access = candidate
        .recovery_access()
        .map_err(|e| Error::Read(e.to_string()))?;
    session
        .minimal_bootstrap_candidate(&access)
        .map_err(|e| Error::Read(e.to_string()))?
        .ok_or(Error::Changed)
}

pub(super) fn validate_published(
    evidence: &ExitSessionPublication,
    intent: SessionExitIntent,
    revision: beryl_model::SessionRevision,
    candidate: &mut HomeRecoveryCandidate,
    session: &SessionState,
) -> Result<(), ExitSessionValidationError> {
    use ExitSessionValidationError as Error;
    let snapshot = read_candidate(evidence, candidate, session)?;
    let header = snapshot.header();
    if header.exit_intent() != intent
        || header.revision() != revision
        || header.fallback() != evidence.source.header().fallback()
        || snapshot.windows().len() != evidence.result_windows.len()
        || snapshot.windows().len() != evidence.source.windows().len()
        || snapshot
            .windows()
            .iter()
            .zip(&evidence.result_windows)
            .zip(evidence.source.windows())
            .any(|((actual, (id, revision, placement)), source)| {
                actual.window_id() != *id
                    || actual.revision() != *revision
                    || actual.placement() != placement
                    || actual.remembered_target() != source.remembered_target()
                    || actual.selected_thread() != source.selected_thread()
            })
    {
        return Err(Error::Changed);
    }
    Ok(())
}
