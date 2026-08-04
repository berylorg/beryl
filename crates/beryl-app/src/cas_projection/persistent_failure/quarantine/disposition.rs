use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::cas_projection::{
    connection::{InertConnectionEpochAttachment, PendingProjectionConnectionOwner},
    persistent_failure::{
        PersistentFailureCutIdentity, PersistentFailureRecoveryInventory,
        coordinator::{
            PendingProjectionTerminalDispositionFence, PersistentFailureRecoveryDrain,
            PersistentFailureTerminalDispositionCoordinatorWitness,
            PersistentFailureTerminalDispositionDrain,
        },
        retention::{
            PersistentFailureTerminalDispositionWitness, PersistentFailureTerminalRetirementError,
        },
    },
};

use super::{
    PendingProjectionQuarantineAuthority, PendingProjectionQuarantineOwnedTopology,
    PersistentFailurePendingProjectionQuarantine,
    PersistentFailurePendingProjectionQuarantineReason,
};

#[derive(Clone, Copy)]
enum TerminalDispositionFailure {
    Checkout,
    Synchronization,
    LocalAuthority,
    Connection,
    OldEpoch,
    FinalFence,
}

#[must_use]
struct PersistentFailureTerminalDispositionError {
    _inventory: Option<PersistentFailureRecoveryInventory>,
    _fence: Option<PendingProjectionTerminalDispositionFence>,
    _coordinator_witness: Option<PersistentFailureTerminalDispositionCoordinatorWitness>,
    _terminal_witness: Option<PersistentFailureTerminalDispositionWitness>,
    _orphaned_authorities: Vec<PendingProjectionQuarantineAuthority>,
    _reason: TerminalDispositionFailure,
}

struct TerminalDispositionState {
    inventory: Option<PersistentFailureRecoveryInventory>,
    fence: Option<PendingProjectionTerminalDispositionFence>,
    coordinator_witness: Option<PersistentFailureTerminalDispositionCoordinatorWitness>,
    connection_owners: Vec<PendingProjectionConnectionOwner>,
    orphaned_authorities: Vec<PendingProjectionQuarantineAuthority>,
    first_failure: Option<TerminalDispositionFailure>,
}

impl PersistentFailurePendingProjectionQuarantine {
    /// Consumes pre-adoption quarantine authority during final supervisor shutdown.
    ///
    /// Success is reported only after every reachable owner is terminally settled and the exact
    /// retained-service escrow has been removed without closing the supervisor-owned home.
    pub(in crate::cas_projection) fn dispose_for_supervisor_shutdown(self) -> Result<(), ()> {
        match dispose_terminal_quarantine(self) {
            Ok(witness) => {
                let _cut = witness.cut_identity();
                Ok(())
            }
            Err(_owning_error) => Err(()),
        }
    }
}

fn dispose_terminal_quarantine(
    quarantine: PersistentFailurePendingProjectionQuarantine,
) -> Result<PersistentFailureTerminalDispositionWitness, PersistentFailureTerminalDispositionError>
{
    let inventory = quarantine.into_inventory();
    let fence = match inventory.checkout_pending_quarantine_for_terminal_disposition() {
        Ok(fence) => fence,
        Err(_) => {
            return Err(PersistentFailureTerminalDispositionError {
                _inventory: Some(inventory),
                _fence: None,
                _coordinator_witness: None,
                _terminal_witness: None,
                _orphaned_authorities: Vec::new(),
                _reason: TerminalDispositionFailure::Checkout,
            });
        }
    };
    let mut state = TerminalDispositionState {
        inventory: Some(inventory),
        fence: Some(fence),
        coordinator_witness: None,
        connection_owners: Vec::new(),
        orphaned_authorities: Vec::new(),
        first_failure: None,
    };

    state.drain_terminal_escrow();
    state.dispose_connections();
    state.drain_terminal_escrow();
    state.connection_owners.clear();

    let fence = state
        .fence
        .take()
        .expect("an active terminal disposition retains its coordinator fence");
    match state.inventory().complete_terminal_disposition(fence) {
        Ok(witness) => state.coordinator_witness = Some(witness),
        Err(fence) => {
            state.fence = Some(fence);
            state.note_failure(TerminalDispositionFailure::FinalFence);
            return Err(state.into_error());
        }
    }

    let inventory = state
        .inventory
        .take()
        .expect("terminal retirement consumes the retained inventory once");
    let coordinator_witness = state
        .coordinator_witness
        .take()
        .expect("terminal retirement consumes the coordinator witness once");
    match inventory.complete_terminal_retirement(coordinator_witness) {
        Ok(witness) if state.first_failure.is_none() => Ok(witness),
        Ok(witness) => Err(state.into_error_with_terminal_witness(witness)),
        Err(error) => {
            state.restore_terminal_retirement_error(error);
            state.note_failure(TerminalDispositionFailure::OldEpoch);
            Err(state.into_error())
        }
    }
}

