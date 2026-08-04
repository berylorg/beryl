use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

use beryl_home_store::HomeStore;

use super::*;
use crate::cas_projection::{
    connection::{
        ConnectionEpochIdentity,
        lifecycle::ProjectionConnectionIdentityObservation,
        registry::{
            LoadedRegistryRecoveryAuthorityKind, LoadedRegistryRecoveryObservation,
            authenticate_recovery_observation,
        },
    },
    persistent_failure::{
        PendingProjectionWitness, PersistentFailureCutIdentity, PersistentFailureGeneration,
        PersistentFailureProjectionRetainer,
    },
    service_config::{ProjectionPreactivationRecoveryHold, ProjectionWorkerPermit},
};

/// Pointer-exact stable connection observation that exposes no executable connection handle.
#[derive(Clone)]
pub(in crate::cas_projection) struct StableProjectionConnectionObservation {
    connection: Arc<ProjectionConnection>,
    identity: ProjectionConnectionIdentityObservation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum StableProjectionConnectionAuthentication {
    Current,
    Retired,
    Mismatch,
}

impl StableProjectionConnectionObservation {
    fn new(connection: &Arc<ProjectionConnection>) -> Self {
        Self {
            connection: Arc::clone(connection),
            identity: connection.identity_observation(),
        }
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn corrupt_identity_for_test(&mut self) {
        self.identity = ProjectionConnectionIdentityObservation::new(
            self.identity
                .connection_generation()
                .checked_add(1)
                .unwrap(),
            self.identity.runtime_id(),
            self.identity.process_generation(),
        );
    }

    pub(in crate::cas_projection) const fn identity(
        &self,
    ) -> ProjectionConnectionIdentityObservation {
        self.identity
    }

    pub(in crate::cas_projection) fn matches_connection(
        &self,
        connection: &Arc<ProjectionConnection>,
    ) -> bool {
        Arc::ptr_eq(&self.connection, connection)
            && self.identity == connection.identity_observation()
    }

    pub(in crate::cas_projection) fn same_connection(&self, other: &Self) -> bool {
        self == other
    }

    pub(in crate::cas_projection) fn connection(&self) -> &Arc<ProjectionConnection> {
        &self.connection
    }

    /// Authenticates that this exact stable connection currently owns the expected adopted epoch.
    ///
    /// The check correlates the pointer and immutable stable identity with the forwarding hub's
    /// current epoch, then requires both the complete epoch identity and retained home pointer to
    /// match. It grants no command, router, or projection authority.
    pub(in crate::cas_projection) fn authenticate_current_adopted_epoch(
        &self,
        expected_home: &Arc<HomeStore>,
        expected_epoch: ConnectionEpochIdentity,
    ) -> Result<StableProjectionConnectionAuthentication, ProjectionCoordinatorError> {
        if self.identity != self.connection.identity_observation()
            || expected_epoch.home_id() != expected_home.home_id()
        {
            return Ok(StableProjectionConnectionAuthentication::Mismatch);
        }
        if self.connection.is_retired() {
            return Ok(StableProjectionConnectionAuthentication::Retired);
        }
        let current = match self.connection.current_epoch() {
            Ok(current) => current,
            Err(_) if self.connection.is_retired() => {
                return Ok(StableProjectionConnectionAuthentication::Retired);
            }
            Err(error) => return Err(error),
        };
        if self.connection.is_retired() {
            return Ok(StableProjectionConnectionAuthentication::Retired);
        }
        Ok(
            if current.identity == expected_epoch && Arc::ptr_eq(&current.home, expected_home) {
                StableProjectionConnectionAuthentication::Current
            } else {
                StableProjectionConnectionAuthentication::Mismatch
            },
        )
    }
}

impl std::fmt::Debug for StableProjectionConnectionObservation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StableProjectionConnectionObservation")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl PartialEq for StableProjectionConnectionObservation {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.connection, &other.connection) && self.identity == other.identity
    }
}

impl Eq for StableProjectionConnectionObservation {}

impl Hash for StableProjectionConnectionObservation {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.connection).hash(state);
        self.identity.hash(state);
    }
}

/// Read-only exact lease-owner witness for recovery grouping and registry auditing.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::cas_projection) struct LoadedLeaseRecoveryObservation {
    connection: StableProjectionConnectionObservation,
    registry: LoadedRegistryRecoveryObservation,
    active: bool,
    surrender_cut_identity: Option<PersistentFailureCutIdentity>,
    retained_cut_identity: Option<PersistentFailureCutIdentity>,
}

