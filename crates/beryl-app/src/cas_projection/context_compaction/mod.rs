//! Exact same-thread context-compaction coordination.

pub(super) mod coordinator;
mod target;

#[cfg(feature = "test-faults")]
pub use coordinator::{
    ContextCompactionCapacityTestGuard, ContextCompactionLifecycleTestHarness,
    ContextCompactionStagingPauseController, ContextCompactionTerminalResponseTestOutcome,
    ContextCompactionWaitTestHarness,
};
pub(in crate::cas_projection) use coordinator::{
    ContextCompactionCoordinator, LifecycleCompactionAdmission,
};
pub use coordinator::{
    ContextCompactionDiagnostics, ContextCompactionError, ContextCompactionOutcome,
    ContextCompactionRequest,
};
pub(in crate::cas_projection) use target::ContextCompactionTargetAuthority;
