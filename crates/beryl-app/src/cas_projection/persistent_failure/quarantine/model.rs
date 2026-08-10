use std::fmt;

use beryl_home_store::HomeGeneration;
use beryl_model::{
    BerylHomeId, BindingRevision, CasLoadedSessionGeneration, CasThreadId, ExecutionBinding,
    SyndicThreadId,
};
use syndic_storage::CasLineageProof;

use super::super::{
    PersistentFailureGeneration, PersistentFailureRecoveryInventory, ProjectionServiceGeneration,
};
use crate::cas_projection::connection::{
    LoadedThreadKey, LocalLoadedRegistryDispositionOwner, PendingProjectionConnectionOwner,
    PendingProjectionLeaseOwner, StableProjectionConnectionObservation,
};

/// Opaque, non-executable ownership of every pending projection retained by one failed service.
#[must_use = "the quarantine owns the retained failed service and every local disposition"]
pub struct PersistentFailurePendingProjectionQuarantine {
    pub(super) inventory: PersistentFailureRecoveryInventory,
}

/// Bounded content-free observation of one pending-projection quarantine.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PersistentFailurePendingProjectionQuarantineMetadata {
    pub(in crate::cas_projection::persistent_failure) group_count: usize,
    pub(in crate::cas_projection::persistent_failure) candidate_count: usize,
    pub(in crate::cas_projection::persistent_failure) retained_connection_count: usize,
    pub(in crate::cas_projection::persistent_failure) local_disposition_count: usize,
    pub(in crate::cas_projection::persistent_failure) late_publication_count: usize,
    pub(in crate::cas_projection::persistent_failure) promotable: bool,
}

/// Closed reason why a recovery inventory could not publish a usable quarantine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentFailurePendingProjectionQuarantineReason {
    /// The sealed recovery inventory was incomplete, unstable, or already consumed.
    InventoryNotPromotable,
    /// Retained cut authority could not be observed through its synchronization boundary.
    RetentionUnavailable,
    /// An owner or installed stage did not belong to the exact finished cut.
    CutIdentityMismatch,
    /// The retained service and candidate topology disagreed about stable connection identity.
    ConnectionIdentityMismatch,
    /// A retained connection had already retired or lost its synchronized authority boundary.
    ConnectionUnavailable,
    /// The exact loaded-registry sibling set contained authority absent from the drain.
    MissingSiblingToken,
    /// Two drained owners claimed the same loaded-registry token.
    DuplicateToken,
    /// Wrappers grouped under one loaded identity did not share one complete witness.
    WitnessDisagreement,
    /// A retained target projection or router guard had the wrong cut-local disposition.
    TargetDispositionMismatch,
    /// Promotion or cleanup barrier ownership did not match the exact connection topology.
    BarrierDispositionMismatch,
    /// The loaded-registry audit or atomic commit was unavailable.
    RegistryUnavailable,
    /// Authority published after inventory sealing or while the quarantine was checked out.
    LatePublication,
}