impl TerminalDispositionState {
    fn inventory(&self) -> &PersistentFailureRecoveryInventory {
        self.inventory
            .as_ref()
            .expect("an active terminal disposition retains its recovery inventory")
    }

    fn note_failure(&mut self, failure: TerminalDispositionFailure) {
        self.first_failure.get_or_insert(failure);
    }

    fn drain_terminal_escrow(&mut self) {
        let cut = self.inventory().cut_identity();
        let result = self.inventory().drain_terminal_disposition_escrow(
            self.fence
                .as_ref()
                .expect("an active terminal disposition retains its coordinator fence"),
        );
        let PersistentFailureTerminalDispositionDrain {
            publications,
            mut authorities,
            synchronization_poisoned,
        } = match result {
            Ok(drain) => drain,
            Err(_) => {
                self.note_failure(TerminalDispositionFailure::Synchronization);
                return;
            }
        };
        if synchronization_poisoned {
            self.note_failure(TerminalDispositionFailure::Synchronization);
        }
        authorities.push(PendingProjectionQuarantineAuthority {
            topology: PendingProjectionQuarantineOwnedTopology::Inert {
                drain: publications,
            },
            reason: Some(PersistentFailurePendingProjectionQuarantineReason::LatePublication),
        });
        for authority in authorities {
            let disposition = catch_unwind(AssertUnwindSafe(|| settle_authority(authority, cut)));
            match disposition {
                Ok((owners, None)) => self.connection_owners.extend(owners),
                Ok((owners, Some(authority))) => {
                    self.connection_owners.extend(owners);
                    let retained = self.inventory().retain_terminal_disposition_authority(
                        self.fence
                            .as_ref()
                            .expect("terminal disposition retains its coordinator fence"),
                        authority,
                    );
                    self.note_failure(TerminalDispositionFailure::LocalAuthority);
                    match retained {
                        Ok(true) => {
                            self.note_failure(TerminalDispositionFailure::Synchronization);
                        }
                        Ok(false) => {}
                        Err(authority) => {
                            self.orphaned_authorities.push(authority);
                            self.note_failure(TerminalDispositionFailure::Synchronization);
                        }
                    }
                }
                Err(_) => self.note_failure(TerminalDispositionFailure::LocalAuthority),
            }
        }
    }

    fn dispose_connections(&mut self) {
        let cut = self.inventory().cut_identity();
        let mut connections = self.inventory().retained_service_connections();
        connections.sort_unstable_by_key(|connection| {
            connection.identity_observation().connection_generation()
        });
        let mut attachments =
            Vec::<InertConnectionEpochAttachment>::with_capacity(connections.len());
        for connection in &connections {
            match catch_unwind(AssertUnwindSafe(|| connection.make_adoption_inert(cut))) {
                Ok(attachment) => attachments.push(attachment),
                Err(_) => self.note_failure(TerminalDispositionFailure::Connection),
            }
        }
        self.connection_owners.clear();
        for attachment in attachments {
            let failed = catch_unwind(AssertUnwindSafe(|| {
                attachment.dispose_after_adoption_failure()
            }))
            .map_or(true, |result| result.is_err());
            if failed {
                self.note_failure(TerminalDispositionFailure::Connection);
            }
        }
        for connection in connections {
            let failed = catch_unwind(AssertUnwindSafe(|| {
                connection.dispose_inert_driver_after_adoption_failure()
            }))
            .map_or(true, |result| result.is_err());
            if failed {
                self.note_failure(TerminalDispositionFailure::Connection);
            }
        }
    }

    fn restore_terminal_retirement_error(
        &mut self,
        error: PersistentFailureTerminalRetirementError,
    ) {
        let (_reason, inventory, witness) = error.into_parts();
        self.inventory = Some(inventory);
        self.coordinator_witness = Some(witness);
    }

