use super::*;

use crate::cas_projection::persistent_failure::quarantine::{
    PendingProjectionAdoptionTopology, PendingProjectionQuarantineAuthority,
    PendingProjectionQuarantineOwnedTopology, PersistentFailurePendingProjectionQuarantineMetadata,
    PersistentFailurePendingProjectionQuarantineReason,
};

mod terminal;

pub(in crate::cas_projection::persistent_failure) use terminal::{
    PendingProjectionTerminalDispositionFence,
    PersistentFailureTerminalDispositionCoordinatorWitness,
    PersistentFailureTerminalDispositionDrain,
};

#[must_use = "the checkout owns the exact normalized quarantine topology"]
pub(in crate::cas_projection) struct PendingProjectionAdoptionCheckout {
    topology: Option<PendingProjectionAdoptionTopology>,
    fence: Option<PersistentFailureAdoptionFence>,
    #[cfg(test)]
    adversary: Option<super::test_support::AdversarialQuarantineTopologyForTest>,
    #[cfg(test)]
    secondary_inventory: Option<crate::cas_projection::PersistentFailureRecoveryInventory>,
    #[cfg(test)]
    secondary_topology: Option<PendingProjectionAdoptionTopology>,
    #[cfg(test)]
    secondary_fence: Option<PersistentFailureAdoptionFence>,
    #[cfg(test)]
    secondary_cut: Option<PersistentFailureCutIdentity>,
    #[cfg(test)]
    secondary_connections: Vec<Arc<ProjectionConnection>>,
    #[cfg(test)]
    secondary_inert_attachments:
        Vec<crate::cas_projection::connection::InertConnectionEpochAttachment>,
    #[cfg(test)]
    reached_owner_count: usize,
    #[cfg(test)]
    adversary_applied: bool,
    #[cfg(test)]
    secondary_terminalized: bool,
}

/// Non-cloneable witness that the exact quarantine remains isolated from late publication.
#[must_use = "publication must consume and revalidate the committed adoption fence"]
pub(in crate::cas_projection) struct PersistentFailureAdoptionFence {
    escrow: Arc<PendingProjectionAdoptionEscrow>,
}

/// One-shot proof that adoption retirement won the race with every late old-cut publication.
#[must_use = "the later process publication commit must consume this retirement witness"]
pub(in crate::cas_projection) struct PersistentFailureAdoptionRetirementWitness {
    cut: PersistentFailureCutIdentity,
    escrow: Arc<PendingProjectionAdoptionEscrow>,
}

/// Owning retirement failure; no publication witness was issued.
#[must_use = "the failure retains the terminal adoption fence"]
pub(in crate::cas_projection) struct PersistentFailureAdoptionFenceRetirementError {
    reason: PersistentFailurePendingProjectionQuarantineReason,
    fence: PersistentFailureAdoptionFence,
}

