use std::{
    collections::HashMap,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
};

use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::BerylHomeId;

use super::{
    MasterCommandGate, PersistentFailureCutIdentity, PersistentFailureGeneration,
    PersistentFailureNotification, ProjectionServiceGeneration,
};
#[cfg(test)]
use crate::cas_projection::connection::{
    PersistentFailureTargetGuardDisposition, PersistentFailureTargetIneligibility,
};
use crate::cas_projection::{
    LoadedCasProjection, SameNativeReacquisitionAnchor,
    connection::{
        CleanupFailureTransfer, ConnectionPromotionReservation, FailureRetainedCleanupOwner,
        FailureRetainedPromotionReservation, FailureRetainedRawLoadedLease,
        FailureRetainedRawQuarantinedAnchor, FailureRetainedRawReacquisitionReservation,
        PersistentFailureCompletion, PersistentFailureDriverResult,
        PersistentFailureNoDispatchReason, PersistentFailureTargetWitness, ProjectionConnection,
        PromotionFailureTransfer,
    },
    stop::StopCoordinator,
};

mod recovery;

pub(in crate::cas_projection) use recovery::{
    PendingProjectionAdoptionCheckout, PersistentFailureAdoptionFence,
    PersistentFailureAdoptionFenceRetirementError, PersistentFailureAdoptionRetirementWitness,
};

pub(in crate::cas_projection::persistent_failure) use recovery::{
    PendingProjectionTerminalDispositionFence, PersistentFailureRecoveryDrain,
    PersistentFailureRecoveryDrainError, PersistentFailureTerminalDispositionCoordinatorWitness,
    PersistentFailureTerminalDispositionDrain,
};

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
    state: PersistentFailureCutState,
    service_generation: ProjectionServiceGeneration,
    failure_generation: Option<PersistentFailureGeneration>,
    target_count: usize,
    retained_projection_count: usize,
    retained_promotion_count: usize,
    retained_cleanup_count: usize,
}

/// Exact content-free counts for one sealed persistent-failure recovery inventory.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PersistentFailureRecoveryInventoryCounts {
    complete_candidate_count: usize,
    target_projection_count: usize,
    reacquisition_anchor_count: usize,
    raw_loaded_lease_count: usize,
    raw_quarantined_anchor_count: usize,
    raw_reacquisition_reservation_count: usize,
    promotion_count: usize,
    cleanup_count: usize,
    connection_count: usize,
    target_result_count: usize,
}

pub(super) struct PersistentFailureRecoveryInventoryObservation {
    pub(super) retained_counts: PersistentFailureRecoveryInventoryCounts,
    pub(super) late_publication_count: usize,
    pub(super) retention_poisoned: bool,
    pub(super) pending_quarantine_available: bool,
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

struct CoordinatorState {
    phase: PersistentFailureCutState,
    failure_generation: Option<PersistentFailureGeneration>,
    target_count: usize,
    retained_connections: Vec<Arc<ProjectionConnection>>,
    retained_results: Vec<PersistentFailureRetainedTarget>,
    retained_projections: Vec<LoadedCasProjection>,
    retained_target_projections: Vec<LoadedCasProjection>,
    retained_reacquisition_anchors: Vec<SameNativeReacquisitionAnchor>,
    retained_raw_loaded_leases: Vec<FailureRetainedRawLoadedLease>,
    retained_raw_quarantined_anchors: Vec<FailureRetainedRawQuarantinedAnchor>,
    retained_raw_reacquisition_reservations: Vec<FailureRetainedRawReacquisitionReservation>,
    retained_promotion_reservations: Vec<FailureRetainedPromotionReservation>,
    retained_cleanup_owners: Vec<FailureRetainedCleanupOwner>,
    sealed_counts: Option<PersistentFailureRecoveryInventoryCounts>,
    late_publication_count: usize,
    pending_quarantine: PendingQuarantineStage,
}

enum PendingQuarantineStage {
    Available,
    ConversionCheckedOut,
    Installed(super::quarantine::PendingProjectionQuarantineAuthority),
    AdoptionCheckedOut(Arc<PendingProjectionAdoptionEscrow>),
    Adopted(Arc<PendingProjectionAdoptionEscrow>),
    AdoptionRetired(Arc<PendingProjectionAdoptionEscrow>),
    TerminalDispositionCheckedOut(Arc<PendingProjectionTerminalDispositionEscrow>),
    TerminalDispositionComplete(Arc<PendingProjectionTerminalDispositionEscrow>),
    Conflicted(Vec<super::quarantine::PendingProjectionQuarantineAuthority>),
}

struct PendingProjectionAdoptionEscrow {
    late: Mutex<PendingProjectionAdoptionEscrowState>,
}

struct PendingProjectionAdoptionEscrowState {
    publications: PersistentFailureRecoveryDrain,
    authorities: Vec<super::quarantine::PendingProjectionQuarantineAuthority>,
}

struct PendingProjectionTerminalDispositionEscrow {
    late: Mutex<PendingProjectionTerminalDispositionEscrowState>,
}

struct PendingProjectionTerminalDispositionEscrowState {
    publications: PersistentFailureRecoveryDrain,
    authorities: Vec<super::quarantine::PendingProjectionQuarantineAuthority>,
}

enum RetainedPublication {
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
    const fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }

