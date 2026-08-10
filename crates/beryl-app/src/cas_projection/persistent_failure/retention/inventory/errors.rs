use super::*;

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
pub(in crate::cas_projection::persistent_failure::retention::inventory) fn retention_seal_observer()
-> &'static std::sync::Mutex<Option<std::sync::mpsc::SyncSender<bool>>> {
    static OBSERVER: std::sync::OnceLock<
        std::sync::Mutex<Option<std::sync::mpsc::SyncSender<bool>>>,
    > = std::sync::OnceLock::new();
    OBSERVER.get_or_init(|| std::sync::Mutex::new(None))
}
