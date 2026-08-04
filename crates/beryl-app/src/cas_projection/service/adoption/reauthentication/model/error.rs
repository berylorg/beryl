use std::fmt;

use super::*;

impl ProjectionCandidateLedgerSealFailure {
    #[must_use]
    pub fn into_retryable(
        self,
    ) -> Result<ProjectionCandidateLedgerSealError, TerminalAdoptedProjectionConnectionService>
    {
        match self {
            Self::Retryable(error) => Ok(error),
            Self::Terminal(terminal) => Err(terminal),
        }
    }

    #[must_use]
    pub fn into_terminal(
        self,
    ) -> Result<TerminalAdoptedProjectionConnectionService, ProjectionCandidateLedgerSealError>
    {
        match self {
            Self::Retryable(error) => Err(error),
            Self::Terminal(terminal) => Ok(terminal),
        }
    }
}

impl From<ProjectionCandidateLedgerSealError> for ProjectionCandidateLedgerSealFailure {
    fn from(error: ProjectionCandidateLedgerSealError) -> Self {
        Self::Retryable(error)
    }
}

impl ProjectionCandidateLedgerSealError {
    pub(in super::super) fn new(
        reason: ProjectionCandidateLedgerSealReason,
        candidate_id: Option<ProjectionCandidateId>,
        ledger: super::super::AdoptedProjectionCandidateReauthenticationLedger,
    ) -> Self {
        Self {
            reason,
            candidate_id,
            ledger: Some(ledger),
        }
    }

    #[must_use]
    pub const fn reason(&self) -> ProjectionCandidateLedgerSealReason {
        self.reason
    }

    #[must_use]
    pub const fn candidate_id(&self) -> Option<ProjectionCandidateId> {
        self.candidate_id
    }

    #[must_use]
    pub fn metadata(&self) -> ProjectionCandidateLedgerMetadata {
        self.ledger
            .as_ref()
            .expect("a seal error retains its complete owning ledger")
            .metadata()
    }

    #[must_use]
    pub fn into_ledger(mut self) -> super::super::AdoptedProjectionCandidateReauthenticationLedger {
        self.ledger
            .take()
            .expect("a seal error returns its complete ledger once")
    }
}

impl fmt::Debug for CandidateSetConvergedAdoptedProjectionConnectionService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CandidateSetConvergedAdoptedProjectionConnectionService")
            .field("metadata", &self.metadata())
            .field("accepted_candidate_count", &self.accepted_candidate_count())
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for TerminalAdoptedProjectionConnectionService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TerminalAdoptedProjectionConnectionService")
            .field("reason", &self.reason)
            .field("metadata", &self.metadata())
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for ProjectionCandidateLedgerSealFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Retryable(error) => formatter.debug_tuple("Retryable").field(error).finish(),
            Self::Terminal(terminal) => formatter.debug_tuple("Terminal").field(terminal).finish(),
        }
    }
}

impl fmt::Debug for ProjectionCandidateLedgerSealError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectionCandidateLedgerSealError")
            .field("reason", &self.reason)
            .field("candidate_id", &self.candidate_id)
            .field("metadata", &self.metadata())
            .finish_non_exhaustive()
    }
}

impl fmt::Display for ProjectionCandidateLedgerSealError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "candidate ledger could not seal: {:?}",
            self.reason
        )
    }
}

impl std::error::Error for ProjectionCandidateLedgerSealError {}

impl fmt::Display for ProjectionCandidateLedgerSealFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Retryable(error) => error.fmt(formatter),
            Self::Terminal(terminal) => write!(
                formatter,
                "adopted service became terminal before candidate-set seal: {:?}",
                terminal.reason()
            ),
        }
    }
}

impl std::error::Error for ProjectionCandidateLedgerSealFailure {}
