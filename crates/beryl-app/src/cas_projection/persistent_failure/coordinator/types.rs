use super::*;

/// Stable phase of the process-local persistent-failure safety cut.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentFailureCutState {
    Armed,
    Cutting,
    Finished,
    Incomplete,
    Stopped,
}

/// Bounded content-free observation of the one-shot persistent-failure coordinator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PersistentFailureCutSnapshot {
    pub(super) state: PersistentFailureCutState,
    pub(super) service_generation: ProjectionServiceGeneration,
    pub(super) failure_generation: Option<PersistentFailureGeneration>,
    pub(super) target_count: usize,
    pub(super) retained_projection_count: usize,
    pub(super) retained_promotion_count: usize,
    pub(super) retained_cleanup_count: usize,
}

/// Exact content-free counts for one sealed persistent-failure recovery inventory.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PersistentFailureRecoveryInventoryCounts {
    pub(super) complete_candidate_count: usize,
    pub(super) target_projection_count: usize,
    pub(super) reacquisition_anchor_count: usize,
    pub(super) raw_loaded_lease_count: usize,
    pub(super) raw_quarantined_anchor_count: usize,
    pub(super) raw_reacquisition_reservation_count: usize,
    pub(super) promotion_count: usize,
    pub(super) cleanup_count: usize,
    pub(super) connection_count: usize,
    pub(super) target_result_count: usize,
}

pub struct PersistentFailureRecoveryInventoryObservation {
    pub(in crate::cas_projection::persistent_failure) retained_counts:
        PersistentFailureRecoveryInventoryCounts,
    pub(in crate::cas_projection::persistent_failure) late_publication_count: usize,
    pub(in crate::cas_projection::persistent_failure) retention_poisoned: bool,
    pub(in crate::cas_projection::persistent_failure) pending_quarantine_available: bool,
}

impl PersistentFailureRecoveryInventoryCounts {
    /// Returns the number of complete worker-surrendered pre-activation wrappers.
    #[must_use]
    pub const fn complete_candidate_count(self) -> usize {
        self.complete_candidate_count
    }

    /// Returns the number of complete wrappers surrendered from frozen targets.
    #[must_use]
    pub const fn target_projection_count(self) -> usize {
        self.target_projection_count
    }

    /// Returns the number of complete same-native reacquisition anchors.
    #[must_use]
    pub const fn reacquisition_anchor_count(self) -> usize {
        self.reacquisition_anchor_count
    }

    /// Returns the number of raw loaded-lease tokens.
    #[must_use]
    pub const fn raw_loaded_lease_count(self) -> usize {
        self.raw_loaded_lease_count
    }

    /// Returns the number of raw quarantined-anchor tokens.
    #[must_use]
    pub const fn raw_quarantined_anchor_count(self) -> usize {
        self.raw_quarantined_anchor_count
    }

    /// Returns the number of raw reacquisition-reservation tokens.
    #[must_use]
    pub const fn raw_reacquisition_reservation_count(self) -> usize {
        self.raw_reacquisition_reservation_count
    }

    /// Returns the number of exact scheduled-promotion barriers.
    #[must_use]
    pub const fn promotion_count(self) -> usize {
        self.promotion_count
    }

    /// Returns the number of exact cleanup owners.
    #[must_use]
    pub const fn cleanup_count(self) -> usize {
        self.cleanup_count
    }

    /// Returns the number of connections retained by the completed cut.
    #[must_use]
    pub const fn connection_count(self) -> usize {
        self.connection_count
    }

    /// Returns the number of bounded target-disposition results retained by the cut.
    #[must_use]
    pub const fn target_result_count(self) -> usize {
        self.target_result_count
    }
}

impl PersistentFailureCutSnapshot {
    #[must_use]
    pub const fn state(self) -> PersistentFailureCutState {
        self.state
    }

    #[must_use]
    pub const fn service_generation(self) -> ProjectionServiceGeneration {
        self.service_generation
    }

    #[must_use]
    pub const fn failure_generation(self) -> Option<PersistentFailureGeneration> {
        self.failure_generation
    }

    #[must_use]
    pub const fn target_count(self) -> usize {
        self.target_count
    }

    /// Returns the bounded number of already-loaded projections preserved for recovery.
    #[must_use]
    pub const fn retained_projection_count(self) -> usize {
        self.retained_projection_count
    }

    /// Returns the bounded number of scheduled-promotion barriers preserved for recovery.
    #[must_use]
    pub const fn retained_promotion_count(self) -> usize {
        self.retained_promotion_count
    }

