use std::{
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
};

mod model;
mod seal;
#[cfg(test)]
mod test_faults;
#[cfg(test)]
mod test_support;
mod transaction;
mod validation;

pub(in crate::cas_projection) use model::RecoveredProjectionLaneStagingError;
use model::{CandidateLedgerEntry, CandidateLedgerGroup, DormantRecoveredProjection};
pub use model::{
    CandidateSetConvergedAdoptedProjectionConnectionService, ProjectionCandidateDispositionOutcome,
    ProjectionCandidateId, ProjectionCandidateLedgerAccessError, ProjectionCandidateLedgerMetadata,
    ProjectionCandidateLedgerSealError, ProjectionCandidateLedgerSealFailure,
    ProjectionCandidateLedgerSealReason, ProjectionCandidateMetadata,
    ProjectionCandidateReauthenticationOutcome, ProjectionCandidateReauthenticationReason,
    ProjectionCandidateReauthenticationStatus, RecoveredProjectionCandidateMetadata,
    TerminalAdoptedProjectionConnectionService, TerminalAdoptedProjectionConnectionServiceReason,
};

use super::super::super::{
    connection::PendingProjectionConnectionOwner,
    persistent_failure::PendingProjectionCandidateGroup,
};
use super::{AdoptedUnpublishedProjectionConnectionService, ServiceAdoptionAttempt};

/// Sole owning ledger for every quarantined candidate of one adopted unpublished service.
#[must_use = "the ledger must converge before the adopted service can be published"]
pub struct AdoptedProjectionCandidateReauthenticationLedger {
    attempt: Option<Box<ServiceAdoptionAttempt>>,
    groups: Vec<CandidateLedgerGroup>,
    connection_owners: Vec<PendingProjectionConnectionOwner>,
    terminal_reason: Option<TerminalAdoptedProjectionConnectionServiceReason>,
    #[cfg(test)]
    test_pauses: test_support::CandidateReauthenticationPauseRegistry,
}

impl AdoptedUnpublishedProjectionConnectionService {
    /// Consumes this adopted-but-unpublished authority into its one exact candidate ledger.
    #[must_use]
    pub fn begin_candidate_reauthentication(
        mut self,
    ) -> AdoptedProjectionCandidateReauthenticationLedger {
        let mut attempt = self
            .attempt
            .take()
            .expect("an adopted unpublished service begins reauthentication once");
        let (groups, connection_owners) = attempt
            .topology
            .as_mut()
            .expect("an adopted service retains its complete quarantine topology")
            .take_reauthentication_parts();
        AdoptedProjectionCandidateReauthenticationLedger::new(attempt, groups, connection_owners)
    }
}

impl AdoptedProjectionCandidateReauthenticationLedger {
    fn new(
        attempt: Box<ServiceAdoptionAttempt>,
        groups: Vec<PendingProjectionCandidateGroup>,
        connection_owners: Vec<PendingProjectionConnectionOwner>,
    ) -> Self {
        let expected_groups = attempt.metadata.group_count();
        let expected_candidates = attempt.metadata.candidate_count();
        let expected_connections = attempt.metadata.connection_count();
        let groups = groups
            .into_iter()
            .map(|group| {
                let (identity, witness, candidates) = group.into_parts();
                CandidateLedgerGroup {
                    identity,
                    witness,
                    entries: candidates
                        .into_iter()
                        .map(|owner| Some(CandidateLedgerEntry::Unprocessed(owner)))
                        .collect(),
                }
            })
            .collect::<Vec<_>>();
        let actual_candidates = groups
            .iter()
            .map(|group| group.entries.len())
            .sum::<usize>();
        assert_eq!(groups.len(), expected_groups);
        assert_eq!(actual_candidates, expected_candidates);
        assert_eq!(connection_owners.len(), expected_connections);
        Self {
            attempt: Some(attempt),
            groups,
            connection_owners,
            terminal_reason: None,
            #[cfg(test)]
            test_pauses: test_support::CandidateReauthenticationPauseRegistry::new(),
        }
    }

