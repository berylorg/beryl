//! Exclusive execution of one pending ordinary Syndic turn.

mod capture;
mod converge;
mod error;
mod execute;
mod model;
mod preflight;

pub use error::OrdinaryTurnExecutionError;
pub use model::{
    OrdinaryDynamicToolContext, OrdinaryDynamicToolHandler, OrdinaryNotStartedProjection,
    OrdinaryTurnCaptureLoss, OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionRequest,
    OrdinaryTurnNotStarted,
};