    /// Returns the bounded number of exact cleanup owners preserved for recovery.
    #[must_use]
    pub const fn retained_cleanup_count(self) -> usize {
        self.retained_cleanup_count
    }
}

pub(super) struct CoordinatorState {
    pub(super) phase: PersistentFailureCutState,
    pub(super) failure_generation: Option<PersistentFailureGeneration>,
    pub(super) target_count: usize,
    pub(super) retained_connections: Vec<Arc<ProjectionConnection>>,
    pub(super) retained_results: Vec<PersistentFailureRetainedTarget>,
    pub(super) retained_projections: Vec<LoadedCasProjection>,
    pub(super) retained_target_projections: Vec<LoadedCasProjection>,
    pub(super) retained_reacquisition_anchors: Vec<SameNativeReacquisitionAnchor>,
    pub(super) retained_raw_loaded_leases: Vec<FailureRetainedRawLoadedLease>,
    pub(super) retained_raw_quarantined_anchors: Vec<FailureRetainedRawQuarantinedAnchor>,
    pub(super) retained_raw_reacquisition_reservations:
        Vec<FailureRetainedRawReacquisitionReservation>,
    pub(super) retained_promotion_reservations: Vec<FailureRetainedPromotionReservation>,
    pub(super) retained_cleanup_owners: Vec<FailureRetainedCleanupOwner>,
    pub(super) sealed_counts: Option<PersistentFailureRecoveryInventoryCounts>,
    pub(in crate::cas_projection::persistent_failure) late_publication_count: usize,
    pub(super) pending_quarantine: PendingQuarantineStage,
}

pub(super) enum PendingQuarantineStage {
    Available,
    ConversionCheckedOut,
    Installed(super::super::quarantine::PendingProjectionQuarantineAuthority),
    AdoptionCheckedOut(Arc<PendingProjectionAdoptionEscrow>),
    Adopted(Arc<PendingProjectionAdoptionEscrow>),
    AdoptionRetired(Arc<PendingProjectionAdoptionEscrow>),
    TerminalDispositionCheckedOut(Arc<PendingProjectionTerminalDispositionEscrow>),
    TerminalDispositionComplete(Arc<PendingProjectionTerminalDispositionEscrow>),
    Conflicted(Vec<super::super::quarantine::PendingProjectionQuarantineAuthority>),
}

pub(super) struct PendingProjectionAdoptionEscrow {
    pub(super) late: Mutex<PendingProjectionAdoptionEscrowState>,
}

pub(super) struct PendingProjectionAdoptionEscrowState {
    pub(super) publications: PersistentFailureRecoveryDrain,
    pub(super) authorities: Vec<super::super::quarantine::PendingProjectionQuarantineAuthority>,
}

pub(super) struct PendingProjectionTerminalDispositionEscrow {
    pub(super) late: Mutex<PendingProjectionTerminalDispositionEscrowState>,
}

pub(super) struct PendingProjectionTerminalDispositionEscrowState {
    pub(super) publications: PersistentFailureRecoveryDrain,
    pub(super) authorities: Vec<super::super::quarantine::PendingProjectionQuarantineAuthority>,
}

pub(super) enum RetainedPublication {
    Projection(LoadedCasProjection),
    TargetProjection(LoadedCasProjection),
    ReacquisitionAnchor(SameNativeReacquisitionAnchor),
    RawLoadedLease(FailureRetainedRawLoadedLease),
    RawQuarantinedAnchor(FailureRetainedRawQuarantinedAnchor),
    RawReacquisitionReservation(FailureRetainedRawReacquisitionReservation),
    Promotion(FailureRetainedPromotionReservation),
    Cleanup(FailureRetainedCleanupOwner),
}