    fn retain_late_drain(&mut self, drain: PersistentFailureRecoveryDrain) {
        let authority = super::quarantine::PendingProjectionQuarantineAuthority {
            topology: super::quarantine::PendingProjectionQuarantineOwnedTopology::Inert { drain },
            reason: Some(
                super::quarantine::PersistentFailurePendingProjectionQuarantineReason::LatePublication,
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
    fn recovery_inventory_counts(&self) -> PersistentFailureRecoveryInventoryCounts {
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

    fn note_retained_publication(&mut self) {
        if self.sealed_counts.is_some() {
            self.late_publication_count = self
                .late_publication_count
                .checked_add(1)
                .expect("worker and router bounds keep late retention counts representable");
        }
    }

    fn retain_publication(&mut self, publication: RetainedPublication) {
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
    fn new() -> Arc<Self> {
        Arc::new(Self {
            late: Mutex::new(PendingProjectionAdoptionEscrowState {
                publications: PersistentFailureRecoveryDrain::default(),
                authorities: Vec::new(),
            }),
        })
    }

    fn retain(&self, publication: RetainedPublication) {
        let mut late = self
            .late
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        publication.push_into_drain(&mut late.publications);
    }

    fn retain_authority(&self, authority: super::quarantine::PendingProjectionQuarantineAuthority) {
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
    fn new() -> Arc<Self> {
        Arc::new(Self {
            late: Mutex::new(PendingProjectionTerminalDispositionEscrowState {
                publications: PersistentFailureRecoveryDrain::default(),
                authorities: Vec::with_capacity(1),
            }),
        })
    }

    fn retain(&self, publication: RetainedPublication) {
        let mut late = self
            .late
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        publication.push_into_drain(&mut late.publications);
    }

    fn retain_authority(&self, authority: super::quarantine::PendingProjectionQuarantineAuthority) {
        self.late
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .authorities
            .push(authority);
    }
}

impl RetainedPublication {
    fn push_into_state(self, state: &mut CoordinatorState) {
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

    fn into_recovery_drain(self) -> PersistentFailureRecoveryDrain {
        let mut drain = PersistentFailureRecoveryDrain::default();
        self.push_into_drain(&mut drain);
        drain
    }

    fn push_into_drain(self, drain: &mut PersistentFailureRecoveryDrain) {
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
pub(super) struct PersistentFailureRetainedTarget {
    pub(super) witness: PersistentFailureTargetWitness,
    pub(super) result: PersistentFailureDriverResult,
}

struct PendingPersistentFailureResult {
    completion: PersistentFailureCompletion,
}

pub(in crate::cas_projection) struct PersistentFailureCoordinator {
    service_generation: ProjectionServiceGeneration,
    notification: PersistentFailureNotification,
    stop_requested: Arc<AtomicBool>,
    state: Arc<(Mutex<CoordinatorState>, Condvar)>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

/// Cloneable sink that transfers already-loaded worker projections into the one-shot cut.
#[derive(Clone)]
pub(in crate::cas_projection) struct PersistentFailureProjectionRetainer {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    notification: PersistentFailureNotification,
    state: Arc<(Mutex<CoordinatorState>, Condvar)>,
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

struct WorkerContext {
    home: Arc<HomeStore>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
    notification: PersistentFailureNotification,
    gate: MasterCommandGate,
    stop_coordinator: Arc<StopCoordinator>,
    connections: Arc<crate::cas_projection::service_registry::ProjectionServiceConnectionRegistry>,
    stop_requested: Arc<AtomicBool>,
    state: Arc<(Mutex<CoordinatorState>, Condvar)>,
}

impl PersistentFailureCoordinator {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection) fn start(
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        service_generation: ProjectionServiceGeneration,
        gate: MasterCommandGate,
        notification: PersistentFailureNotification,
        receiver: mpsc::Receiver<()>,
        stop_coordinator: Arc<StopCoordinator>,
        connections: Arc<
            crate::cas_projection::service_registry::ProjectionServiceConnectionRegistry,
        >,
    ) -> Result<Self, std::io::Error> {
        Self::start_with_startup_gate(
            home,
            home_id,
            home_generation,
            service_generation,
            gate,
            notification,
            receiver,
            stop_coordinator,
            connections,
            crate::cas_projection::service_startup::ServiceStartupGate::open_gate(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection) fn start_with_startup_gate(
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        service_generation: ProjectionServiceGeneration,
        gate: MasterCommandGate,
        notification: PersistentFailureNotification,
        receiver: mpsc::Receiver<()>,
        stop_coordinator: Arc<StopCoordinator>,
        connections: Arc<
            crate::cas_projection::service_registry::ProjectionServiceConnectionRegistry,
        >,
        startup: Arc<crate::cas_projection::service_startup::ServiceStartupGate>,
    ) -> Result<Self, std::io::Error> {
        let stop_requested = Arc::new(AtomicBool::new(false));
        let state = Arc::new((
            Mutex::new(CoordinatorState {
                phase: PersistentFailureCutState::Armed,
                failure_generation: None,
                target_count: 0,
                retained_connections: Vec::new(),
                retained_results: Vec::new(),
                retained_projections: Vec::new(),
                retained_target_projections: Vec::new(),
                retained_reacquisition_anchors: Vec::new(),
                retained_raw_loaded_leases: Vec::new(),
                retained_raw_quarantined_anchors: Vec::new(),
                retained_raw_reacquisition_reservations: Vec::new(),
                retained_promotion_reservations: Vec::new(),
                retained_cleanup_owners: Vec::new(),
                sealed_counts: None,
                late_publication_count: 0,
                pending_quarantine: PendingQuarantineStage::Available,
            }),
            Condvar::new(),
        ));
        let context = WorkerContext {
            home,
            home_id,
            home_generation,
            service_generation,
            notification: notification.clone(),
            gate,
            stop_coordinator,
            connections,
            stop_requested: Arc::clone(&stop_requested),
            state: Arc::clone(&state),
        };
        let handle = std::thread::Builder::new()
            .name("beryl-persistent-failure-cut".to_owned())
            .spawn(move || {
                if startup.wait() {
                    run_worker(receiver, context);
                }
            })?;
        Ok(Self {
            service_generation,
            notification,
            stop_requested,
            state,
            handle: Mutex::new(Some(handle)),
        })
    }

    pub(in crate::cas_projection) fn notification(&self) -> PersistentFailureNotification {
        self.notification.clone()
    }

    pub(in crate::cas_projection) fn snapshot(&self) -> PersistentFailureCutSnapshot {
        let state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        PersistentFailureCutSnapshot {
            state: state.phase,
            service_generation: self.service_generation,
            failure_generation: state.failure_generation,
            target_count: state.target_count,
            retained_projection_count: state
                .retained_projections
                .len()
                .checked_add(state.retained_target_projections.len())
                .and_then(|count| count.checked_add(state.retained_reacquisition_anchors.len()))
                .and_then(|count| count.checked_add(state.retained_raw_loaded_leases.len()))
                .and_then(|count| count.checked_add(state.retained_raw_quarantined_anchors.len()))
                .and_then(|count| {
                    count.checked_add(state.retained_raw_reacquisition_reservations.len())
                })
                .expect("retained projection allocations fit the process address space"),
            retained_promotion_count: state.retained_promotion_reservations.len(),
            retained_cleanup_count: state.retained_cleanup_owners.len(),
        }
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn retained_loaded_projection_counts_for_test(
        &self,
    ) -> (usize, usize) {
        let state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        (
            state.retained_projections.len(),
            state.retained_raw_loaded_leases.len(),
        )
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn retained_reacquisition_anchor_counts_for_test(
        &self,
    ) -> (usize, usize) {
        let state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        (
            state.retained_reacquisition_anchors.len(),
            state.retained_raw_quarantined_anchors.len(),
        )
    }

    #[cfg(test)]
    pub(super) fn orphan_one_retained_promotion_for_test(&self) -> bool {
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        assert_eq!(state.phase, PersistentFailureCutState::Finished);
        assert!(state.sealed_counts.is_none());
        state.retained_promotion_reservations.pop().is_some()
    }

    #[cfg(test)]
    pub(super) fn orphan_one_retained_connection_for_test(&self) -> bool {
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        assert_eq!(state.phase, PersistentFailureCutState::Finished);
        assert!(state.sealed_counts.is_none());
        state.retained_connections.pop().is_some()
    }

    #[cfg(test)]
    pub(super) fn orphan_one_retained_target_result_for_test(&self) -> bool {
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        assert_eq!(state.phase, PersistentFailureCutState::Finished);
        assert!(state.sealed_counts.is_none());
        state.retained_results.pop().is_some()
    }

    #[cfg(test)]
    pub(super) fn corrupt_one_target_disposition_for_test(&self) -> bool {
        let (witness, connections) = {
            let state = self
                .state
                .0
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            assert_eq!(state.phase, PersistentFailureCutState::Finished);
            assert!(state.sealed_counts.is_none());
            let Some(target) = state.retained_results.first() else {
                return false;
            };
            (target.witness.clone(), state.retained_connections.clone())
        };
        let Some(connection) = connections
            .iter()
            .find(|connection| connection.identity_observation() == witness.connection())
        else {
            return false;
        };
        let Ok(observation) = witness.observe_guard(connection) else {
            return false;
        };
        let mismatched = match observation.disposition() {
            PersistentFailureTargetGuardDisposition::Frozen => {
                PersistentFailureDriverResult::NoDispatch(
                    PersistentFailureNoDispatchReason::RandomUnavailable,
                )
            }
            PersistentFailureTargetGuardDisposition::Spent => {
                PersistentFailureDriverResult::NoDispatch(
                    PersistentFailureNoDispatchReason::Router(
                        PersistentFailureTargetIneligibility::RouterUnavailable,
                    ),
                )
            }
        };
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let Some(target) = state
            .retained_results
            .iter_mut()
            .find(|target| target.witness == witness)
        else {
            return false;
        };
        target.result = mismatched;
        true
    }

    pub(in crate::cas_projection) fn projection_retainer(
        &self,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
    ) -> PersistentFailureProjectionRetainer {
        PersistentFailureProjectionRetainer {
            home_id,
            home_generation,
            notification: self.notification.clone(),
            state: Arc::clone(&self.state),
        }
    }

    pub(in crate::cas_projection) fn request_shutdown(&self) {
        self.stop_requested.store(true, Ordering::Release);
        self.notification.wake_worker();
    }

    pub(in crate::cas_projection) fn join(&self) -> Result<(), ()> {
        let (handle, owner_poisoned) = match self.handle.lock() {
            Ok(mut handle) => (handle.take(), false),
            Err(poison) => (poison.into_inner().take(), true),
        };
        if handle.is_some_and(|handle| handle.join().is_err()) || owner_poisoned {
            return Err(());
        }
        Ok(())
    }

    pub(super) fn seal_retention(&self) -> Result<PersistentFailureRecoveryInventoryCounts, ()> {
        let mut state = self.state.0.lock().map_err(|_| ())?;
        if state.phase != PersistentFailureCutState::Finished || state.sealed_counts.is_some() {
            return Err(());
        }
        let counts = state.recovery_inventory_counts();
        state.sealed_counts = Some(counts);
        Ok(counts)
    }

    pub(super) fn recovery_inventory_observation(
        &self,
    ) -> PersistentFailureRecoveryInventoryObservation {
        match self.state.0.lock() {
            Ok(state) => PersistentFailureRecoveryInventoryObservation {
                retained_counts: state.recovery_inventory_counts(),
                late_publication_count: state.late_publication_count,
                retention_poisoned: false,
                pending_quarantine_available: state.pending_quarantine.is_available(),
            },
            Err(poison) => {
                let state = poison.into_inner();
                PersistentFailureRecoveryInventoryObservation {
                    retained_counts: state.recovery_inventory_counts(),
                    late_publication_count: state.late_publication_count,
                    retention_poisoned: true,
                    pending_quarantine_available: state.pending_quarantine.is_available(),
                }
            }
        }
    }

    #[cfg(test)]
    pub(super) fn poison_recovery_inventory_retention_for_test(&self) {
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _state = self
                .state
                .0
                .lock()
                .expect("retention state starts unpoisoned");
            panic!("poison retained capability state for inventory test");
        }));
        assert!(panicked.is_err());
    }
}

impl PersistentFailureProjectionRetainer {
    pub(in crate::cas_projection) fn service_generation(&self) -> ProjectionServiceGeneration {
        self.notification.service_generation()
    }

    pub(in crate::cas_projection) fn failure_observed(&self) -> bool {
        self.notification.failure_observed()
    }

    pub(in crate::cas_projection) fn cut_identity(
        &self,
        failure_generation: PersistentFailureGeneration,
    ) -> PersistentFailureCutIdentity {
        PersistentFailureCutIdentity::new(
            self.home_id,
            self.home_generation,
            self.notification.service_generation(),
            failure_generation,
        )
    }

    pub(in crate::cas_projection) fn promotion_failure_transfer(&self) -> PromotionFailureTransfer {
        let state = Arc::clone(&self.state);
        PromotionFailureTransfer::new(
            self.cut_identity(PersistentFailureGeneration::FIRST),
            move |retained| {
                let mut state = state.0.lock().unwrap_or_else(|poison| poison.into_inner());
                state.retain_publication(RetainedPublication::Promotion(retained));
            },
        )
    }

    pub(in crate::cas_projection) fn cleanup_failure_transfer(&self) -> CleanupFailureTransfer {
        let state = Arc::clone(&self.state);
        CleanupFailureTransfer::new(
            self.cut_identity(PersistentFailureGeneration::FIRST),
            move |retained| {
                let mut state = state.0.lock().unwrap_or_else(|poison| poison.into_inner());
                state.retain_publication(RetainedPublication::Cleanup(retained));
            },
        )
    }

    pub(in crate::cas_projection) fn retain_promotion(
        &self,
        reservation: ConnectionPromotionReservation,
    ) {
        debug_assert!(self.failure_observed());
        let _ = reservation.retain_for_persistent_failure();
    }

    pub(in crate::cas_projection) fn retain(&self, projection: LoadedCasProjection) {
        debug_assert!(self.failure_observed());
        self.retain_from_exact_settlement(projection);
    }

    pub(in crate::cas_projection) fn retain_from_exact_settlement(
        &self,
        projection: LoadedCasProjection,
    ) {
        debug_assert_eq!(projection.home_id(), self.home_id);
        debug_assert_eq!(projection.home_generation(), self.home_generation);
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.retain_publication(RetainedPublication::Projection(projection));
    }

    pub(in crate::cas_projection) fn retain_target(&self, projection: LoadedCasProjection) {
        debug_assert!(self.failure_observed());
        debug_assert_eq!(projection.home_id(), self.home_id);
        debug_assert_eq!(projection.home_generation(), self.home_generation);
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.retain_publication(RetainedPublication::TargetProjection(projection));
    }

    pub(in crate::cas_projection) fn retain_reacquisition_anchor(
        &self,
        anchor: SameNativeReacquisitionAnchor,
    ) {
        debug_assert!(self.failure_observed());
        self.retain_reacquisition_anchor_from_exact_settlement(anchor);
    }

    pub(in crate::cas_projection) fn retain_reacquisition_anchor_from_exact_settlement(
        &self,
        anchor: SameNativeReacquisitionAnchor,
    ) {
        debug_assert_eq!(anchor.home_id(), self.home_id);
        debug_assert_eq!(anchor.home_generation(), self.home_generation);
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.retain_publication(RetainedPublication::ReacquisitionAnchor(anchor));
    }

    pub(in crate::cas_projection) fn retain_raw_loaded_lease(
        &self,
        lease: FailureRetainedRawLoadedLease,
    ) {
        let identity = lease.identity();
        debug_assert_eq!(identity.home_id, self.home_id);
        debug_assert_eq!(identity.home_generation, self.home_generation);
        debug_assert_eq!(
            identity.service_generation,
            self.notification.service_generation()
        );
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.retain_publication(RetainedPublication::RawLoadedLease(lease));
    }

    pub(in crate::cas_projection) fn retain_raw_quarantined_anchor(
        &self,
        anchor: FailureRetainedRawQuarantinedAnchor,
    ) {
        let identity = anchor.identity();
        debug_assert_eq!(identity.home_id, self.home_id);
        debug_assert_eq!(identity.home_generation, self.home_generation);
        debug_assert_eq!(
            identity.service_generation,
            self.notification.service_generation()
        );
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.retain_publication(RetainedPublication::RawQuarantinedAnchor(anchor));
    }

    pub(in crate::cas_projection) fn retain_raw_reacquisition_reservation(
        &self,
        reservation: FailureRetainedRawReacquisitionReservation,
    ) {
        let identity = reservation.identity();
        debug_assert_eq!(identity.home_id, self.home_id);
        debug_assert_eq!(identity.home_generation, self.home_generation);
        debug_assert_eq!(
            identity.service_generation,
            self.notification.service_generation()
        );
        let mut state = self
            .state
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.retain_publication(RetainedPublication::RawReacquisitionReservation(
            reservation,
        ));
    }
}

impl Drop for PersistentFailureCoordinator {
    fn drop(&mut self) {
        self.stop_requested.store(true, Ordering::Release);
        self.notification.wake_worker();
        // Explicit service shutdown owns the join. An implicit drop can run while
        // another teardown owner is still unwinding, so it may only request stop
        // and detach the worker.
        let _ = self
            .handle
            .get_mut()
            .unwrap_or_else(|poison| poison.into_inner())
            .take();
    }
}

fn run_worker(receiver: mpsc::Receiver<()>, context: WorkerContext) {
    while receiver.recv().is_ok() {
        if context.stop_requested.load(Ordering::Acquire) {
            finish_worker(
                &context,
                PersistentFailureCutState::Stopped,
                None,
                Vec::new(),
                Vec::new(),
            );
            return;
        }
        if !context.notification.failure_observed() {
            continue;
        }
        let identity = PersistentFailureCutIdentity::new(
            context.home_id,
            context.home_generation,
            context.service_generation,
            PersistentFailureGeneration::FIRST,
        );
        {
            let mut state = context
                .state
                .0
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if state.phase != PersistentFailureCutState::Armed {
                continue;
            }
            state.phase = PersistentFailureCutState::Cutting;
            state.failure_generation = Some(identity.failure_generation);
            context.state.1.notify_all();
        }
        if !context
            .gate
            .close_for_persistent_failure(identity.failure_generation)
            .unwrap_or(false)
        {
            finish_worker(
                &context,
                PersistentFailureCutState::Stopped,
                Some(identity.failure_generation),
                Vec::new(),
                Vec::new(),
            );
            return;
        }
        context.notification.mark_cut_elected();
        let stop_freeze_failed = context
            .stop_coordinator
            .freeze_for_persistent_failure(identity)
            .is_err();
        let drain_failed = context.gate.wait_until_drained().is_err();
        let connections = snapshot_connections(&context.connections);
        if stop_freeze_failed || drain_failed {
            finish_worker(
                &context,
                PersistentFailureCutState::Incomplete,
                Some(identity.failure_generation),
                connections,
                Vec::new(),
            );
            return;
        }
        let Ok(results) = freeze_and_dispatch_targets(&context, identity, &connections) else {
            finish_worker(
                &context,
                PersistentFailureCutState::Incomplete,
                Some(identity.failure_generation),
                connections,
                Vec::new(),
            );
            return;
        };
        finish_worker(
            &context,
            PersistentFailureCutState::Finished,
            Some(identity.failure_generation),
            connections,
            results,
        );
        return;
    }
    finish_worker(
        &context,
        PersistentFailureCutState::Stopped,
        None,
        Vec::new(),
        Vec::new(),
    );
}

fn snapshot_connections(
    connections: &Arc<crate::cas_projection::service_registry::ProjectionServiceConnectionRegistry>,
) -> Vec<Arc<ProjectionConnection>> {
    let mut retained = connections
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    retained.retain(|connection| !connection.is_detached());
    retained.clone()
}

fn freeze_and_dispatch_targets(
    context: &WorkerContext,
    identity: PersistentFailureCutIdentity,
    connections: &[Arc<ProjectionConnection>],
) -> Result<Vec<PersistentFailureRetainedTarget>, ()> {
    let mut stop_evidence_by_connection = Vec::with_capacity(connections.len());
    for connection in connections {
        let mut stop_evidence = HashMap::new();
        let threads = connection
            .persistent_failure_target_threads(identity)
            .map_err(|_| ())?;
        for thread_id in threads {
            let evidence = context
                .stop_coordinator
                .persistent_failure_evidence(identity, thread_id)
                .map_err(|_| ())?;
            stop_evidence.insert(thread_id, evidence);
        }
        stop_evidence_by_connection.push(stop_evidence);
    }
    let mut frozen = Vec::with_capacity(connections.len());
    for (connection, stop_evidence) in connections.iter().zip(&stop_evidence_by_connection) {
        let candidates = connection
            .freeze_persistent_failure_targets(identity, stop_evidence)
            .map_err(|_| ())?;
        frozen.push((connection, candidates));
    }
    let mut retained_results = Vec::new();
    let mut pending_results = Vec::new();
    for (connection, batch) in frozen {
        let candidates = batch.into_candidates();
        let mut proofs = Vec::new();
        let mut proof_witnesses = Vec::new();
        for candidate in candidates {
            let (witness, proof) = candidate.into_parts();
            match proof {
                Ok(proof) => {
                    proof_witnesses.push(witness);
                    proofs.push(proof);
                }
                Err(reason) => retained_results.push(PersistentFailureRetainedTarget {
                    witness,
                    result: PersistentFailureDriverResult::NoDispatch(
                        PersistentFailureNoDispatchReason::Router(reason),
                    ),
                }),
            }
        }
        match connection.install_persistent_failure_obligations(identity, proofs) {
            Ok(completions) if completions.len() == proof_witnesses.len() => {
                pending_results.extend(
                    completions
                        .into_iter()
                        .map(|completion| PendingPersistentFailureResult { completion }),
                );
            }
            Ok(_) | Err(()) => {
                retained_results.extend(proof_witnesses.into_iter().map(|witness| {
                    PersistentFailureRetainedTarget {
                        witness,
                        result: PersistentFailureDriverResult::NoDispatch(
                            PersistentFailureNoDispatchReason::DriverUnavailable,
                        ),
                    }
                }));
            }
        }
    }
    retained_results.extend(pending_results.into_iter().map(|pending| {
        let completed = pending.completion.wait_with_witness();
        let (witness, result) = completed.into_parts();
        PersistentFailureRetainedTarget { witness, result }
    }));
    Ok(retained_results)
}

fn finish_worker(
    context: &WorkerContext,
    phase: PersistentFailureCutState,
    failure_generation: Option<PersistentFailureGeneration>,
    retained_connections: Vec<Arc<ProjectionConnection>>,
    retained_results: Vec<PersistentFailureRetainedTarget>,
) {
    let mut state = context
        .state
        .0
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    state.phase = phase;
    state.failure_generation = failure_generation;
    state.target_count = retained_results.len();
    state.retained_connections = retained_connections;
    state.retained_results = retained_results;
    context.state.1.notify_all();
}
