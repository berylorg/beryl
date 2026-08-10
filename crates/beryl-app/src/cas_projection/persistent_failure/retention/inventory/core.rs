use super::*;

/// Non-cloneable sealed ownership of every capability retained by one finished failure cut.
#[must_use = "the recovery inventory owns the failed service and all retained authority"]
pub struct PersistentFailureRecoveryInventory {
    pub(super) retained: Arc<PersistentFailureRetainedService>,
    pub(super) escrow: Arc<PersistentFailureServiceEscrowCell>,
    pub(super) sealed_counts: Option<PersistentFailureRecoveryInventoryCounts>,
    pub(super) scheduler_quiescence: SchedulerQuiescence,
    pub(super) active: bool,
}

/// Content-free metadata for one sealed persistent-failure recovery inventory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PersistentFailureRecoveryInventoryMetadata {
    pub(super) sealed_counts: Option<PersistentFailureRecoveryInventoryCounts>,
    pub(super) retained_counts: PersistentFailureRecoveryInventoryCounts,
    pub(super) late_publication_count: usize,
    pub(super) retention_poisoned: bool,
    pub(super) promotable: bool,
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
    pub(super) reason: PersistentFailureOldServiceEpochRetirementReason,
    pub(super) inventory: PersistentFailureRecoveryInventory,
    pub(super) witness: PersistentFailureAdoptionRetirementWitness,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SchedulerQuiescence {
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
        super::super::super::coordinator::PersistentFailureRecoveryDrain,
        super::super::super::coordinator::PersistentFailureRecoveryDrainError,
    > {
        let sealed_counts = self.sealed_counts.ok_or(
            super::super::super::coordinator::PersistentFailureRecoveryDrainError::NotStable,
        )?;
        if self.scheduler_quiescence != SchedulerQuiescence::Clean
            || !self.metadata().is_promotable()
        {
            return Err(
                super::super::super::coordinator::PersistentFailureRecoveryDrainError::NotStable,
            );
        }
        self.retained
            .persistent_failure
            .checkout_recovery_drain(self.cut_identity(), sealed_counts)
    }

    pub(in crate::cas_projection::persistent_failure) fn install_pending_quarantine(
        &self,
        authority: super::super::super::quarantine::PendingProjectionQuarantineAuthority,
    ) -> Option<super::super::super::quarantine::PersistentFailurePendingProjectionQuarantineReason>
    {
        self.retained
            .persistent_failure
            .install_pending_quarantine(self.cut_identity(), authority)
    }

    pub(in crate::cas_projection::persistent_failure) fn pending_quarantine_metadata(
        &self,
    ) -> super::super::super::quarantine::PersistentFailurePendingProjectionQuarantineMetadata {
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

    /// Makes this checked-out inventory a terminal inert owner that can never re-enter adoption.
    ///
    /// The caller must first make every exact retained connection core inert. This transition is
    /// intentionally allocation-free and only disarms Drop re-escrow; it does not retire, publish,
    /// or restore any retained service capability.
    pub(in crate::cas_projection) fn disarm_reescrow_after_terminal_inert(&mut self) {
        self.active = false;
    }

    #[cfg(test)]
    pub(in crate::cas_projection) const fn reescrow_is_disarmed_for_test(&self) -> bool {
        !self.active
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
        super::super::super::coordinator::PendingProjectionAdoptionCheckout,
        super::super::super::quarantine::PersistentFailurePendingProjectionQuarantineReason,
    > {
        self.retained
            .persistent_failure
            .checkout_pending_quarantine_for_adoption(self.cut_identity())
    }

    pub(in crate::cas_projection) fn commit_pending_quarantine_adoption<R>(
        &self,
        fence: &super::super::super::coordinator::PersistentFailureAdoptionFence,
        commit: impl FnOnce() -> R,
    ) -> Result<
        R,
        super::super::super::quarantine::PersistentFailurePendingProjectionQuarantineReason,
    > {
        self.retained
            .persistent_failure
            .commit_pending_quarantine_adoption(self.cut_identity(), fence, commit)
    }

    #[allow(dead_code, reason = "Phase 86 consumes the Phase 82 publication fence")]
    pub(in crate::cas_projection) fn retire_pending_quarantine_adoption(
        &self,
        fence: super::super::super::coordinator::PersistentFailureAdoptionFence,
    ) -> Result<
        super::super::super::coordinator::PersistentFailureAdoptionRetirementWitness,
        super::super::super::coordinator::PersistentFailureAdoptionFenceRetirementError,
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

    pub(super) fn old_epoch_retirement_error(
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

    pub(super) fn validate_exact_retirement_owner(
        &self,
    ) -> Result<(), PersistentFailureOldServiceEpochRetirementReason> {
        if Arc::strong_count(&self.retained) != 1 {
            return Err(
                PersistentFailureOldServiceEpochRetirementReason::RetainedServiceOwnerAliased,
            );
        }
        validate_exact_retirement_escrow(self.cut_identity(), &self.escrow)
    }

    pub(super) fn remove_exact_retirement_owner(
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
