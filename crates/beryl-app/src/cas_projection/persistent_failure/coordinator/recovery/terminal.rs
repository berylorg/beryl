use super::*;

/// Non-cloneable fence for the dedicated pre-adoption terminal-disposition stage.
#[must_use = "terminal disposition must drain and complete its exact late-owner escrow"]
pub(in crate::cas_projection::persistent_failure) struct PendingProjectionTerminalDispositionFence {
    escrow: Arc<PendingProjectionTerminalDispositionEscrow>,
    synchronization_poisoned: bool,
}

/// One-shot coordinator proof that terminal disposition consumed every reachable old-cut owner.
#[must_use = "the retained-service owner must consume this proof before reporting shutdown"]
pub(in crate::cas_projection::persistent_failure) struct PersistentFailureTerminalDispositionCoordinatorWitness
{
    cut: PersistentFailureCutIdentity,
    escrow: Arc<PendingProjectionTerminalDispositionEscrow>,
    synchronization_poisoned: bool,
}

pub(in crate::cas_projection::persistent_failure) struct PersistentFailureTerminalDispositionDrain {
    pub(in crate::cas_projection::persistent_failure) publications: PersistentFailureRecoveryDrain,
    pub(in crate::cas_projection::persistent_failure) authorities:
        Vec<PendingProjectionQuarantineAuthority>,
    pub(in crate::cas_projection::persistent_failure) synchronization_poisoned: bool,
}

impl PersistentFailureTerminalDispositionCoordinatorWitness {
    pub(in crate::cas_projection::persistent_failure) const fn cut_identity(
        &self,
    ) -> PersistentFailureCutIdentity {
        self.cut
    }

    pub(in crate::cas_projection::persistent_failure) fn escrow_is_empty(&self) -> bool {
        let late = self
            .escrow
            .late
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        late.publications.counts() == Default::default() && late.authorities.is_empty()
    }

    pub(in crate::cas_projection::persistent_failure) const fn synchronization_poisoned(
        &self,
    ) -> bool {
        self.synchronization_poisoned
    }
}

impl PersistentFailureCoordinator {
    pub(in crate::cas_projection::persistent_failure) fn checkout_pending_quarantine_for_terminal_disposition(
        &self,
        identity: PersistentFailureCutIdentity,
    ) -> Result<
        PendingProjectionTerminalDispositionFence,
        PersistentFailurePendingProjectionQuarantineReason,
    > {
        // The Arc and the one-authority fast-path storage are reserved before the coordinator
        // lock. Moving Installed or Conflicted ownership into the fresh escrow is then allocation-
        // free while authority is fenced.
        let escrow = PendingProjectionTerminalDispositionEscrow::new();
        let (mut state, synchronization_poisoned) = match self.state.0.lock() {
            Ok(state) => (state, false),
            Err(poison) => (poison.into_inner(), true),
        };
        if state.phase != PersistentFailureCutState::Finished
            || state.failure_generation != Some(identity.failure_generation)
            || self.service_generation != identity.service_generation
        {
            return Err(PersistentFailurePendingProjectionQuarantineReason::CutIdentityMismatch);
        }
        let previous = std::mem::replace(
            &mut state.pending_quarantine,
            PendingQuarantineStage::Available,
        );
        let mut terminal = escrow
            .late
            .lock()
            .expect("a fresh terminal-disposition escrow cannot be poisoned");
        match previous {
            PendingQuarantineStage::Installed(authority) => terminal.authorities.push(authority),
            PendingQuarantineStage::Conflicted(authorities) => {
                terminal.authorities = authorities;
            }
            previous => {
                drop(terminal);
                state.pending_quarantine = previous;
                return Err(
                    PersistentFailurePendingProjectionQuarantineReason::InventoryNotPromotable,
                );
            }
        }
        drop(terminal);
        state.pending_quarantine =
            PendingQuarantineStage::TerminalDispositionCheckedOut(Arc::clone(&escrow));
        Ok(PendingProjectionTerminalDispositionFence {
            escrow,
            synchronization_poisoned,
        })
    }

