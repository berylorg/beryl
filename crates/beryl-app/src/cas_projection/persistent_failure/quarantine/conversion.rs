mod normalization;
mod preflight;

#[cfg(test)]
fn checkout_observers() -> &'static std::sync::Mutex<
    std::collections::HashMap<
        PersistentFailureCutIdentity,
        (
            std::sync::mpsc::SyncSender<()>,
            std::sync::mpsc::Receiver<()>,
        ),
    >,
> {
    static OBSERVER: std::sync::OnceLock<
        std::sync::Mutex<
            std::collections::HashMap<
                PersistentFailureCutIdentity,
                (
                    std::sync::mpsc::SyncSender<()>,
                    std::sync::mpsc::Receiver<()>,
                ),
            >,
        >,
    > = std::sync::OnceLock::new();
    OBSERVER.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

use crate::cas_projection::persistent_failure::{
    PersistentFailureCutIdentity, PersistentFailureRecoveryInventory,
    coordinator::{PersistentFailureRecoveryDrain, PersistentFailureRecoveryDrainError},
};

use self::{normalization::normalize_recovery_topology, preflight::preflight_recovery_topology};
use super::{
    PendingProjectionQuarantineAuthority, PendingProjectionQuarantineOwnedTopology,
    PersistentFailurePendingProjectionQuarantine,
    PersistentFailurePendingProjectionQuarantineError,
    PersistentFailurePendingProjectionQuarantineReason,
};

impl PersistentFailureRecoveryInventory {
    #[cfg(test)]
    pub(in crate::cas_projection) fn observe_next_pending_quarantine_checkout_for_test(
        &self,
        reached: std::sync::mpsc::SyncSender<()>,
        resume: std::sync::mpsc::Receiver<()>,
    ) {
        let replaced = checkout_observers()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .insert(self.cut_identity(), (reached, resume));
        assert!(replaced.is_none());
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn observe_next_connection_owner_install_for_test(
        &self,
        reached: std::sync::mpsc::SyncSender<()>,
        resume: std::sync::mpsc::Receiver<()>,
    ) {
        normalization::observe_connection_owner_install_for_test(
            self.cut_identity(),
            reached,
            resume,
        );
    }

    /// Consumes one sealed cut inventory into bounded, non-executable local quarantine ownership.
    pub fn into_pending_projection_quarantine(
        self,
    ) -> Result<
        PersistentFailurePendingProjectionQuarantine,
        PersistentFailurePendingProjectionQuarantineError,
    > {
        let identity = self.cut_identity();
        let drain = match self.checkout_pending_quarantine_drain() {
            Ok(drain) => drain,
            Err(error) => {
                return Err(PersistentFailurePendingProjectionQuarantineError {
                    inventory: self,
                    reason: drain_error_reason(error),
                });
            }
        };
        #[cfg(test)]
        let checkout_observer = checkout_observers()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .remove(&identity);
        #[cfg(test)]
        if let Some((reached, resume)) = checkout_observer {
            let _ = reached.send(());
            let _ = resume.recv();
        }
        let preflight = match preflight_recovery_topology(&self, identity, &drain) {
            Ok(preflight) => preflight,
            Err(reason) => {
                return Err(install_inert_failure(self, identity, drain, reason));
            }
        };
        match normalize_recovery_topology(identity, drain, preflight) {
            Ok(authority) => {
                let install_reason = self.install_pending_quarantine(authority);
                if let Some(reason) = install_reason {
                    return Err(PersistentFailurePendingProjectionQuarantineError {
                        inventory: self,
                        reason,
                    });
                }
                Ok(PersistentFailurePendingProjectionQuarantine { inventory: self })
            }
            Err((authority, reason)) => {
                let reason = self.install_pending_quarantine(authority).unwrap_or(reason);
                Err(PersistentFailurePendingProjectionQuarantineError {
                    inventory: self,
                    reason,
                })
            }
        }
    }
}

fn drain_error_reason(
    error: PersistentFailureRecoveryDrainError,
) -> PersistentFailurePendingProjectionQuarantineReason {
    match error {
        PersistentFailureRecoveryDrainError::NotStable => {
            PersistentFailurePendingProjectionQuarantineReason::InventoryNotPromotable
        }
        PersistentFailureRecoveryDrainError::Poisoned => {
            PersistentFailurePendingProjectionQuarantineReason::RetentionUnavailable
        }
    }
}

fn install_inert_failure(
    inventory: PersistentFailureRecoveryInventory,
    _identity: PersistentFailureCutIdentity,
    drain: PersistentFailureRecoveryDrain,
    reason: PersistentFailurePendingProjectionQuarantineReason,
) -> PersistentFailurePendingProjectionQuarantineError {
    let authority = PendingProjectionQuarantineAuthority {
        topology: PendingProjectionQuarantineOwnedTopology::Inert { drain },
        reason: Some(reason),
    };
    let reason = inventory
        .install_pending_quarantine(authority)
        .unwrap_or(reason);
    PersistentFailurePendingProjectionQuarantineError { inventory, reason }
}
