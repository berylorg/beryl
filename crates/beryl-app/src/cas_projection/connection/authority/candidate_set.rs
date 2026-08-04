use super::*;
use crate::cas_projection::connection::registry::{
    LoadedRegistryRecoveryAuthorityKind, LoadedRegistryRecoveryObservation,
    authenticate_recovery_observations,
};

/// Internal reason one complete connection-owner set could not cross the seal boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum CandidateSetConnectionOwnerSealFailure {
    CapacityUnavailable,
    TopologyMismatch,
    AuthorityUnavailable,
    ConnectionRetired,
    RegistryAuthenticationUnavailable,
    RegistryTokenMismatch { observation_index: usize },
}

/// Private retirement fence retained by one candidate-set-converged adopted service.
///
/// This value exposes no connection, registry, or execution operation. Its only behavior is the
/// poison-safe release performed when the converged authority is consumed or dropped.
#[must_use = "the converged connection owner keeps dormant registry tokens retirement-safe"]
pub(in crate::cas_projection) struct CandidateSetConvergedProjectionConnectionOwner {
    authority: Arc<ConnectionRegistryAuthority>,
    connection: Arc<ProjectionConnection>,
    id: PendingProjectionQuarantineAuthorityId,
    identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    active: bool,
}

/// Final-publication hold over one exact stable connection authority and retirement gate.
pub(in crate::cas_projection) struct CandidateSetRecoveryPublicationBarrier<'a> {
    owner: &'a CandidateSetConvergedProjectionConnectionOwner,
    state: std::sync::MutexGuard<'a, ConnectionAuthorityState>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum CandidateSetRecoveryPublicationFailure {
    AuthorityUnavailable,
    ConnectionRetired,
    TopologyMismatch,
}

/// Atomically authenticates and transfers every exact reauthentication barrier.
///
/// Callers hold the corresponding forwarding-epoch and adopted-service registry barriers. This
/// function then holds every connection authority gate in stable generation order, authenticates
/// the complete accepted-token set under one loaded-registry lock, and performs one infallible
/// pending-to-converged state transition. Failure leaves `owners` unchanged.
pub(in crate::cas_projection) fn seal_pending_projection_connection_owners(
    owners: &mut Vec<PendingProjectionConnectionOwner>,
    observations: &[LoadedRegistryRecoveryObservation],
    #[cfg(test)] before_commit: impl FnOnce(),
) -> Result<
    Vec<CandidateSetConvergedProjectionConnectionOwner>,
    CandidateSetConnectionOwnerSealFailure,
> {
    let mut connections = Vec::new();
    let mut sealed = Vec::new();
    let mut gates = Vec::new();
    if connections.try_reserve_exact(owners.len()).is_err()
        || sealed.try_reserve_exact(owners.len()).is_err()
        || gates.try_reserve_exact(owners.len()).is_err()
    {
        return Err(CandidateSetConnectionOwnerSealFailure::CapacityUnavailable);
    }

    for owner in owners.iter() {
        if !owner.active
            || {
                #[cfg(test)]
                {
                    owner.force_candidate_set_topology_mismatch
                }
                #[cfg(not(test))]
                {
                    false
                }
            }
            || !Arc::ptr_eq(&owner.authority, &owner.connection.authority)
            || connections
                .last()
                .is_some_and(|previous: &Arc<ProjectionConnection>| {
                    previous.authority.generation.get() >= owner.authority.generation.get()
                })
        {
            return Err(CandidateSetConnectionOwnerSealFailure::TopologyMismatch);
        }
        connections.push(Arc::clone(&owner.connection));
    }
    if observations.iter().enumerate().any(|(index, observation)| {
        observation.authority().kind() != LoadedRegistryRecoveryAuthorityKind::ActiveLease
            || !connections.iter().any(|connection| {
                connection.authority.generation == observation.connection_generation()
            })
            || observations[..index].contains(observation)
    }) {
        return Err(CandidateSetConnectionOwnerSealFailure::TopologyMismatch);
    }

    for connection in &connections {
        let gate = connection
            .authority
            .gate
            .lock()
            .map_err(|_| CandidateSetConnectionOwnerSealFailure::AuthorityUnavailable)?;
        gates.push(gate);
    }

    for (owner, state) in owners.iter().zip(gates.iter()) {
        if owner.authority.is_retired() || state.retirement_complete {
            return Err(CandidateSetConnectionOwnerSealFailure::ConnectionRetired);
        }
        if state.pending_projection_quarantine
            != Some(PendingProjectionQuarantineAuthorityState {
                id: owner.id,
                identity: owner.identity,
                stage: PendingProjectionQuarantineStage::Reauthentication,
            })
        {
            return Err(CandidateSetConnectionOwnerSealFailure::TopologyMismatch);
        }
    }

    match authenticate_recovery_observations(observations) {
        Ok(None) => {}
        Ok(Some(observation_index)) => {
            return Err(
                CandidateSetConnectionOwnerSealFailure::RegistryTokenMismatch { observation_index },
            );
        }
        Err(_) => {
            return Err(CandidateSetConnectionOwnerSealFailure::RegistryAuthenticationUnavailable);
        }
    }

    #[cfg(test)]
    before_commit();

    for (owner, state) in owners.iter().zip(gates.iter_mut()) {
        state.pending_projection_quarantine = Some(PendingProjectionQuarantineAuthorityState {
            id: owner.id,
            identity: owner.identity,
            stage: PendingProjectionQuarantineStage::CandidateSetConverged,
        });
    }
    for mut owner in owners.drain(..) {
        sealed.push(CandidateSetConvergedProjectionConnectionOwner {
            authority: Arc::clone(&owner.authority),
            connection: Arc::clone(&owner.connection),
            id: owner.id,
            identity: owner.identity,
            active: true,
        });
        owner.active = false;
    }
    for connection in &connections {
        connection.authority.retirement_changed.notify_all();
    }
    Ok(sealed)
}

