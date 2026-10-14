//! Exclusive execution of one pending ordinary Syndic turn.

mod converge;
#[cfg(test)]
pub(crate) use converge::{
    converge_selected_transcript_after_observation_for_test,
    step_selected_transcript_after_observation_for_test,
};
mod error;
mod execute;
mod model;
pub(in crate::cas_projection) mod preflight;

pub(crate) use converge::TerminalHistoryCompletion;
pub(in crate::cas_projection) use converge::{
    converge_terminal_history, converge_terminal_history_candidate,
};

#[cfg(feature = "test-faults")]
#[doc(hidden)]
pub use super::input_replay::{
    OrdinaryInputReplayDiagnostics, OrdinaryInputReplayDiagnosticsSnapshot,
    SourcePageHandoffBarrierController,
};
pub use error::OrdinaryTurnExecutionError;
pub use model::{
    BranchDiscussionResolutionContext, OrdinaryDynamicToolContext, OrdinaryDynamicToolHandlers,
    OrdinaryNotStartedProjection, OrdinaryTurnCaptureLoss, OrdinaryTurnExecutionFailure,
    OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionRequest, OrdinaryTurnNotStarted,
};
