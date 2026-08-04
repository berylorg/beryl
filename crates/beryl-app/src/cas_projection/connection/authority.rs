use super::lifecycle::ProjectionConnectionIdentityObservation;
use super::*;

mod candidate_set;

pub(in crate::cas_projection) use candidate_set::{
    CandidateSetConnectionOwnerSealFailure, CandidateSetConvergedProjectionConnectionOwner,
    CandidateSetRecoveryPublicationBarrier, CandidateSetRecoveryPublicationFailure,
    seal_pending_projection_connection_owners,
};

#[derive(Debug)]
pub(in crate::cas_projection) struct ConnectionRegistryAuthority {
    pub(super) generation: ConnectionGeneration,
    runtime_id: RuntimeId,
    process_generation: CasProcessGeneration,
    retired: AtomicBool,
    gate: Mutex<ConnectionAuthorityState>,
    retirement_changed: std::sync::Condvar,
}

#[derive(Debug)]
pub(in crate::cas_projection) struct ConnectionAuthorityState {
    session_owner_live: bool,
    scheduled_promotion: Option<ScheduledPromotionAuthority>,
    cleanup_owners: std::collections::HashMap<CleanupAuthorityId, CleanupAuthorityState>,
    pending_projection_quarantine: Option<PendingProjectionQuarantineAuthorityState>,
    next_promotion_id: u64,
    next_cleanup_id: u64,
    next_pending_projection_quarantine_id: u64,
    retirement_complete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PromotionAuthorityId(u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct CleanupAuthorityId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingProjectionQuarantineAuthorityId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExactAuthorityState {
    Live,
    FailureRetained(crate::cas_projection::persistent_failure::PersistentFailureCutIdentity),
}

type CleanupAuthorityState = ExactAuthorityState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScheduledPromotionAuthority {
    id: PromotionAuthorityId,
    state: ExactAuthorityState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingProjectionQuarantineAuthorityState {
    id: PendingProjectionQuarantineAuthorityId,
    identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    stage: PendingProjectionQuarantineStage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingProjectionQuarantineStage {
    Reauthentication,
    CandidateSetConverged,
}

type PromotionFailurePublisher =
    Box<dyn FnOnce(FailureRetainedPromotionReservation) + Send + 'static>;
type CleanupFailurePublisher = Box<dyn FnOnce(FailureRetainedCleanupOwner) + Send + 'static>;

pub(in crate::cas_projection) struct PromotionFailureTransfer {
    identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    publish: Option<PromotionFailurePublisher>,
}

pub(in crate::cas_projection) struct CleanupFailureTransfer {
    identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    publish: Option<CleanupFailurePublisher>,
}

impl PromotionFailureTransfer {
    pub(in crate::cas_projection) fn new(
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
        publish: impl FnOnce(FailureRetainedPromotionReservation) + Send + 'static,
    ) -> Self {
        Self {
            identity,
            publish: Some(Box::new(publish)),
        }
    }
}

impl CleanupFailureTransfer {
    pub(in crate::cas_projection) fn new(
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
        publish: impl FnOnce(FailureRetainedCleanupOwner) + Send + 'static,
    ) -> Self {
        Self {
            identity,
            publish: Some(Box::new(publish)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum ConnectionRetirementOutcome {
    Complete,
    FailureRetained(crate::cas_projection::persistent_failure::PersistentFailureCutIdentity),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum ConnectionPromotionReleaseOutcome {
    Ordinary,
    PersistentFailure,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExactSettlementOutcome {
    Ordinary { should_detach: bool },
    PersistentFailure,
    Closed,
}

/// One-shot authority that prevents connection retirement from overtaking durable promotion.
pub(in crate::cas_projection) struct ConnectionPromotionReservation {
    authority: Arc<ConnectionRegistryAuthority>,
    connection: Arc<ProjectionConnection>,
    id: PromotionAuthorityId,
    command: Option<crate::cas_projection::persistent_failure::LiveCommandPermit>,
    failure_transfer: Option<PromotionFailureTransfer>,
    active: bool,
}

pub(in crate::cas_projection) struct ConnectionCleanupOwner {
    authority: Arc<ConnectionRegistryAuthority>,
    connection: Arc<ProjectionConnection>,
    id: CleanupAuthorityId,
    command: Option<crate::cas_projection::persistent_failure::LiveCommandPermit>,
    failure_transfer: Option<CleanupFailureTransfer>,
    active: bool,
}

pub(in crate::cas_projection) struct FailureRetainedPromotionReservation {
    authority: Arc<ConnectionRegistryAuthority>,
    connection: Arc<ProjectionConnection>,
    id: PromotionAuthorityId,
    identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
}

pub(in crate::cas_projection) struct FailureRetainedCleanupOwner {
    authority: Arc<ConnectionRegistryAuthority>,
    connection: Arc<ProjectionConnection>,
    id: CleanupAuthorityId,
    identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
}

/// Non-cloneable connection authority retained by one pending-projection quarantine.
///
/// This replaces the exact failure-retained promotion and cleanup barriers under the connection
/// gate. While it remains installed, connection retirement cannot invalidate loaded-registry
/// authority owned by the quarantine.
#[must_use = "the owner prevents connection retirement from overtaking quarantine ownership"]
pub(in crate::cas_projection) struct PendingProjectionConnectionOwner {
    authority: Arc<ConnectionRegistryAuthority>,
    connection: Arc<ProjectionConnection>,
    id: PendingProjectionQuarantineAuthorityId,
    identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    active: bool,
    #[cfg(test)]
    force_candidate_set_topology_mismatch: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum PendingProjectionConnectionOwnerInstallFailure {
    AuthorityUnavailable,
    TopologyMismatch,
}

pub(in crate::cas_projection) struct PendingProjectionConnectionOwnerInstallError {
    failure: PendingProjectionConnectionOwnerInstallFailure,
    promotions: Vec<FailureRetainedPromotionReservation>,
    cleanup: Vec<FailureRetainedCleanupOwner>,
}

#[cfg(test)]
struct RetirementGateAttemptHook {
    connection_generation: u64,
    reached: std::sync::mpsc::SyncSender<()>,
}

#[cfg(test)]
pub(in crate::cas_projection) struct RetirementGateAttemptObservation {
    reached: std::sync::mpsc::Receiver<()>,
}

#[cfg(test)]
static RETIREMENT_GATE_ATTEMPT_HOOK: std::sync::OnceLock<Mutex<Option<RetirementGateAttemptHook>>> =
    std::sync::OnceLock::new();

/// Read-only proof that one private connection barrier remains retained by an exact failure cut.
///
/// The witness carries no barrier id and cannot release, consume, or otherwise authorize work.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::cas_projection) struct FailureRetainedConnectionOwnerWitness {
    cut_identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    connection: ProjectionConnectionIdentityObservation,
}

/// Read-only failure while proving one connection's complete retained barrier topology.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum FailureRetainedBarrierTopologyError {
    AuthorityPoisoned,
    InvalidExpectedPromotionCount,
    LiveAuthority,
    CutMismatch,
    CountMismatch,
}

impl FailureRetainedConnectionOwnerWitness {
    pub(in crate::cas_projection) const fn cut_identity(
        self,
    ) -> crate::cas_projection::persistent_failure::PersistentFailureCutIdentity {
        self.cut_identity
    }

    pub(in crate::cas_projection) const fn connection(
        self,
    ) -> ProjectionConnectionIdentityObservation {
        self.connection
    }
}

impl std::fmt::Debug for ConnectionCleanupOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConnectionCleanupOwner")
            .field("connection_generation", &self.authority.generation)
            .field("active", &self.active)
            .finish_non_exhaustive()
    }
}

impl Default for ConnectionAuthorityState {
    fn default() -> Self {
        Self {
            session_owner_live: true,
            scheduled_promotion: None,
            cleanup_owners: std::collections::HashMap::new(),
            pending_projection_quarantine: None,
            next_promotion_id: 1,
            next_cleanup_id: 1,
            next_pending_projection_quarantine_id: 1,
            retirement_complete: false,
        }
    }
}

impl ConnectionRegistryAuthority {
    pub(in crate::cas_projection) fn new(
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
    ) -> Result<Self, ProjectionCoordinatorError> {
        Ok(Self {
            generation: allocate_connection_generation()?,
            runtime_id,
            process_generation,
            retired: AtomicBool::new(false),
            gate: Mutex::new(ConnectionAuthorityState::default()),
            retirement_changed: std::sync::Condvar::new(),
        })
    }

    pub(in crate::cas_projection) fn is_retired(&self) -> bool {
        self.retired.load(Ordering::Acquire)
    }

    pub(super) fn retirement_complete(&self) -> bool {
        self.gate
            .lock()
            .map(|state| state.retirement_complete)
            .unwrap_or(false)
    }

    pub(super) fn validate_failure_retained_barrier_topology(
        &self,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
        expected_promotion_count: usize,
        expected_cleanup_count: usize,
    ) -> Result<(), FailureRetainedBarrierTopologyError> {
        if expected_promotion_count > 1 {
            return Err(FailureRetainedBarrierTopologyError::InvalidExpectedPromotionCount);
        }
        let state = self
            .gate
            .lock()
            .map_err(|_| FailureRetainedBarrierTopologyError::AuthorityPoisoned)?;
        if let Some(promotion) = state.scheduled_promotion {
            match promotion.state {
                ExactAuthorityState::Live => {
                    return Err(FailureRetainedBarrierTopologyError::LiveAuthority);
                }
                ExactAuthorityState::FailureRetained(retained) if retained != identity => {
                    return Err(FailureRetainedBarrierTopologyError::CutMismatch);
                }
                ExactAuthorityState::FailureRetained(_) => {}
            }
        }
        for cleanup in state.cleanup_owners.values() {
            match cleanup {
                ExactAuthorityState::Live => {
                    return Err(FailureRetainedBarrierTopologyError::LiveAuthority);
                }
                ExactAuthorityState::FailureRetained(retained) if *retained != identity => {
                    return Err(FailureRetainedBarrierTopologyError::CutMismatch);
                }
                ExactAuthorityState::FailureRetained(_) => {}
            }
        }
        if usize::from(state.scheduled_promotion.is_some()) != expected_promotion_count
            || state.cleanup_owners.len() != expected_cleanup_count
        {
            return Err(FailureRetainedBarrierTopologyError::CountMismatch);
        }
        Ok(())
    }

    pub(super) fn install_pending_projection_quarantine_owner(
        self: &Arc<Self>,
        connection: &Arc<ProjectionConnection>,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
        promotions: Vec<FailureRetainedPromotionReservation>,
        cleanup: Vec<FailureRetainedCleanupOwner>,
    ) -> Result<PendingProjectionConnectionOwner, PendingProjectionConnectionOwnerInstallError>
    {
        let owner_topology_matches = promotions.iter().all(|owner| {
            owner.identity == identity
                && Arc::ptr_eq(&owner.authority, self)
                && Arc::ptr_eq(&owner.connection, connection)
        }) && cleanup.iter().all(|owner| {
            owner.identity == identity
                && Arc::ptr_eq(&owner.authority, self)
                && Arc::ptr_eq(&owner.connection, connection)
        });
        if !owner_topology_matches || !Arc::ptr_eq(self, &connection.authority) {
            return Err(PendingProjectionConnectionOwnerInstallError::new(
                PendingProjectionConnectionOwnerInstallFailure::TopologyMismatch,
                promotions,
                cleanup,
            ));
        }

        let mut state = match self.gate.lock() {
            Ok(state) => state,
            Err(_) => {
                return Err(PendingProjectionConnectionOwnerInstallError::new(
                    PendingProjectionConnectionOwnerInstallFailure::AuthorityUnavailable,
                    promotions,
                    cleanup,
                ));
            }
        };
        if self.is_retired()
            || state.retirement_complete
            || state.pending_projection_quarantine.is_some()
        {
            return Err(PendingProjectionConnectionOwnerInstallError::new(
                PendingProjectionConnectionOwnerInstallFailure::AuthorityUnavailable,
                promotions,
                cleanup,
            ));
        }

        let promotion_matches = match (promotions.as_slice(), state.scheduled_promotion) {
            ([], None) => true,
            (
                [owner],
                Some(ScheduledPromotionAuthority {
                    id,
                    state: ExactAuthorityState::FailureRetained(retained_identity),
                }),
            ) => owner.id == id && retained_identity == identity,
            _ => false,
        };
        let cleanup_ids = cleanup
            .iter()
            .map(|owner| owner.id)
            .collect::<std::collections::HashSet<_>>();
        let cleanup_matches = cleanup_ids.len() == cleanup.len()
            && cleanup_ids.len() == state.cleanup_owners.len()
            && cleanup_ids.iter().all(|id| {
                state.cleanup_owners.get(id)
                    == Some(&ExactAuthorityState::FailureRetained(identity))
            });
        if !promotion_matches || !cleanup_matches {
            return Err(PendingProjectionConnectionOwnerInstallError::new(
                PendingProjectionConnectionOwnerInstallFailure::TopologyMismatch,
                promotions,
                cleanup,
            ));
        }

        let id =
            PendingProjectionQuarantineAuthorityId(state.next_pending_projection_quarantine_id);
        state.next_pending_projection_quarantine_id = state
            .next_pending_projection_quarantine_id
            .checked_add(1)
            .expect("connection quarantine authority IDs cannot exhaust during one process");
        state.scheduled_promotion = None;
        state.cleanup_owners.clear();
        state.pending_projection_quarantine = Some(PendingProjectionQuarantineAuthorityState {
            id,
            identity,
            stage: PendingProjectionQuarantineStage::Reauthentication,
        });
        self.retirement_changed.notify_all();
        Ok(PendingProjectionConnectionOwner {
            authority: Arc::clone(self),
            connection: Arc::clone(connection),
            id,
            identity,
            active: true,
            #[cfg(test)]
            force_candidate_set_topology_mismatch: false,
        })
    }

    pub(in crate::cas_projection) fn release_session_owner(
        &self,
        elect_ordinary_retirement: impl FnOnce() -> bool,
    ) -> Result<bool, ProjectionCoordinatorError> {
        let mut state = self.lock()?;
        state.session_owner_live = false;
        self.elect_detachment_locked(&mut state, elect_ordinary_retirement)
    }

    pub(in crate::cas_projection) fn mark_session_owner_released(&self) {
        let mut state = match self.gate.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.session_owner_live = false;
    }

    pub(super) fn acquire_cleanup_owner(
        self: &Arc<Self>,
        connection: &Arc<ProjectionConnection>,
        command: crate::cas_projection::persistent_failure::LiveCommandPermit,
        failure_transfer: CleanupFailureTransfer,
    ) -> Result<Option<ConnectionCleanupOwner>, ProjectionCoordinatorError> {
        let mut state = self.lock()?;
        if self.is_retired() {
            return Ok(None);
        }
        let id = CleanupAuthorityId(state.next_cleanup_id);
        state.next_cleanup_id = state
            .next_cleanup_id
            .checked_add(1)
            .expect("connection cleanup authority IDs cannot exhaust during one process");
        let replaced = state.cleanup_owners.insert(id, ExactAuthorityState::Live);
        debug_assert!(replaced.is_none());
        Ok(Some(ConnectionCleanupOwner {
            authority: Arc::clone(self),
            connection: Arc::clone(connection),
            id,
            command: Some(command),
            failure_transfer: Some(failure_transfer),
            active: true,
        }))
    }

    fn elect_detachment_locked(
        &self,
        state: &mut ConnectionAuthorityState,
        elect_ordinary_retirement: impl FnOnce() -> bool,
    ) -> Result<bool, ProjectionCoordinatorError> {
        if state.session_owner_live
            || state.scheduled_promotion.is_some()
            || !state.cleanup_owners.is_empty()
            || state.pending_projection_quarantine.is_some()
        {
            return Ok(false);
        }
        if self.is_retired() {
            self.complete_retirement_locked(state);
            return Ok(true);
        }
        if registry::connection_has_authority(self.generation)? || !elect_ordinary_retirement() {
            return Ok(false);
        }
        self.retire_locked(state);
        Ok(true)
    }

    pub(super) fn reserve_scheduled_promotion(
        self: &Arc<Self>,
        connection: &Arc<ProjectionConnection>,
        command: crate::cas_projection::persistent_failure::LiveCommandPermit,
        failure_transfer: PromotionFailureTransfer,
    ) -> Result<Option<ConnectionPromotionReservation>, ProjectionCoordinatorError> {
        let mut state = self.lock()?;
        if self.is_retired() || state.scheduled_promotion.is_some() {
            return Ok(None);
        }
        let id = PromotionAuthorityId(state.next_promotion_id);
        state.next_promotion_id = state
            .next_promotion_id
            .checked_add(1)
            .expect("connection promotion authority IDs cannot exhaust during one process");
        state.scheduled_promotion = Some(ScheduledPromotionAuthority {
            id,
            state: ExactAuthorityState::Live,
        });
        Ok(Some(ConnectionPromotionReservation {
            authority: Arc::clone(self),
            connection: Arc::clone(connection),
            id,
            command: Some(command),
            failure_transfer: Some(failure_transfer),
            active: true,
        }))
    }

    pub(super) fn register_new(
        &self,
        key: LoadedThreadKey,
        owner: SyndicThreadId,
        command: &crate::cas_projection::persistent_failure::LiveCommandPermit,
        seed: &mut RawLoadedLeaseSeed,
    ) -> Result<Option<()>, ProjectionCoordinatorError> {
        let _gate = self.lock()?;
        if self.is_retired() {
            return Ok(None);
        }
        match command.commit_if_current(|| {
            let (generation, token) = registry::register_new(key, self.generation, owner)?;
            seed.arm(generation, token);
            Ok(())
        }) {
            Ok(result) => result.map(Some),
            Err(_) => Ok(None),
        }
    }

    pub(super) fn acquire_existing(
        &self,
        key: &LoadedThreadKey,
        owner: SyndicThreadId,
        command: &crate::cas_projection::persistent_failure::LiveCommandPermit,
        seed: &mut RawLoadedLeaseSeed,
    ) -> Result<Option<ExistingSubscription>, ProjectionCoordinatorError> {
        let _gate = self.lock()?;
        if self.is_retired() {
            return Ok(None);
        }
        match command.commit_if_current(|| {
            let subscription = registry::acquire_existing(key, self.generation, owner)?;
            if let ExistingSubscription::Exact { generation, token } = subscription {
                seed.arm(generation, token);
            }
            Ok(subscription)
        }) {
            Ok(result) => result.map(Some),
            Err(_) => Ok(None),
        }
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn register_new_for_test(
        &self,
        key: LoadedThreadKey,
        owner: SyndicThreadId,
    ) -> Result<Option<(CasLoadedSessionGeneration, LeaseToken)>, ProjectionCoordinatorError> {
        let _gate = self.lock()?;
        if self.is_retired() {
            return Ok(None);
        }
        registry::register_new(key, self.generation, owner).map(Some)
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn acquire_existing_for_test(
        &self,
        key: &LoadedThreadKey,
        owner: SyndicThreadId,
    ) -> Result<Option<ExistingSubscription>, ProjectionCoordinatorError> {
        let _gate = self.lock()?;
        if self.is_retired() {
            return Ok(None);
        }
        registry::acquire_existing(key, self.generation, owner).map(Some)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn quarantine_exact(
        &self,
        key: &LoadedThreadKey,
        owner: SyndicThreadId,
        generation: CasLoadedSessionGeneration,
        token: LeaseToken,
        command: &crate::cas_projection::persistent_failure::LiveCommandPermit,
        seed: &mut RawQuarantinedAnchorSeed,
    ) -> Result<Option<()>, ProjectionCoordinatorError> {
        let _gate = self.lock()?;
        if self.is_retired() {
            return Ok(None);
        }
        match command.commit_if_current(|| {
            let anchor =
                registry::quarantine_exact(key, self.generation, owner, generation, token)?;
            let Some(anchor) = anchor else {
                return Ok(None);
            };
            seed.arm_quarantined(anchor);
            Ok(Some(()))
        }) {
            Ok(result) => result,
            Err(_) => Ok(None),
        }
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn quarantine_exact_for_test(
        &self,
        key: &LoadedThreadKey,
        owner: SyndicThreadId,
        generation: CasLoadedSessionGeneration,
        token: LeaseToken,
    ) -> Result<Option<ReacquisitionAnchorToken>, ProjectionCoordinatorError> {
        let _gate = self.lock()?;
        if self.is_retired() {
            return Ok(None);
        }
        registry::quarantine_exact(key, self.generation, owner, generation, token)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn reserve_reacquisition(
        old: &Self,
        old_router: &EventRouter,
        old_command: &crate::cas_projection::persistent_failure::LiveCommandPermit,
        new: &Self,
        new_router: &EventRouter,
        new_command: &crate::cas_projection::persistent_failure::LiveCommandPermit,
        key: &LoadedThreadKey,
        owner: SyndicThreadId,
        generation: CasLoadedSessionGeneration,
        anchor: ReacquisitionAnchorToken,
        seed: &mut RawReacquisitionReservationSeed,
    ) -> Result<Option<()>, ProjectionCoordinatorError> {
        Self::with_live_pair(old, new, |old_state, new_state| {
            if !Self::routers_admit_reacquisition(
                old, old_state, old_router, new, new_state, new_router, key,
            )? {
                return Ok(None);
            }
            match old_command.commit_pair_if_current(new_command, || {
                let reservation = registry::reserve_reacquisition(
                    key,
                    old.generation,
                    new.generation,
                    owner,
                    generation,
                    anchor,
                )?;
                let Some(reservation) = reservation else {
                    return Ok(None);
                };
                seed.arm(reservation);
                Ok(Some(()))
            }) {
                Ok(result) => result,
                Err(_) => Ok(None),
            }
        })
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection) fn reserve_reacquisition_for_test(
        old: &Self,
        old_router: &EventRouter,
        old_command: &crate::cas_projection::persistent_failure::LiveCommandPermit,
        new: &Self,
        new_router: &EventRouter,
        new_command: &crate::cas_projection::persistent_failure::LiveCommandPermit,
        key: &LoadedThreadKey,
        owner: SyndicThreadId,
        generation: CasLoadedSessionGeneration,
        anchor: ReacquisitionAnchorToken,
    ) -> Result<Option<ReacquisitionReservationToken>, ProjectionCoordinatorError> {
        Self::with_live_pair(old, new, |old_state, new_state| {
            if !Self::routers_admit_reacquisition(
                old, old_state, old_router, new, new_state, new_router, key,
            )? {
                return Ok(None);
            }
            match old_command.commit_pair_if_current(new_command, || {
                registry::reserve_reacquisition(
                    key,
                    old.generation,
                    new.generation,
                    owner,
                    generation,
                    anchor,
                )
            }) {
                Ok(result) => result,
                Err(_) => Ok(None),
            }
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn transfer_quarantined(
        old: &Self,
        old_router: &EventRouter,
        old_command: &crate::cas_projection::persistent_failure::LiveCommandPermit,
        new: &Self,
        new_router: &EventRouter,
        new_command: &crate::cas_projection::persistent_failure::LiveCommandPermit,
        key: &LoadedThreadKey,
        owner: SyndicThreadId,
        generation: CasLoadedSessionGeneration,
        anchor: ReacquisitionAnchorToken,
        reservation: ReacquisitionReservationToken,
        seed: &mut RawLoadedLeaseSeed,
        postcommit: impl FnOnce(&mut RawLoadedLeaseSeed),
    ) -> Result<Option<()>, ProjectionCoordinatorError> {
        Self::with_live_pair(old, new, |old_state, new_state| {
            if !Self::routers_admit_reacquisition(
                old, old_state, old_router, new, new_state, new_router, key,
            )? {
                return Ok(None);
            }
            match old_command.commit_pair_if_current(new_command, || {
                let transferred = registry::transfer_quarantined(
                    key,
                    old.generation,
                    new.generation,
                    owner,
                    generation,
                    anchor,
                    reservation,
                )?;
                let Some((generation, token)) = transferred else {
                    return Ok(None);
                };
                seed.arm(generation, token);
                postcommit(seed);
                Ok(Some(()))
            }) {
                Ok(result) => result,
                Err(_) => Ok(None),
            }
        })
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection) fn transfer_quarantined_for_test(
        old: &Self,
        old_router: &EventRouter,
        old_command: &crate::cas_projection::persistent_failure::LiveCommandPermit,
        new: &Self,
        new_router: &EventRouter,
        new_command: &crate::cas_projection::persistent_failure::LiveCommandPermit,
        key: &LoadedThreadKey,
        owner: SyndicThreadId,
        generation: CasLoadedSessionGeneration,
        anchor: ReacquisitionAnchorToken,
        reservation: ReacquisitionReservationToken,
    ) -> Result<Option<(CasLoadedSessionGeneration, LeaseToken)>, ProjectionCoordinatorError> {
        Self::with_live_pair(old, new, |old_state, new_state| {
            if !Self::routers_admit_reacquisition(
                old, old_state, old_router, new, new_state, new_router, key,
            )? {
                return Ok(None);
            }
            match old_command.commit_pair_if_current(new_command, || {
                registry::transfer_quarantined(
                    key,
                    old.generation,
                    new.generation,
                    owner,
                    generation,
                    anchor,
                    reservation,
                )
            }) {
                Ok(result) => result,
                Err(_) => Ok(None),
            }
        })
    }

    fn with_live_pair<T>(
        first: &Self,
        second: &Self,
        operation: impl FnOnce(
            &mut ConnectionAuthorityState,
            &mut ConnectionAuthorityState,
        ) -> Result<Option<T>, ProjectionCoordinatorError>,
    ) -> Result<Option<T>, ProjectionCoordinatorError> {
        if first.generation == second.generation {
            return Ok(None);
        }
        if first.generation.get() < second.generation.get() {
            let mut first_gate = first.lock()?;
            let mut second_gate = second.lock()?;
            if first.is_retired() || second.is_retired() {
                return Ok(None);
            }
            return operation(&mut first_gate, &mut second_gate);
        }
        let mut second_gate = second.lock()?;
        let mut first_gate = first.lock()?;
        if first.is_retired() || second.is_retired() {
            return Ok(None);
        }
        operation(&mut first_gate, &mut second_gate)
    }

    fn routers_admit_reacquisition(
        old: &Self,
        old_state: &mut ConnectionAuthorityState,
        old_router: &EventRouter,
        new: &Self,
        new_state: &mut ConnectionAuthorityState,
        new_router: &EventRouter,
        key: &LoadedThreadKey,
    ) -> Result<bool, ProjectionCoordinatorError> {
        match old_router.permits_reacquisition_thread(&key.cas_thread_id) {
            Ok(true) => {}
            Ok(false) => {
                old.invalidate_thread_locked(old_state, key)?;
                return Ok(false);
            }
            Err(error) => {
                old.retire_locked(old_state);
                return Err(error);
            }
        }
        match new_router.permits_reacquisition_thread(&key.cas_thread_id) {
            Ok(true) => Ok(true),
            Ok(false) => {
                new.invalidate_thread_locked(new_state, key)?;
                Ok(false)
            }
            Err(error) => {
                new.retire_locked(new_state);
                Err(error)
            }
        }
    }

    fn invalidate_thread_locked(
        &self,
        state: &mut ConnectionAuthorityState,
        key: &LoadedThreadKey,
    ) -> Result<bool, ProjectionCoordinatorError> {
        registry::invalidate_connection_thread(key, self.generation).inspect_err(|_| {
            self.retire_locked(state);
        })
    }

    /// Revokes one remote thread only on this exact connection generation.
    ///
    /// The caller records the router-lane fence first and releases that lock before entering this
    /// connection gate. Registry invalidation then shares the same serialization used by
    /// retirement, replacement reservation, and native transfer.
    pub(in crate::cas_projection) fn record_thread_closed(
        &self,
        cas_thread_id: &CasThreadId,
    ) -> Result<bool, ProjectionCoordinatorError> {
        let mut state = self.lock()?;
        let key = LoadedThreadKey {
            runtime_id: self.runtime_id,
            process_generation: self.process_generation,
            cas_thread_id: cas_thread_id.clone(),
        };
        self.invalidate_thread_locked(&mut state, &key)
    }

    pub(in crate::cas_projection) fn retire(
        &self,
    ) -> Result<ConnectionRetirementOutcome, ProjectionCoordinatorError> {
        #[cfg(test)]
        observe_retirement_gate_attempt(self.generation.get());
        let (mut state, poisoned) = match self.gate.lock() {
            Ok(gate) => (gate, false),
            Err(poisoned) => (poisoned.into_inner(), true),
        };
        self.retire_locked(&mut state);
        if poisoned {
            return Err(Self::authority_state_error());
        }
        while !state.retirement_complete {
            if let Some(identity) = Self::failure_retained_identity(&state)? {
                return Ok(ConnectionRetirementOutcome::FailureRetained(identity));
            }
            state = match self.retirement_changed.wait(state) {
                Ok(state) => state,
                Err(_) => return Err(Self::authority_state_error()),
            };
            self.complete_retirement_locked(&mut state);
        }
        Ok(ConnectionRetirementOutcome::Complete)
    }

    fn settle_cleanup_owner(
        self: &Arc<Self>,
        id: CleanupAuthorityId,
        connection: &Arc<ProjectionConnection>,
        command: &crate::cas_projection::persistent_failure::LiveCommandPermit,
        failure_transfer: CleanupFailureTransfer,
    ) -> Result<ExactSettlementOutcome, ProjectionCoordinatorError> {
        let state = self.lock()?;
        let state = std::cell::RefCell::new(state);
        let failure_transfer = std::cell::RefCell::new(Some(failure_transfer));
        let result = command.commit_or_transfer(
            || {
                let mut state = state.borrow_mut();
                if state.cleanup_owners.get(&id) != Some(&ExactAuthorityState::Live) {
                    return Err(Self::authority_state_error());
                }
                state.cleanup_owners.remove(&id);
                self.retirement_changed.notify_all();
                let should_detach = self.elect_detachment_locked(&mut state, || {
                    connection.begin_ordinary_retirement_under_gate()
                })?;
                Ok(ExactSettlementOutcome::Ordinary { should_detach })
            },
            |failure_generation| {
                let mut transfer = failure_transfer
                    .borrow_mut()
                    .take()
                    .expect("one cleanup settlement consumes one failure transfer");
                let mut identity = transfer.identity;
                identity.failure_generation = failure_generation;
                if identity.service_generation != command.service_generation() {
                    return Err(Self::authority_state_error());
                }
                let mut state = state.borrow_mut();
                let Some(authority) = state.cleanup_owners.get_mut(&id) else {
                    return Err(Self::authority_state_error());
                };
                if *authority != ExactAuthorityState::Live {
                    return Err(Self::authority_state_error());
                }
                *authority = ExactAuthorityState::FailureRetained(identity);
                let retained = FailureRetainedCleanupOwner {
                    authority: Arc::clone(self),
                    connection: Arc::clone(connection),
                    id,
                    identity,
                };
                transfer
                    .publish
                    .take()
                    .expect("live cleanup failure transfer retains its publisher")(
                    retained
                );
                self.retirement_changed.notify_all();
                Ok(ExactSettlementOutcome::PersistentFailure)
            },
            || {
                let mut state = state.borrow_mut();
                if state.cleanup_owners.get(&id) != Some(&ExactAuthorityState::Live) {
                    return Err(Self::authority_state_error());
                }
                state.cleanup_owners.remove(&id);
                if self.is_retired() {
                    self.complete_retirement_locked(&mut state);
                }
                self.retirement_changed.notify_all();
                Ok(ExactSettlementOutcome::Closed)
            },
        );
        match result {
            Ok(Ok(outcome)) => Ok(outcome),
            Ok(Err(error)) => {
                let mut state = state.borrow_mut();
                if state.cleanup_owners.get(&id) == Some(&ExactAuthorityState::Live) {
                    state.cleanup_owners.remove(&id);
                }
                self.retire_locked(&mut state);
                Err(error)
            }
            Err(_) => {
                let mut state = state.borrow_mut();
                if state.cleanup_owners.get(&id) == Some(&ExactAuthorityState::Live) {
                    state.cleanup_owners.remove(&id);
                }
                self.retire_locked(&mut state);
                Err(Self::authority_state_error())
            }
        }
    }

    pub(super) fn retire_locked(&self, state: &mut ConnectionAuthorityState) {
        self.retired.store(true, Ordering::Release);
        self.complete_retirement_locked(state);
    }

    fn complete_retirement_locked(&self, state: &mut ConnectionAuthorityState) {
        if state.scheduled_promotion.is_some()
            || !state.cleanup_owners.is_empty()
            || state.pending_projection_quarantine.is_some()
            || state.retirement_complete
        {
            return;
        }
        let _ = registry::invalidate_connection(self.generation);
        state.retirement_complete = true;
        self.retirement_changed.notify_all();
    }

    fn failure_retained_identity(
        state: &ConnectionAuthorityState,
    ) -> Result<
        Option<crate::cas_projection::persistent_failure::PersistentFailureCutIdentity>,
        ProjectionCoordinatorError,
    > {
        let promotion_identity =
            state
                .scheduled_promotion
                .and_then(|authority| match authority.state {
                    ExactAuthorityState::FailureRetained(identity) => Some(identity),
                    ExactAuthorityState::Live => None,
                });
        let mut retained_identity = promotion_identity;
        for authority in state.cleanup_owners.values() {
            let ExactAuthorityState::FailureRetained(identity) = authority else {
                continue;
            };
            if retained_identity.is_some_and(|existing| existing != *identity) {
                return Err(Self::authority_state_error());
            }
            retained_identity = Some(*identity);
        }
        if let Some(quarantine) = state.pending_projection_quarantine {
            if retained_identity.is_some_and(|existing| existing != quarantine.identity) {
                return Err(Self::authority_state_error());
            }
            retained_identity = Some(quarantine.identity);
        }
        Ok(retained_identity)
    }

    fn release_pending_projection_quarantine_owner(
        &self,
        id: PendingProjectionQuarantineAuthorityId,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
        stage: PendingProjectionQuarantineStage,
    ) {
        let mut state = match self.gate.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        if state.pending_projection_quarantine
            != Some(PendingProjectionQuarantineAuthorityState {
                id,
                identity,
                stage,
            })
        {
            return;
        }
        state.pending_projection_quarantine = None;
        if self.is_retired() {
            self.complete_retirement_locked(&mut state);
        }
        self.retirement_changed.notify_all();
    }

    fn authority_state_error() -> ProjectionCoordinatorError {
        ProjectionCoordinatorError::RegistryPoisoned {
            registry: crate::cas_projection::ProjectionRegistryKind::ProjectionConnection,
        }
    }

    fn settle_scheduled_promotion(
        self: &Arc<Self>,
        id: PromotionAuthorityId,
        connection: &Arc<ProjectionConnection>,
        command: &crate::cas_projection::persistent_failure::LiveCommandPermit,
        failure_transfer: PromotionFailureTransfer,
    ) -> Result<ExactSettlementOutcome, ProjectionCoordinatorError> {
        let state = self.lock()?;
        let state = std::cell::RefCell::new(state);
        let failure_transfer = std::cell::RefCell::new(Some(failure_transfer));
        let result = command.commit_or_transfer(
            || {
                let mut state = state.borrow_mut();
                if !matches!(
                    state.scheduled_promotion,
                    Some(ScheduledPromotionAuthority {
                        id: active_id,
                        state: ExactAuthorityState::Live,
                    }) if active_id == id
                ) {
                    return Err(Self::authority_state_error());
                }
                state.scheduled_promotion = None;
                self.retirement_changed.notify_all();
                let should_detach = self.elect_detachment_locked(&mut state, || {
                    connection.begin_ordinary_retirement_under_gate()
                })?;
                Ok(ExactSettlementOutcome::Ordinary { should_detach })
            },
            |failure_generation| {
                let mut transfer = failure_transfer
                    .borrow_mut()
                    .take()
                    .expect("one promotion settlement consumes one failure transfer");
                let mut identity = transfer.identity;
                identity.failure_generation = failure_generation;
                if identity.service_generation != command.service_generation() {
                    return Err(Self::authority_state_error());
                }
                let mut state = state.borrow_mut();
                let Some(authority) = state.scheduled_promotion.as_mut() else {
                    return Err(Self::authority_state_error());
                };
                if authority.id != id || authority.state != ExactAuthorityState::Live {
                    return Err(Self::authority_state_error());
                }
                authority.state = ExactAuthorityState::FailureRetained(identity);
                let retained = FailureRetainedPromotionReservation {
                    authority: Arc::clone(self),
                    connection: Arc::clone(connection),
                    id,
                    identity,
                };
                transfer
                    .publish
                    .take()
                    .expect("live promotion failure transfer retains its publisher")(
                    retained
                );
                self.retirement_changed.notify_all();
                Ok(ExactSettlementOutcome::PersistentFailure)
            },
            || {
                let mut state = state.borrow_mut();
                if !matches!(
                    state.scheduled_promotion,
                    Some(ScheduledPromotionAuthority {
                        id: active_id,
                        state: ExactAuthorityState::Live,
                    }) if active_id == id
                ) {
                    return Err(Self::authority_state_error());
                }
                state.scheduled_promotion = None;
                if self.is_retired() {
                    self.complete_retirement_locked(&mut state);
                }
                self.retirement_changed.notify_all();
                Ok(ExactSettlementOutcome::Closed)
            },
        );
        match result {
            Ok(Ok(outcome)) => Ok(outcome),
            Ok(Err(error)) => {
                let mut state = state.borrow_mut();
                if matches!(
                    state.scheduled_promotion,
                    Some(ScheduledPromotionAuthority {
                        id: active_id,
                        state: ExactAuthorityState::Live,
                    }) if active_id == id
                ) {
                    state.scheduled_promotion = None;
                }
                self.retire_locked(&mut state);
                Err(error)
            }
            Err(_) => {
                let mut state = state.borrow_mut();
                if matches!(
                    state.scheduled_promotion,
                    Some(ScheduledPromotionAuthority {
                        id: active_id,
                        state: ExactAuthorityState::Live,
                    }) if active_id == id
                ) {
                    state.scheduled_promotion = None;
                }
                self.retire_locked(&mut state);
                Err(Self::authority_state_error())
            }
        }
    }

    fn consume_failure_retained_promotion(
        &self,
        id: PromotionAuthorityId,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    ) -> Result<bool, ProjectionCoordinatorError> {
        let mut state = self.lock()?;
        if !matches!(
            state.scheduled_promotion,
            Some(ScheduledPromotionAuthority {
                id: active_id,
                state: ExactAuthorityState::FailureRetained(active_identity),
            }) if active_id == id && active_identity == identity
        ) {
            return Ok(false);
        }
        state.scheduled_promotion = None;
        if self.is_retired() {
            self.complete_retirement_locked(&mut state);
        }
        self.retirement_changed.notify_all();
        Ok(true)
    }

    fn observes_failure_retained_promotion(
        &self,
        id: PromotionAuthorityId,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    ) -> Result<bool, ProjectionCoordinatorError> {
        let state = self
            .gate
            .lock()
            .map_err(|_| Self::authority_state_error())?;
        Ok(matches!(
            state.scheduled_promotion,
            Some(ScheduledPromotionAuthority {
                id: active_id,
                state: ExactAuthorityState::FailureRetained(active_identity),
            }) if active_id == id && active_identity == identity
        ))
    }

    fn consume_failure_retained_cleanup(
        &self,
        id: CleanupAuthorityId,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    ) -> Result<bool, ProjectionCoordinatorError> {
        let mut state = self.lock()?;
        if state.cleanup_owners.get(&id) != Some(&ExactAuthorityState::FailureRetained(identity)) {
            return Ok(false);
        }
        state.cleanup_owners.remove(&id);
        if self.is_retired() {
            self.complete_retirement_locked(&mut state);
        }
        self.retirement_changed.notify_all();
        Ok(true)
    }

    fn observes_failure_retained_cleanup(
        &self,
        id: CleanupAuthorityId,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    ) -> Result<bool, ProjectionCoordinatorError> {
        let state = self
            .gate
            .lock()
            .map_err(|_| Self::authority_state_error())?;
        Ok(state.cleanup_owners.get(&id) == Some(&ExactAuthorityState::FailureRetained(identity)))
    }

    pub(super) fn lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, ConnectionAuthorityState>, ProjectionCoordinatorError>
    {
        match self.gate.lock() {
            Ok(gate) => Ok(gate),
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                self.retire_locked(&mut state);
                drop(state);
                Err(ProjectionCoordinatorError::RegistryPoisoned {
                    registry: crate::cas_projection::ProjectionRegistryKind::ProjectionConnection,
                })
            }
        }
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn lock_for_test(
        &self,
    ) -> std::sync::MutexGuard<'_, ConnectionAuthorityState> {
        self.gate.lock().unwrap()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn poison_for_recovery_test(&self) {
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _state = self
                .gate
                .lock()
                .expect("connection authority starts unpoisoned");
            panic!("poison connection authority after quarantine installation");
        }));
        assert!(panicked.is_err());
        assert!(self.gate.is_poisoned());
    }

    #[cfg(test)]
    pub(in crate::cas_projection) const fn generation_for_test(&self) -> ConnectionGeneration {
        self.generation
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn observe_next_retirement_gate_attempt_for_test(
        &self,
    ) -> RetirementGateAttemptObservation {
        let (reached, observation) = std::sync::mpsc::sync_channel(1);
        let slot = RETIREMENT_GATE_ATTEMPT_HOOK.get_or_init(|| Mutex::new(None));
        let mut slot = slot.lock().expect("retirement gate-attempt hook is usable");
        assert!(
            slot.is_none(),
            "only one retirement gate attempt may be observed"
        );
        *slot = Some(RetirementGateAttemptHook {
            connection_generation: self.generation.get(),
            reached,
        });
        RetirementGateAttemptObservation {
            reached: observation,
        }
    }
}

#[cfg(test)]
impl RetirementGateAttemptObservation {
    pub(in crate::cas_projection) fn wait(self, timeout: std::time::Duration) {
        self.reached
            .recv_timeout(timeout)
            .expect("the exact retirement reaches its authority-gate attempt");
    }
}

#[cfg(test)]
fn observe_retirement_gate_attempt(connection_generation: u64) {
    let slot = RETIREMENT_GATE_ATTEMPT_HOOK.get_or_init(|| Mutex::new(None));
    let hook = {
        let mut slot = slot.lock().expect("retirement gate-attempt hook is usable");
        if slot
            .as_ref()
            .is_some_and(|hook| hook.connection_generation == connection_generation)
        {
            slot.take()
        } else {
            None
        }
    };
    if let Some(hook) = hook {
        let _ = hook.reached.send(());
    }
}

impl ConnectionPromotionReservation {
    fn settle(&mut self) -> Result<ExactSettlementOutcome, ProjectionCoordinatorError> {
        let command = self
            .command
            .as_ref()
            .expect("active promotion reservation retains its command permit");
        let failure_transfer = self
            .failure_transfer
            .take()
            .expect("active promotion reservation retains its failure transfer");
        self.authority.settle_scheduled_promotion(
            self.id,
            &self.connection,
            command,
            failure_transfer,
        )
    }

    pub(in crate::cas_projection) fn release(
        mut self,
    ) -> Result<ConnectionPromotionReleaseOutcome, ProjectionCoordinatorError> {
        let connection = Arc::clone(&self.connection);
        let settlement = self.settle();
        self.active = false;
        self.command.take();
        match settlement? {
            ExactSettlementOutcome::Ordinary {
                should_detach: true,
            } => {
                connection.shutdown_after_ordinary_retirement()?;
                Ok(ConnectionPromotionReleaseOutcome::Ordinary)
            }
            ExactSettlementOutcome::Ordinary {
                should_detach: false,
            } => Ok(ConnectionPromotionReleaseOutcome::Ordinary),
            ExactSettlementOutcome::PersistentFailure => {
                Ok(ConnectionPromotionReleaseOutcome::PersistentFailure)
            }
            ExactSettlementOutcome::Closed => Ok(ConnectionPromotionReleaseOutcome::Closed),
        }
    }

    pub(in crate::cas_projection) fn retain_for_persistent_failure(
        mut self,
    ) -> Result<(), ProjectionCoordinatorError> {
        let settlement = self.settle();
        self.active = false;
        self.command.take();
        match settlement? {
            ExactSettlementOutcome::PersistentFailure => Ok(()),
            ExactSettlementOutcome::Ordinary { .. } | ExactSettlementOutcome::Closed => {
                Err(ConnectionRegistryAuthority::authority_state_error())
            }
        }
    }
}

impl FailureRetainedPromotionReservation {
    pub(in crate::cas_projection) fn observe_for_recovery(
        &self,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    ) -> Result<Option<FailureRetainedConnectionOwnerWitness>, ProjectionCoordinatorError> {
        if identity != self.identity
            || !Arc::ptr_eq(&self.authority, &self.connection.authority)
            || !self
                .authority
                .observes_failure_retained_promotion(self.id, identity)?
        {
            return Ok(None);
        }
        Ok(Some(FailureRetainedConnectionOwnerWitness {
            cut_identity: identity,
            connection: self.connection.identity_observation(),
        }))
    }

    pub(in crate::cas_projection) fn consume_for_recovery(
        self,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    ) -> Result<Arc<ProjectionConnection>, Self> {
        if identity != self.identity
            || !matches!(
                self.authority
                    .consume_failure_retained_promotion(self.id, identity),
                Ok(true)
            )
        {
            return Err(self);
        }
        Ok(Arc::clone(&self.connection))
    }

    pub(in crate::cas_projection) const fn identity(
        &self,
    ) -> crate::cas_projection::persistent_failure::PersistentFailureCutIdentity {
        self.identity
    }

    pub(in crate::cas_projection) fn matches_connection(
        &self,
        connection: &Arc<ProjectionConnection>,
    ) -> bool {
        Arc::ptr_eq(&self.connection, connection)
            && Arc::ptr_eq(&self.authority, &connection.authority)
            && self.connection.identity_observation() == connection.identity_observation()
    }
}

impl FailureRetainedCleanupOwner {
    pub(in crate::cas_projection) fn observe_for_recovery(
        &self,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    ) -> Result<Option<FailureRetainedConnectionOwnerWitness>, ProjectionCoordinatorError> {
        if identity != self.identity
            || !Arc::ptr_eq(&self.authority, &self.connection.authority)
            || !self
                .authority
                .observes_failure_retained_cleanup(self.id, identity)?
        {
            return Ok(None);
        }
        Ok(Some(FailureRetainedConnectionOwnerWitness {
            cut_identity: identity,
            connection: self.connection.identity_observation(),
        }))
    }

    pub(in crate::cas_projection) fn consume_for_recovery(
        self,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    ) -> Result<Arc<ProjectionConnection>, Self> {
        if identity != self.identity
            || !matches!(
                self.authority
                    .consume_failure_retained_cleanup(self.id, identity),
                Ok(true)
            )
        {
            return Err(self);
        }
        Ok(Arc::clone(&self.connection))
    }

    pub(in crate::cas_projection) const fn identity(
        &self,
    ) -> crate::cas_projection::persistent_failure::PersistentFailureCutIdentity {
        self.identity
    }

    pub(in crate::cas_projection) fn matches_connection(
        &self,
        connection: &Arc<ProjectionConnection>,
    ) -> bool {
        Arc::ptr_eq(&self.connection, connection)
            && Arc::ptr_eq(&self.authority, &connection.authority)
            && self.connection.identity_observation() == connection.identity_observation()
    }
}

impl PendingProjectionConnectionOwnerInstallError {
    fn new(
        failure: PendingProjectionConnectionOwnerInstallFailure,
        promotions: Vec<FailureRetainedPromotionReservation>,
        cleanup: Vec<FailureRetainedCleanupOwner>,
    ) -> Self {
        Self {
            failure,
            promotions,
            cleanup,
        }
    }

    pub(in crate::cas_projection) fn into_parts(
        self,
    ) -> (
        PendingProjectionConnectionOwnerInstallFailure,
        Vec<FailureRetainedPromotionReservation>,
        Vec<FailureRetainedCleanupOwner>,
    ) {
        (self.failure, self.promotions, self.cleanup)
    }
}

impl PendingProjectionConnectionOwner {
    #[cfg(test)]
    pub(in crate::cas_projection) fn force_candidate_set_topology_mismatch_for_test(&mut self) {
        self.force_candidate_set_topology_mismatch = true;
    }

    pub(in crate::cas_projection) fn stable_connection_for_inert_failure(
        &self,
    ) -> &Arc<ProjectionConnection> {
        &self.connection
    }

    pub(in crate::cas_projection) fn is_promotable(&self) -> bool {
        self.active
            && Arc::ptr_eq(&self.authority, &self.connection.authority)
            && !self.authority.is_retired()
            && !self.authority.gate.is_poisoned()
    }

    pub(in crate::cas_projection) fn observe_for_adoption(
        &self,
        identity: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
    ) -> Result<Option<Arc<ProjectionConnection>>, ProjectionCoordinatorError> {
        if !self.active
            || self.identity != identity
            || !Arc::ptr_eq(&self.authority, &self.connection.authority)
            || self.authority.is_retired()
        {
            return Ok(None);
        }
        let state = self.authority.gate.lock().map_err(|_| {
            ProjectionCoordinatorError::RegistryPoisoned {
                registry: crate::cas_projection::ProjectionRegistryKind::ProjectionConnection,
            }
        })?;
        if state.pending_projection_quarantine
            != Some(PendingProjectionQuarantineAuthorityState {
                id: self.id,
                identity,
                stage: PendingProjectionQuarantineStage::Reauthentication,
            })
            || state.retirement_complete
        {
            return Ok(None);
        }
        Ok(Some(Arc::clone(&self.connection)))
    }

    fn release(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        self.authority.release_pending_projection_quarantine_owner(
            self.id,
            self.identity,
            PendingProjectionQuarantineStage::Reauthentication,
        );
    }
}

impl Drop for PendingProjectionConnectionOwner {
    fn drop(&mut self) {
        self.release();
    }
}

impl ConnectionCleanupOwner {
    fn settle(&mut self) -> Result<ExactSettlementOutcome, ProjectionCoordinatorError> {
        let command = self
            .command
            .as_ref()
            .expect("active cleanup owner retains its command permit");
        let failure_transfer = self
            .failure_transfer
            .take()
            .expect("active cleanup owner retains its failure transfer");
        self.authority
            .settle_cleanup_owner(self.id, &self.connection, command, failure_transfer)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn commit_loaded_release(
        &self,
        key: &LoadedThreadKey,
        owner: SyndicThreadId,
        generation: CasLoadedSessionGeneration,
        token: LeaseToken,
    ) -> Result<Option<registry::ReleaseDisposition>, ProjectionCoordinatorError> {
        if !self.active {
            return Err(ConnectionRegistryAuthority::authority_state_error());
        }
        let _state = self.authority.lock()?;
        let command = self
            .command
            .as_ref()
            .expect("active cleanup owner retains its command permit");
        match command.commit_if_current(|| {
            registry::release_exact(key, self.authority.generation, owner, generation, token)
        }) {
            Ok(result) => result.map(Some),
            Err(_) => Ok(None),
        }
    }

    pub(in crate::cas_projection) fn finish(mut self) -> Result<(), ProjectionCoordinatorError> {
        let connection = Arc::clone(&self.connection);
        let settlement = self.settle();
        self.active = false;
        self.command.take();
        match settlement? {
            ExactSettlementOutcome::Ordinary {
                should_detach: true,
            } => connection.shutdown_after_ordinary_retirement(),
            ExactSettlementOutcome::Ordinary {
                should_detach: false,
            }
            | ExactSettlementOutcome::PersistentFailure
            | ExactSettlementOutcome::Closed => Ok(()),
        }
    }

    pub(in crate::cas_projection) fn retain_for_persistent_failure(
        mut self,
    ) -> Result<(), ProjectionCoordinatorError> {
        let settlement = self.settle();
        self.active = false;
        self.command.take();
        match settlement? {
            ExactSettlementOutcome::PersistentFailure => Ok(()),
            ExactSettlementOutcome::Ordinary { .. } | ExactSettlementOutcome::Closed => {
                Err(ConnectionRegistryAuthority::authority_state_error())
            }
        }
    }
}

impl Drop for ConnectionCleanupOwner {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let settlement = self.settle();
        self.active = false;
        self.command.take();
        match settlement {
            Ok(ExactSettlementOutcome::Ordinary {
                should_detach: true,
            }) => self.connection.signal_ordinary_retirement(),
            Err(_) => self.connection.request_ordinary_retirement(),
            Ok(
                ExactSettlementOutcome::Ordinary {
                    should_detach: false,
                }
                | ExactSettlementOutcome::PersistentFailure
                | ExactSettlementOutcome::Closed,
            ) => {}
        }
    }
}

impl Drop for ConnectionPromotionReservation {
    fn drop(&mut self) {
        if self.active {
            let settlement = self.settle();
            self.active = false;
            self.command.take();
            match settlement {
                Ok(ExactSettlementOutcome::Ordinary {
                    should_detach: true,
                }) => self.connection.signal_ordinary_retirement(),
                Err(_) => self.connection.request_ordinary_retirement(),
                Ok(
                    ExactSettlementOutcome::Ordinary {
                        should_detach: false,
                    }
                    | ExactSettlementOutcome::PersistentFailure
                    | ExactSettlementOutcome::Closed,
                ) => {}
            }
        }
    }
}
