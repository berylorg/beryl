//! Replayable submitted-input descriptors for streamed `turn/start`.

use serde::{Deserialize, Serialize};

mod correlation;
mod source;
mod wire;

pub use correlation::{
    CheckedUserMessage, StreamedUserMessageCorrelation, StreamedUserMessageCorrelationError,
    UserMessageEchoLifecycle,
};
pub(crate) use correlation::{
    StreamedUserMessageVerifier, StreamedUserMessageVerifierGuard,
    StreamedUserMessageVerifierHandle, StreamedUserMessageVerifierSlot,
};
pub(crate) use source::StreamedInputPass;
pub use source::{
    StreamedInputDescriptor, StreamedInputDescriptorKind, StreamedInputHeader,
    StreamedInputSequenceDigest, StreamedInputSequenceDigestAccumulator,
    StreamedInputSequenceDigestError, StreamedInputSource, StreamedInputSourceError,
    StreamedInputSourceIdentity, StreamedInputSourceRevision, StreamedLocalImageDescriptor,
    StreamedTextDescriptor, StreamedTextPage, StreamedTextSourceId, TextSourceProof,
};
pub(crate) use wire::{
    StreamedInputJsonWriteFailure, StreamedInputSourceFailureSlot, StreamedTurnStartParams,
    StreamedTurnSteerParams, write_source_aware_json,
};

/// Maximum UTF-8 payload requested for one submitted-text page.
pub const STREAMED_TEXT_MAX_PAGE_BYTES: usize = 64 * 1024;

/// Optional image-detail selection for one streamed local-image descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageDetail {
    Auto,
    Low,
    High,
    Original,
}

/// Exact nonnegative lifecycle timestamp for a correlated submitted-input echo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(transparent)]
pub struct ItemLifecycleTimestampMs(u64);

impl ItemLifecycleTimestampMs {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}