    /// Returns bounded content-free counts for this exact owning ledger.
    #[must_use]
    pub fn metadata(&self) -> ProjectionCandidateLedgerMetadata {
        let mut unprocessed = 0;
        let mut rejected = 0;
        let mut accepted = 0;
        let mut disposed = 0;
        for entry in self.groups.iter().flat_map(|group| &group.entries) {
            match entry
                .as_ref()
                .expect("a ledger entry is vacant only during one synchronous transition")
            {
                CandidateLedgerEntry::Unprocessed(_) => unprocessed += 1,
                CandidateLedgerEntry::Rejected { .. } => rejected += 1,
                CandidateLedgerEntry::Accepted(_) => accepted += 1,
                CandidateLedgerEntry::Disposed => disposed += 1,
            }
        }
        ProjectionCandidateLedgerMetadata::new(
            self.attempt
                .as_ref()
                .expect("an owning ledger retains its adoption attempt")
                .metadata,
            self.connection_owners.len(),
            self.terminal_reason,
            unprocessed,
            rejected,
            accepted,
            disposed,
        )
    }

    /// Returns the content-free state of one stable candidate id.
    #[must_use]
    pub fn candidate(
        &self,
        candidate_id: ProjectionCandidateId,
    ) -> Option<ProjectionCandidateMetadata> {
        self.groups
            .get(candidate_id.group_index())?
            .entries
            .get(candidate_id.candidate_index())?
            .as_ref()
            .map(|entry| entry.metadata(candidate_id))
    }

