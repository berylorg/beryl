//! Exclusive execution of one pending ordinary Syndic turn.

mod converge;
mod error;
mod execute;
mod model;
pub(in crate::cas_projection) mod preflight;

pub(in crate::cas_projection) use converge::converge_terminal_history;

#[cfg(feature = "test-faults")]
#[doc(hidden)]
pub use super::input_replay::{
    OrdinaryInputReplayDiagnostics, OrdinaryInputReplayDiagnosticsSnapshot,
    SourcePageHandoffBarrierController,
};
pub use error::OrdinaryTurnExecutionError;
pub use model::{
    OrdinaryDynamicToolContext, OrdinaryDynamicToolHandlers, OrdinaryNotStartedProjection,
    OrdinaryTurnCaptureLoss, OrdinaryTurnExecutionFailure, OrdinaryTurnExecutionOutcome,
    OrdinaryTurnExecutionRequest, OrdinaryTurnNotStarted,
};
