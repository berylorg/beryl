use std::{
    collections::HashMap,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
};

use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::BerylHomeId;

use super::{
    MasterCommandGate, PersistentFailureCutIdentity, PersistentFailureGeneration,
    PersistentFailureNotification, ProjectionServiceGeneration,
};
#[cfg(test)]
use crate::cas_projection::connection::{
    PersistentFailureTargetGuardDisposition, PersistentFailureTargetIneligibility,
};
use crate::cas_projection::{
    LoadedCasProjection, SameNativeReacquisitionAnchor,
    connection::{
        CleanupFailureTransfer, ConnectionPromotionReservation, FailureRetainedCleanupOwner,
        FailureRetainedPromotionReservation, FailureRetainedRawLoadedLease,
        FailureRetainedRawQuarantinedAnchor, FailureRetainedRawReacquisitionReservation,
        PersistentFailureCompletion, PersistentFailureDriverResult,
        PersistentFailureNoDispatchReason, PersistentFailureTargetWitness, ProjectionConnection,
        PromotionFailureTransfer,
    },
    stop::StopCoordinator,
};

mod recovery;

#[cfg(test)]
mod test_support;

pub(in crate::cas_projection) use recovery::{
    PendingProjectionAdoptionCheckout, PersistentFailureAdoptionFence,
    PersistentFailureAdoptionFenceRetirementError, PersistentFailureAdoptionRetirementWitness,
};

pub(in crate::cas_projection::persistent_failure) use recovery::{
    PendingProjectionTerminalDispositionFence, PersistentFailureRecoveryDrain,
    PersistentFailureRecoveryDrainError, PersistentFailureTerminalDispositionCoordinatorWitness,
    PersistentFailureTerminalDispositionDrain,
};

mod lifecycle;
mod retainer;
mod types;
mod worker;

use types::*;
pub(in crate::cas_projection) use types::{
    PersistentFailureCoordinator, PersistentFailureProjectionRetainer,
};
pub use types::{
    PersistentFailureCutSnapshot, PersistentFailureCutState,
    PersistentFailureRecoveryInventoryCounts,
};
pub(in crate::cas_projection::persistent_failure) use types::{
    PersistentFailureRecoveryInventoryObservation, PersistentFailureRetainedTarget,
};