    /// Iterates over every original candidate in stable group/candidate order.
    pub fn candidates(&self) -> impl Iterator<Item = ProjectionCandidateMetadata> + '_ {
        self.groups
            .iter()
            .enumerate()
            .flat_map(|(group_index, group)| {
                group
                    .entries
                    .iter()
                    .enumerate()
                    .map(move |(candidate_index, entry)| {
                        let candidate_id = ProjectionCandidateId::new(group_index, candidate_index);
                        entry
                            .as_ref()
                            .expect(
                                "a ledger entry is vacant only during one synchronous transition",
                            )
                            .metadata(candidate_id)
                    })
            })
    }

    /// Reauthenticates one unprocessed or rejected candidate without making it executable.
    pub fn reauthenticate_candidate(
        &mut self,
        candidate_id: ProjectionCandidateId,
    ) -> Result<ProjectionCandidateReauthenticationOutcome, ProjectionCandidateLedgerAccessError>
    {
        if self.candidate(candidate_id).is_none() {
            return Err(ProjectionCandidateLedgerAccessError::UnknownCandidate);
        }
        if self.terminal_reason.is_some() {
            return Err(ProjectionCandidateLedgerAccessError::LedgerTerminal);
        }
        #[cfg(test)]
        let test_pauses = self.test_pauses.clone();
        let attempt = self
            .attempt
            .as_ref()
            .expect("an owning ledger retains its adoption attempt");
        let group = self
            .groups
            .get_mut(candidate_id.group_index())
            .ok_or(ProjectionCandidateLedgerAccessError::UnknownCandidate)?;
        let entry = group
            .entries
            .get_mut(candidate_id.candidate_index())
            .ok_or(ProjectionCandidateLedgerAccessError::UnknownCandidate)?;
        let previous = entry
            .take()
            .expect("a ledger entry is vacant only during one synchronous transition");
        let owner = match previous {
            CandidateLedgerEntry::Unprocessed(owner)
            | CandidateLedgerEntry::Rejected { owner, .. } => owner,
            accepted @ CandidateLedgerEntry::Accepted(_) => {
                *entry = Some(accepted);
                return Err(ProjectionCandidateLedgerAccessError::CandidateAccepted);
            }
            disposed @ CandidateLedgerEntry::Disposed => {
                *entry = Some(disposed);
                return Err(ProjectionCandidateLedgerAccessError::CandidateDisposed);
            }
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            transaction::reauthenticate_pending_candidate(
                attempt,
                candidate_id,
                &group.identity,
                &group.witness,
                &owner,
                #[cfg(test)]
                &test_pauses,
            )
        }))
        .unwrap_or(Err(
            ProjectionCandidateReauthenticationReason::UnexpectedUnwind,
        ));
        let terminal_reason = match result {
            Ok(()) => {
                let metadata =
                    RecoveredProjectionCandidateMetadata::new(candidate_id, attempt.metadata);
                let accepted = DormantRecoveredProjection::new(
                    metadata,
                    group.identity.clone(),
                    group.witness.clone(),
                    owner.into_dormant_recovered_owner(),
                );
                *entry = Some(CandidateLedgerEntry::Accepted(accepted));
                None
            }
            Err(reason) => {
                *entry = Some(CandidateLedgerEntry::Rejected { owner, reason });
                match reason {
                    ProjectionCandidateReauthenticationReason::ServiceTerminal(reason) => {
                        Some(reason)
                    }
                    _ => None,
                }
            }
        };
        if let Some(reason) = terminal_reason {
            self.mark_terminal(reason);
        }
        let candidate = self
            .candidate(candidate_id)
            .expect("the synchronous transition restores its owning entry");
        Ok(ProjectionCandidateReauthenticationOutcome::new(candidate))
    }

    /// Consumes one unprocessed or rejected candidate through poison-safe local revocation.
    pub fn dispose_candidate(
        &mut self,
        candidate_id: ProjectionCandidateId,
    ) -> Result<ProjectionCandidateDispositionOutcome, ProjectionCandidateLedgerAccessError> {
        if self.candidate(candidate_id).is_none() {
            return Err(ProjectionCandidateLedgerAccessError::UnknownCandidate);
        }
        if self.terminal_reason.is_some() {
            return Err(ProjectionCandidateLedgerAccessError::LedgerTerminal);
        }
        let group = self
            .groups
            .get_mut(candidate_id.group_index())
            .ok_or(ProjectionCandidateLedgerAccessError::UnknownCandidate)?;
        let entry = group
            .entries
            .get_mut(candidate_id.candidate_index())
            .ok_or(ProjectionCandidateLedgerAccessError::UnknownCandidate)?;
        let previous = entry
            .take()
            .expect("a ledger entry is vacant only during one synchronous transition");
        let owner = match previous {
            CandidateLedgerEntry::Unprocessed(owner)
            | CandidateLedgerEntry::Rejected { owner, .. } => owner,
            accepted @ CandidateLedgerEntry::Accepted(_) => {
                *entry = Some(accepted);
                return Err(ProjectionCandidateLedgerAccessError::CandidateAccepted);
            }
            disposed @ CandidateLedgerEntry::Disposed => {
                *entry = Some(disposed);
                return Err(ProjectionCandidateLedgerAccessError::CandidateDisposed);
            }
        };
        let replacement_hold = owner.dispose_local();
        *entry = Some(CandidateLedgerEntry::Disposed);
        drop(replacement_hold);
        Ok(ProjectionCandidateDispositionOutcome::new(candidate_id))
    }

    /// Consumes a fully resolved nonterminal ledger into candidate-set-converged authority.
    pub fn seal(
        self,
    ) -> Result<
        CandidateSetConvergedAdoptedProjectionConnectionService,
        ProjectionCandidateLedgerSealFailure,
    > {
        seal::seal(self)
    }

    /// Consumes an observed service-terminal ledger into its sole whole-attempt disposition owner.
    pub fn into_terminal_service(self) -> Result<TerminalAdoptedProjectionConnectionService, Self> {
        let Some(reason) = self.terminal_reason else {
            return Err(self);
        };
        Ok(TerminalAdoptedProjectionConnectionService::new(
            reason, self,
        ))
    }

    pub(in crate::cas_projection) fn dispose_after_recovery_failure(
        mut self,
    ) -> Result<(), super::super::ProjectionConnectionServiceCloseError> {
        if let Some(attempt) = self.attempt.as_mut() {
            attempt.make_inert();
        }
        for group in &mut self.groups {
            for entry in &mut group.entries {
                match entry
                    .take()
                    .expect("a recovery-failure ledger retains every candidate owner")
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
        self.connection_owners.clear();
        self.attempt
            .as_mut()
            .map_or(Ok(()), |attempt| attempt.dispose_inert())
    }

    pub(super) fn mark_terminal(
        &mut self,
        reason: TerminalAdoptedProjectionConnectionServiceReason,
    ) {
        self.terminal_reason.get_or_insert(reason);
        let reason = ProjectionCandidateReauthenticationReason::ServiceTerminal(reason);
        for group in &mut self.groups {
            for entry in &mut group.entries {
                let previous = entry
                    .take()
                    .expect("a terminal transition retains every candidate entry");
                *entry = Some(match previous {
                    CandidateLedgerEntry::Unprocessed(owner)
                    | CandidateLedgerEntry::Rejected { owner, .. } => {
                        CandidateLedgerEntry::Rejected { owner, reason }
                    }
                    CandidateLedgerEntry::Accepted(accepted) => CandidateLedgerEntry::Rejected {
                        owner: accepted.into_pending_owner(),
                        reason,
                    },
                    CandidateLedgerEntry::Disposed => CandidateLedgerEntry::Disposed,
                });
            }
        }
    }
}

impl fmt::Debug for AdoptedProjectionCandidateReauthenticationLedger {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdoptedProjectionCandidateReauthenticationLedger")
            .field("metadata", &self.metadata())
            .finish_non_exhaustive()
    }
}
