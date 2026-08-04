use std::{
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use beryl_home_store::HomeGeneration;
use beryl_model::BerylHomeId;

use super::{
    PersistentFailureCutCompletion, PersistentFailureCutHandoff, PersistentFailureRetainedService,
    PersistentFailureServiceEscrowCell, retained_services,
};
use crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerExit;
use crate::cas_projection::connection::ProjectionConnection;
use crate::cas_projection::persistent_failure::{
    LiveCommandGateStatus, PersistentFailureAdoptionRetirementWitness,
    PersistentFailureCutIdentity, PersistentFailureCutSnapshot, PersistentFailureGeneration,
    PersistentFailureRecoveryInventoryCounts, ProjectionServiceGeneration,
};

mod terminal;

pub(in crate::cas_projection::persistent_failure) use terminal::{
    PersistentFailureTerminalDispositionWitness, PersistentFailureTerminalRetirementError,
};

/// Non-cloneable sealed ownership of every capability retained by one finished failure cut.
#[must_use = "the recovery inventory owns the failed service and all retained authority"]
pub struct PersistentFailureRecoveryInventory {
    retained: Arc<PersistentFailureRetainedService>,
    escrow: Arc<PersistentFailureServiceEscrowCell>,
    sealed_counts: Option<PersistentFailureRecoveryInventoryCounts>,
    scheduler_quiescence: SchedulerQuiescence,
    active: bool,
}

/// Content-free metadata for one sealed persistent-failure recovery inventory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PersistentFailureRecoveryInventoryMetadata {
    sealed_counts: Option<PersistentFailureRecoveryInventoryCounts>,
    retained_counts: PersistentFailureRecoveryInventoryCounts,
    late_publication_count: usize,
    retention_poisoned: bool,
    promotable: bool,
}

/// Owning failure produced while converting a cut handoff into a recovery inventory.
#[must_use = "the failure retains either the original handoff or an inert recovery inventory"]
pub enum PersistentFailureRecoveryInventoryError {
    /// The cut did not reach its finished stable boundary; the original handoff remains owned.
    CutIncomplete(PersistentFailureCutHandoff),
    /// The pointer-identical service escrow was not available; the original handoff remains owned.
    EscrowUnavailable(PersistentFailureCutHandoff),
    /// The joined scheduler reported a fatal worker outcome; the sealed inventory is inert.
    SchedulerFatal(PersistentFailureRecoveryInventory),
    /// The scheduler thread panicked while joining; the sealed inventory is inert.
    SchedulerPanicked(PersistentFailureRecoveryInventory),
    /// The retained scheduler-owner boundary was poisoned; the sealed inventory is inert.
    SchedulerPoisoned(PersistentFailureRecoveryInventory),
    /// The retained command-gate boundary was poisoned after scheduler quiescence.
    CommandGatePoisoned(PersistentFailureRecoveryInventory),
    /// The retained service had already lost its scheduler owner; the sealed inventory is inert.
    SchedulerUnavailable(PersistentFailureRecoveryInventory),
    /// The retained finished coordinator rejected its one retention seal; the inventory is inert.
    RetentionSealRejected(PersistentFailureRecoveryInventory),
}

/// Exact reason why the closed old service epoch could not be retired safely.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum PersistentFailureOldServiceEpochRetirementReason {
    RetirementWitnessCutMismatch,
    RetainedServiceOwnerAliased,
    EscrowRegistryPoisoned,
    EscrowIdentityMismatch,
    EscrowOwnerAliased,
    EscrowPoisoned,
    EscrowNotCheckedOut,
    ContextCompactionUnavailable,
    ContextCompactionShutdown,
    ExecutionProviderUnavailable,
    ExecutionProviderAliased,
    ExecutionProviderPoisoned,
    CleanupPanicked,
}

/// Owning failure from explicit old-service-epoch retirement.
///
/// The unchanged retirement witness and recovery inventory remain together, so a failed cleanup
/// cannot silently discard either final-publication authority or the retained old service.
#[must_use = "the failure retains the recovery inventory and retirement witness"]
pub(in crate::cas_projection) struct PersistentFailureOldServiceEpochRetirementError {
    reason: PersistentFailureOldServiceEpochRetirementReason,
    inventory: PersistentFailureRecoveryInventory,
    witness: PersistentFailureAdoptionRetirementWitness,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SchedulerQuiescence {
    Clean,
    Fatal,
    Panicked,
    Poisoned,
    CommandGatePoisoned,
    Unavailable,
}

impl PersistentFailureRecoveryInventoryMetadata {
    /// Returns the exact retained counts captured by the coordinator's one-way seal.
    #[must_use]
    pub const fn sealed_counts(self) -> Option<PersistentFailureRecoveryInventoryCounts> {
        self.sealed_counts
    }

