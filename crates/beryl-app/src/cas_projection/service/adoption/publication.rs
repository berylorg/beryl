use std::{
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use super::super::super::{
    connection::CandidateSetConvergedProjectionConnectionOwner,
    persistent_failure::PersistentFailureAdoptionRetirementWitness,
    service_supervisor::{PublishedServiceEpoch, RunningServiceSlot},
};
use super::super::ProjectionConnectionServiceCloseError;
use super::{
    CandidateSetConvergedAdoptedProjectionConnectionService,
    PersistentFailureServiceAdoptionMetadata, RecoveredProjectionLaneStagingError,
    ServiceAdoptionAttempt,
};

mod transaction;
mod validation;

/// Bounded content-free result of one successful same-home service publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveredServicePublicationMetadata {
    adoption: PersistentFailureServiceAdoptionMetadata,
    accepted_candidate_count: usize,
}

/// Why a converged adopted service became terminal before final process publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveredServicePublicationReason {
    RecoveredProjectionLaneUnavailable,
    PublicationCapacityUnavailable,
    PublicationTopologyMismatch,
    AdoptionPublisherWon,
    OldConnectionEpochRetirement,
    OldServiceEpochRetirement,
    ReplacementWorkerArm,
    ForwardingEpochUnavailable,
    StableConnectionAuthorityUnavailable,
    StableConnectionRetired,
    RecoveredHomeMismatch,
    RecoveredHomeNotHealthy,
    RecoveredServiceMismatch,
    StartupNotConverged,
    StartupFenceUnavailable,
    ServiceMembershipUnavailable,
    ServiceMembershipMismatch,
    ProcessServiceSlotUnavailable,
    UnexpectedUnwind,
}

/// Non-retryable ownership of one failed post-convergence recovery publication.
#[must_use = "the error owns the complete unpublished replacement until explicitly disposed"]
pub struct RecoveredServicePublicationError {
    reason: RecoveredServicePublicationReason,
    metadata: RecoveredServicePublicationMetadata,
    owner: Option<RecoveredServicePublicationOwner>,
}

enum RecoveredServicePublicationOwner {
    Lane(RecoveredProjectionLaneStagingError),
    Attempt(Box<RecoveryServicePublicationAttempt>),
}

struct RecoveryServicePublicationAttempt {
    metadata: RecoveredServicePublicationMetadata,
    attempt: Box<ServiceAdoptionAttempt>,
    connection_owners: Vec<CandidateSetConvergedProjectionConnectionOwner>,
    retirement_witness: Option<PersistentFailureAdoptionRetirementWitness>,
    prepared_epoch: Option<PublishedServiceEpoch>,
}

#[derive(Clone, Copy)]
enum ReplacementWorkerPublicationState {
    RetiredUnarmed,
    Armed,
}

impl RecoveredServicePublicationMetadata {
    #[must_use]
    pub const fn adoption(self) -> PersistentFailureServiceAdoptionMetadata {
        self.adoption
    }

    #[must_use]
    pub const fn accepted_candidate_count(self) -> usize {
        self.accepted_candidate_count
    }
}

impl RecoveredServicePublicationError {
    fn lane(
        metadata: RecoveredServicePublicationMetadata,
        error: RecoveredProjectionLaneStagingError,
    ) -> Self {
        Self {
            reason: RecoveredServicePublicationReason::RecoveredProjectionLaneUnavailable,
            metadata,
            owner: Some(RecoveredServicePublicationOwner::Lane(error)),
        }
    }

    fn attempt(
        reason: RecoveredServicePublicationReason,
        attempt: Box<RecoveryServicePublicationAttempt>,
    ) -> Self {
        let metadata = attempt.metadata;
        Self {
            reason,
            metadata,
            owner: Some(RecoveredServicePublicationOwner::Attempt(attempt)),
        }
    }

    #[must_use]
    pub const fn reason(&self) -> RecoveredServicePublicationReason {
        self.reason
    }

    #[must_use]
    pub const fn metadata(&self) -> RecoveredServicePublicationMetadata {
        self.metadata
    }

    /// Cancels and joins every unpublished replacement worker and releases candidate authority
    /// without ever closing the supervisor-retained home.
    pub fn dispose(mut self) -> Result<(), ProjectionConnectionServiceCloseError> {
        match self.owner.take() {
            Some(RecoveredServicePublicationOwner::Lane(error)) => {
                error.dispose_after_recovery_failure()
            }
            Some(RecoveredServicePublicationOwner::Attempt(mut attempt)) => attempt.dispose(),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
impl RecoveredServicePublicationError {
    pub(in crate::cas_projection) fn startup_fence_is_closed_for_test(&self) -> bool {
        let Some(RecoveredServicePublicationOwner::Attempt(attempt)) = self.owner.as_ref() else {
            return false;
        };
        attempt
            .attempt
            .adopted_startup_gate
            .as_ref()
            .is_some_and(|startup| startup.is_closed())
    }

    pub(in crate::cas_projection) fn replacement_scheduler_pass_count_for_test(
        &self,
    ) -> Option<u64> {
        let RecoveredServicePublicationOwner::Attempt(attempt) = self.owner.as_ref()? else {
            return None;
        };
        Some(
            attempt
                .attempt
                .adopted_service
                .as_ref()?
                .accepted_input_scheduler_diagnostics()
                .recovered_pending_pass_count(),
        )
    }
}

impl fmt::Debug for RecoveredServicePublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecoveredServicePublicationError")
            .field("reason", &self.reason)
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for RecoveredServicePublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "recovered service publication became terminal: {:?}",
            self.reason
        )
    }
}

impl std::error::Error for RecoveredServicePublicationError {}

impl CandidateSetConvergedAdoptedProjectionConnectionService {
    pub(in crate::cas_projection) fn publish_recovered_service(
        self,
        slot: &Arc<RunningServiceSlot>,
    ) -> Result<RecoveredServicePublicationMetadata, RecoveredServicePublicationError> {
        let metadata = RecoveredServicePublicationMetadata {
            adoption: self.metadata(),
            accepted_candidate_count: self.accepted_candidate_count(),
        };
        let staged = self
            .stage_recovered_projection_lane()
            .map_err(|error| RecoveredServicePublicationError::lane(metadata, error))?;
        let (attempt, connection_owners) = staged.into_parts();
        let mut publication = Box::new(RecoveryServicePublicationAttempt {
            metadata,
            attempt,
            connection_owners,
            retirement_witness: None,
            prepared_epoch: None,
        });
        let outcome = catch_unwind(AssertUnwindSafe(|| publication.run(slot)));
        match outcome {
            Ok(Ok(())) => Ok(metadata),
            Ok(Err(reason)) => Err(RecoveredServicePublicationError::attempt(
                reason,
                publication,
            )),
            Err(_) if publication.attempt.publication_committed => Ok(metadata),
            Err(_) => Err(RecoveredServicePublicationError::attempt(
                RecoveredServicePublicationReason::UnexpectedUnwind,
                publication,
            )),
        }
    }
}