/// Non-executable ownership of one loaded candidate token and its exact worker admission hold.
#[must_use = "the pending projection lease owner retains local registry and worker authority"]
pub(in crate::cas_projection) struct PendingProjectionLeaseOwner {
    observation: LoadedLeaseRecoveryObservation,
    token: LeaseToken,
    admission: Option<ProjectionPreactivationRecoveryHold>,
    unsubscribe_timeout: std::time::Duration,
    registry_live: bool,
}

/// Dormant accepted ownership of one exact loaded token and replacement worker hold.
///
/// This typestate has no lease or projection materialization surface. Phase 83 may only retain it,
/// reauthenticate it, or demote it back to the pending owner for explicit disposition.
#[must_use = "the dormant recovered owner retains exact registry and worker authority"]
pub(in crate::cas_projection) struct DormantRecoveredProjectionLeaseOwner {
    pending: PendingProjectionLeaseOwner,
}

/// Non-executable ownership of one registry authority selected for local disposition.
#[must_use = "the disposition owner must survive until the registry commit or settle locally"]
pub(in crate::cas_projection) struct LocalLoadedRegistryDispositionOwner {
    observation: LoadedLeaseRecoveryObservation,
    admission: Option<ProjectionPreactivationRecoveryHold>,
    registry_live: bool,
}

impl LoadedLeaseRecoveryObservation {
    fn new(
        connection: &Arc<ProjectionConnection>,
        registry: LoadedRegistryRecoveryObservation,
        active: bool,
        surrender_cut_identity: Option<PersistentFailureCutIdentity>,
        retained_cut_identity: Option<PersistentFailureCutIdentity>,
    ) -> Self {
        Self {
            connection: StableProjectionConnectionObservation::new(connection),
            registry,
            active,
            surrender_cut_identity,
            retained_cut_identity,
        }
    }

    pub(in crate::cas_projection) fn connection(&self) -> &StableProjectionConnectionObservation {
        &self.connection
    }

    pub(in crate::cas_projection) fn registry(&self) -> &LoadedRegistryRecoveryObservation {
        &self.registry
    }

    pub(in crate::cas_projection) const fn is_active(&self) -> bool {
        self.active
    }

    pub(in crate::cas_projection) const fn surrender_cut_identity(
        &self,
    ) -> Option<PersistentFailureCutIdentity> {
        self.surrender_cut_identity
    }

    pub(in crate::cas_projection) const fn retained_cut_identity(
        &self,
    ) -> Option<PersistentFailureCutIdentity> {
        self.retained_cut_identity
    }

    pub(in crate::cas_projection) fn is_exact_for_connection(&self) -> bool {
        let connection = self.connection.identity();
        connection.connection_generation() == self.registry.connection_generation().get()
            && connection.runtime_id() == self.registry.key().runtime_id
            && connection.process_generation() == self.registry.key().process_generation
            && connection.process_generation() == self.registry.loaded_generation().process()
    }

    pub(in crate::cas_projection) fn is_active_candidate_for(
        &self,
        identity: PersistentFailureCutIdentity,
    ) -> bool {
        self.active
            && self.retained_cut_identity.is_none()
            && self.surrender_cut_identity == Some(identity)
            && self.registry.authority().kind() == LoadedRegistryRecoveryAuthorityKind::ActiveLease
            && self.is_exact_for_connection()
    }
}

impl PendingProjectionLeaseOwner {
    #[cfg(test)]
    pub(in crate::cas_projection) fn corrupt_stable_connection_identity_for_test(&mut self) {
        self.observation.connection.corrupt_identity_for_test();
    }

    pub(in crate::cas_projection) fn observation(&self) -> &LoadedLeaseRecoveryObservation {
        &self.observation
    }

    pub(in crate::cas_projection) fn registry_observation(
        &self,
    ) -> &LoadedRegistryRecoveryObservation {
        self.observation.registry()
    }

    pub(in crate::cas_projection) fn is_exact_candidate_for(
        &self,
        identity: PersistentFailureCutIdentity,
    ) -> bool {
        self.registry_live
            && self.admission.is_some()
            && self.observation.is_active_candidate_for(identity)
    }