    /// Returns exact owners still waiting in the retention stage, including late publication before
    /// quarantine installation. After checkout, normalized ownership is reported by quarantine
    /// metadata instead.
    #[must_use]
    pub const fn retained_counts(self) -> PersistentFailureRecoveryInventoryCounts {
        self.retained_counts
    }

    /// Returns the number of retained publications that arrived after the seal.
    #[must_use]
    pub const fn late_publication_count(self) -> usize {
        self.late_publication_count
    }

    /// Reports whether the retained-capability synchronization boundary was poisoned.
    #[must_use]
    pub const fn retention_poisoned(self) -> bool {
        self.retention_poisoned
    }

    /// Reports whether later recovery phases may consume this inventory for promotion.
    #[must_use]
    pub const fn is_promotable(self) -> bool {
        self.promotable
    }
}

impl PersistentFailureOldServiceEpochRetirementError {
    pub(in crate::cas_projection) const fn reason(
        &self,
    ) -> PersistentFailureOldServiceEpochRetirementReason {
        self.reason
    }

    pub(in crate::cas_projection) fn into_parts(
        self,
    ) -> (
        PersistentFailureRecoveryInventory,
        PersistentFailureAdoptionRetirementWitness,
    ) {
        (self.inventory, self.witness)
    }
}

impl fmt::Debug for PersistentFailureOldServiceEpochRetirementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PersistentFailureOldServiceEpochRetirementError")
            .field("reason", &self.reason)
            .field("inventory", &self.inventory)
            .field("witness_cut", &self.witness.cut_identity())
            .finish_non_exhaustive()
    }
}

impl fmt::Display for PersistentFailureOldServiceEpochRetirementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "old service epoch retirement failed: {:?}",
            self.reason
        )
    }
}

impl std::error::Error for PersistentFailureOldServiceEpochRetirementError {}

impl PersistentFailureRecoveryInventory {
    #[cfg(test)]
    pub(in crate::cas_projection) fn escrow_is_checked_out_for_test(&self) -> bool {
        self.escrow
            .retained
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .is_empty()
    }

    /// Returns the exact Beryl-home identity retained by this inventory.
    #[must_use]
    pub fn home_id(&self) -> BerylHomeId {
        self.retained.home_id
    }

    /// Returns the exact failed home generation retained by this inventory.
    #[must_use]
    pub fn home_generation(&self) -> HomeGeneration {
        self.retained.home_generation
    }

    /// Returns the exact failed projection-service generation retained by this inventory.
    #[must_use]
    pub fn service_generation(&self) -> ProjectionServiceGeneration {
        self.retained.service_generation
    }

    /// Returns the exact persistent-failure generation retained by this inventory.
    #[must_use]
    pub fn failure_generation(&self) -> PersistentFailureGeneration {
        self.retained.failure_generation
    }

    /// Returns a content-free snapshot of the cut that produced this inventory.
    #[must_use]
    pub fn cut_snapshot(&self) -> PersistentFailureCutSnapshot {
        self.retained.persistent_failure.snapshot()
    }

    /// Returns content-free exact counts and promotion eligibility for the retained inventory.
    #[must_use]
    pub fn metadata(&self) -> PersistentFailureRecoveryInventoryMetadata {
        let observation = self
            .retained
            .persistent_failure
            .recovery_inventory_observation();
        PersistentFailureRecoveryInventoryMetadata {
            sealed_counts: self.sealed_counts,
            retained_counts: observation.retained_counts,
            late_publication_count: observation.late_publication_count,
            retention_poisoned: observation.retention_poisoned,
            promotable: self.scheduler_quiescence == SchedulerQuiescence::Clean
                && self.sealed_counts.is_some()
                && observation.late_publication_count == 0
                && !observation.retention_poisoned
                && observation.pending_quarantine_available,
        }
    }

    pub(in crate::cas_projection) fn cut_identity(&self) -> PersistentFailureCutIdentity {
        PersistentFailureCutIdentity::new(
            self.home_id(),
            self.home_generation(),
            self.service_generation(),
            self.failure_generation(),
        )
    }

