use std::time::Duration;

use beryl_backend::{
    DynamicToolCallRequest, DynamicToolCallResponse, JsonRpcError, ManagedBackendError,
    TurnStartOptions,
};
use beryl_model::{SyndicThreadId, SyndicTurnId};
use syndic_storage::TurnEndStatus;

use crate::cas_projection::{LiveEventTargetCloseReason, LoadedCasProjection};

/// Caller-selected provider options for one already admitted ordinary turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrdinaryTurnExecutionRequest {
    start_options: TurnStartOptions,
    request_timeout: Duration,
}

impl OrdinaryTurnExecutionRequest {
    #[must_use]
    pub const fn new(start_options: TurnStartOptions, request_timeout: Duration) -> Self {
        Self {
            start_options,
            request_timeout,
        }
    }

    #[must_use]
    pub const fn start_options(&self) -> &TurnStartOptions {
        &self.start_options
    }

    #[must_use]
    pub const fn request_timeout(&self) -> Duration {
        self.request_timeout
    }
}

/// Exact durable context supplied to Beryl's dynamic-tool implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrdinaryDynamicToolContext {
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
}

impl OrdinaryDynamicToolContext {
    pub(super) const fn new(thread_id: SyndicThreadId, turn_id: SyndicTurnId) -> Self {
        Self { thread_id, turn_id }
    }

    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn turn_id(self) -> SyndicTurnId {
        self.turn_id
    }
}

/// Feature-owned handler for every dynamic-tool request routed to an ordinary turn.
pub trait OrdinaryDynamicToolHandler {
    fn respond(
        &mut self,
        context: OrdinaryDynamicToolContext,
        request: &DynamicToolCallRequest,
    ) -> DynamicToolCallResponse;
}

/// Exact reason why CAS proved that an attempted start did not begin a turn.
#[derive(Debug)]
pub enum OrdinaryTurnNotStarted {
    ExactRejection(JsonRpcError),
    ProvenNotDispatched(Box<ManagedBackendError>),
}

/// Local projection authority after CAS proved that the ordinary turn did not start.
#[derive(Debug)]
pub enum OrdinaryNotStartedProjection {
    /// The exact loaded CAS projection remained reusable without resume or recovery.
    Retained(Box<LoadedCasProjection>),
    /// The durable turn remains pending, but the loaded connection/target could not be retained.
    Unavailable { reason: Box<str> },
}

/// Why an admitted possibly-started turn lost live capture before a provider terminal fact.
#[derive(Debug)]
pub enum OrdinaryTurnCaptureLoss {
    StartAuthorityLost(Box<crate::cas_projection::ProjectionExecutionError>),
    StartCompletionUnknown(Box<ManagedBackendError>),
    TargetConfirmationFailed(crate::cas_projection::LiveEventTargetError),
    TargetClosed(LiveEventTargetCloseReason),
}

/// Durable result of one exclusive ordinary-turn execution attempt.
#[must_use = "ordinary execution outcomes retain exact projection or loss authority"]
#[derive(Debug)]
pub enum OrdinaryTurnExecutionOutcome {
    NotStarted {
        projection: OrdinaryNotStartedProjection,
        reason: OrdinaryTurnNotStarted,
    },
    Terminal {
        projection: Box<LoadedCasProjection>,
        status: TurnEndStatus,
    },
    Incomplete {
        reason: OrdinaryTurnCaptureLoss,
    },
}
