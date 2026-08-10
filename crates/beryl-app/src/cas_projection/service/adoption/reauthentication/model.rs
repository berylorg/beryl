use beryl_home_store::HomeGeneration;
use beryl_model::BerylHomeId;

use super::super::super::super::{
    ProjectionServiceGeneration,
    accepted_input_scheduler::{RecoveredProjectionLaneParts, RecoveredProjectionLaneStageReason},
    connection::{
        CandidateSetConvergedProjectionConnectionOwner, DormantRecoveredProjectionLeaseOwner,
        PendingProjectionLeaseOwner,
    },
    persistent_failure::{PendingProjectionGroupIdentity, PendingProjectionWitness},
};
use super::super::super::ProjectionConnectionServiceCloseError;
use super::super::{PersistentFailureServiceAdoptionMetadata, ServiceAdoptionAttempt};

mod api;
mod error;
mod terminal;

/// Stable position of one candidate in the complete adopted quarantine topology.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectionCandidateId {
    group_index: usize,
    candidate_index: usize,
}

/// Content-free state of one exact ledger entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionCandidateReauthenticationStatus {
    Unprocessed,
    Rejected,
    Accepted,
    Disposed,
}

/// Service-wide authority loss that makes one committed adoption permanently unpublishable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalAdoptedProjectionConnectionServiceReason {
    RecoveredServiceUnavailable,
    RecoveredHomeIdentityMismatch,
    RecoveredHomeGenerationMismatch,
    RecoveredHomeNotHealthy,
    StableConnectionRetired,
    StableConnectionAuthenticationUnavailable,
    StableConnectionMismatch,
    ServiceMembershipUnavailable,
    ServiceMembershipMismatch,
    LoadedRegistryAuthenticationUnavailable,
    FinalRecoveredHomeMismatch,
}

/// Content-free reason why one owning candidate remains rejected in the ledger.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionCandidateReauthenticationReason {
    ServiceTerminal(TerminalAdoptedProjectionConnectionServiceReason),
    CandidateWitnessMismatch,
    CandidateConnectionMismatch,
    PreReadRegistryTokenMismatch,
    PendingOrdinaryTurnUnavailable,
    PendingOrdinaryProjectionMismatch,
    PendingOrdinaryConcurrentChange,
    PendingOrdinaryInputContentUnavailable,
    PendingOrdinaryInputAssetReferenceSetMismatch,
    PendingOrdinaryReadUnavailable,
    PendingOrdinaryInvariant,
    PostReadRegistryTokenMismatch,
    SealRegistryTokenMismatch,
    UnexpectedUnwind,
}

/// Content-free recovered-generation identity retained for one accepted entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveredProjectionCandidateMetadata {
    candidate_id: ProjectionCandidateId,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
}

/// Content-free observation of one ledger entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectionCandidateMetadata {
    candidate_id: ProjectionCandidateId,
    status: ProjectionCandidateReauthenticationStatus,
    rejection_reason: Option<ProjectionCandidateReauthenticationReason>,
    recovered: Option<RecoveredProjectionCandidateMetadata>,
}

/// Content-free outcome of one authorized reauthentication attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectionCandidateReauthenticationOutcome {
    candidate: ProjectionCandidateMetadata,
}

/// Content-free outcome of one explicit local disposition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectionCandidateDispositionOutcome {
    candidate_id: ProjectionCandidateId,
}

/// Bounded counts and service identity for one owning reauthentication ledger.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectionCandidateLedgerMetadata {
    adoption: PersistentFailureServiceAdoptionMetadata,
    connection_owner_count: usize,
    terminal_reason: Option<TerminalAdoptedProjectionConnectionServiceReason>,
    unprocessed_count: usize,
    rejected_count: usize,
    accepted_count: usize,
    disposed_count: usize,
}

/// Content-free reason why a consuming seal returned its owning ledger.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionCandidateLedgerSealReason {
    OutstandingCandidate,
    AcceptedInventoryCapacityUnavailable,
    AcceptedCandidateAuthenticationFailed,
    ConnectionOwnerCapacityUnavailable,
    UnexpectedUnwind,
}

/// A consuming seal either returns its retryable ledger or a terminal whole-adoption owner.
#[must_use = "a failed seal still owns the complete adopted service authority"]
pub enum ProjectionCandidateLedgerSealFailure {
    Retryable(ProjectionCandidateLedgerSealError),
    Terminal(TerminalAdoptedProjectionConnectionService),
}

/// Invalid candidate selection or state transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProjectionCandidateLedgerAccessError {
    #[error("the candidate id is not present in this ledger")]
    UnknownCandidate,
    #[error("an accepted candidate cannot be reauthenticated or disposed")]
    CandidateAccepted,
    #[error("a disposed candidate cannot be reauthenticated or disposed again")]
    CandidateDisposed,
    #[error("the adopted service is terminal and permits only whole-attempt disposition")]
    LedgerTerminal,
}

pub(super) struct CandidateLedgerGroup {
    pub(super) identity: PendingProjectionGroupIdentity,
    pub(super) witness: PendingProjectionWitness,
    pub(super) entries: Vec<Option<CandidateLedgerEntry>>,
}