impl PendingQuarantineStage {
    pub(super) const fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }

    pub(super) fn retain_late_drain(&mut self, drain: PersistentFailureRecoveryDrain) {
        let authority = super::super::quarantine::PendingProjectionQuarantineAuthority {
            topology: super::super::quarantine::PendingProjectionQuarantineOwnedTopology::Inert { drain },
            reason: Some(
                super::super::quarantine::PersistentFailurePendingProjectionQuarantineReason::LatePublication,
            ),
        };
        let previous = std::mem::replace(self, Self::Available);
        *self = match previous {
            Self::Available | Self::ConversionCheckedOut => Self::Installed(authority),
            Self::Installed(existing) => Self::Conflicted(vec![existing, authority]),
            Self::AdoptionCheckedOut(escrow) => {
                escrow.retain_authority(authority);
                Self::AdoptionCheckedOut(escrow)
            }
            Self::Adopted(escrow) => {
                escrow.retain_authority(authority);
                Self::Adopted(escrow)
            }
            Self::AdoptionRetired(escrow) => {
                escrow.retain_authority(authority);
                Self::AdoptionRetired(escrow)
            }
            Self::TerminalDispositionCheckedOut(escrow) => {
                escrow.retain_authority(authority);
                Self::TerminalDispositionCheckedOut(escrow)
            }
            Self::TerminalDispositionComplete(escrow) => {
                escrow.retain_authority(authority);
                Self::TerminalDispositionComplete(escrow)
            }
            Self::Conflicted(mut authorities) => {
                authorities.push(authority);
                Self::Conflicted(authorities)
            }
        };
    }
}

impl CoordinatorState {
    pub(super) fn recovery_inventory_counts(&self) -> PersistentFailureRecoveryInventoryCounts {
        PersistentFailureRecoveryInventoryCounts {
            complete_candidate_count: self.retained_projections.len(),
            target_projection_count: self.retained_target_projections.len(),
            reacquisition_anchor_count: self.retained_reacquisition_anchors.len(),
            raw_loaded_lease_count: self.retained_raw_loaded_leases.len(),
            raw_quarantined_anchor_count: self.retained_raw_quarantined_anchors.len(),
            raw_reacquisition_reservation_count: self.retained_raw_reacquisition_reservations.len(),
            promotion_count: self.retained_promotion_reservations.len(),
            cleanup_count: self.retained_cleanup_owners.len(),
            connection_count: self.retained_connections.len(),
            target_result_count: self.retained_results.len(),
        }
    }

    pub(super) fn note_retained_publication(&mut self) {
        if self.sealed_counts.is_some() {
            self.late_publication_count = self
                .late_publication_count
                .checked_add(1)
                .expect("worker and router bounds keep late retention counts representable");
        }
    }

    pub(super) fn retain_publication(&mut self, publication: RetainedPublication) {
        self.note_retained_publication();
        if let PendingQuarantineStage::AdoptionCheckedOut(escrow)
        | PendingQuarantineStage::Adopted(escrow)
        | PendingQuarantineStage::AdoptionRetired(escrow) = &self.pending_quarantine
        {
            escrow.retain(publication);
            return;
        }
        if let PendingQuarantineStage::TerminalDispositionCheckedOut(escrow)
        | PendingQuarantineStage::TerminalDispositionComplete(escrow) = &self.pending_quarantine
        {
            escrow.retain(publication);
            return;
        }
        if matches!(
            &self.pending_quarantine,
            PendingQuarantineStage::Installed(_) | PendingQuarantineStage::Conflicted(_)
        ) {
            self.pending_quarantine
                .retain_late_drain(publication.into_recovery_drain());
        } else {
            publication.push_into_state(self);
        }
    }
}

impl PendingProjectionAdoptionEscrow {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            late: Mutex::new(PendingProjectionAdoptionEscrowState {
                publications: PersistentFailureRecoveryDrain::default(),
                authorities: Vec::new(),
            }),
        })
    }

    pub(super) fn retain(&self, publication: RetainedPublication) {
        let mut late = self
            .late
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        publication.push_into_drain(&mut late.publications);
    }

    pub(super) fn retain_authority(
        &self,
        authority: super::super::quarantine::PendingProjectionQuarantineAuthority,
    ) {
        let mut late = self
            .late
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        late.authorities.push(authority);
    }

    pub(in crate::cas_projection::persistent_failure) fn is_empty(&self) -> Result<bool, ()> {
        self.late
            .lock()
            .map(|late| {
                late.publications.counts() == Default::default() && late.authorities.is_empty()
            })
            .map_err(|_| ())
    }
}

impl PendingProjectionTerminalDispositionEscrow {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            late: Mutex::new(PendingProjectionTerminalDispositionEscrowState {
                publications: PersistentFailureRecoveryDrain::default(),
                authorities: Vec::with_capacity(1),
            }),
        })
    }

    pub(super) fn retain(&self, publication: RetainedPublication) {
        let mut late = self
            .late
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        publication.push_into_drain(&mut late.publications);
    }

    pub(super) fn retain_authority(
        &self,
        authority: super::super::quarantine::PendingProjectionQuarantineAuthority,
    ) {
        self.late
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .authorities
            .push(authority);
    }
}

