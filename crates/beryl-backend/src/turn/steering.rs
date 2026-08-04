use std::fmt;

use beryl_model::{CasItemId, CasThreadId, CasTurnId};
use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::{
    BoundedResponseTextError, OrderedTurnStreamSubmitCause, PROTOCOL_IDENTITY_MAX_BYTES,
    ProtocolIdentity,
};

use super::{
    ItemLifecycleTimestampMs, StreamedInputSource, StreamedUserMessageCorrelationError,
    UserMessageEchoLifecycle,
};

/// Maximum UTF-8 length of one opaque CAS `clientUserMessageId`.
pub const CLIENT_USER_MESSAGE_ID_MAX_BYTES: usize = PROTOCOL_IDENTITY_MAX_BYTES;

/// One bounded opaque correlation supplied to and echoed by CAS steering.
#[derive(Clone, PartialEq, Eq)]
pub struct ClientUserMessageId(ProtocolIdentity);

impl ClientUserMessageId {
    pub fn try_new(value: &str) -> Result<Self, BoundedResponseTextError> {
        ProtocolIdentity::try_new(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for ClientUserMessageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClientUserMessageId")
            .field("utf8_bytes", &self.as_str().len())
            .finish()
    }
}

impl Serialize for ClientUserMessageId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

/// Exact active turn accepted by one successful `turn/steer`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SteeredTurn {
    turn_id: CasTurnId,
}

impl SteeredTurn {
    pub(crate) const fn new(turn_id: CasTurnId) -> Self {
        Self { turn_id }
    }

    #[must_use]
    pub const fn turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }
}

/// Compact bounded `turn/steer` result retained by the incremental decoder.
#[doc(hidden)]
#[derive(Debug, PartialEq, Eq)]
pub struct TurnSteerResponseWire {
    turn_id: CasTurnId,
}

impl TurnSteerResponseWire {
    pub(crate) fn try_new(turn_id: &str) -> Option<Self> {
        Some(Self {
            turn_id: CasTurnId::new(turn_id).ok()?,
        })
    }

    #[doc(hidden)]
    #[must_use]
    pub const fn turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }

    pub(crate) fn into_steered(self) -> SteeredTurn {
        SteeredTurn::new(self.turn_id)
    }
}

/// Correlation-first selection passed through the connection's ordered sink.
pub struct SteeringUserMessageSelection {
    lifecycle: UserMessageEchoLifecycle,
    item_id: CasItemId,
    client_user_message_id: ClientUserMessageId,
}

impl SteeringUserMessageSelection {
    pub(crate) const fn new(
        lifecycle: UserMessageEchoLifecycle,
        item_id: CasItemId,
        client_user_message_id: ClientUserMessageId,
    ) -> Self {
        Self {
            lifecycle,
            item_id,
            client_user_message_id,
        }
    }

    #[must_use]
    pub const fn lifecycle(&self) -> UserMessageEchoLifecycle {
        self.lifecycle
    }

    #[must_use]
    pub const fn item_id(&self) -> &CasItemId {
        &self.item_id
    }

    #[must_use]
    pub const fn client_user_message_id(&self) -> &ClientUserMessageId {
        &self.client_user_message_id
    }

    #[must_use]
    pub fn into_parts(self) -> (UserMessageEchoLifecycle, CasItemId, ClientUserMessageId) {
        (self.lifecycle, self.item_id, self.client_user_message_id)
    }
}

impl fmt::Debug for SteeringUserMessageSelection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SteeringUserMessageSelection")
            .field("lifecycle", &self.lifecycle)
            .field("item_id", &self.item_id)
            .field("client_user_message_id", &self.client_user_message_id)
            .finish()
    }
}

/// Fresh replay authority selected by the ordered sink for one delayed echo.
pub struct SteeringUserMessageSource {
    expected_thread_id: CasThreadId,
    expected_turn_id: CasTurnId,
    source: Box<dyn StreamedInputSource>,
}