pub(super) enum CandidateLedgerEntry {
    Unprocessed(PendingProjectionLeaseOwner),
    Rejected {
        owner: PendingProjectionLeaseOwner,
        reason: ProjectionCandidateReauthenticationReason,
    },
    Accepted(DormantRecoveredProjection),
    Disposed,
}

pub(super) struct DormantRecoveredProjection {
    metadata: RecoveredProjectionCandidateMetadata,
    identity: PendingProjectionGroupIdentity,
    witness: PendingProjectionWitness,
    owner: DormantRecoveredProjectionLeaseOwner,
}

pub(super) struct SealedDormantRecoveredProjectionInventory {
    accepted_count: usize,
    accepted: Option<Vec<RecoveredProjectionLaneParts>>,
    connection_owners: Vec<CandidateSetConvergedProjectionConnectionOwner>,
}

#[must_use = "the converged adopted service remains unpublished until recovery publication consumes it"]
pub struct CandidateSetConvergedAdoptedProjectionConnectionService {
    pub(super) attempt: Option<Box<ServiceAdoptionAttempt>>,
    pub(super) accepted_inventory: Option<SealedDormantRecoveredProjectionInventory>,
}

#[must_use = "staged recovery publication retains the complete adopted service and connection fences"]
pub(in crate::cas_projection) struct StagedRecoveredProjectionConnectionService {
    attempt: Option<Box<ServiceAdoptionAttempt>>,
    connection_owners: Vec<CandidateSetConvergedProjectionConnectionOwner>,
}

#[must_use = "a staging failure returns the intact converged adopted service"]
pub(in crate::cas_projection) struct RecoveredProjectionLaneStagingError {
    reason: RecoveredProjectionLaneStageReason,
    converged: Option<CandidateSetConvergedAdoptedProjectionConnectionService>,
}

/// Non-retryable ownership of one complete adopted service after service-wide authority loss.
#[must_use = "the terminal adopted service must be explicitly disposed or retained inert"]
pub struct TerminalAdoptedProjectionConnectionService {
    pub(super) reason: TerminalAdoptedProjectionConnectionServiceReason,
    pub(super) ledger: Option<super::AdoptedProjectionCandidateReauthenticationLedger>,
}

#[must_use = "the seal error owns the complete retryable candidate ledger"]
pub struct ProjectionCandidateLedgerSealError {
    pub(super) reason: ProjectionCandidateLedgerSealReason,
    pub(super) candidate_id: Option<ProjectionCandidateId>,
    pub(super) ledger: Option<super::AdoptedProjectionCandidateReauthenticationLedger>,
}

impl CandidateLedgerEntry {
    pub(super) fn metadata(
        &self,
        candidate_id: ProjectionCandidateId,
    ) -> ProjectionCandidateMetadata {
        match self {
            Self::Unprocessed(_) => ProjectionCandidateMetadata::new(
                candidate_id,
                ProjectionCandidateReauthenticationStatus::Unprocessed,
                None,
                None,
            ),
            Self::Rejected { reason, .. } => ProjectionCandidateMetadata::new(
                candidate_id,
                ProjectionCandidateReauthenticationStatus::Rejected,
                Some(*reason),
                None,
            ),
            Self::Accepted(accepted) => ProjectionCandidateMetadata::new(
                candidate_id,
                ProjectionCandidateReauthenticationStatus::Accepted,
                None,
                Some(accepted.metadata),
            ),
            Self::Disposed => ProjectionCandidateMetadata::new(
                candidate_id,
                ProjectionCandidateReauthenticationStatus::Disposed,
                None,
                None,
            ),
        }
    }
}

impl DormantRecoveredProjection {
    pub(super) fn new(
        metadata: RecoveredProjectionCandidateMetadata,
        identity: PendingProjectionGroupIdentity,
        witness: PendingProjectionWitness,
        owner: DormantRecoveredProjectionLeaseOwner,
    ) -> Self {
        Self {
            metadata,
            identity,
            witness,
            owner,
        }
    }

    pub(super) const fn metadata(&self) -> RecoveredProjectionCandidateMetadata {
        self.metadata
    }

    pub(super) const fn identity(&self) -> &PendingProjectionGroupIdentity {
        &self.identity
    }

    pub(super) const fn witness(&self) -> &PendingProjectionWitness {
        &self.witness
    }

    pub(super) const fn owner(&self) -> &DormantRecoveredProjectionLeaseOwner {
        &self.owner
    }

    pub(super) fn into_pending_owner(self) -> PendingProjectionLeaseOwner {
        self.owner.into_pending_owner()
    }

    fn into_lane_parts(self) -> RecoveredProjectionLaneParts {
        (self.witness, self.owner)
    }
}

impl SealedDormantRecoveredProjectionInventory {
    pub(super) fn new(
        accepted: Vec<DormantRecoveredProjection>,
        connection_owners: Vec<CandidateSetConvergedProjectionConnectionOwner>,
    ) -> Self {
        let accepted_count = accepted.len();
        Self {
            accepted_count,
            accepted: Some(
                accepted
                    .into_iter()
                    .map(DormantRecoveredProjection::into_lane_parts)
                    .collect(),
            ),
            connection_owners,
        }
    }