impl CandidateSetConvergedProjectionConnectionOwner {
    pub(in crate::cas_projection) fn connection_for_recovery_publication(
        &self,
    ) -> &Arc<ProjectionConnection> {
        &self.connection
    }

    pub(in crate::cas_projection) fn connection_generation_for_recovery_publication(&self) -> u64 {
        self.authority.generation.get()
    }

    pub(in crate::cas_projection) fn lock_for_recovery_publication(
        &self,
        expected: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    ) -> Result<CandidateSetRecoveryPublicationBarrier<'_>, CandidateSetRecoveryPublicationFailure>
    {
        if !self.active
            || self.identity != expected
            || !Arc::ptr_eq(&self.authority, &self.connection.authority)
        {
            return Err(CandidateSetRecoveryPublicationFailure::TopologyMismatch);
        }
        let state = self
            .authority
            .gate
            .lock()
            .map_err(|_| CandidateSetRecoveryPublicationFailure::AuthorityUnavailable)?;
        if self.authority.is_retired() || state.retirement_complete {
            return Err(CandidateSetRecoveryPublicationFailure::ConnectionRetired);
        }
        if state.pending_projection_quarantine
            != Some(PendingProjectionQuarantineAuthorityState {
                id: self.id,
                identity: self.identity,
                stage: PendingProjectionQuarantineStage::CandidateSetConverged,
            })
        {
            return Err(CandidateSetRecoveryPublicationFailure::TopologyMismatch);
        }
        Ok(CandidateSetRecoveryPublicationBarrier { owner: self, state })
    }

    fn release(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        self.authority.release_pending_projection_quarantine_owner(
            self.id,
            self.identity,
            PendingProjectionQuarantineStage::CandidateSetConverged,
        );
    }
}

impl CandidateSetRecoveryPublicationBarrier<'_> {
    pub(in crate::cas_projection) fn validates(
        &self,
        connection: &Arc<ProjectionConnection>,
        expected: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    ) -> bool {
        self.owner.active
            && self.owner.identity == expected
            && Arc::ptr_eq(&self.owner.connection, connection)
            && Arc::ptr_eq(&self.owner.authority, &connection.authority)
            && !self.owner.authority.is_retired()
            && !self.state.retirement_complete
            && self.state.pending_projection_quarantine
                == Some(PendingProjectionQuarantineAuthorityState {
                    id: self.owner.id,
                    identity: self.owner.identity,
                    stage: PendingProjectionQuarantineStage::CandidateSetConverged,
                })
    }
}

impl Drop for CandidateSetConvergedProjectionConnectionOwner {
    fn drop(&mut self) {
        self.release();
    }
}

impl std::fmt::Debug for CandidateSetConvergedProjectionConnectionOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CandidateSetConvergedProjectionConnectionOwner")
            .field("connection_generation", &self.authority.generation)
            .field("connection", &self.connection.identity_observation())
            .field("active", &self.active)
            .finish_non_exhaustive()
    }
}
