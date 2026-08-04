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

impl CoordinatorState {
    fn take_recovery_drain(&mut self) -> PersistentFailureRecoveryDrain {
        PersistentFailureRecoveryDrain {
            retained_connections: std::mem::take(&mut self.retained_connections),
            retained_results: std::mem::take(&mut self.retained_results),
            retained_projections: std::mem::take(&mut self.retained_projections),
            retained_target_projections: std::mem::take(&mut self.retained_target_projections),
            retained_reacquisition_anchors: std::mem::take(
                &mut self.retained_reacquisition_anchors,
            ),
            retained_raw_loaded_leases: std::mem::take(&mut self.retained_raw_loaded_leases),
            retained_raw_quarantined_anchors: std::mem::take(
                &mut self.retained_raw_quarantined_anchors,
            ),
            retained_raw_reacquisition_reservations: std::mem::take(
                &mut self.retained_raw_reacquisition_reservations,
            ),
            retained_promotion_reservations: std::mem::take(
                &mut self.retained_promotion_reservations,
            ),
            retained_cleanup_owners: std::mem::take(&mut self.retained_cleanup_owners),
        }
    }
}

impl PersistentFailureCoordinator {
    pub(in crate::cas_projection::persistent_failure) fn checkout_pending_quarantine_for_adoption(
        &self,
        identity: PersistentFailureCutIdentity,
    ) -> Result<PendingProjectionAdoptionCheckout, PersistentFailurePendingProjectionQuarantineReason>
    {
        let escrow = PendingProjectionAdoptionEscrow::new();
        let mut state = self.state.0.lock().map_err(|_| {
            PersistentFailurePendingProjectionQuarantineReason::RetentionUnavailable
        })?;
        if state.phase != PersistentFailureCutState::Finished
            || state.failure_generation != Some(identity.failure_generation)
            || self.service_generation != identity.service_generation
            || state.late_publication_count != 0
        {
            return Err(PersistentFailurePendingProjectionQuarantineReason::CutIdentityMismatch);
        }
        let previous = std::mem::replace(
            &mut state.pending_quarantine,
            PendingQuarantineStage::AdoptionCheckedOut(Arc::clone(&escrow)),
        );
        let PendingQuarantineStage::Installed(authority) = previous else {
            state.pending_quarantine = previous;
            return Err(PersistentFailurePendingProjectionQuarantineReason::InventoryNotPromotable);
        };
        if authority.reason.is_some()
            || !matches!(
                &authority.topology,
                PendingProjectionQuarantineOwnedTopology::Normalized { .. }
            )
        {
            state.pending_quarantine = PendingQuarantineStage::Installed(authority);
            return Err(PersistentFailurePendingProjectionQuarantineReason::InventoryNotPromotable);
        }
        let PendingProjectionQuarantineOwnedTopology::Normalized {
            groups,
            connection_owners,
            remainder,
            pending_local_dispositions,
            settled_disposition_count,
        } = authority.topology
        else {
            unreachable!("the adoption checkout validated normalized quarantine topology")
        };
        Ok(PendingProjectionAdoptionCheckout {
            topology: Some(PendingProjectionAdoptionTopology::from_normalized(
                groups,
                connection_owners,
                remainder,
                pending_local_dispositions,
                settled_disposition_count,
            )),
            fence: Some(PersistentFailureAdoptionFence::new(escrow)),
        })
    }

    pub(in crate::cas_projection::persistent_failure) fn commit_pending_quarantine_adoption<R>(
        &self,
        identity: PersistentFailureCutIdentity,
        fence: &PersistentFailureAdoptionFence,
        commit: impl FnOnce() -> R,
    ) -> Result<R, PersistentFailurePendingProjectionQuarantineReason> {
        let escrow = fence.escrow();
        let mut state = self.state.0.lock().map_err(|_| {
            PersistentFailurePendingProjectionQuarantineReason::RetentionUnavailable
        })?;
        let late = escrow.late.lock().map_err(|_| {
            PersistentFailurePendingProjectionQuarantineReason::RetentionUnavailable
        })?;
        if state.phase != PersistentFailureCutState::Finished
            || state.failure_generation != Some(identity.failure_generation)
            || self.service_generation != identity.service_generation
            || state.late_publication_count != 0
            || !matches!(
                &state.pending_quarantine,
                PendingQuarantineStage::AdoptionCheckedOut(current)
                    if Arc::ptr_eq(current, escrow)
            )
            || late.publications.counts() != Default::default()
            || !late.authorities.is_empty()
        {
            return Err(PersistentFailurePendingProjectionQuarantineReason::LatePublication);
        }
        state.pending_quarantine = PendingQuarantineStage::Adopted(Arc::clone(escrow));
        Ok(commit())
    }