    pub(super) fn len(&self) -> usize {
        self.accepted_count
    }

    #[cfg(test)]
    pub(super) fn connection_owner_count(&self) -> usize {
        self.connection_owners.len()
    }
}

impl CandidateSetConvergedAdoptedProjectionConnectionService {
    #[must_use]
    pub fn metadata(&self) -> PersistentFailureServiceAdoptionMetadata {
        self.attempt
            .as_ref()
            .expect("a converged adopted service retains its exact adoption attempt")
            .metadata
    }

    #[must_use]
    pub fn accepted_candidate_count(&self) -> usize {
        self.accepted_inventory
            .as_ref()
            .expect("a converged adopted service retains its dormant accepted inventory")
            .len()
    }

    pub(in crate::cas_projection) fn stage_recovered_projection_lane(
        mut self,
    ) -> Result<StagedRecoveredProjectionConnectionService, RecoveredProjectionLaneStagingError>
    {
        let inventory = self
            .accepted_inventory
            .as_mut()
            .expect("a converged adopted service retains its dormant accepted inventory");
        let Some(entries) = inventory.accepted.take() else {
            return Err(RecoveredProjectionLaneStagingError::new(
                RecoveredProjectionLaneStageReason::AlreadyStaged,
                self,
            ));
        };
        let service = self
            .attempt
            .as_ref()
            .and_then(|attempt| attempt.adopted_service.as_ref())
            .expect("a converged adoption retains its replacement service");
        if let Err(error) = service.stage_recovered_projection_lane(entries) {
            let (reason, entries) = error.into_parts();
            inventory.accepted = Some(entries);
            return Err(RecoveredProjectionLaneStagingError::new(reason, self));
        }
        let connection_owners = std::mem::take(&mut inventory.connection_owners);
        let attempt = self
            .attempt
            .take()
            .expect("a staged converged service transfers its adoption attempt once");
        Ok(StagedRecoveredProjectionConnectionService {
            attempt: Some(attempt),
            connection_owners,
        })
    }

    pub(in crate::cas_projection) fn dispose_after_recovery_failure(
        mut self,
    ) -> Result<(), ProjectionConnectionServiceCloseError> {
        if let Some(mut inventory) = self.accepted_inventory.take() {
            if let Some(entries) = inventory.accepted.take() {
                for (_witness, owner) in entries {
                    drop(owner.into_pending_owner().dispose_local());
                }
            }
            inventory.connection_owners.clear();
        }
        let Some(mut attempt) = self.attempt.take() else {
            return Ok(());
        };
        attempt.make_inert();
        attempt.dispose_inert()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn retained_connection_owner_count_for_test(&self) -> usize {
        self.accepted_inventory
            .as_ref()
            .expect("a converged adopted service retains its retirement fences")
            .connection_owner_count()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn retain_late_adoption_authority_for_test(&self) {
        self.attempt
            .as_ref()
            .and_then(|attempt| attempt.inventory.as_ref())
            .expect("a converged adopted service retains its old-cut inventory")
            .retain_late_adoption_authority_for_test();
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn retain_late_after_publication_fence_for_test(&mut self) {
        self.attempt
            .as_mut()
            .expect("a converged adopted service retains its adoption attempt")
            .retain_late_after_publication_fence_for_test();
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn fail_replacement_ingester_arm_for_test(
        &mut self,
        index: usize,
    ) {
        self.attempt
            .as_mut()
            .expect("a converged adopted service retains its adoption attempt")
            .fail_replacement_ingester_arm_for_test(index);
    }
}

impl StagedRecoveredProjectionConnectionService {
    pub(in crate::cas_projection) fn into_parts(
        mut self,
    ) -> (
        Box<ServiceAdoptionAttempt>,
        Vec<CandidateSetConvergedProjectionConnectionOwner>,
    ) {
        (
            self.attempt
                .take()
                .expect("staged recovery publication transfers its adoption attempt once"),
            std::mem::take(&mut self.connection_owners),
        )
    }
}

impl RecoveredProjectionLaneStagingError {
    fn new(
        reason: RecoveredProjectionLaneStageReason,
        converged: CandidateSetConvergedAdoptedProjectionConnectionService,
    ) -> Self {
        Self {
            reason,
            converged: Some(converged),
        }
    }

    pub(in crate::cas_projection) const fn reason(&self) -> RecoveredProjectionLaneStageReason {
        self.reason
    }

    pub(in crate::cas_projection) fn into_converged(
        mut self,
    ) -> CandidateSetConvergedAdoptedProjectionConnectionService {
        self.converged
            .take()
            .expect("a staging error returns its converged owner once")
    }

    pub(in crate::cas_projection) fn dispose_after_recovery_failure(
        self,
    ) -> Result<(), ProjectionConnectionServiceCloseError> {
        self.into_converged().dispose_after_recovery_failure()
    }
}