    pub(in crate::cas_projection::persistent_failure) fn checkout_pending_quarantine_drain(
        &self,
    ) -> Result<
        super::super::coordinator::PersistentFailureRecoveryDrain,
        super::super::coordinator::PersistentFailureRecoveryDrainError,
    > {
        let sealed_counts = self
            .sealed_counts
            .ok_or(super::super::coordinator::PersistentFailureRecoveryDrainError::NotStable)?;
        if self.scheduler_quiescence != SchedulerQuiescence::Clean
            || !self.metadata().is_promotable()
        {
            return Err(super::super::coordinator::PersistentFailureRecoveryDrainError::NotStable);
        }
        self.retained
            .persistent_failure
            .checkout_recovery_drain(self.cut_identity(), sealed_counts)
    }

    pub(in crate::cas_projection::persistent_failure) fn install_pending_quarantine(
        &self,
        authority: super::super::quarantine::PendingProjectionQuarantineAuthority,
    ) -> Option<super::super::quarantine::PersistentFailurePendingProjectionQuarantineReason> {
        self.retained
            .persistent_failure
            .install_pending_quarantine(self.cut_identity(), authority)
    }

    pub(in crate::cas_projection::persistent_failure) fn pending_quarantine_metadata(
        &self,
    ) -> super::super::quarantine::PersistentFailurePendingProjectionQuarantineMetadata {
        self.retained
            .persistent_failure
            .pending_quarantine_metadata()
    }

    pub(in crate::cas_projection) fn retained_service_connections(
        &self,
    ) -> Vec<Arc<ProjectionConnection>> {
        self.retained.retained_connections.clone()
    }

    pub(in crate::cas_projection) fn retained_service_connection_slice(
        &self,
    ) -> &[Arc<ProjectionConnection>] {
        &self.retained.retained_connections
    }

    pub(in crate::cas_projection) fn retained_home(&self) -> &Arc<beryl_home_store::HomeStore> {
        &self.retained.home
    }

    pub(in crate::cas_projection) fn retained_worker_pool(
        &self,
    ) -> crate::cas_projection::service_config::ProjectionWorkerPool {
        self.retained.workers.clone()
    }

    pub(in crate::cas_projection) fn retained_connection_registry(
        &self,
    ) -> Arc<crate::cas_projection::service_registry::ProjectionServiceConnectionRegistry> {
        Arc::clone(&self.retained.connections)
    }

    pub(in crate::cas_projection) fn retained_service_config(
        &self,
    ) -> crate::cas_projection::ProjectionServiceConfig {
        self.retained.config.clone()
    }

    pub(in crate::cas_projection) fn checkout_pending_quarantine_for_adoption(
        &self,
    ) -> Result<
        super::super::coordinator::PendingProjectionAdoptionCheckout,
        super::super::quarantine::PersistentFailurePendingProjectionQuarantineReason,
    > {
        self.retained
            .persistent_failure
            .checkout_pending_quarantine_for_adoption(self.cut_identity())
    }

    pub(in crate::cas_projection) fn commit_pending_quarantine_adoption<R>(
        &self,
        fence: &super::super::coordinator::PersistentFailureAdoptionFence,
        commit: impl FnOnce() -> R,
    ) -> Result<R, super::super::quarantine::PersistentFailurePendingProjectionQuarantineReason>
    {
        self.retained
            .persistent_failure
            .commit_pending_quarantine_adoption(self.cut_identity(), fence, commit)
    }

    #[allow(dead_code, reason = "Phase 86 consumes the Phase 82 publication fence")]
    pub(in crate::cas_projection) fn retire_pending_quarantine_adoption(
        &self,
        fence: super::super::coordinator::PersistentFailureAdoptionFence,
    ) -> Result<
        super::super::coordinator::PersistentFailureAdoptionRetirementWitness,
        super::super::coordinator::PersistentFailureAdoptionFenceRetirementError,
    > {
        self.retained
            .persistent_failure
            .retire_pending_quarantine_adoption(self.cut_identity(), fence)
    }