/// Owning failure from the all-or-nothing quarantine conversion.
#[must_use = "the error retains the complete inventory or inert converted topology"]
pub struct PersistentFailurePendingProjectionQuarantineError {
    pub(super) inventory: PersistentFailureRecoveryInventory,
    pub(super) reason: PersistentFailurePendingProjectionQuarantineReason,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct PendingProjectionWitness {
    pub(in crate::cas_projection::persistent_failure) home_id: BerylHomeId,
    pub(in crate::cas_projection::persistent_failure) home_generation: HomeGeneration,
    pub(in crate::cas_projection::persistent_failure) syndic_thread_id: SyndicThreadId,
    pub(in crate::cas_projection::persistent_failure) binding_revision: BindingRevision,
    pub(in crate::cas_projection::persistent_failure) execution_binding: ExecutionBinding,
    pub(in crate::cas_projection::persistent_failure) cas_thread_id: CasThreadId,
    pub(in crate::cas_projection::persistent_failure) loaded_session_generation:
        CasLoadedSessionGeneration,
    pub(in crate::cas_projection::persistent_failure) lineage_proof: CasLineageProof,
}

pub(in crate::cas_projection) struct PendingProjectionCandidateGroup {
    pub(in crate::cas_projection::persistent_failure) identity: PendingProjectionGroupIdentity,
    pub(in crate::cas_projection::persistent_failure) witness: PendingProjectionWitness,
    pub(in crate::cas_projection::persistent_failure) candidates: Vec<PendingProjectionLeaseOwner>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::cas_projection) struct PendingProjectionGroupIdentity {
    pub(in crate::cas_projection::persistent_failure) connection:
        StableProjectionConnectionObservation,
    pub(in crate::cas_projection::persistent_failure) key: LoadedThreadKey,
    pub(in crate::cas_projection::persistent_failure) owner: SyndicThreadId,
    pub(in crate::cas_projection::persistent_failure) loaded_generation: CasLoadedSessionGeneration,
}

pub(in crate::cas_projection::persistent_failure) enum PendingProjectionQuarantineOwnedTopology {
    Normalized {
        groups: Vec<PendingProjectionCandidateGroup>,
        connection_owners: Vec<PendingProjectionConnectionOwner>,
        remainder: super::super::coordinator::PersistentFailureRecoveryDrain,
        pending_local_dispositions: Vec<LocalLoadedRegistryDispositionOwner>,
        settled_disposition_count: usize,
    },
    Inert {
        drain: super::super::coordinator::PersistentFailureRecoveryDrain,
    },
}

pub(in crate::cas_projection::persistent_failure) struct PendingProjectionQuarantineAuthority {
    pub(in crate::cas_projection::persistent_failure) topology:
        PendingProjectionQuarantineOwnedTopology,
    pub(in crate::cas_projection::persistent_failure) reason:
        Option<PersistentFailurePendingProjectionQuarantineReason>,
}

/// Exact normalized quarantine ownership checked out for one service-epoch adoption attempt.
///
/// The retained remainder and local dispositions deliberately remain opaque. Later recovery phases
/// may inspect candidate and connection identity through this owner without gaining coordinator or
/// registry mutation authority.
#[must_use = "the topology owns every normalized candidate and local disposition"]
pub(in crate::cas_projection) struct PendingProjectionAdoptionTopology {
    groups: Vec<PendingProjectionCandidateGroup>,
    connection_owners: Vec<PendingProjectionConnectionOwner>,
    remainder: super::super::coordinator::PersistentFailureRecoveryDrain,
    pending_local_dispositions: Vec<LocalLoadedRegistryDispositionOwner>,
    settled_disposition_count: usize,
}

impl PendingProjectionAdoptionTopology {
    pub(in crate::cas_projection::persistent_failure) fn from_normalized(
        groups: Vec<PendingProjectionCandidateGroup>,
        connection_owners: Vec<PendingProjectionConnectionOwner>,
        remainder: super::super::coordinator::PersistentFailureRecoveryDrain,
        pending_local_dispositions: Vec<LocalLoadedRegistryDispositionOwner>,
        settled_disposition_count: usize,
    ) -> Self {
        Self {
            groups,
            connection_owners,
            remainder,
            pending_local_dispositions,
            settled_disposition_count,
        }
    }

    pub(in crate::cas_projection) fn groups(&self) -> &[PendingProjectionCandidateGroup] {
        &self.groups
    }

    pub(in crate::cas_projection) fn groups_mut(
        &mut self,
    ) -> &mut [PendingProjectionCandidateGroup] {
        &mut self.groups
    }