    /// Authenticates this exact active token under the actual loaded-registry lock.
    pub(in crate::cas_projection) fn authenticate_live_exact(
        &self,
    ) -> Result<bool, ProjectionCoordinatorError> {
        if !self.registry_live
            || self.admission.is_none()
            || !self.observation.is_active()
            || !self.observation.is_exact_for_connection()
            || self.observation.registry().authority().kind()
                != LoadedRegistryRecoveryAuthorityKind::ActiveLease
        {
            return Ok(false);
        }
        authenticate_recovery_observation(self.observation.registry())
    }

    pub(in crate::cas_projection) fn stable_connection_observation(
        &self,
    ) -> &StableProjectionConnectionObservation {
        self.observation.connection()
    }

    pub(in crate::cas_projection) fn exchange_recovery_hold(
        &mut self,
        replacement: ProjectionPreactivationRecoveryHold,
    ) -> ProjectionPreactivationRecoveryHold {
        self.admission
            .replace(replacement)
            .expect("a preflighted pending candidate retains one exact old recovery hold")
    }

    /// Consumes an authenticated accepted candidate into non-executable dormant ownership.
    ///
    /// The caller performs the final live-token authentication immediately before this ownership-
    /// only transition. No registry operation, lease construction, or projection construction
    /// occurs here.
    pub(in crate::cas_projection) fn into_dormant_recovered_owner(
        self,
    ) -> DormantRecoveredProjectionLeaseOwner {
        assert!(
            self.registry_live
                && self.admission.is_some()
                && self.observation.is_active()
                && self.observation.is_exact_for_connection()
                && self.observation.registry().authority().kind()
                    == LoadedRegistryRecoveryAuthorityKind::ActiveLease,
            "only one structurally exact pending candidate can become dormant recovered ownership"
        );
        DormantRecoveredProjectionLeaseOwner { pending: self }
    }

    /// Revokes or confirms absence of the exact local token and returns its replacement hold.
    ///
    /// Disposition deliberately recovers a poisoned registry guard only to remove this globally
    /// unique token. It cannot certify liveness or restore ordinary registry operation.
    pub(in crate::cas_projection) fn dispose_local(
        mut self,
    ) -> ProjectionPreactivationRecoveryHold {
        assert!(
            self.registry_live && self.admission.is_some(),
            "an adopted pending candidate retains live registry ownership and one replacement hold"
        );
        crate::cas_projection::connection::registry::settle_recovery_observation_locally(
            self.observation.registry(),
        );
        self.registry_live = false;
        self.admission
            .take()
            .expect("a locally disposed candidate returns its exact replacement hold")
    }
}

impl DormantRecoveredProjectionLeaseOwner {
    pub(in crate::cas_projection) fn authenticate_live_exact(
        &self,
    ) -> Result<bool, ProjectionCoordinatorError> {
        self.pending.authenticate_live_exact()
    }

    pub(in crate::cas_projection) fn stable_connection_observation(
        &self,
    ) -> &StableProjectionConnectionObservation {
        self.pending.stable_connection_observation()
    }

    pub(in crate::cas_projection) fn registry_observation(
        &self,
    ) -> &LoadedRegistryRecoveryObservation {
        self.pending.registry_observation()
    }

    fn matches_witness(&self, witness: &PendingProjectionWitness) -> bool {
        let registry = self.registry_observation();
        registry.owner() == *witness.syndic_thread_id()
            && registry.loaded_generation() == *witness.loaded_session_generation()
            && registry.key().runtime_id == witness.execution_binding().runtime_id()
            && &registry.key().cas_thread_id == witness.cas_thread_id()
    }