    /// Consumes the closed old service epoch after its exact adoption-retirement witness exists.
    ///
    /// Success joins the old context-compaction coordinator, fences the epoch provider view,
    /// removes the exact process escrow entry, and disables Drop re-escrow. The same witness is
    /// returned unchanged for the later final publication commit. Every failure remains owning.
    pub(in crate::cas_projection) fn retire_old_service_epoch(
        mut self,
        witness: PersistentFailureAdoptionRetirementWitness,
    ) -> Result<
        PersistentFailureAdoptionRetirementWitness,
        PersistentFailureOldServiceEpochRetirementError,
    > {
        if witness.cut_identity() != self.cut_identity() {
            return Err(self.old_epoch_retirement_error(
                PersistentFailureOldServiceEpochRetirementReason::RetirementWitnessCutMismatch,
                witness,
            ));
        }
        if let Err(reason) = self.validate_exact_retirement_owner() {
            return Err(self.old_epoch_retirement_error(reason, witness));
        }
        match catch_unwind(AssertUnwindSafe(|| {
            self.retained.shutdown_old_service_epoch()
        })) {
            Ok(Ok(())) => {}
            Ok(Err(reason)) => {
                return Err(self.old_epoch_retirement_error(reason, witness));
            }
            Err(_) => {
                return Err(self.old_epoch_retirement_error(
                    PersistentFailureOldServiceEpochRetirementReason::CleanupPanicked,
                    witness,
                ));
            }
        }
        if let Err(reason) = self.remove_exact_retirement_owner() {
            return Err(self.old_epoch_retirement_error(reason, witness));
        }
        self.active = false;
        Ok(witness)
    }

    fn old_epoch_retirement_error(
        self,
        reason: PersistentFailureOldServiceEpochRetirementReason,
        witness: PersistentFailureAdoptionRetirementWitness,
    ) -> PersistentFailureOldServiceEpochRetirementError {
        PersistentFailureOldServiceEpochRetirementError {
            reason,
            inventory: self,
            witness,
        }
    }

    fn validate_exact_retirement_owner(
        &self,
    ) -> Result<(), PersistentFailureOldServiceEpochRetirementReason> {
        if Arc::strong_count(&self.retained) != 1 {
            return Err(
                PersistentFailureOldServiceEpochRetirementReason::RetainedServiceOwnerAliased,
            );
        }
        validate_exact_retirement_escrow(self.cut_identity(), &self.escrow)
    }

    fn remove_exact_retirement_owner(
        &self,
    ) -> Result<(), PersistentFailureOldServiceEpochRetirementReason> {
        if Arc::strong_count(&self.retained) != 1 {
            return Err(
                PersistentFailureOldServiceEpochRetirementReason::RetainedServiceOwnerAliased,
            );
        }
        remove_exact_retirement_escrow(self.cut_identity(), &self.escrow)
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn retain_late_adoption_authority_for_test(&self) {
        self.retained
            .persistent_failure
            .retain_late_adoption_authority_for_test(self.cut_identity());
    }

    pub(in crate::cas_projection) fn old_gate_matches_cut(&self) -> bool {
        self.retained
            .command_gate
            .matches_failure(self.service_generation(), self.failure_generation())
    }

    pub(in crate::cas_projection) fn current_service_connections(
        &self,
    ) -> Result<Vec<Arc<ProjectionConnection>>, ()> {
        self.retained
            .connections
            .lock()
            .map(|connections| connections.clone())
            .map_err(|_| ())
    }
}

impl fmt::Debug for PersistentFailureRecoveryInventory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PersistentFailureRecoveryInventory")
            .field("home_id", &self.home_id())
            .field("home_generation", &self.home_generation())
            .field("service_generation", &self.service_generation())
            .field("failure_generation", &self.failure_generation())
            .field("cut_snapshot", &self.cut_snapshot())
            .field("metadata", &self.metadata())
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for PersistentFailureRecoveryInventoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CutIncomplete(handoff) => formatter
                .debug_tuple("CutIncomplete")
                .field(handoff)
                .finish(),
            Self::EscrowUnavailable(handoff) => formatter
                .debug_tuple("EscrowUnavailable")
                .field(handoff)
                .finish(),
            Self::SchedulerFatal(inventory) => formatter
                .debug_tuple("SchedulerFatal")
                .field(inventory)
                .finish(),
            Self::SchedulerPanicked(inventory) => formatter
                .debug_tuple("SchedulerPanicked")
                .field(inventory)
                .finish(),
            Self::SchedulerPoisoned(inventory) => formatter
                .debug_tuple("SchedulerPoisoned")
                .field(inventory)
                .finish(),
            Self::CommandGatePoisoned(inventory) => formatter
                .debug_tuple("CommandGatePoisoned")
                .field(inventory)
                .finish(),
            Self::SchedulerUnavailable(inventory) => formatter
                .debug_tuple("SchedulerUnavailable")
                .field(inventory)
                .finish(),
            Self::RetentionSealRejected(inventory) => formatter
                .debug_tuple("RetentionSealRejected")
                .field(inventory)
                .finish(),
        }
    }
}