    /// Atomically retires the old-cut publication fence after every old source is quiescent.
    /// Process publication must later consume the returned witness outside these locks.
    #[allow(dead_code, reason = "Phase 86 consumes the Phase 82 publication fence")]
    pub(in crate::cas_projection::persistent_failure) fn retire_pending_quarantine_adoption(
        &self,
        identity: PersistentFailureCutIdentity,
        fence: PersistentFailureAdoptionFence,
    ) -> Result<
        PersistentFailureAdoptionRetirementWitness,
        PersistentFailureAdoptionFenceRetirementError,
    > {
        let validation = (|| {
            let escrow = fence.escrow();
            let mut state = self.state.0.lock().map_err(|_| {
                PersistentFailurePendingProjectionQuarantineReason::RetentionUnavailable
            })?;
            let late = escrow.late.lock().map_err(|_| {
                PersistentFailurePendingProjectionQuarantineReason::RetentionUnavailable
            })?;
            if state.phase != PersistentFailureCutState::Finished
                || state.failure_generation != Some(identity.failure_generation)
                || self.service_generation != identity.service_generation
                || state.late_publication_count != 0
                || !matches!(
                    &state.pending_quarantine,
                    PendingQuarantineStage::Adopted(current) if Arc::ptr_eq(current, escrow)
                )
                || late.publications.counts() != Default::default()
                || !late.authorities.is_empty()
            {
                return Err(PersistentFailurePendingProjectionQuarantineReason::LatePublication);
            }
            state.pending_quarantine = PendingQuarantineStage::AdoptionRetired(Arc::clone(escrow));
            Ok(())
        })();
        match validation {
            Ok(()) => Ok(PersistentFailureAdoptionRetirementWitness {
                cut: identity,
                escrow: fence.escrow,
            }),
            Err(reason) => Err(PersistentFailureAdoptionFenceRetirementError { reason, fence }),
        }
    }

    #[cfg(test)]
    pub(in crate::cas_projection::persistent_failure) fn retain_late_adoption_authority_for_test(
        &self,
        identity: PersistentFailureCutIdentity,
    ) {
        let authority = PendingProjectionQuarantineAuthority {
            topology: PendingProjectionQuarantineOwnedTopology::Inert {
                drain: PersistentFailureRecoveryDrain::default(),
            },
            reason: Some(PersistentFailurePendingProjectionQuarantineReason::LatePublication),
        };
        let _ = self.install_pending_quarantine(identity, authority);
    }

    pub(in crate::cas_projection::persistent_failure) fn checkout_recovery_drain(
        &self,
        identity: PersistentFailureCutIdentity,
        sealed_counts: PersistentFailureRecoveryInventoryCounts,
    ) -> Result<PersistentFailureRecoveryDrain, PersistentFailureRecoveryDrainError> {
        let mut state = self
            .state
            .0
            .lock()
            .map_err(|_| PersistentFailureRecoveryDrainError::Poisoned)?;
        if state.phase != PersistentFailureCutState::Finished
            || state.failure_generation != Some(identity.failure_generation)
            || self.service_generation != identity.service_generation
            || state.sealed_counts != Some(sealed_counts)
            || state.recovery_inventory_counts() != sealed_counts
            || state.late_publication_count != 0
            || !state.pending_quarantine.is_available()
        {
            return Err(PersistentFailureRecoveryDrainError::NotStable);
        }
        state.pending_quarantine = PendingQuarantineStage::ConversionCheckedOut;
        Ok(state.take_recovery_drain())
    }

    pub(in crate::cas_projection::persistent_failure) fn install_pending_quarantine(
        &self,
        identity: PersistentFailureCutIdentity,
        mut authority: PendingProjectionQuarantineAuthority,
    ) -> Option<PersistentFailurePendingProjectionQuarantineReason> {
        let (mut state, retention_poisoned) = match self.state.0.lock() {
            Ok(state) => (state, false),
            Err(poison) => (poison.into_inner(), true),
        };
        let mut reason = authority.reason;
        if retention_poisoned {
            reason.get_or_insert(
                PersistentFailurePendingProjectionQuarantineReason::RetentionUnavailable,
            );
        }
        if state.phase != PersistentFailureCutState::Finished
            || state.failure_generation != Some(identity.failure_generation)
            || self.service_generation != identity.service_generation
            || !matches!(
                &state.pending_quarantine,
                PendingQuarantineStage::ConversionCheckedOut
            )
        {
            reason.get_or_insert(
                PersistentFailurePendingProjectionQuarantineReason::CutIdentityMismatch,
            );
        }
        let late_drain = (state.recovery_inventory_counts() != Default::default())
            .then(|| state.take_recovery_drain());
        if state.late_publication_count != 0 || late_drain.is_some() {
            reason
                .get_or_insert(PersistentFailurePendingProjectionQuarantineReason::LatePublication);
        }
        authority.reason = reason;
        let previous = std::mem::replace(
            &mut state.pending_quarantine,
            PendingQuarantineStage::Available,
        );
        state.pending_quarantine = match previous {
            PendingQuarantineStage::ConversionCheckedOut | PendingQuarantineStage::Available => {
                PendingQuarantineStage::Installed(authority)
            }
            PendingQuarantineStage::Installed(existing) => {
                PendingQuarantineStage::Conflicted(vec![existing, authority])
            }
            PendingQuarantineStage::AdoptionCheckedOut(escrow) => {
                escrow.retain_authority(authority);
                PendingQuarantineStage::AdoptionCheckedOut(escrow)
            }
            PendingQuarantineStage::Adopted(escrow) => {
                escrow.retain_authority(authority);
                PendingQuarantineStage::Adopted(escrow)
            }
            PendingQuarantineStage::AdoptionRetired(escrow) => {
                escrow.retain_authority(authority);
                PendingQuarantineStage::AdoptionRetired(escrow)
            }
            PendingQuarantineStage::TerminalDispositionCheckedOut(escrow) => {
                escrow.retain_authority(authority);
                PendingQuarantineStage::TerminalDispositionCheckedOut(escrow)
            }
            PendingQuarantineStage::TerminalDispositionComplete(escrow) => {
                escrow.retain_authority(authority);
                PendingQuarantineStage::TerminalDispositionComplete(escrow)
            }
            PendingQuarantineStage::Conflicted(mut authorities) => {
                authorities.push(authority);
                PendingQuarantineStage::Conflicted(authorities)
            }
        };
        if let Some(late_drain) = late_drain {
            state.pending_quarantine.retain_late_drain(late_drain);
        }
        reason
    }