impl SteeringUserMessageSource {
    #[must_use]
    pub fn new(
        expected_thread_id: CasThreadId,
        expected_turn_id: CasTurnId,
        source: Box<dyn StreamedInputSource>,
    ) -> Self {
        Self {
            expected_thread_id,
            expected_turn_id,
            source,
        }
    }

    #[must_use]
    pub const fn expected_thread_id(&self) -> &CasThreadId {
        &self.expected_thread_id
    }

    #[must_use]
    pub const fn expected_turn_id(&self) -> &CasTurnId {
        &self.expected_turn_id
    }

    pub(crate) fn into_parts(self) -> (CasThreadId, CasTurnId, Box<dyn StreamedInputSource>) {
        (self.expected_thread_id, self.expected_turn_id, self.source)
    }
}

impl fmt::Debug for SteeringUserMessageSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SteeringUserMessageSource")
            .field("expected_thread_id", &self.expected_thread_id)
            .field("expected_turn_id", &self.expected_turn_id)
            .finish_non_exhaustive()
    }
}

/// Compact proof that one delayed correlation-bearing lifecycle matched fresh replay.
pub struct CheckedSteeringUserMessage {
    lifecycle: UserMessageEchoLifecycle,
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    item_id: CasItemId,
    timestamp: ItemLifecycleTimestampMs,
    client_user_message_id: ClientUserMessageId,
    checked_input_items: u64,
}

impl CheckedSteeringUserMessage {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        lifecycle: UserMessageEchoLifecycle,
        thread_id: CasThreadId,
        turn_id: CasTurnId,
        item_id: CasItemId,
        timestamp: ItemLifecycleTimestampMs,
        client_user_message_id: ClientUserMessageId,
        checked_input_items: u64,
    ) -> Self {
        Self {
            lifecycle,
            thread_id,
            turn_id,
            item_id,
            timestamp,
            client_user_message_id,
            checked_input_items,
        }
    }

    #[must_use]
    pub const fn lifecycle(&self) -> UserMessageEchoLifecycle {
        self.lifecycle
    }

    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    #[must_use]
    pub const fn turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }

    #[must_use]
    pub const fn item_id(&self) -> &CasItemId {
        &self.item_id
    }

    #[must_use]
    pub const fn timestamp(&self) -> ItemLifecycleTimestampMs {
        self.timestamp
    }

    #[must_use]
    pub const fn client_user_message_id(&self) -> &ClientUserMessageId {
        &self.client_user_message_id
    }

    #[must_use]
    pub const fn checked_input_items(&self) -> u64 {
        self.checked_input_items
    }

    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        UserMessageEchoLifecycle,
        CasThreadId,
        CasTurnId,
        CasItemId,
        ItemLifecycleTimestampMs,
        ClientUserMessageId,
        u64,
    ) {
        (
            self.lifecycle,
            self.thread_id,
            self.turn_id,
            self.item_id,
            self.timestamp,
            self.client_user_message_id,
            self.checked_input_items,
        )
    }
}

impl fmt::Debug for CheckedSteeringUserMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CheckedSteeringUserMessage")
            .field("lifecycle", &self.lifecycle)
            .field("thread_id", &self.thread_id)
            .field("turn_id", &self.turn_id)
            .field("item_id", &self.item_id)
            .field("timestamp", &self.timestamp)
            .field("client_user_message_id", &self.client_user_message_id)
            .field("checked_input_items", &self.checked_input_items)
            .finish()
    }
}

/// Why a correlation-selected delayed lifecycle was abandoned before commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SteeringUserMessageAbandonReason {
    SchemaFailure,
    CorrelationFailure,
    MissingOrMalformedRoute,
    CapacityFull,
    Timeout,
    ReceiverLost,
    Cancelled,
    SinkRejected,
    TransportLost,
}

/// Ownership-preserving failure to select fresh steering replay authority.
pub struct SteeringUserMessageSelectionError {
    selection: SteeringUserMessageSelection,
    cause: OrderedTurnStreamSubmitCause,
}

