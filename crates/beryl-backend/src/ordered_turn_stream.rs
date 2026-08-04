//! One synchronous source-order boundary for compact controls and provider observations.

use beryl_stream::PageLease;
use thiserror::Error;

use crate::{
    ApprovalRequest, CheckedSteeringUserMessage, CheckedSteeringUserMessageSubmitError,
    CheckedUserMessage, DynamicToolArgumentControl, DynamicToolArgumentFragment, DynamicToolCall,
    DynamicToolCallAbandonReason, ExactForegroundTurn, NormalTurnTerminal,
    ProviderObservationAbandonReason, ProviderObservationBegin, ProviderObservationControl,
    ProviderObservationFragment, ProviderObservationRoute, SteeringUserMessageAbandonReason,
    SteeringUserMessageSelection, SteeringUserMessageSelectionError, SteeringUserMessageSource,
    StopAttemptDisposition, StopOperationCorrelation, ThreadClosed, ThreadStatusChanged,
    TurnStarted,
};

/// One operation in the exact order read from a backend connection.
pub enum OrderedTurnStreamOperation {
    /// One exact loaded-thread status transition.
    ThreadStatusChanged(ThreadStatusChanged),
    /// One exact loaded-thread closure notification.
    ThreadClosed(ThreadClosed),
    /// One exact running-turn identity publication.
    TurnStarted(TurnStarted),
    /// One fully checked submitted-user lifecycle control.
    CheckedUserMessage(CheckedUserMessage),
    /// One completely validated status-only normal turn terminal.
    NormalTurnTerminal(NormalTurnTerminal),
    /// One compact approval request carrying exact response authority.
    Approval(ApprovalRequest),
    /// Selects the exact dynamic-tool call before any argument bytes are forwarded.
    DynamicBegin(DynamicToolCall),
    /// Forwards one structural dynamic-argument event.
    DynamicArgumentControl(DynamicToolArgumentControl),
    /// Requests the connection's sole foreground page for dynamic scalar bytes.
    DynamicAcquirePage,
    /// Transfers one decoded dynamic scalar fragment and its page lease.
    DynamicArgumentFragment(DynamicToolArgumentFragment),
    /// Seals the completely validated dynamic argument stream.
    DynamicSeal,
    /// Abandons the selected dynamic call after releasing any page lease.
    DynamicAbandon(DynamicToolCallAbandonReason),
    /// Begins one provider observation.
    ProviderBegin(ProviderObservationBegin),
    /// Forwards one structural provider control.
    ProviderControl(ProviderObservationControl),
    /// Requests the connection's sole foreground page for provider bytes.
    ProviderAcquirePage,
    /// Transfers one provider fragment and its page lease.
    ProviderFragment(ProviderObservationFragment),
    /// Seals one complete provider observation.
    ProviderSeal(ProviderObservationRoute),
    /// Abandons one incomplete provider observation.
    ProviderAbandon(ProviderObservationAbandonReason),
}

impl std::fmt::Debug for OrderedTurnStreamOperation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ThreadStatusChanged(status) => formatter
                .debug_tuple("ThreadStatusChanged")
                .field(status)
                .finish(),
            Self::ThreadClosed(closed) => {
                formatter.debug_tuple("ThreadClosed").field(closed).finish()
            }
            Self::TurnStarted(turn) => formatter.debug_tuple("TurnStarted").field(turn).finish(),
            Self::CheckedUserMessage(message) => formatter
                .debug_tuple("CheckedUserMessage")
                .field(message)
                .finish(),
            Self::NormalTurnTerminal(terminal) => formatter
                .debug_tuple("NormalTurnTerminal")
                .field(terminal)
                .finish(),
            Self::Approval(request) => formatter.debug_tuple("Approval").field(request).finish(),
            Self::DynamicBegin(call) => formatter.debug_tuple("DynamicBegin").field(call).finish(),
            Self::DynamicArgumentControl(control) => formatter
                .debug_tuple("DynamicArgumentControl")
                .field(control)
                .finish(),
            Self::DynamicAcquirePage => formatter.write_str("DynamicAcquirePage"),
            Self::DynamicArgumentFragment(fragment) => formatter
                .debug_struct("DynamicArgumentFragment")
                .field("kind", &fragment.kind())
                .field("offset", &fragment.offset())
                .field("bytes", &fragment.bytes().len())
                .finish(),
            Self::DynamicSeal => formatter.write_str("DynamicSeal"),
            Self::DynamicAbandon(reason) => formatter
                .debug_tuple("DynamicAbandon")
                .field(reason)
                .finish(),
            Self::ProviderBegin(begin) => {
                formatter.debug_tuple("ProviderBegin").field(begin).finish()
            }
            Self::ProviderControl(control) => formatter
                .debug_tuple("ProviderControl")
                .field(control)
                .finish(),
            Self::ProviderAcquirePage => formatter.write_str("ProviderAcquirePage"),
            Self::ProviderFragment(fragment) => formatter
                .debug_struct("ProviderFragment")
                .field("context", &fragment.context())
                .field("bytes", &fragment.bytes().len())
                .finish(),
            Self::ProviderSeal(route) => {
                formatter.debug_tuple("ProviderSeal").field(route).finish()
            }
            Self::ProviderAbandon(reason) => formatter
                .debug_tuple("ProviderAbandon")
                .field(reason)
                .finish(),
        }
    }
}