impl fmt::Display for PersistentFailureRecoveryInventoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::CutIncomplete(_) => "the persistent-failure cut is incomplete",
            Self::EscrowUnavailable(_) => {
                "the pointer-identical persistent-failure service escrow is unavailable"
            }
            Self::SchedulerFatal(_) => "the accepted-input scheduler reported a fatal outcome",
            Self::SchedulerPanicked(_) => "the accepted-input scheduler panicked while joining",
            Self::SchedulerPoisoned(_) => {
                "the accepted-input scheduler owner was poisoned before joining"
            }
            Self::CommandGatePoisoned(_) => {
                "the retained live-command gate was poisoned after scheduler quiescence"
            }
            Self::SchedulerUnavailable(_) => {
                "the retained service no longer owns its accepted-input scheduler"
            }
            Self::RetentionSealRejected(_) => {
                "the persistent-failure coordinator rejected its retention seal"
            }
        })
    }
}

impl std::error::Error for PersistentFailureRecoveryInventoryError {}

impl PersistentFailureRecoveryInventoryError {
    /// Returns the unchanged cut handoff for a pre-inventory conversion failure.
    pub fn into_handoff(self) -> Result<PersistentFailureCutHandoff, Self> {
        match self {
            Self::CutIncomplete(handoff) | Self::EscrowUnavailable(handoff) => Ok(handoff),
            error => Err(error),
        }
    }

    /// Returns the sealed inert inventory for a post-extraction conversion failure.
    pub fn into_inventory(self) -> Result<PersistentFailureRecoveryInventory, Self> {
        match self {
            Self::SchedulerFatal(inventory)
            | Self::SchedulerPanicked(inventory)
            | Self::SchedulerPoisoned(inventory)
            | Self::CommandGatePoisoned(inventory)
            | Self::SchedulerUnavailable(inventory)
            | Self::RetentionSealRejected(inventory) => Ok(inventory),
            error => Err(error),
        }
    }
}

#[cfg(test)]
fn retention_seal_observer() -> &'static std::sync::Mutex<Option<std::sync::mpsc::SyncSender<bool>>>
{
    static OBSERVER: std::sync::OnceLock<
        std::sync::Mutex<Option<std::sync::mpsc::SyncSender<bool>>>,
    > = std::sync::OnceLock::new();
    OBSERVER.get_or_init(|| std::sync::Mutex::new(None))
}