    /// Restores the exact loaded lease and scheduled-worker permit after service publication.
    ///
    /// The registry token remains continuously owned by this value and is moved straight into the
    /// lease. The replacement recovery hold is split over its original admission; this transition
    /// performs no registry mutation and no worker-pool acquisition.
    pub(in crate::cas_projection) fn into_loaded_projection_lease(
        mut self,
        witness: &PendingProjectionWitness,
        retainer: PersistentFailureProjectionRetainer,
    ) -> (LoadedProjectionLease, ProjectionWorkerPermit) {
        assert!(
            self.pending.registry_live
                && self.pending.admission.is_some()
                && self.pending.observation.is_active()
                && self.pending.observation.is_exact_for_connection()
                && self.pending.observation.registry().authority().kind()
                    == LoadedRegistryRecoveryAuthorityKind::ActiveLease
                && self.matches_witness(witness),
            "only exact dormant recovered provenance can restore one loaded projection"
        );

        let registry = self.pending.registry_observation().clone();
        assert_eq!(
            registry,
            LoadedRegistryRecoveryObservation::active(
                registry.key().clone(),
                registry.connection_generation(),
                registry.owner(),
                registry.loaded_generation(),
                self.pending.token,
            ),
            "the dormant owner must retain the private token represented by its observation"
        );
        let connection = Arc::clone(self.pending.stable_connection_observation().connection());
        let key = registry.key().clone();
        let owner = registry.owner();
        let generation = registry.loaded_generation();
        let token = self.pending.token;
        let unsubscribe_timeout = self.pending.unsubscribe_timeout;
        let recovery_hold = self
            .pending
            .admission
            .take()
            .expect("a dormant recovered owner retains one replacement worker hold");
        let (worker, surrender) = recovery_hold.into_worker_and_surrender(retainer);
        self.pending.registry_live = false;
        drop(self);

        (
            LoadedProjectionLease::new(
                connection,
                key,
                owner,
                generation,
                token,
                unsubscribe_timeout,
                Some(surrender),
            ),
            worker,
        )
    }

    /// Demotes a revoked accepted entry back to its exact owning rejection state.
    pub(in crate::cas_projection) fn into_pending_owner(self) -> PendingProjectionLeaseOwner {
        self.pending
    }
}

impl LocalLoadedRegistryDispositionOwner {
    pub(in crate::cas_projection) fn observation(&self) -> &LoadedLeaseRecoveryObservation {
        &self.observation
    }

    pub(in crate::cas_projection) fn registry_observation(
        &self,
    ) -> &LoadedRegistryRecoveryObservation {
        self.observation.registry()
    }

    pub(in crate::cas_projection) fn is_exact_for_cut(
        &self,
        identity: PersistentFailureCutIdentity,
    ) -> bool {
        let surrender_matches = match self.observation.registry().authority().kind() {
            LoadedRegistryRecoveryAuthorityKind::ActiveLease
            | LoadedRegistryRecoveryAuthorityKind::QuarantinedAnchor => {
                self.observation.surrender_cut_identity() == Some(identity)
                    && self.admission.is_some()
            }
            LoadedRegistryRecoveryAuthorityKind::ReacquisitionReservation => {
                self.observation
                    .surrender_cut_identity()
                    .map_or(true, |surrender| surrender == identity)
                    && (self.observation.surrender_cut_identity().is_some()
                        == self.admission.is_some())
            }
        };
        self.registry_live
            && self.observation.is_active()
            && self.observation.retained_cut_identity() == Some(identity)
            && self.observation.is_exact_for_connection()
            && surrender_matches
    }

    pub(in crate::cas_projection) fn is_exact_target_disposition_for(
        &self,
        identity: PersistentFailureCutIdentity,
    ) -> bool {
        let surrender_matches = self
            .observation
            .surrender_cut_identity()
            .map_or(true, |surrender| surrender == identity);
        self.registry_live
            && self.observation.is_active()
            && self.observation.retained_cut_identity().is_none()
            && self.observation.is_exact_for_connection()
            && self.observation.registry().authority().kind()
                == LoadedRegistryRecoveryAuthorityKind::ActiveLease
            && surrender_matches
            && (self.observation.surrender_cut_identity().is_some() == self.admission.is_some())
    }

    pub(in crate::cas_projection) fn is_exact_same_native_disposition_for(
        &self,
        identity: PersistentFailureCutIdentity,
    ) -> bool {
        self.registry_live
            && self.observation.is_active()
            && self.observation.retained_cut_identity().is_none()
            && self.observation.surrender_cut_identity() == Some(identity)
            && self.observation.is_exact_for_connection()
            && self.observation.registry().authority().kind()
                == LoadedRegistryRecoveryAuthorityKind::QuarantinedAnchor
            && self.admission.is_some()
    }

    /// Releases this local owner after its exact authority was removed by the batch commit.
    pub(in crate::cas_projection) fn finish_after_registry_commit(mut self) {
        self.registry_live = false;
    }
}