    pub(in crate::cas_projection) fn connection_owners(
        &self,
    ) -> &[PendingProjectionConnectionOwner] {
        &self.connection_owners
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn take_connection_owners_for_adversarial_test(
        &mut self,
    ) -> Vec<PendingProjectionConnectionOwner> {
        std::mem::take(&mut self.connection_owners)
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn replace_connection_owners_for_adversarial_test(
        &mut self,
        owners: Vec<PendingProjectionConnectionOwner>,
    ) -> Vec<PendingProjectionConnectionOwner> {
        std::mem::replace(&mut self.connection_owners, owners)
    }

    pub(in crate::cas_projection) fn group_count(&self) -> usize {
        self.groups.len()
    }

    pub(in crate::cas_projection) fn candidate_count(&self) -> usize {
        self.groups.iter().fold(0usize, |count, group| {
            count
                .checked_add(group.candidates.len())
                .expect("bounded adoption candidate counts fit in memory")
        })
    }

    pub(in crate::cas_projection) fn connection_owner_count(&self) -> usize {
        self.connection_owners.len()
    }

    pub(in crate::cas_projection) fn retained_connection_count(&self) -> usize {
        self.remainder.retained_connections.len()
    }

    pub(in crate::cas_projection) fn local_disposition_count(&self) -> usize {
        self.remainder
            .local_disposition_count()
            .checked_add(self.pending_local_dispositions.len())
            .expect("bounded pending local disposition counts fit in memory")
            .checked_add(self.settled_disposition_count)
            .expect("bounded adoption disposition counts fit in memory")
    }

    /// Transfers the complete candidate and connection-quarantine authority into the sole
    /// reauthentication ledger while leaving the opaque old-service remainder in this topology.
    pub(in crate::cas_projection) fn take_reauthentication_parts(
        &mut self,
    ) -> (
        Vec<PendingProjectionCandidateGroup>,
        Vec<PendingProjectionConnectionOwner>,
    ) {
        debug_assert!(
            self.pending_local_dispositions.is_empty(),
            "a promotable adopted topology settled every noncandidate disposition"
        );
        (
            std::mem::take(&mut self.groups),
            std::mem::take(&mut self.connection_owners),
        )
    }
}

impl PendingProjectionCandidateGroup {
    pub(in crate::cas_projection) fn identity(&self) -> &PendingProjectionGroupIdentity {
        &self.identity
    }

    pub(in crate::cas_projection) fn witness(&self) -> &PendingProjectionWitness {
        &self.witness
    }

    pub(in crate::cas_projection) fn candidates(&self) -> &[PendingProjectionLeaseOwner] {
        &self.candidates
    }

    pub(in crate::cas_projection) fn candidates_mut(
        &mut self,
    ) -> &mut [PendingProjectionLeaseOwner] {
        &mut self.candidates
    }

    pub(in crate::cas_projection) fn into_parts(
        self,
    ) -> (
        PendingProjectionGroupIdentity,
        PendingProjectionWitness,
        Vec<PendingProjectionLeaseOwner>,
    ) {
        (self.identity, self.witness, self.candidates)
    }
}

impl PendingProjectionGroupIdentity {
    #[cfg(test)]
    pub(in crate::cas_projection) fn corrupt_connection_identity_for_test(&mut self) {
        self.connection.corrupt_identity_for_test();
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn replace_cas_thread_id_for_test(
        &mut self,
        cas_thread_id: CasThreadId,
    ) {
        self.key.cas_thread_id = cas_thread_id;
    }

    pub(in crate::cas_projection) fn connection(&self) -> &StableProjectionConnectionObservation {
        &self.connection
    }

    pub(in crate::cas_projection) fn key(&self) -> &LoadedThreadKey {
        &self.key
    }

    pub(in crate::cas_projection) fn owner(&self) -> &SyndicThreadId {
        &self.owner
    }

    pub(in crate::cas_projection) fn loaded_generation(&self) -> &CasLoadedSessionGeneration {
        &self.loaded_generation
    }
}

impl PendingProjectionWitness {
    #[cfg(test)]
    pub(in crate::cas_projection) fn replace_home_id_for_test(&mut self, home_id: BerylHomeId) {
        self.home_id = home_id;
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn replace_home_generation_for_test(
        &mut self,
        home_generation: HomeGeneration,
    ) {
        self.home_generation = home_generation;
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn replace_syndic_thread_id_for_test(
        &mut self,
        syndic_thread_id: SyndicThreadId,
    ) {
        self.syndic_thread_id = syndic_thread_id;
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn replace_loaded_session_generation_for_test(
        &mut self,
        loaded_session_generation: CasLoadedSessionGeneration,
    ) {
        self.loaded_session_generation = loaded_session_generation;
    }

    pub(in crate::cas_projection) fn home_id(&self) -> &BerylHomeId {
        &self.home_id
    }

    pub(in crate::cas_projection) fn home_generation(&self) -> &HomeGeneration {
        &self.home_generation
    }

    pub(in crate::cas_projection) fn syndic_thread_id(&self) -> &SyndicThreadId {
        &self.syndic_thread_id
    }

    pub(in crate::cas_projection) fn binding_revision(&self) -> &BindingRevision {
        &self.binding_revision
    }

    pub(in crate::cas_projection) fn execution_binding(&self) -> &ExecutionBinding {
        &self.execution_binding
    }

    pub(in crate::cas_projection) fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }

    pub(in crate::cas_projection) fn loaded_session_generation(
        &self,
    ) -> &CasLoadedSessionGeneration {
        &self.loaded_session_generation
    }

    pub(in crate::cas_projection) fn lineage_proof(&self) -> &CasLineageProof {
        &self.lineage_proof
    }
}

impl PersistentFailurePendingProjectionQuarantineMetadata {
    #[must_use]
    pub const fn group_count(self) -> usize {
        self.group_count
    }

    #[must_use]
    pub const fn candidate_count(self) -> usize {
        self.candidate_count
    }

    #[must_use]
    pub const fn retained_connection_count(self) -> usize {
        self.retained_connection_count
    }

    #[must_use]
    pub const fn local_disposition_count(self) -> usize {
        self.local_disposition_count
    }

    #[must_use]
    pub const fn late_publication_count(self) -> usize {
        self.late_publication_count
    }

    #[must_use]
    pub const fn is_promotable(self) -> bool {
        self.promotable
    }
}

impl PersistentFailurePendingProjectionQuarantine {
    pub(in crate::cas_projection) fn into_inventory(self) -> PersistentFailureRecoveryInventory {
        self.inventory
    }

    #[must_use]
    pub fn home_id(&self) -> BerylHomeId {
        self.inventory.home_id()
    }

    #[must_use]
    pub fn home_generation(&self) -> HomeGeneration {
        self.inventory.home_generation()
    }

    #[must_use]
    pub fn service_generation(&self) -> ProjectionServiceGeneration {
        self.inventory.service_generation()
    }

    #[must_use]
    pub fn failure_generation(&self) -> PersistentFailureGeneration {
        self.inventory.failure_generation()
    }

    #[must_use]
    pub fn metadata(&self) -> PersistentFailurePendingProjectionQuarantineMetadata {
        self.inventory.pending_quarantine_metadata()
    }
}

impl PersistentFailurePendingProjectionQuarantineError {
    #[must_use]
    pub const fn reason(&self) -> PersistentFailurePendingProjectionQuarantineReason {
        self.reason
    }

    #[must_use]
    pub fn inventory(&self) -> &PersistentFailureRecoveryInventory {
        &self.inventory
    }

    /// Returns bounded content-free metadata for the inert authority retained by this error.
    #[must_use]
    pub fn metadata(&self) -> PersistentFailurePendingProjectionQuarantineMetadata {
        self.inventory.pending_quarantine_metadata()
    }

    #[must_use]
    pub fn into_inventory(self) -> PersistentFailureRecoveryInventory {
        self.inventory
    }
}

impl fmt::Debug for PersistentFailurePendingProjectionQuarantine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PersistentFailurePendingProjectionQuarantine")
            .field("home_id", &self.home_id())
            .field("home_generation", &self.home_generation())
            .field("service_generation", &self.service_generation())
            .field("failure_generation", &self.failure_generation())
            .field("metadata", &self.metadata())
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for PersistentFailurePendingProjectionQuarantineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PersistentFailurePendingProjectionQuarantineError")
            .field("reason", &self.reason)
            .field("inventory", &self.inventory)
            .finish()
    }
}

impl fmt::Display for PersistentFailurePendingProjectionQuarantineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "the persistent-failure inventory could not form an exact pending-projection quarantine: {:?}",
            self.reason
        )
    }
}

impl std::error::Error for PersistentFailurePendingProjectionQuarantineError {}
