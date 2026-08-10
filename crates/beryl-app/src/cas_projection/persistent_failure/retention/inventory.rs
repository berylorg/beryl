use std::{
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use beryl_home_store::HomeGeneration;
use beryl_model::BerylHomeId;

use super::{
    PersistentFailureCutCompletion, PersistentFailureCutHandoff, PersistentFailureRetainedService,
    PersistentFailureServiceEscrowCell, retained_services,
};
use crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerExit;
use crate::cas_projection::connection::ProjectionConnection;
use crate::cas_projection::persistent_failure::{
    LiveCommandGateStatus, PersistentFailureAdoptionRetirementWitness,
    PersistentFailureCutIdentity, PersistentFailureCutSnapshot, PersistentFailureGeneration,
    PersistentFailureRecoveryInventoryCounts, ProjectionServiceGeneration,
};

mod terminal;

pub(in crate::cas_projection::persistent_failure) use terminal::{
    PersistentFailureTerminalDispositionWitness, PersistentFailureTerminalRetirementError,
};

mod core;
mod errors;
mod handoff;
mod service;

use core::*;
pub(in crate::cas_projection) use core::{
    PersistentFailureOldServiceEpochRetirementError,
    PersistentFailureOldServiceEpochRetirementReason,
};
pub use core::{
    PersistentFailureRecoveryInventory, PersistentFailureRecoveryInventoryError,
    PersistentFailureRecoveryInventoryMetadata,
};
pub(in crate::cas_projection::persistent_failure::retention) use handoff::remove_exact_retirement_escrow;
use handoff::*;