    pub(in crate::cas_projection::persistent_failure) fn drain_terminal_disposition_escrow(
        &self,
        identity: PersistentFailureCutIdentity,
        fence: &PendingProjectionTerminalDispositionFence,
    ) -> Result<
        PersistentFailureTerminalDispositionDrain,
        PersistentFailurePendingProjectionQuarantineReason,
    > {
        let (state, state_poisoned) = match self.state.0.lock() {
            Ok(state) => (state, false),
            Err(poison) => (poison.into_inner(), true),
        };
        if state.phase != PersistentFailureCutState::Finished
            || state.failure_generation != Some(identity.failure_generation)
            || self.service_generation != identity.service_generation
            || !matches!(
                &state.pending_quarantine,
                PendingQuarantineStage::TerminalDispositionCheckedOut(current)
                    if Arc::ptr_eq(current, &fence.escrow)
            )
        {
            return Err(PersistentFailurePendingProjectionQuarantineReason::CutIdentityMismatch);
        }
        let (mut late, escrow_poisoned) = match fence.escrow.late.lock() {
            Ok(late) => (late, false),
            Err(poison) => (poison.into_inner(), true),
        };
        Ok(PersistentFailureTerminalDispositionDrain {
            publications: std::mem::take(&mut late.publications),
            authorities: std::mem::take(&mut late.authorities),
            synchronization_poisoned: fence.synchronization_poisoned
                || state_poisoned
                || escrow_poisoned,
        })
    }

    pub(in crate::cas_projection::persistent_failure) fn retain_terminal_disposition_authority(
        &self,
        identity: PersistentFailureCutIdentity,
        fence: &PendingProjectionTerminalDispositionFence,
        authority: PendingProjectionQuarantineAuthority,
    ) -> Result<bool, PendingProjectionQuarantineAuthority> {
        let (state, state_poisoned) = match self.state.0.lock() {
            Ok(state) => (state, false),
            Err(poison) => (poison.into_inner(), true),
        };
        if state.phase != PersistentFailureCutState::Finished
            || state.failure_generation != Some(identity.failure_generation)
            || self.service_generation != identity.service_generation
            || !matches!(
                &state.pending_quarantine,
                PendingQuarantineStage::TerminalDispositionCheckedOut(current)
                    if Arc::ptr_eq(current, &fence.escrow)
            )
        {
            return Err(authority);
        }
        let (mut late, escrow_poisoned) = match fence.escrow.late.lock() {
            Ok(late) => (late, false),
            Err(poison) => (poison.into_inner(), true),
        };
        late.authorities.push(authority);
        Ok(state_poisoned || escrow_poisoned)
    }

    pub(in crate::cas_projection::persistent_failure) fn complete_terminal_disposition(
        &self,
        identity: PersistentFailureCutIdentity,
        fence: PendingProjectionTerminalDispositionFence,
    ) -> Result<
        PersistentFailureTerminalDispositionCoordinatorWitness,
        PendingProjectionTerminalDispositionFence,
    > {
        let (mut state, state_poisoned) = match self.state.0.lock() {
            Ok(state) => (state, false),
            Err(poison) => (poison.into_inner(), true),
        };
        let (late, escrow_poisoned) = match fence.escrow.late.lock() {
            Ok(late) => (late, false),
            Err(poison) => (poison.into_inner(), true),
        };
        let matches = state.phase == PersistentFailureCutState::Finished
            && state.failure_generation == Some(identity.failure_generation)
            && self.service_generation == identity.service_generation
            && matches!(
                &state.pending_quarantine,
                PendingQuarantineStage::TerminalDispositionCheckedOut(current)
                    if Arc::ptr_eq(current, &fence.escrow)
            );
        let empty = late.publications.counts() == Default::default() && late.authorities.is_empty();
        drop(late);
        if !matches || !empty {
            return Err(fence);
        }
        state.pending_quarantine =
            PendingQuarantineStage::TerminalDispositionComplete(Arc::clone(&fence.escrow));
        Ok(PersistentFailureTerminalDispositionCoordinatorWitness {
            cut: identity,
            escrow: fence.escrow,
            synchronization_poisoned: fence.synchronization_poisoned
                || state_poisoned
                || escrow_poisoned,
        })
    }
}