    fn into_error(self) -> PersistentFailureTerminalDispositionError {
        PersistentFailureTerminalDispositionError {
            _inventory: self.inventory,
            _fence: self.fence,
            _coordinator_witness: self.coordinator_witness,
            _terminal_witness: None,
            _orphaned_authorities: self.orphaned_authorities,
            _reason: self
                .first_failure
                .unwrap_or(TerminalDispositionFailure::FinalFence),
        }
    }

    fn into_error_with_terminal_witness(
        self,
        witness: PersistentFailureTerminalDispositionWitness,
    ) -> PersistentFailureTerminalDispositionError {
        PersistentFailureTerminalDispositionError {
            _inventory: self.inventory,
            _fence: self.fence,
            _coordinator_witness: self.coordinator_witness,
            _terminal_witness: Some(witness),
            _orphaned_authorities: self.orphaned_authorities,
            _reason: self
                .first_failure
                .unwrap_or(TerminalDispositionFailure::FinalFence),
        }
    }
}

fn settle_authority(
    authority: PendingProjectionQuarantineAuthority,
    cut: PersistentFailureCutIdentity,
) -> (
    Vec<PendingProjectionConnectionOwner>,
    Option<PendingProjectionQuarantineAuthority>,
) {
    let mut connection_owners = Vec::new();
    let remaining = match authority.topology {
        PendingProjectionQuarantineOwnedTopology::Normalized {
            groups,
            connection_owners: owners,
            remainder,
            pending_local_dispositions,
            settled_disposition_count: _,
        } => {
            for group in groups {
                let (_, _, candidates) = group.into_parts();
                for candidate in candidates {
                    drop(candidate.dispose_local());
                }
            }
            drop(pending_local_dispositions);
            connection_owners = owners;
            settle_recovery_drain(remainder, cut)
        }
        PendingProjectionQuarantineOwnedTopology::Inert { drain } => {
            settle_recovery_drain(drain, cut)
        }
    };
    let remaining = remaining.map(|drain| PendingProjectionQuarantineAuthority {
        topology: PendingProjectionQuarantineOwnedTopology::Inert { drain },
        reason: Some(PersistentFailurePendingProjectionQuarantineReason::RetentionUnavailable),
    });
    (connection_owners, remaining)
}

fn settle_recovery_drain(
    mut drain: PersistentFailureRecoveryDrain,
    cut: PersistentFailureCutIdentity,
) -> Option<PersistentFailureRecoveryDrain> {
    drain.retained_connections.clear();
    drain.retained_results.clear();
    for projection in std::mem::take(&mut drain.retained_projections) {
        drop(
            projection
                .into_pending_projection_lease_owner()
                .dispose_local(),
        );
    }
    for projection in std::mem::take(&mut drain.retained_target_projections) {
        drop(projection.into_local_registry_disposition_owner());
    }
    for anchor in std::mem::take(&mut drain.retained_reacquisition_anchors) {
        match anchor.into_local_registry_disposition_owner() {
            Ok(owner) => drop(owner),
            Err(anchor) => drop(anchor),
        }
    }
    for retained in std::mem::take(&mut drain.retained_raw_loaded_leases) {
        drop(retained.into_local_registry_disposition_owner());
    }
    for retained in std::mem::take(&mut drain.retained_raw_quarantined_anchors) {
        drop(retained.into_local_registry_disposition_owner());
    }
    for retained in std::mem::take(&mut drain.retained_raw_reacquisition_reservations) {
        drop(retained.into_local_registry_disposition_owner());
    }

    let mut failed_promotions = Vec::new();
    for owner in std::mem::take(&mut drain.retained_promotion_reservations) {
        match owner.consume_for_recovery(cut) {
            Ok(connection) => drop(connection),
            Err(owner) => failed_promotions.push(owner),
        }
    }
    drain.retained_promotion_reservations = failed_promotions;
    let mut failed_cleanup = Vec::new();
    for owner in std::mem::take(&mut drain.retained_cleanup_owners) {
        match owner.consume_for_recovery(cut) {
            Ok(connection) => drop(connection),
            Err(owner) => failed_cleanup.push(owner),
        }
    }
    drain.retained_cleanup_owners = failed_cleanup;
    if drain.retained_promotion_reservations.is_empty() && drain.retained_cleanup_owners.is_empty()
    {
        None
    } else {
        Some(drain)
    }
}