/// Exact interruption fact returned after one approval is synchronously routed.
///
/// Durable stop ownership reports correlations only. It is not an authorization to dispatch the
/// claimed attempt and does not assign provider idempotency semantics to either correlation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApprovalInterruption {
    /// The protocol denial itself interrupts command execution or file change work.
    NotRequired,
    /// Permission routing durably admitted or joined the exact stop before denial.
    DurableStopOwned {
        /// Opaque identity of the durable stop operation.
        operation: StopOperationCorrelation,
        /// Exact foreground target owned by that operation.
        target: ExactForegroundTurn,
        /// Correlation and byte-dispatch state of the operation's sole claimed attempt.
        attempt_disposition: StopAttemptDisposition,
    },
}

/// Synchronous result of applying one approval operation to exact app target authority.
pub enum ApprovalOperationCompletion {
    /// The exact route accepted the approval and any separate driver obligation was admitted.
    Routed { interruption: ApprovalInterruption },
    /// A classified exact target failed locally and returned the non-routed approval.
    TargetFailed {
        request: ApprovalRequest,
        cause: OrderedTurnStreamSubmitCause,
    },
}

impl std::fmt::Debug for ApprovalOperationCompletion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Routed { interruption } => formatter
                .debug_struct("Routed")
                .field("interruption", interruption)
                .finish(),
            Self::TargetFailed { request, cause } => formatter
                .debug_struct("TargetFailed")
                .field("request", request)
                .field("cause", cause)
                .finish(),
        }
    }
}

/// Completion of one synchronously applied ordered operation.
pub enum OrderedTurnStreamCompletion {
    /// The operation was synchronously applied without returning ownership.
    Applied,
    /// The approval request was routed or returned with its exact ownership.
    Approval(ApprovalOperationCompletion),
    /// The sole foreground JSON page was admitted or exchanged back to the connection.
    PageLease(PageLease),
}

impl std::fmt::Debug for OrderedTurnStreamCompletion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Applied => formatter.write_str("Applied"),
            Self::Approval(completion) => {
                formatter.debug_tuple("Approval").field(completion).finish()
            }
            Self::PageLease(lease) => formatter
                .debug_struct("PageLease")
                .field("len", &lease.len())
                .field("capacity", &lease.capacity())
                .finish(),
        }
    }
}

/// Content-free rejection reasons from the ordered consumer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderedTurnStreamRejection {
    InvalidControl,
    SchemaMismatch,
    StagingConflict,
}

/// Typed terminal cause for a rejected ordered submission.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrderedTurnStreamSubmitCause {
    #[error("ordered turn-stream sink is unavailable")]
    Unavailable,
    #[error("ordered turn-stream sink capacity is full")]
    CapacityFull,
    #[error("ordered turn-stream blocking submission timed out")]
    Timeout,
    #[error("ordered turn-stream receiver was lost")]
    ReceiverLost,
    #[error("ordered turn-stream operation was cancelled")]
    Cancelled,
    #[error("ordered turn-stream sink rejected typed input: {0:?}")]
    Rejected(OrderedTurnStreamRejection),
}