impl PendingProjectionAdoptionCheckout {
    pub(in crate::cas_projection) fn into_parts(
        mut self,
    ) -> (
        PendingProjectionAdoptionTopology,
        PersistentFailureAdoptionFence,
    ) {
        (
            self.topology
                .take()
                .expect("adoption checkout retains the normalized quarantine topology"),
            self.fence
                .take()
                .expect("adoption checkout retains its terminal publication fence"),
        )
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn take_parts(
        &mut self,
    ) -> (
        PendingProjectionAdoptionTopology,
        PersistentFailureAdoptionFence,
    ) {
        (
            self.topology
                .take()
                .expect("adoption checkout retains the normalized quarantine topology"),
            self.fence
                .take()
                .expect("adoption checkout retains its terminal publication fence"),
        )
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn apply_adversarial_topology_for_test(
        &mut self,
        primary_topology: &mut PendingProjectionAdoptionTopology,
        primary_cut: PersistentFailureCutIdentity,
    ) -> Result<(), PersistentFailurePendingProjectionQuarantineReason> {
        let Some(adversary) = self.adversary.take() else {
            return Ok(());
        };
        self.adversary_applied = true;
        match adversary {
            super::test_support::AdversarialQuarantineTopologyForTest::RetiredConnectionOwner => {
                if primary_topology.connection_owner_count() != 1 {
                    return Err(
                        PersistentFailurePendingProjectionQuarantineReason::ConnectionIdentityMismatch,
                    );
                }
                let connection =
                    primary_topology.connection_owners()[0].stable_connection_for_inert_failure();
                match connection.retire_authority_for_recovery_test() {
                    Ok(crate::cas_projection::connection::ConnectionRetirementOutcome::FailureRetained(
                        cut,
                    )) if cut == primary_cut => {}
                    Ok(_) => {
                        return Err(
                            PersistentFailurePendingProjectionQuarantineReason::ConnectionUnavailable,
                        );
                    }
                    Err(_) => {
                        return Err(
                            PersistentFailurePendingProjectionQuarantineReason::ConnectionUnavailable,
                        );
                    }
                }
                self.reached_owner_count = 1;
                Ok(())
            }
            super::test_support::AdversarialQuarantineTopologyForTest::ExtraConnectionOwner(
                secondary,
            ) => {
                let mut secondary_topology = self.checkout_secondary_for_test(secondary)?;
                let mut secondary_owners =
                    secondary_topology.take_connection_owners_for_adversarial_test();
                if primary_topology.connection_owner_count() != 1 || secondary_owners.len() != 1 {
                    return Err(
                        PersistentFailurePendingProjectionQuarantineReason::ConnectionIdentityMismatch,
                    );
                }
                let mut primary_owners =
                    primary_topology.take_connection_owners_for_adversarial_test();
                primary_owners.append(&mut secondary_owners);
                let displaced =
                    primary_topology.replace_connection_owners_for_adversarial_test(primary_owners);
                debug_assert!(displaced.is_empty());
                self.reached_owner_count = 2;
                self.secondary_topology = Some(secondary_topology);
                Ok(())
            }
            super::test_support::AdversarialQuarantineTopologyForTest::ForeignFailureCutOwner(
                secondary,
            ) => {
                let mut secondary_topology = self.checkout_secondary_for_test(secondary)?;
                let secondary_owners =
                    secondary_topology.take_connection_owners_for_adversarial_test();
                if primary_topology.connection_owner_count() != 1 || secondary_owners.len() != 1 {
                    return Err(
                        PersistentFailurePendingProjectionQuarantineReason::ConnectionIdentityMismatch,
                    );
                }
                let primary_owners = primary_topology
                    .replace_connection_owners_for_adversarial_test(secondary_owners);
                let displaced = secondary_topology
                    .replace_connection_owners_for_adversarial_test(primary_owners);
                debug_assert!(displaced.is_empty());
                self.reached_owner_count = 2;
                self.secondary_topology = Some(secondary_topology);
                Ok(())
            }
        }
    }

    #[cfg(test)]
    fn checkout_secondary_for_test(
        &mut self,
        secondary: crate::cas_projection::PersistentFailurePendingProjectionQuarantine,
    ) -> Result<PendingProjectionAdoptionTopology, PersistentFailurePendingProjectionQuarantineReason>
    {
        let inventory = secondary.into_inventory();
        let cut = inventory.cut_identity();
        self.secondary_inventory = Some(inventory);
        let checkout = self
            .secondary_inventory
            .as_ref()
            .expect("the adversarial checkout retains its secondary inventory")
            .checkout_pending_quarantine_for_adoption()?;
        let (topology, fence) = checkout.into_parts();
        self.secondary_connections = topology
            .connection_owners()
            .iter()
            .map(|owner| Arc::clone(owner.stable_connection_for_inert_failure()))
            .collect();
        self.secondary_inert_attachments = Vec::with_capacity(self.secondary_connections.len());
        self.secondary_cut = Some(cut);
        self.secondary_fence = Some(fence);
        Ok(topology)
    }

    #[cfg(test)]
    pub(in crate::cas_projection) const fn adversary_was_applied_for_test(&self) -> bool {
        self.adversary_applied
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn reached_owner_count_for_test(&self) -> usize {
        self.reached_owner_count
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn retained_owner_count_for_test(&self) -> usize {
        self.secondary_topology
            .as_ref()
            .map_or(0, PendingProjectionAdoptionTopology::connection_owner_count)
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn secondary_connection_count_for_test(&self) -> usize {
        self.secondary_connections.len()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn secondary_inert_attachment_count_for_test(&self) -> usize {
        self.secondary_inert_attachments.len()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn secondary_inventory_reescrow_is_disarmed_for_test(
        &self,
    ) -> bool {
        self.secondary_inventory
            .as_ref()
            .is_none_or(crate::cas_projection::PersistentFailureRecoveryInventory::reescrow_is_disarmed_for_test)
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn secondary_connections_are_inert_for_test(&self) -> bool {
        self.secondary_connections
            .iter()
            .all(|connection| connection.forwarding_epoch_is_inert_and_detached_for_test())
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn owns_secondary_connection_for_test(
        &self,
        candidate: &Arc<ProjectionConnection>,
    ) -> bool {
        self.secondary_connections
            .iter()
            .any(|connection| Arc::ptr_eq(connection, candidate))
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn make_adversarial_connections_inert_for_test(&mut self) {
        if self.secondary_terminalized {
            return;
        }
        if let Some(cut) = self.secondary_cut {
            for connection in &self.secondary_connections {
                let attachment = connection.make_adoption_inert_in_place(cut);
                if !attachment.is_empty() {
                    self.secondary_inert_attachments.push(attachment);
                }
            }
        }
        if let Some(inventory) = self.secondary_inventory.as_mut() {
            inventory.disarm_reescrow_after_terminal_inert();
        }
        self.secondary_terminalized = true;
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn dispose_adversarial_connections_for_test(&mut self) -> bool {
        self.make_adversarial_connections_inert_for_test();
        let mut failed = false;
        for attachment in self.secondary_inert_attachments.drain(..) {
            failed |= attachment.dispose_after_adoption_failure().is_err();
        }
        for connection in &self.secondary_connections {
            failed |= connection
                .dispose_inert_driver_after_adoption_failure()
                .is_err();
        }
        failed
    }
}

impl PersistentFailureAdoptionFence {
    fn new(escrow: Arc<PendingProjectionAdoptionEscrow>) -> Self {
        Self { escrow }
    }

    fn escrow(&self) -> &Arc<PendingProjectionAdoptionEscrow> {
        &self.escrow
    }
}

impl PersistentFailureAdoptionFenceRetirementError {
    pub(in crate::cas_projection) const fn reason(
        &self,
    ) -> PersistentFailurePendingProjectionQuarantineReason {
        self.reason
    }

    pub(in crate::cas_projection) fn into_fence(self) -> PersistentFailureAdoptionFence {
        self.fence
    }
}

impl PersistentFailureAdoptionRetirementWitness {
    #[allow(dead_code, reason = "Phase 86 consumes the retirement witness")]
    pub(in crate::cas_projection) const fn cut_identity(&self) -> PersistentFailureCutIdentity {
        self.cut
    }
}

#[derive(Default)]
pub(in crate::cas_projection::persistent_failure) struct PersistentFailureRecoveryDrain {
    pub(in crate::cas_projection::persistent_failure) retained_connections:
        Vec<Arc<ProjectionConnection>>,
    pub(in crate::cas_projection::persistent_failure) retained_results:
        Vec<PersistentFailureRetainedTarget>,
    pub(in crate::cas_projection::persistent_failure) retained_projections:
        Vec<LoadedCasProjection>,
    pub(in crate::cas_projection::persistent_failure) retained_target_projections:
        Vec<LoadedCasProjection>,
    pub(in crate::cas_projection::persistent_failure) retained_reacquisition_anchors:
        Vec<SameNativeReacquisitionAnchor>,
    pub(in crate::cas_projection::persistent_failure) retained_raw_loaded_leases:
        Vec<FailureRetainedRawLoadedLease>,
    pub(in crate::cas_projection::persistent_failure) retained_raw_quarantined_anchors:
        Vec<FailureRetainedRawQuarantinedAnchor>,
    pub(in crate::cas_projection::persistent_failure) retained_raw_reacquisition_reservations:
        Vec<FailureRetainedRawReacquisitionReservation>,
    pub(in crate::cas_projection::persistent_failure) retained_promotion_reservations:
        Vec<FailureRetainedPromotionReservation>,
    pub(in crate::cas_projection::persistent_failure) retained_cleanup_owners:
        Vec<FailureRetainedCleanupOwner>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection::persistent_failure) enum PersistentFailureRecoveryDrainError {
    NotStable,
    Poisoned,
}

impl PersistentFailureRecoveryDrain {
    pub(super) fn counts(&self) -> PersistentFailureRecoveryInventoryCounts {
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

    pub(in crate::cas_projection::persistent_failure) fn local_disposition_count(&self) -> usize {
        self.retained_target_projections
            .len()
            .checked_add(self.retained_reacquisition_anchors.len())
            .and_then(|count| count.checked_add(self.retained_raw_loaded_leases.len()))
            .and_then(|count| count.checked_add(self.retained_raw_quarantined_anchors.len()))
            .and_then(|count| count.checked_add(self.retained_raw_reacquisition_reservations.len()))
            .and_then(|count| count.checked_add(self.retained_promotion_reservations.len()))
            .and_then(|count| count.checked_add(self.retained_cleanup_owners.len()))
            .and_then(|count| count.checked_add(self.retained_results.len()))
            .expect("bounded retained disposition counts fit in memory")
    }
}

mod core;
