use std::time::Duration;

use beryl_backend::{DynamicToolCallResponse, JsonRpcError, ManagedBackendError, TurnStartOptions};
use beryl_model::{SyndicThreadId, SyndicTurnId};
use syndic_storage::TurnEndStatus;
use thiserror::Error;

use super::OrdinaryTurnExecutionError;
#[cfg(feature = "test-faults")]
use crate::cas_projection::input_replay::OrdinaryInputReplayDiagnostics;
use crate::cas_projection::{
    LiveEventTargetCloseReason, LoadedCasProjection, SameNativeReacquisitionAnchor,
};
use crate::{
    BranchDiscussionResolutionRequest, BranchDiscussionResolutionRequestHandler,
    LifecycleYieldRequest, LifecycleYieldRequestHandler,
};

/// Caller-selected provider options for one already admitted ordinary turn.
#[derive(Clone, Debug)]
pub struct OrdinaryTurnExecutionRequest {
    start_options: TurnStartOptions,
    request_timeout: Duration,
    context_compaction_timeout: Duration,
    #[cfg(feature = "test-faults")]
    input_replay_diagnostics: OrdinaryInputReplayDiagnostics,
}

impl OrdinaryTurnExecutionRequest {
    #[must_use]
    pub fn new(start_options: TurnStartOptions, request_timeout: Duration) -> Self {
        Self {
            start_options,
            request_timeout,
            context_compaction_timeout: Duration::from_secs(180),
            #[cfg(feature = "test-faults")]
            input_replay_diagnostics: OrdinaryInputReplayDiagnostics::new(),
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

    /// Overrides the completion deadline snapshotted for an automatic lifecycle compaction.
    #[must_use]
    pub const fn with_context_compaction_timeout(mut self, timeout: Duration) -> Self {
        self.context_compaction_timeout = timeout;
        self
    }

    #[must_use]
    pub const fn context_compaction_timeout(&self) -> Duration {
        self.context_compaction_timeout
    }

    #[cfg(feature = "test-faults")]
    #[doc(hidden)]
    /// Returns this request's shared content-free input-replay diagnostics.
    #[must_use]
    pub fn input_replay_diagnostics(&self) -> OrdinaryInputReplayDiagnostics {
        self.input_replay_diagnostics.clone()
    }
}

impl PartialEq for OrdinaryTurnExecutionRequest {
    fn eq(&self, other: &Self) -> bool {
        self.start_options == other.start_options
            && self.request_timeout == other.request_timeout
            && self.context_compaction_timeout == other.context_compaction_timeout
    }
}

impl Eq for OrdinaryTurnExecutionRequest {}

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

/// Capability-free dispatcher over two separately narrowed feature handlers.
pub struct OrdinaryDynamicToolHandlers<'a> {
    lifecycle: &'a mut dyn LifecycleYieldRequestHandler,
    branch: &'a mut dyn BranchDiscussionResolutionRequestHandler,
}

impl<'a> OrdinaryDynamicToolHandlers<'a> {
    /// Binds the feature-owned handlers used for one ordinary execution.
    #[must_use]
    pub fn new(
        lifecycle: &'a mut dyn LifecycleYieldRequestHandler,
        branch: &'a mut dyn BranchDiscussionResolutionRequestHandler,
    ) -> Self {
        Self { lifecycle, branch }
    }

    pub(super) fn respond_lifecycle_yield(
        &mut self,
        context: OrdinaryDynamicToolContext,
        request: LifecycleYieldRequest,
    ) -> DynamicToolCallResponse {
        self.lifecycle.respond_lifecycle_yield(context, request)
    }

    pub(super) fn respond_branch_discussion_resolution(
        &mut self,
        context: OrdinaryDynamicToolContext,
        request: BranchDiscussionResolutionRequest,
    ) -> DynamicToolCallResponse {
        self.branch
            .respond_branch_discussion_resolution(context, request)
    }
}

/// Failure classified by whether the caller's loaded projection remains reusable.
#[derive(Debug, Error)]
pub enum OrdinaryTurnExecutionFailure {
    /// Activation was not attempted, or the publication layer proved it did not occur.
    #[error("ordinary turn execution failed before binding activation")]
    PreActivation {
        /// Exact loaded projection retained for a corrected retry.
        projection: Box<LoadedCasProjection>,
        /// Typed failure which prevented activation.
        #[source]
        source: OrdinaryTurnExecutionError,
    },
    /// Activation authority is uncertain, so the loaded projection cannot be retried.
    #[error("ordinary turn binding activation failed without reusable projection authority")]
    Activation {
        /// Typed activation failure.
        #[source]
        source: OrdinaryTurnExecutionError,
    },
    /// Binding activation succeeded before execution failed.
    #[error("ordinary turn execution failed after binding activation")]
    AfterActivation {
        /// Typed failure after activation consumed the loaded projection's retry authority.
        #[source]
        source: OrdinaryTurnExecutionError,
    },
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
    /// Exact terminal history handed its valid projection to ownerless lifecycle compaction.
    LifecycleContinuationScheduled {
        status: TurnEndStatus,
    },
    /// Provider terminal fact was durable, but the capture session must be replaced before reuse.
    ReacquisitionRequired {
        /// Sole non-execution subscription anchor for an exact fresh-connection handoff.
        anchor: Box<SameNativeReacquisitionAnchor>,
        /// Exact provider terminal outcome retained independently of history completeness.
        status: TurnEndStatus,
    },
    Incomplete {
        reason: OrdinaryTurnCaptureLoss,
    },
}
