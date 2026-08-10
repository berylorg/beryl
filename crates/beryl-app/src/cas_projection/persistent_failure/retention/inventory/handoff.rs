use super::*;

impl PersistentFailureCutHandoff {
    #[cfg(test)]
    pub(in crate::cas_projection) fn observe_next_retention_seal_for_test(
        observer: std::sync::mpsc::SyncSender<bool>,
    ) {
        *super::errors::retention_seal_observer()
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
        if let Some(observer) = super::errors::retention_seal_observer()
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

pub(super) fn validate_exact_retirement_escrow(
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

pub(in crate::cas_projection::persistent_failure::retention) fn remove_exact_retirement_escrow(
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
