use std::{
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
use crate::cas_projection::connection::PersistentFailureTargetIneligibility;
use crate::cas_projection::{
    LoadedCasProjection,
    connection::{
        PersistentFailureCompletion, PersistentFailureDriverResult,
        PersistentFailureInterruptDisposition, PersistentFailureNoDispatchReason,
        ProjectionConnection,
    },
    stop::StopCoordinator,
};

mod lifecycle;
mod terminal_disposer;
mod types;
mod worker;

use types::*;
pub(in crate::cas_projection) use types::{
    PersistentFailureCoordinator, PersistentFailureTerminalDisposer,
};
pub use types::{
    PersistentFailureCutCompletion, PersistentFailureCutSnapshot, PersistentFailureCutState,
    PersistentFailureTerminalEvidence,
};
