//! Bounded provider-observation ingress owned by one backend connection.

mod grammar;

use beryl_model::{CasThreadId, CasTurnId};
use beryl_stream::PageLease;
use thiserror::Error;

pub use grammar::{
    ProviderContainer, ProviderEnumValue, ProviderField, ProviderFiniteF64,
    ProviderObservationControl, ProviderScalar, ProviderStructuredPosition, ProviderValueContext,
};

/// Closed lifecycle method selected before an item's public fields are decoded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderItemLifecycle {
    Started,
    Completed,
}

/// Closed captured item vocabulary. Request-scoped `userMessage` is deliberately absent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderItemKind {
    HookPrompt,
    AgentMessage,
    Plan,
    Reasoning,
    CommandExecution,
    FileChange,
    McpToolCall,
    DynamicToolCall,
    CollabAgentToolCall,
    SubAgentActivity,
    WebSearch,
    ImageView,
    Sleep,
    StandaloneImageGeneration,
    EnteredReviewMode,
    ExitedReviewMode,
    ContextCompaction,
}

impl ProviderItemKind {
    #[must_use]
    pub const fn permits_completion_only(self) -> bool {
        matches!(self, Self::SubAgentActivity)
    }
}

/// All nine pinned delta methods, carrying their exact expected item kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderDeltaKind {
    AgentMessage,
    Plan,
    ReasoningSummaryPartAdded,
    ReasoningSummaryText,
    ReasoningTextObserved,
    CommandExecutionOutput,
    FileChangeOutput,
    FileChangePatchUpdated,
    McpToolCallProgress,
}

impl ProviderDeltaKind {
    #[must_use]
    pub const fn expected_item_kind(self) -> ProviderItemKind {
        match self {
            Self::AgentMessage => ProviderItemKind::AgentMessage,
            Self::Plan => ProviderItemKind::Plan,
            Self::ReasoningSummaryPartAdded
            | Self::ReasoningSummaryText
            | Self::ReasoningTextObserved => ProviderItemKind::Reasoning,
            Self::CommandExecutionOutput => ProviderItemKind::CommandExecution,
            Self::FileChangeOutput | Self::FileChangePatchUpdated => ProviderItemKind::FileChange,
            Self::McpToolCallProgress => ProviderItemKind::McpToolCall,
        }
    }
}

/// Schema selection passed to the caller before any size-unbounded public field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderObservationBegin {
    Item {
        lifecycle: ProviderItemLifecycle,
        kind: ProviderItemKind,
    },
    Delta {
        kind: ProviderDeltaKind,
    },
}

/// Structurally validated route decoded after the unpublished item body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderObservationRoute {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
}

impl ProviderObservationRoute {
    #[must_use]
    pub const fn new(thread_id: CasThreadId, turn_id: CasTurnId) -> Self {
        Self { thread_id, turn_id }
    }

    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    #[must_use]
    pub const fn turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }
}

/// One decoded UTF-8 page for the currently open typed field.
pub struct ProviderObservationFragment {
    context: ProviderValueContext,
    lease: PageLease,
}

impl ProviderObservationFragment {
    pub(crate) const fn new(context: ProviderValueContext, lease: PageLease) -> Self {
        Self { context, lease }
    }

    #[must_use]
    pub const fn context(&self) -> ProviderValueContext {
        self.context
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.lease.as_slice()
    }

    #[must_use]
    pub fn into_lease(self) -> PageLease {
        self.lease
    }
}

/// Why an incomplete unpublished observation was abandoned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderObservationAbandonReason {
    SchemaFailure,
    MissingOrMalformedRoute,
    CapacityFull,
    Timeout,
    ReceiverLost,
    Cancelled,
    SinkRejected,
    TransportLost,
}

/// Closed parse failures for targeted pinned provider messages.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ProviderObservationSchemaError {
    #[error("provider JSON-RPC envelope shape did not match the pinned schema")]
    EnvelopeShape,
    #[error("provider schema field was duplicated")]
    DuplicateField,
    #[error("provider schema contained an unknown field")]
    UnknownField,
    #[error("provider schema omitted a required field")]
    MissingField,
    #[error("provider schema variant was unknown or appeared after public payload")]
    UnknownOrLateVariant,
    #[error("provider schema field had the wrong JSON type")]
    WrongType,
    #[error("provider item lifecycle was invalid for its pinned item kind")]
    InvalidLifecycle,
    #[error("provider index was negative, non-integral, or outside u64")]
    InvalidIndex,
    #[error("provider bounded identity was invalid")]
    InvalidIdentity,
    #[error("provider structured value exceeded depth 128")]
    StructuredDepthExceeded,
    #[error("provider JSON string or UTF-8 escape was malformed")]
    InvalidString,
    #[error("provider observation route was missing or malformed")]
    MissingOrMalformedRoute,
    #[error("provider message was ambiguous or drifted from the pinned schema")]
    AmbiguousSchema,
    #[error("provider inline image bytes require prior admitted asset authority")]
    InlineImageRequiresAsset,
}

/// Failure while decoding or staging one provider observation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ProviderObservationError {
    #[error(transparent)]
    Schema(#[from] ProviderObservationSchemaError),
    #[error(transparent)]
    Submit(#[from] crate::OrderedTurnStreamSubmitCause),
    #[error("ordered turn-stream sink returned the wrong completion kind")]
    UnexpectedCompletion,
}

pub(crate) fn abandon_reason(
    error: crate::OrderedTurnStreamSubmitCause,
) -> ProviderObservationAbandonReason {
    match error {
        crate::OrderedTurnStreamSubmitCause::Unavailable => {
            ProviderObservationAbandonReason::ReceiverLost
        }
        crate::OrderedTurnStreamSubmitCause::CapacityFull => {
            ProviderObservationAbandonReason::CapacityFull
        }
        crate::OrderedTurnStreamSubmitCause::Timeout => ProviderObservationAbandonReason::Timeout,
        crate::OrderedTurnStreamSubmitCause::ReceiverLost => {
            ProviderObservationAbandonReason::ReceiverLost
        }
        crate::OrderedTurnStreamSubmitCause::Cancelled => {
            ProviderObservationAbandonReason::Cancelled
        }
        crate::OrderedTurnStreamSubmitCause::Rejected(_) => {
            ProviderObservationAbandonReason::SinkRejected
        }
    }
}
