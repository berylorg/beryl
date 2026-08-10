#[cfg(test)]
use super::super::super::super::super::service_config::ProjectionWorkerPoolDiagnostics;
use super::{
    CandidateLedgerEntry, ProjectionCandidateId, ProjectionCandidateLedgerMetadata,
    ProjectionCandidateMetadata, ProjectionConnectionServiceCloseError,
    TerminalAdoptedProjectionConnectionService, TerminalAdoptedProjectionConnectionServiceReason,
};

impl TerminalAdoptedProjectionConnectionService {
    pub(in super::super) fn new(
        reason: TerminalAdoptedProjectionConnectionServiceReason,
        ledger: super::super::AdoptedProjectionCandidateReauthenticationLedger,
    ) -> Self {
        Self {
            reason,
            ledger: Some(ledger),
        }
    }

    #[must_use]
    pub const fn reason(&self) -> TerminalAdoptedProjectionConnectionServiceReason {
        self.reason
    }

    #[must_use]
    pub fn metadata(&self) -> ProjectionCandidateLedgerMetadata {
        self.ledger
            .as_ref()
            .expect("a terminal adopted service retains its complete ledger")
            .metadata()
    }

    /// Returns the content-free terminal state of one original candidate.
    #[must_use]
    pub fn candidate(
        &self,
        candidate_id: ProjectionCandidateId,
    ) -> Option<ProjectionCandidateMetadata> {
        self.ledger
            .as_ref()
            .expect("a terminal adopted service retains its complete ledger")
            .candidate(candidate_id)
    }

    /// Iterates over every original candidate in stable group/candidate order.
    pub fn candidates(&self) -> impl Iterator<Item = ProjectionCandidateMetadata> + '_ {
        self.ledger
            .as_ref()
            .expect("a terminal adopted service retains its complete ledger")
            .candidates()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn replacement_worker_diagnostics_for_test(
        &self,
    ) -> ProjectionWorkerPoolDiagnostics {
        self.ledger
            .as_ref()
            .expect("a terminal adopted service retains its complete ledger")
            .replacement_worker_diagnostics_for_test()
    }

    /// Consumes the whole failed adoption through local revocation and explicit inert cleanup.
    ///
    /// Cleanup attempts every worker join before returning the first typed shutdown failure and
    /// never closes the retained same-home recovery authority.
    pub fn dispose(mut self) -> Result<(), ProjectionConnectionServiceCloseError> {
        let mut ledger = self
            .ledger
            .take()
            .expect("a terminal adopted service is disposed once");
        if let Some(attempt) = ledger.attempt.as_mut() {
            attempt.make_inert();
        }
        for group in &mut ledger.groups {
            for entry in &mut group.entries {
                match entry
                    .take()
                    .expect("a terminal ledger retains every candidate owner")
                {
                    CandidateLedgerEntry::Unprocessed(owner)
                    | CandidateLedgerEntry::Rejected { owner, .. } => {
                        drop(owner.dispose_local());
                    }
                    CandidateLedgerEntry::Accepted(accepted) => {
                        drop(accepted.into_pending_owner().dispose_local());
                    }
                    CandidateLedgerEntry::Disposed => {}
                }
                *entry = Some(CandidateLedgerEntry::Disposed);
            }
        }
        ledger.connection_owners.clear();
        if let Some(attempt) = ledger.attempt.as_mut() {
            attempt.dispose_inert()
        } else {
            Ok(())
        }
    }
}