impl PersistentFailureCutHandoff {
    #[cfg(test)]
    pub(in crate::cas_projection) fn observe_next_retention_seal_for_test(
        observer: std::sync::mpsc::SyncSender<bool>,
    ) {
        *retention_seal_observer()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(observer);
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn poison_scheduler_owner_for_test(&self) {
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _scheduler = self
                .retained
                .scheduler
                .lock()
                .expect("scheduler owner starts unpoisoned");
            panic!("poison retained scheduler owner for inventory test");
        }));
        assert!(panicked.is_err());
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn poison_command_gate_for_test(&self) {
        self.retained.command_gate.poison_for_test();
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn poison_retention_for_test(&self) {
        self.retained
            .persistent_failure
            .poison_recovery_inventory_retention_for_test();
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn orphan_one_retained_promotion_for_test(&self) -> bool {
        self.retained
            .persistent_failure
            .orphan_one_retained_promotion_for_test()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn orphan_one_retained_connection_for_test(&self) -> bool {
        self.retained
            .persistent_failure
            .orphan_one_retained_connection_for_test()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn orphan_one_retained_target_result_for_test(&self) -> bool {
        self.retained
            .persistent_failure
            .orphan_one_retained_target_result_for_test()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn corrupt_one_target_disposition_for_test(&self) -> bool {
        self.retained
            .persistent_failure
            .corrupt_one_target_disposition_for_test()
    }

    /// Consumes one finished cut into its exact sealed recovery inventory.
    ///
    /// Incomplete cuts retain this handoff unchanged. A scheduler fatal outcome or panic returns
    /// an owning error containing the sealed but permanently non-promotable inventory.
    pub fn into_recovery_inventory(
        self,
    ) -> Result<PersistentFailureRecoveryInventory, PersistentFailureRecoveryInventoryError> {
        if self.completion() != PersistentFailureCutCompletion::Finished {
            return Err(PersistentFailureRecoveryInventoryError::CutIncomplete(self));
        }
        let (retained, escrow) = self
            .extract_exact_escrow()
            .map_err(PersistentFailureRecoveryInventoryError::EscrowUnavailable)?;
        let scheduler_quiescence = retained.quiesce_scheduler();
        #[cfg(test)]
        if let Some(observer) = retention_seal_observer()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
        {
            let _ = observer.send(retained.scheduler_signal.diagnostics().stopped());
        }
        let sealed_counts = retained.persistent_failure.seal_retention().ok();
        let inventory = PersistentFailureRecoveryInventory {
            retained,
            escrow,
            sealed_counts,
            scheduler_quiescence,
            active: true,
        };
        if sealed_counts.is_none() {
            return Err(PersistentFailureRecoveryInventoryError::RetentionSealRejected(inventory));
        }
        match scheduler_quiescence {
            SchedulerQuiescence::Clean => Ok(inventory),
            SchedulerQuiescence::Fatal => Err(
                PersistentFailureRecoveryInventoryError::SchedulerFatal(inventory),
            ),
            SchedulerQuiescence::Panicked => Err(
                PersistentFailureRecoveryInventoryError::SchedulerPanicked(inventory),
            ),
            SchedulerQuiescence::Poisoned => Err(
                PersistentFailureRecoveryInventoryError::SchedulerPoisoned(inventory),
            ),
            SchedulerQuiescence::CommandGatePoisoned => {
                Err(PersistentFailureRecoveryInventoryError::CommandGatePoisoned(inventory))
            }
            SchedulerQuiescence::Unavailable => {
                Err(PersistentFailureRecoveryInventoryError::SchedulerUnavailable(inventory))
            }
        }
    }

    fn extract_exact_escrow(
        self,
    ) -> Result<
        (
            Arc<PersistentFailureRetainedService>,
            Arc<PersistentFailureServiceEscrowCell>,
        ),
        Self,
    > {
        let identity = PersistentFailureCutIdentity::new(
            self.home_id(),
            self.home_generation(),
            self.service_generation(),
            self.failure_generation(),
        );
        let escrow_cell = Arc::clone(&self._escrow);
        let services = match retained_services().lock() {
            Ok(services) => services,
            Err(_) => return Err(self),
        };
        if !services
            .get(&identity)
            .is_some_and(|cell| Arc::ptr_eq(cell, &escrow_cell))
        {
            drop(services);
            return Err(self);
        }
        let retained = {
            let mut escrow = match escrow_cell.retained.lock() {
                Ok(escrow) => escrow,
                Err(_) => {
                    drop(services);
                    return Err(self);
                }
            };
            if escrow.len() != 1 || !Arc::ptr_eq(&escrow[0], &self.retained) {
                drop(escrow);
                drop(services);
                return Err(self);
            }
            escrow.pop()
        };
        drop(services);
        let Some(retained) = retained else {
            return Err(self);
        };
        Ok((retained, escrow_cell))
    }
}

fn validate_exact_retirement_escrow(
    identity: PersistentFailureCutIdentity,
    expected: &Arc<PersistentFailureServiceEscrowCell>,
) -> Result<(), PersistentFailureOldServiceEpochRetirementReason> {
    let services = retained_services()
        .lock()
        .map_err(|_| PersistentFailureOldServiceEpochRetirementReason::EscrowRegistryPoisoned)?;
    let Some(cell) = services.get(&identity) else {
        return Err(PersistentFailureOldServiceEpochRetirementReason::EscrowIdentityMismatch);
    };
    if !Arc::ptr_eq(cell, expected) {
        return Err(PersistentFailureOldServiceEpochRetirementReason::EscrowIdentityMismatch);
    }
    if Arc::strong_count(expected) != 2 {
        return Err(PersistentFailureOldServiceEpochRetirementReason::EscrowOwnerAliased);
    }
    let escrow = expected
        .retained
        .lock()
        .map_err(|_| PersistentFailureOldServiceEpochRetirementReason::EscrowPoisoned)?;
    if !escrow.is_empty() {
        return Err(PersistentFailureOldServiceEpochRetirementReason::EscrowNotCheckedOut);
    }
    Ok(())
}

pub(super) fn remove_exact_retirement_escrow(
    identity: PersistentFailureCutIdentity,
    expected: &Arc<PersistentFailureServiceEscrowCell>,
) -> Result<(), PersistentFailureOldServiceEpochRetirementReason> {
    let mut services = retained_services()
        .lock()
        .map_err(|_| PersistentFailureOldServiceEpochRetirementReason::EscrowRegistryPoisoned)?;
    let Some(cell) = services.get(&identity) else {
        return Err(PersistentFailureOldServiceEpochRetirementReason::EscrowIdentityMismatch);
    };
    if !Arc::ptr_eq(cell, expected) {
        return Err(PersistentFailureOldServiceEpochRetirementReason::EscrowIdentityMismatch);
    }
    if Arc::strong_count(expected) != 2 {
        return Err(PersistentFailureOldServiceEpochRetirementReason::EscrowOwnerAliased);
    }
    let escrow = expected
        .retained
        .lock()
        .map_err(|_| PersistentFailureOldServiceEpochRetirementReason::EscrowPoisoned)?;
    if !escrow.is_empty() {
        return Err(PersistentFailureOldServiceEpochRetirementReason::EscrowNotCheckedOut);
    }
    drop(escrow);
    services.remove(&identity);
    Ok(())
}

impl PersistentFailureRetainedService {
    fn shutdown_old_service_epoch(
        &self,
    ) -> Result<(), PersistentFailureOldServiceEpochRetirementReason> {
        let mut first_failure = None;
        match self.context_compaction.as_ref() {
            Some(context_compaction) => {
                if context_compaction.shutdown().is_err() {
                    first_failure = Some(
                        PersistentFailureOldServiceEpochRetirementReason::ContextCompactionShutdown,
                    );
                }
            }
            None => {
                first_failure = Some(
                    PersistentFailureOldServiceEpochRetirementReason::ContextCompactionUnavailable,
                );
            }
        }

        match self.scheduled_ordinary_provider.as_ref() {
            Some(provider) => {
                if Arc::strong_count(provider) != 1 && first_failure.is_none() {
                    first_failure = Some(
                        PersistentFailureOldServiceEpochRetirementReason::ExecutionProviderAliased,
                    );
                }
                match provider.lock() {
                    Ok(mut provider) => provider.shutdown(),
                    Err(poison) => {
                        poison.into_inner().shutdown();
                        if first_failure.is_none() {
                            first_failure = Some(
                                PersistentFailureOldServiceEpochRetirementReason::ExecutionProviderPoisoned,
                            );
                        }
                    }
                }
            }
            None => {
                if first_failure.is_none() {
                    first_failure = Some(
                        PersistentFailureOldServiceEpochRetirementReason::ExecutionProviderUnavailable,
                    );
                }
            }
        }

        first_failure.map_or(Ok(()), Err)
    }

    fn quiesce_scheduler(&self) -> SchedulerQuiescence {
        let (scheduler, owner_poisoned) = match self.scheduler.lock() {
            Ok(mut scheduler) => (scheduler.take(), false),
            Err(poison) => (poison.into_inner().take(), true),
        };
        let Some(scheduler) = scheduler else {
            self.scheduler_signal.request_shutdown();
            return if owner_poisoned {
                SchedulerQuiescence::Poisoned
            } else {
                SchedulerQuiescence::Unavailable
            };
        };
        scheduler.request_shutdown();
        let exit = match scheduler.join() {
            Err(()) => return SchedulerQuiescence::Panicked,
            Ok(exit) => exit,
        };
        if owner_poisoned {
            return SchedulerQuiescence::Poisoned;
        }
        if exit == AcceptedInputSchedulerExit::Fatal {
            return SchedulerQuiescence::Fatal;
        }
        match self.command_authorizer.status_exact() {
            Ok(LiveCommandGateStatus::PersistentFailure) => SchedulerQuiescence::Clean,
            Ok(
                LiveCommandGateStatus::Open
                | LiveCommandGateStatus::OrdinaryShutdown
                | LiveCommandGateStatus::LocalFailure,
            ) => SchedulerQuiescence::Fatal,
            Err(_) => SchedulerQuiescence::CommandGatePoisoned,
        }
    }
}

impl Drop for PersistentFailureRecoveryInventory {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        self.escrow
            .retained
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(Arc::clone(&self.retained));
    }
}
