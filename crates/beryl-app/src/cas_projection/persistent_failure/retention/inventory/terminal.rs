use super::*;

#[must_use = "successful supervisor shutdown must consume this one-shot terminal witness"]
pub(in crate::cas_projection::persistent_failure) struct PersistentFailureTerminalDispositionWitness
{
    cut: crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
}

#[must_use = "terminal retirement failure retains the old service and coordinator witness"]
pub(in crate::cas_projection::persistent_failure) struct PersistentFailureTerminalRetirementError {
    reason: PersistentFailureOldServiceEpochRetirementReason,
    inventory: PersistentFailureRecoveryInventory,
    witness:
        super::super::super::coordinator::PersistentFailureTerminalDispositionCoordinatorWitness,
}

impl PersistentFailureTerminalDispositionWitness {
    pub(in crate::cas_projection::persistent_failure) const fn cut_identity(
        &self,
    ) -> crate::cas_projection::persistent_failure::PersistentFailureCutIdentity {
        self.cut
    }
}

impl PersistentFailureTerminalRetirementError {
    pub(in crate::cas_projection::persistent_failure) fn into_parts(
        self,
    ) -> (
        PersistentFailureOldServiceEpochRetirementReason,
        PersistentFailureRecoveryInventory,
        super::super::super::coordinator::PersistentFailureTerminalDispositionCoordinatorWitness,
    ) {
        (self.reason, self.inventory, self.witness)
    }
}

impl PersistentFailureRecoveryInventory {
    pub(in crate::cas_projection::persistent_failure) fn checkout_pending_quarantine_for_terminal_disposition(
        &self,
    ) -> Result<
        super::super::super::coordinator::PendingProjectionTerminalDispositionFence,
        super::super::super::quarantine::PersistentFailurePendingProjectionQuarantineReason,
    > {
        self.retained
            .persistent_failure
            .checkout_pending_quarantine_for_terminal_disposition(self.cut_identity())
    }

    pub(in crate::cas_projection::persistent_failure) fn drain_terminal_disposition_escrow(
        &self,
        fence: &super::super::super::coordinator::PendingProjectionTerminalDispositionFence,
    ) -> Result<
        super::super::super::coordinator::PersistentFailureTerminalDispositionDrain,
        super::super::super::quarantine::PersistentFailurePendingProjectionQuarantineReason,
    > {
        self.retained
            .persistent_failure
            .drain_terminal_disposition_escrow(self.cut_identity(), fence)
    }

    pub(in crate::cas_projection::persistent_failure) fn retain_terminal_disposition_authority(
        &self,
        fence: &super::super::super::coordinator::PendingProjectionTerminalDispositionFence,
        authority: super::super::super::quarantine::PendingProjectionQuarantineAuthority,
    ) -> Result<bool, super::super::super::quarantine::PendingProjectionQuarantineAuthority> {
        self.retained
            .persistent_failure
            .retain_terminal_disposition_authority(self.cut_identity(), fence, authority)
    }

    pub(in crate::cas_projection::persistent_failure) fn complete_terminal_disposition(
        &self,
        fence: super::super::super::coordinator::PendingProjectionTerminalDispositionFence,
    ) -> Result<
        super::super::super::coordinator::PersistentFailureTerminalDispositionCoordinatorWitness,
        super::super::super::coordinator::PendingProjectionTerminalDispositionFence,
    > {
        self.retained
            .persistent_failure
            .complete_terminal_disposition(self.cut_identity(), fence)
    }

    pub(in crate::cas_projection::persistent_failure) fn complete_terminal_retirement(
        mut self,
        witness: super::super::super::coordinator::PersistentFailureTerminalDispositionCoordinatorWitness,
    ) -> Result<PersistentFailureTerminalDispositionWitness, PersistentFailureTerminalRetirementError>
    {
        if witness.cut_identity() != self.cut_identity() || !witness.escrow_is_empty() {
            return Err(PersistentFailureTerminalRetirementError {
                reason:
                    PersistentFailureOldServiceEpochRetirementReason::RetirementWitnessCutMismatch,
                inventory: self,
                witness,
            });
        }
        let mut first_failure = witness
            .synchronization_poisoned()
            .then_some(PersistentFailureOldServiceEpochRetirementReason::EscrowPoisoned);
        let shutdown_failure = match catch_unwind(AssertUnwindSafe(|| {
            self.retained.shutdown_old_service_epoch()
        })) {
            Ok(Ok(())) => None,
            Ok(Err(reason)) => Some(reason),
            Err(_) => Some(PersistentFailureOldServiceEpochRetirementReason::CleanupPanicked),
        };
        if let Some(reason) = shutdown_failure {
            first_failure.get_or_insert(reason);
        }
        if !witness.escrow_is_empty() {
            first_failure.get_or_insert(
                PersistentFailureOldServiceEpochRetirementReason::EscrowNotCheckedOut,
            );
        }
        match self.remove_exact_retirement_owner() {
            Ok(()) => self.active = false,
            Err(reason) => {
                first_failure.get_or_insert(reason);
            }
        }
        if let Some(reason) = first_failure {
            return Err(PersistentFailureTerminalRetirementError {
                reason,
                inventory: self,
                witness,
            });
        }
        Ok(PersistentFailureTerminalDispositionWitness {
            cut: witness.cut_identity(),
        })
    }
}