impl std::fmt::Debug for PendingProjectionLeaseOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PendingProjectionLeaseOwner")
            .field("observation", &self.observation)
            .field("admission_held", &self.admission.is_some())
            .field("unsubscribe_timeout", &self.unsubscribe_timeout)
            .field("registry_live", &self.registry_live)
            .finish()
    }
}

impl std::fmt::Debug for DormantRecoveredProjectionLeaseOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DormantRecoveredProjectionLeaseOwner")
            .field("pending", &self.pending)
            .finish()
    }
}

impl std::fmt::Debug for LocalLoadedRegistryDispositionOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalLoadedRegistryDispositionOwner")
            .field("observation", &self.observation)
            .field("admission_held", &self.admission.is_some())
            .field("registry_live", &self.registry_live)
            .finish()
    }
}

fn settle_local_owner_drop(observation: &LoadedLeaseRecoveryObservation, registry_live: &mut bool) {
    if *registry_live {
        crate::cas_projection::connection::registry::settle_recovery_observation_locally(
            observation.registry(),
        );
        *registry_live = false;
    }
}

impl Drop for PendingProjectionLeaseOwner {
    fn drop(&mut self) {
        settle_local_owner_drop(&self.observation, &mut self.registry_live);
    }
}

impl Drop for LocalLoadedRegistryDispositionOwner {
    fn drop(&mut self) {
        settle_local_owner_drop(&self.observation, &mut self.registry_live);
    }
}

fn surrender_cut_identity(
    surrender: &ProjectionPreactivationSurrender,
) -> PersistentFailureCutIdentity {
    surrender
        .retainer()
        .cut_identity(PersistentFailureGeneration::FIRST)
}

impl LoadedProjectionLease {
    pub(in crate::cas_projection) fn recovery_observation(&self) -> LoadedLeaseRecoveryObservation {
        LoadedLeaseRecoveryObservation::new(
            &self.connection,
            LoadedRegistryRecoveryObservation::active(
                self.key.clone(),
                self.connection.authority.generation,
                self.owner,
                self.generation,
                self.token,
            ),
            self.active,
            self.preactivation_surrender
                .as_ref()
                .map(surrender_cut_identity),
            None,
        )
    }

    /// Converts this lease into inert local quarantine ownership without settling its token.
    pub(in crate::cas_projection) fn into_pending_projection_lease_owner(
        mut self,
    ) -> PendingProjectionLeaseOwner {
        let observation = self.recovery_observation();
        let token = self.token;
        let unsubscribe_timeout = self.unsubscribe_timeout;
        let admission = self
            .preactivation_surrender
            .take()
            .map(ProjectionPreactivationSurrender::into_recovery_hold);
        self.active = false;
        drop(self);
        PendingProjectionLeaseOwner {
            observation,
            token,
            admission,
            unsubscribe_timeout,
            registry_live: true,
        }
    }

    /// Dematerializes a recovered lease after its separately returned worker permit has dropped.
    pub(in crate::cas_projection) fn into_dormant_recovered_owner(
        self,
    ) -> DormantRecoveredProjectionLeaseOwner {
        let surrender = self
            .preactivation_surrender
            .as_ref()
            .expect("a recovered loaded lease retains its replacement surrender");
        assert!(
            surrender.is_sole_admission_owner(),
            "the recovered worker permit must drop before ordinary dematerialization"
        );
        self.into_pending_projection_lease_owner()
            .into_dormant_recovered_owner()
    }

    /// Rolls back materialization while consuming the separately returned exact worker permit.
    pub(in crate::cas_projection) fn into_dormant_recovered_owner_with_worker(
        self,
        worker: ProjectionWorkerPermit,
    ) -> DormantRecoveredProjectionLeaseOwner {
        assert!(
            self.preactivation_surrender
                .as_ref()
                .is_some_and(|surrender| surrender.matches_worker(&worker)),
            "rollback must consume the worker permit split from this recovered lease"
        );
        drop(worker);
        self.into_dormant_recovered_owner()
    }

    /// Converts a retained target lease into inert local-disposition ownership.
    pub(in crate::cas_projection) fn into_local_registry_disposition_owner(
        mut self,
    ) -> LocalLoadedRegistryDispositionOwner {
        let observation = self.recovery_observation();
        let admission = self
            .preactivation_surrender
            .take()
            .map(ProjectionPreactivationSurrender::into_recovery_hold);
        self.active = false;
        drop(self);
        LocalLoadedRegistryDispositionOwner {
            observation,
            admission,
            registry_live: true,
        }
    }
}

