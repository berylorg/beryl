use beryl_model::{CasItemId, CasThreadId, CasTurnId};
use thiserror::Error;

use crate::ItemLifecycleTimestampMs;

use super::super::StreamedInputSourceError;

/// The pinned direct lifecycle notification being correlated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserMessageEchoLifecycle {
    Started,
    Completed,
}

impl UserMessageEchoLifecycle {
    pub(crate) const fn method(self) -> &'static str {
        match self {
            Self::Started => "item/started",
            Self::Completed => "item/completed",
        }
    }
}

/// Compact proof that one lifecycle echo matched the submitted source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamedUserMessageCorrelation {
    pub(super) item_id: CasItemId,
    pub(super) checked_input_items: u64,
}

impl StreamedUserMessageCorrelation {
    #[must_use]
    pub const fn item_id(&self) -> &CasItemId {
        &self.item_id
    }

    #[must_use]
    pub const fn checked_input_items(&self) -> u64 {
        self.checked_input_items
    }
}

/// Exact bounded failure from replay-backed user-message correlation.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum StreamedUserMessageCorrelationError {
    #[error("a streamed user-message verifier was already installed")]
    VerifierAlreadyInstalled,
    #[error("the streamed user-message verifier scope disagreed with the installed scope")]
    VerifierScopeDisagreement,
    #[error("the streamed user-message verifier proof state was unavailable")]
    VerifierUnavailable,
    #[error("streamed user-message lifecycle {actual:?} was invalid after {state}")]
    LifecycleOrdering {
        actual: UserMessageEchoLifecycle,
        state: &'static str,
    },
    #[error("streamed user-message descriptor replay failed for {lifecycle:?}")]
    DescriptorSource {
        lifecycle: UserMessageEchoLifecycle,
        #[source]
        source: StreamedInputSourceError,
    },
    #[error("streamed user-message notification thread did not match the request target")]
    ThreadMismatch,
    #[error("completed streamed user-message turn did not match its start")]
    TurnMismatch,
    #[error("completed streamed user-message item did not match its start")]
    ItemMismatch,
    #[error("successful turn/start response arrived before both user-message echoes")]
    SuccessfulResponseBeforeBothEchoes,
    #[error("turn/start rejection arrived after a user-message lifecycle echo")]
    RejectionAfterEcho,
    #[error("turn/start response turn did not match the correlated user-message turn")]
    ResponseTurnMismatch,
    #[error("streamed user-message clientId was not null")]
    ClientIdPresent,
    #[error("streamed user-message had {actual} inputs, expected {expected}")]
    InputCountMismatch { expected: u64, actual: u64 },
    #[error("streamed user-message input {item_index} was {actual}, expected {expected}")]
    InputVariantMismatch {
        item_index: u64,
        expected: &'static str,
        actual: &'static str,
    },
    #[error("submitted text source failed while checking input {item_index}")]
    TextSource {
        item_index: u64,
        #[source]
        source: StreamedInputSourceError,
    },
    #[error("streamed text input {item_index} differed at UTF-8 byte {byte_offset}")]
    TextMismatch { item_index: u64, byte_offset: u64 },
    #[error("streamed text input {item_index} ended at {actual}, expected {expected} bytes")]
    TextLengthMismatch {
        item_index: u64,
        expected: u64,
        actual: u64,
    },
    #[error("streamed text input {item_index} did not echo empty text_elements")]
    TextElementsMismatch { item_index: u64 },
    #[error("streamed local-image path at input {item_index} differed at UTF-8 byte {byte_offset}")]
    ImagePathMismatch { item_index: u64, byte_offset: u64 },
    #[error("streamed local-image path at input {item_index} had a different length")]
    ImagePathLengthMismatch { item_index: u64 },
    #[error("streamed local-image detail at input {item_index} differed")]
    ImageDetailMismatch { item_index: u64 },
    #[error("pinned streamed user-message normalization violated {context}")]
    UnsupportedNormalization { context: &'static str },
}

/// Non-cloneable proof that one submitted-user lifecycle echo matched the active replay source.
#[derive(Debug)]
pub struct CheckedUserMessage {
    pub(super) lifecycle: UserMessageEchoLifecycle,
    pub(super) thread_id: CasThreadId,
    pub(super) turn_id: CasTurnId,
    pub(super) timestamp: ItemLifecycleTimestampMs,
    pub(super) correlation: StreamedUserMessageCorrelation,
}

impl CheckedUserMessage {
    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn for_lifecycle_test(
        lifecycle: UserMessageEchoLifecycle,
        thread_id: CasThreadId,
        turn_id: CasTurnId,
        timestamp: ItemLifecycleTimestampMs,
        item_id: CasItemId,
        checked_input_items: u64,
    ) -> Self {
        Self {
            lifecycle,
            thread_id,
            turn_id,
            timestamp,
            correlation: StreamedUserMessageCorrelation {
                item_id,
                checked_input_items,
            },
        }
    }

    /// Returns the checked lifecycle transition carried by this operation.
    #[must_use]
    pub const fn lifecycle(&self) -> UserMessageEchoLifecycle {
        self.lifecycle
    }

    /// Returns the exact CAS thread named by the checked echo.
    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    /// Returns the exact CAS turn named by the checked echo.
    #[must_use]
    pub const fn turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }

    /// Returns the lifecycle observation timestamp carried by the checked echo.
    #[must_use]
    pub const fn timestamp(&self) -> ItemLifecycleTimestampMs {
        self.timestamp
    }

    /// Returns the compact item identity and checked descriptor-count proof.
    #[must_use]
    pub const fn correlation(&self) -> &StreamedUserMessageCorrelation {
        &self.correlation
    }

    /// Consumes the non-cloneable proof into its exact checked parts.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        UserMessageEchoLifecycle,
        CasThreadId,
        CasTurnId,
        ItemLifecycleTimestampMs,
        StreamedUserMessageCorrelation,
    ) {
        (
            self.lifecycle,
            self.thread_id,
            self.turn_id,
            self.timestamp,
            self.correlation,
        )
    }
}
