use std::sync::Mutex;

use super::{
    OutageAssemblyLimits, OutageBuffer, OutageBufferLimits, OutageConnectionIdentity, OutageTarget,
};
use crate::cas_projection::persistent_failure::PersistentFailureCutIdentity;

enum InventoryState {
    Pending(Vec<OutageConnectionIdentity>),
    Ready(OutageBuffer),
    Unavailable,
}

pub(in crate::cas_projection) struct OutageInventory {
    identity: PersistentFailureCutIdentity,
    limits: OutageBufferLimits,
    pub(in crate::cas_projection) assembly_limits: OutageAssemblyLimits,
    state: Mutex<InventoryState>,
}

pub(in crate::cas_projection) enum OutageInventoryAccess<'a> {
    Pending,
    Ready(&'a mut OutageBuffer),
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum OutageInventoryError {
    Identity,
    AlreadyPublished,
    Unrepresentable,
    Poisoned,
}

impl OutageInventory {
    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection) fn snapshot(&self) -> super::OutageCaptureSnapshot {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        match &*state {
            InventoryState::Pending(_) => super::OutageCaptureSnapshot {
                state: super::OutageCaptureState::Pending,
                targets: 0,
                facts: 0,
                gapped_targets: 0,
                encoded_bytes: 0,
            },
            InventoryState::Unavailable => super::OutageCaptureSnapshot {
                state: super::OutageCaptureState::Unavailable,
                targets: 0,
                facts: 0,
                gapped_targets: 0,
                encoded_bytes: 0,
            },
            InventoryState::Ready(buffer) => super::OutageCaptureSnapshot {
                state: super::OutageCaptureState::Ready,
                targets: buffer.targets.len(),
                facts: buffer.facts.len(),
                gapped_targets: buffer.targets.iter().filter(|state| state.gap).count(),
                encoded_bytes: buffer.encoded_bytes(),
            },
        }
    }

    pub(in crate::cas_projection) fn publish_frozen(
        &self,
        identity: PersistentFailureCutIdentity,
        targets: impl Iterator<Item = Result<Option<OutageTarget>, ()>>,
    ) -> Result<(), OutageInventoryError> {
        if !self.matches(identity) {
            return Err(OutageInventoryError::Identity);
        }
        let mut inventory = Vec::new();
        for target in targets {
            match target {
                Ok(Some(target))
                    if inventory.len() < self.limits.max_targets
                        && inventory.try_reserve_exact(1).is_ok() =>
                {
                    inventory.push(target)
                }
                Ok(None) => {}
                _ => {
                    self.retire();
                    return Err(OutageInventoryError::Unrepresentable);
                }
            }
        }
        self.publish(identity, &inventory)
    }

    pub(in crate::cas_projection) fn new(
        identity: PersistentFailureCutIdentity,
        limits: OutageBufferLimits,
        assembly_limits: OutageAssemblyLimits,
    ) -> Self {
        Self {
            identity,
            limits,
            assembly_limits,
            state: Mutex::new(InventoryState::Pending(Vec::new())),
        }
    }

    pub(in crate::cas_projection) fn matches(
        &self,
        identity: PersistentFailureCutIdentity,
    ) -> bool {
        self.identity == identity
    }

    pub(in crate::cas_projection) fn publish(
        &self,
        identity: PersistentFailureCutIdentity,
        targets: &[OutageTarget],
    ) -> Result<(), OutageInventoryError> {
        if !self.matches(identity) {
            return Err(OutageInventoryError::Identity);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| OutageInventoryError::Poisoned)?;
        if !matches!(*state, InventoryState::Pending(_)) {
            return Err(OutageInventoryError::AlreadyPublished);
        }
        // A failed publication is terminal; no subset or replacement inventory may follow it.
        let InventoryState::Pending(gaps) =
            std::mem::replace(&mut *state, InventoryState::Unavailable)
        else {
            unreachable!("pending checked above")
        };
        if targets
            .iter()
            .any(|target| target.connection().home_generation != identity.home_generation.get())
        {
            return Err(OutageInventoryError::Identity);
        }
        let mut buffer = OutageBuffer::new(self.limits, targets)
            .map_err(|_| OutageInventoryError::Unrepresentable)?;
        for connection in gaps {
            connection.record_gap(&mut buffer);
        }
        *state = InventoryState::Ready(buffer);
        Ok(())
    }

    pub(in crate::cas_projection) fn access<T>(
        &self,
        identity: PersistentFailureCutIdentity,
        consume: impl FnOnce(OutageInventoryAccess<'_>) -> T,
    ) -> T {
        if !self.matches(identity) {
            return consume(OutageInventoryAccess::Unavailable);
        }
        let Ok(mut state) = self.state.lock() else {
            return consume(OutageInventoryAccess::Unavailable);
        };
        consume(match &mut *state {
            InventoryState::Pending(_) => OutageInventoryAccess::Pending,
            InventoryState::Ready(buffer) => OutageInventoryAccess::Ready(buffer),
            InventoryState::Unavailable => OutageInventoryAccess::Unavailable,
        })
    }

    pub(in crate::cas_projection) fn retire(&self) {
        *self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = InventoryState::Unavailable;
    }

    pub(in crate::cas_projection) fn record_connection_loss(
        &self,
        identity: PersistentFailureCutIdentity,
        connection: OutageConnectionIdentity,
    ) {
        if !self.matches(identity) || connection.home_generation != identity.home_generation.get() {
            return;
        }
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        match &mut *state {
            InventoryState::Ready(buffer) => connection.record_gap(buffer),
            InventoryState::Pending(gaps) if !gaps.contains(&connection) => {
                if gaps.len() >= self.limits.max_targets
                    || gaps
                        .len()
                        .checked_add(1)
                        .and_then(|count| {
                            count.checked_mul(std::mem::size_of::<OutageConnectionIdentity>())
                        })
                        .is_none_or(|bytes| bytes > self.limits.max_encoded_bytes)
                    || gaps.try_reserve_exact(1).is_err()
                {
                    *state = InventoryState::Unavailable;
                } else {
                    gaps.push(connection);
                }
            }
            InventoryState::Pending(_) | InventoryState::Unavailable => {}
        }
    }
}