impl SteeringUserMessageSelectionError {
    #[must_use]
    pub const fn new(
        selection: SteeringUserMessageSelection,
        cause: OrderedTurnStreamSubmitCause,
    ) -> Self {
        Self { selection, cause }
    }

    #[must_use]
    pub const fn cause(&self) -> OrderedTurnStreamSubmitCause {
        self.cause
    }

    #[must_use]
    pub fn into_parts(self) -> (SteeringUserMessageSelection, OrderedTurnStreamSubmitCause) {
        (self.selection, self.cause)
    }
}

impl fmt::Debug for SteeringUserMessageSelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SteeringUserMessageSelectionError")
            .field("selection", &self.selection)
            .field("cause", &self.cause)
            .finish()
    }
}

impl fmt::Display for SteeringUserMessageSelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "steering user-message source selection failed: {}",
            self.cause
        )
    }
}

impl std::error::Error for SteeringUserMessageSelectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

/// Ownership-preserving failure to commit one checked delayed lifecycle.
pub struct CheckedSteeringUserMessageSubmitError {
    message: CheckedSteeringUserMessage,
    cause: OrderedTurnStreamSubmitCause,
}

impl CheckedSteeringUserMessageSubmitError {
    #[must_use]
    pub const fn new(
        message: CheckedSteeringUserMessage,
        cause: OrderedTurnStreamSubmitCause,
    ) -> Self {
        Self { message, cause }
    }

    #[must_use]
    pub const fn cause(&self) -> OrderedTurnStreamSubmitCause {
        self.cause
    }

    #[must_use]
    pub fn into_parts(self) -> (CheckedSteeringUserMessage, OrderedTurnStreamSubmitCause) {
        (self.message, self.cause)
    }
}

impl fmt::Debug for CheckedSteeringUserMessageSubmitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CheckedSteeringUserMessageSubmitError")
            .field("message", &self.message)
            .field("cause", &self.cause)
            .finish()
    }
}

impl fmt::Display for CheckedSteeringUserMessageSubmitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "checked steering user-message submission failed: {}",
            self.cause
        )
    }
}

impl std::error::Error for CheckedSteeringUserMessageSubmitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

/// Failure while selecting, comparing, or committing a delayed steering echo.
#[derive(Debug, Error)]
pub enum SteeringUserMessageError {
    #[error("steering user-message correlation was missing, malformed, or out of pinned order")]
    MissingOrMalformedCorrelation,
    #[error("steering user-message fresh replay disagreed with its exact source")]
    Correlation {
        #[from]
        source: StreamedUserMessageCorrelationError,
    },
    #[error("steering user-message lifecycle named a turn other than its selected target")]
    TurnMismatch,
    #[error("ordered steering user-message source selection failed: {0}")]
    Selection(OrderedTurnStreamSubmitCause),
    #[error("ordered checked steering user-message submission failed: {0}")]
    Commit(OrderedTurnStreamSubmitCause),
    #[error("ordered steering user-message sink returned an incompatible result")]
    UnexpectedSelection,
}

pub(crate) const fn steering_abandon_reason(
    cause: OrderedTurnStreamSubmitCause,
) -> SteeringUserMessageAbandonReason {
    match cause {
        OrderedTurnStreamSubmitCause::Unavailable | OrderedTurnStreamSubmitCause::ReceiverLost => {
            SteeringUserMessageAbandonReason::ReceiverLost
        }
        OrderedTurnStreamSubmitCause::CapacityFull => {
            SteeringUserMessageAbandonReason::CapacityFull
        }
        OrderedTurnStreamSubmitCause::Timeout => SteeringUserMessageAbandonReason::Timeout,
        OrderedTurnStreamSubmitCause::Cancelled => SteeringUserMessageAbandonReason::Cancelled,
        OrderedTurnStreamSubmitCause::Rejected(_) => SteeringUserMessageAbandonReason::SinkRejected,
    }
}
