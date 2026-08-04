//! Process-local command fencing and one-shot persistent-home-failure coordination.

mod coordinator;
mod gate;
mod model;
mod notification;
mod quarantine;
mod retention;

pub use gate::{LiveCommandAdmissionError, LiveCommandAuthorizer, LiveCommandPermit};
pub use model::{PersistentFailureGeneration, ProjectionServiceGeneration};
pub use notification::{PersistentFailureNotification, PersistentFailureNotificationStatus};
pub use quarantine::{
    PersistentFailurePendingProjectionQuarantine,
    PersistentFailurePendingProjectionQuarantineError,
    PersistentFailurePendingProjectionQuarantineMetadata,
    PersistentFailurePendingProjectionQuarantineReason,
};
pub use retention::{
    PersistentFailureCutCompletion, PersistentFailureCutHandoff,
    PersistentFailureRecoveryInventory, PersistentFailureRecoveryInventoryError,
    PersistentFailureRecoveryInventoryMetadata,
};

pub(in crate::cas_projection) use gate::{
    LiveCommandGateStatus, MasterCommandGate, MasterCommandGateCloseOwner,
    PersistentFailureCommandFrontier,
};
pub(in crate::cas_projection) use model::PersistentFailureCutIdentity;
pub(in crate::cas_projection) use notification::{
    CompletedRecoverySupervisorFlight, persistent_failure_notification_channel,
};
pub(in crate::cas_projection) use quarantine::{
    PendingProjectionAdoptionTopology, PendingProjectionCandidateGroup,
    PendingProjectionGroupIdentity, PendingProjectionWitness,
};
pub(in crate::cas_projection) use retention::{
    PersistentFailureOldServiceEpochRetirementError,
    PersistentFailureOldServiceEpochRetirementReason, PersistentFailureRetainedService,
    PersistentFailureServiceEscrowReservation,
};

#[cfg(test)]
pub(in crate::cas_projection) fn test_failure_notification(
    home: &std::sync::Arc<beryl_home_store::HomeStore>,
    home_id: beryl_model::BerylHomeId,
    home_generation: beryl_home_store::HomeGeneration,
) -> PersistentFailureNotification {
    persistent_failure_notification_channel(
        home,
        home_id,
        home_generation,
        ProjectionServiceGeneration::allocate().expect("test service generation is available"),
    )
    .0
}
pub(in crate::cas_projection) use coordinator::{
    PendingProjectionAdoptionCheckout, PersistentFailureAdoptionFence,
    PersistentFailureAdoptionFenceRetirementError, PersistentFailureAdoptionRetirementWitness,
    PersistentFailureCoordinator, PersistentFailureProjectionRetainer,
};
pub use coordinator::{
    PersistentFailureCutSnapshot, PersistentFailureCutState,
    PersistentFailureRecoveryInventoryCounts,
};