impl RetainedPublication {
    pub(super) fn push_into_state(self, state: &mut CoordinatorState) {
        match self {
            Self::Projection(projection) => state.retained_projections.push(projection),
            Self::TargetProjection(projection) => {
                state.retained_target_projections.push(projection);
            }
            Self::ReacquisitionAnchor(anchor) => {
                state.retained_reacquisition_anchors.push(anchor);
            }
            Self::RawLoadedLease(lease) => state.retained_raw_loaded_leases.push(lease),
            Self::RawQuarantinedAnchor(anchor) => {
                state.retained_raw_quarantined_anchors.push(anchor);
            }
            Self::RawReacquisitionReservation(reservation) => {
                state
                    .retained_raw_reacquisition_reservations
                    .push(reservation);
            }
            Self::Promotion(reservation) => {
                state.retained_promotion_reservations.push(reservation);
            }
            Self::Cleanup(owner) => state.retained_cleanup_owners.push(owner),
        }
    }

    pub(super) fn into_recovery_drain(self) -> PersistentFailureRecoveryDrain {
        let mut drain = PersistentFailureRecoveryDrain::default();
        self.push_into_drain(&mut drain);
        drain
    }

    pub(super) fn push_into_drain(self, drain: &mut PersistentFailureRecoveryDrain) {
        match self {
            Self::Projection(projection) => drain.retained_projections.push(projection),
            Self::TargetProjection(projection) => {
                drain.retained_target_projections.push(projection);
            }
            Self::ReacquisitionAnchor(anchor) => {
                drain.retained_reacquisition_anchors.push(anchor);
            }
            Self::RawLoadedLease(lease) => drain.retained_raw_loaded_leases.push(lease),
            Self::RawQuarantinedAnchor(anchor) => {
                drain.retained_raw_quarantined_anchors.push(anchor);
            }
            Self::RawReacquisitionReservation(reservation) => {
                drain
                    .retained_raw_reacquisition_reservations
                    .push(reservation);
            }
            Self::Promotion(reservation) => {
                drain.retained_promotion_reservations.push(reservation);
            }
            Self::Cleanup(owner) => drain.retained_cleanup_owners.push(owner),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PersistentFailureRetainedTarget {
    pub(in crate::cas_projection::persistent_failure) witness: PersistentFailureTargetWitness,
    pub(in crate::cas_projection::persistent_failure) result: PersistentFailureDriverResult,
}

pub(super) struct PendingPersistentFailureResult {
    pub(super) completion: PersistentFailureCompletion,
}

pub(in crate::cas_projection) struct PersistentFailureCoordinator {
    pub(super) service_generation: ProjectionServiceGeneration,
    pub(super) notification: PersistentFailureNotification,
    pub(super) stop_requested: Arc<AtomicBool>,
    pub(super) state: Arc<(Mutex<CoordinatorState>, Condvar)>,
    pub(super) handle: Mutex<Option<JoinHandle<()>>>,
}

/// Cloneable sink that transfers already-loaded worker projections into the one-shot cut.
#[derive(Clone)]
pub(in crate::cas_projection) struct PersistentFailureProjectionRetainer {
    pub(super) home_id: BerylHomeId,
    pub(super) home_generation: HomeGeneration,
    pub(super) notification: PersistentFailureNotification,
    pub(super) state: Arc<(Mutex<CoordinatorState>, Condvar)>,
}

impl std::fmt::Debug for PersistentFailureProjectionRetainer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PersistentFailureProjectionRetainer")
            .field("home_id", &self.home_id)
            .field("home_generation", &self.home_generation)
            .finish_non_exhaustive()
    }
}

pub(super) struct WorkerContext {
    pub(super) home: Arc<HomeStore>,
    pub(super) home_id: BerylHomeId,
    pub(super) home_generation: HomeGeneration,
    pub(super) service_generation: ProjectionServiceGeneration,
    pub(super) notification: PersistentFailureNotification,
    pub(super) gate: MasterCommandGate,
    pub(super) stop_coordinator: Arc<StopCoordinator>,
    pub(super) connections:
        Arc<crate::cas_projection::service_registry::ProjectionServiceConnectionRegistry>,
    pub(super) stop_requested: Arc<AtomicBool>,
    pub(super) state: Arc<(Mutex<CoordinatorState>, Condvar)>,
}