    pub(in crate::cas_projection::persistent_failure) fn pending_quarantine_metadata(
        &self,
    ) -> PersistentFailurePendingProjectionQuarantineMetadata {
        let (state, retention_poisoned) = match self.state.0.lock() {
            Ok(state) => (state, false),
            Err(poison) => (poison.into_inner(), true),
        };
        let (authorities, conflicted): (&[_], bool) = match &state.pending_quarantine {
            PendingQuarantineStage::Available
            | PendingQuarantineStage::ConversionCheckedOut
            | PendingQuarantineStage::AdoptionCheckedOut(_)
            | PendingQuarantineStage::Adopted(_)
            | PendingQuarantineStage::AdoptionRetired(_)
            | PendingQuarantineStage::TerminalDispositionCheckedOut(_)
            | PendingQuarantineStage::TerminalDispositionComplete(_) => {
                return PersistentFailurePendingProjectionQuarantineMetadata {
                    late_publication_count: state.late_publication_count,
                    ..Default::default()
                };
            }
            PendingQuarantineStage::Installed(authority) => {
                (std::slice::from_ref(authority), false)
            }
            PendingQuarantineStage::Conflicted(authorities) => (authorities, true),
        };
        let mut group_count = 0usize;
        let mut candidate_count = 0usize;
        let mut retained_connection_count = 0usize;
        let mut local_disposition_count = 0usize;
        let mut authority_promotable = !conflicted;
        for authority in authorities {
            authority_promotable &= authority.reason.is_none();
            let (groups, candidates, connections, dispositions) = match &authority.topology {
                PendingProjectionQuarantineOwnedTopology::Normalized {
                    groups,
                    connection_owners,
                    remainder,
                    pending_local_dispositions,
                    settled_disposition_count,
                } => {
                    authority_promotable &=
                        connection_owners.iter().all(|owner| owner.is_promotable());
                    (
                        groups.len(),
                        groups.iter().map(|group| group.candidates.len()).sum(),
                        remainder.retained_connections.len(),
                        remainder
                            .local_disposition_count()
                            .checked_add(pending_local_dispositions.len())
                            .expect("bounded pending local disposition counts fit in memory")
                            .checked_add(*settled_disposition_count)
                            .expect("bounded normalized disposition counts fit in memory"),
                    )
                }
                PendingProjectionQuarantineOwnedTopology::Inert { drain } => (
                    0,
                    0,
                    drain.retained_connections.len(),
                    drain
                        .local_disposition_count()
                        .checked_add(drain.retained_projections.len())
                        .expect("bounded inert disposition counts fit in memory"),
                ),
            };
            group_count = group_count
                .checked_add(groups)
                .expect("bounded quarantine group counts fit in memory");
            candidate_count = candidate_count
                .checked_add(candidates)
                .expect("bounded quarantine candidate counts fit in memory");
            retained_connection_count = retained_connection_count
                .checked_add(connections)
                .expect("bounded retained connection counts fit in memory");
            local_disposition_count = local_disposition_count
                .checked_add(dispositions)
                .expect("bounded local disposition counts fit in memory");
        }
        PersistentFailurePendingProjectionQuarantineMetadata {
            group_count,
            candidate_count,
            retained_connection_count,
            local_disposition_count,
            late_publication_count: state.late_publication_count,
            promotable: authority_promotable
                && state.late_publication_count == 0
                && !retention_poisoned,
        }
    }
}