/// Ownership-preserving terminal failure from one synchronous submission.
pub struct OrderedTurnStreamSubmitError {
    operation: OrderedTurnStreamOperation,
    cause: OrderedTurnStreamSubmitCause,
}

impl OrderedTurnStreamSubmitError {
    #[must_use]
    pub const fn new(
        operation: OrderedTurnStreamOperation,
        cause: OrderedTurnStreamSubmitCause,
    ) -> Self {
        Self { operation, cause }
    }

    /// Returns the typed content-free terminal cause.
    #[must_use]
    pub const fn cause(&self) -> OrderedTurnStreamSubmitCause {
        self.cause
    }

    /// Returns the exact submitted operation to its caller.
    #[must_use]
    pub fn into_operation(self) -> OrderedTurnStreamOperation {
        self.operation
    }

    /// Splits the failure into the exact submitted operation and typed cause.
    #[must_use]
    pub fn into_parts(self) -> (OrderedTurnStreamOperation, OrderedTurnStreamSubmitCause) {
        (self.operation, self.cause)
    }
}

impl std::fmt::Debug for OrderedTurnStreamSubmitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OrderedTurnStreamSubmitError")
            .field("operation", &self.operation)
            .field("cause", &self.cause)
            .finish()
    }
}

impl std::fmt::Display for OrderedTurnStreamSubmitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "ordered turn-stream submission failed: {}",
            self.cause
        )
    }
}

impl std::error::Error for OrderedTurnStreamSubmitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

/// Object-safe synchronous consumer for one connection's exact turn-stream order.
///
/// The consumer must finish applying an operation before returning. On failure it returns the
/// exact submitted operation, including any provider fragment lease.
pub trait OrderedTurnStreamSink: Send {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError>;

    /// Correlation-first selection for one delayed steering `UserMessage`.
    ///
    /// The default keeps existing consumers deliberately unmounted. A steering-aware consumer
    /// must synchronously select the exact target and a fresh replay source before any unbounded
    /// content is decoded.
    fn select_steering_user_message(
        &mut self,
        selection: SteeringUserMessageSelection,
    ) -> Result<SteeringUserMessageSource, SteeringUserMessageSelectionError> {
        Err(SteeringUserMessageSelectionError::new(
            selection,
            OrderedTurnStreamSubmitCause::Unavailable,
        ))
    }

    /// Commits one lifecycle only after fresh replay, content, and exact route all agree.
    fn submit_checked_steering_user_message(
        &mut self,
        message: CheckedSteeringUserMessage,
    ) -> Result<(), CheckedSteeringUserMessageSubmitError> {
        Err(CheckedSteeringUserMessageSubmitError::new(
            message,
            OrderedTurnStreamSubmitCause::Unavailable,
        ))
    }

    /// Releases any correlation-selected state after an incomplete delayed lifecycle.
    fn abandon_steering_user_message(
        &mut self,
        _reason: SteeringUserMessageAbandonReason,
    ) -> Result<(), OrderedTurnStreamSubmitCause> {
        Err(OrderedTurnStreamSubmitCause::Unavailable)
    }
}

/// Binding failures decided before ordered transport polling begins.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrderedTurnStreamBindingError {
    #[error("ordered turn-stream ingestion is unavailable on stdio transport")]
    StdioUnavailable,
    #[error("ordered turn-stream ingestion requires an initialized full notification profile")]
    FullTurnStreamRequired,
    #[error("this session already has an ordered turn-stream sink")]
    AlreadyBound,
    #[error("the backend transport is already closed")]
    TransportClosed,
    #[error("a buffered compact control failed normalization during ordered binding")]
    BufferedNormalization,
    #[error("the ordered sink rejected a buffered compact control: {0}")]
    BufferedSubmission(OrderedTurnStreamSubmitCause),
    #[error("the ordered sink returned the wrong completion for a buffered compact control")]
    BufferedUnexpectedCompletion,
}

/// Result of one bound-session ordered progress poll.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderedTurnStreamProgress {
    Progress,
    Quiet,
}