impl QuarantinedProjectionAnchor {
    pub(in crate::cas_projection) fn recovery_observation(&self) -> LoadedLeaseRecoveryObservation {
        LoadedLeaseRecoveryObservation::new(
            &self.connection,
            LoadedRegistryRecoveryObservation::quarantined(
                self.key.clone(),
                self.connection.authority.generation,
                self.owner,
                self.generation,
                self.token,
            ),
            self.state == QuarantinedAnchorState::Anchored,
            self.preactivation_surrender
                .as_ref()
                .map(surrender_cut_identity),
            None,
        )
    }

    pub(in crate::cas_projection) fn into_local_registry_disposition_owner(
        mut self,
    ) -> Result<LocalLoadedRegistryDispositionOwner, Self> {
        if self.state != QuarantinedAnchorState::Anchored || self.transfer_cleanup.is_some() {
            return Err(self);
        }
        let observation = self.recovery_observation();
        let admission = self
            .preactivation_surrender
            .take()
            .map(ProjectionPreactivationSurrender::into_recovery_hold);
        self.state = QuarantinedAnchorState::Released;
        drop(self);
        Ok(LocalLoadedRegistryDispositionOwner {
            observation,
            admission,
            registry_live: true,
        })
    }
}

impl FailureRetainedRawLoadedLease {
    pub(in crate::cas_projection) fn recovery_observation(&self) -> LoadedLeaseRecoveryObservation {
        LoadedLeaseRecoveryObservation::new(
            &self.connection,
            LoadedRegistryRecoveryObservation::active(
                self.key.clone(),
                self.connection.authority.generation,
                self.owner,
                self.generation,
                self.token,
            ),
            true,
            Some(surrender_cut_identity(&self.preactivation_surrender)),
            Some(self.identity),
        )
    }

    pub(in crate::cas_projection) fn into_local_registry_disposition_owner(
        self,
    ) -> LocalLoadedRegistryDispositionOwner {
        let observation = self.recovery_observation();
        let Self {
            preactivation_surrender,
            ..
        } = self;
        LocalLoadedRegistryDispositionOwner {
            observation,
            admission: Some(preactivation_surrender.into_recovery_hold()),
            registry_live: true,
        }
    }
}

impl FailureRetainedRawQuarantinedAnchor {
    pub(in crate::cas_projection) fn recovery_observation(&self) -> LoadedLeaseRecoveryObservation {
        LoadedLeaseRecoveryObservation::new(
            &self.connection,
            LoadedRegistryRecoveryObservation::quarantined(
                self.key.clone(),
                self.connection.authority.generation,
                self.owner,
                self.generation,
                self.token,
            ),
            true,
            Some(surrender_cut_identity(&self.preactivation_surrender)),
            Some(self.identity),
        )
    }

    pub(in crate::cas_projection) fn into_local_registry_disposition_owner(
        self,
    ) -> LocalLoadedRegistryDispositionOwner {
        let observation = self.recovery_observation();
        let Self {
            preactivation_surrender,
            ..
        } = self;
        LocalLoadedRegistryDispositionOwner {
            observation,
            admission: Some(preactivation_surrender.into_recovery_hold()),
            registry_live: true,
        }
    }
}

impl FailureRetainedRawReacquisitionReservation {
    pub(in crate::cas_projection) fn recovery_observation(&self) -> LoadedLeaseRecoveryObservation {
        LoadedLeaseRecoveryObservation::new(
            &self.connection,
            LoadedRegistryRecoveryObservation::reacquisition_reservation(
                self.key.clone(),
                self.anchor_connection,
                self.connection.authority.generation,
                self.owner,
                self.anchor_generation,
                self.anchor_token,
                self.token,
            ),
            true,
            self.preactivation_surrender
                .as_ref()
                .map(surrender_cut_identity),
            Some(self.identity),
        )
    }

    pub(in crate::cas_projection) fn into_local_registry_disposition_owner(
        self,
    ) -> LocalLoadedRegistryDispositionOwner {
        let observation = self.recovery_observation();
        let Self {
            preactivation_surrender,
            ..
        } = self;
        LocalLoadedRegistryDispositionOwner {
            observation,
            admission: preactivation_surrender
                .map(ProjectionPreactivationSurrender::into_recovery_hold),
            registry_live: true,
        }
    }
}
