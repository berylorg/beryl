mod decode;
mod encode;
mod stream;
mod tags;

pub use decode::decode_bounded_provider_item_frame_v1;
pub use encode::encode_provider_item_frame_v1;
pub use stream::validate_streaming_provider_item_frame_v1;

use super::{
    ProviderFrameHistorySupportV1, ProviderFrameReferenceV1, ProviderFrameTextSpanV1,
    ProviderFrameTextSpanValidatorV1, ProviderItemValidationError, ProviderLifecycleTimestampMsV1,
};

/// Maximum byte slice offered to a streaming provider-frame sink.
pub const PROVIDER_FRAME_CHUNK_MAX_BYTES: usize = 65_536;

/// Maximum allocation accepted by the convenience full-frame decoder.
pub const PROVIDER_FRAME_BOUNDED_DECODE_MAX_BYTES: usize = 65_536;

/// Streaming destination for provider bytes and their separate frame-local span index.
pub trait ProviderFrameSinkV1 {
    type Error;

    fn write_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error>;

    fn write_text_span(&mut self, span: ProviderFrameTextSpanV1) -> Result<(), Self::Error>;
}

/// Destination for spans regenerated during constant-resident structural validation.
pub trait ProviderFrameTextSpanSinkV1 {
    type Error;

    fn write_text_span(&mut self, span: ProviderFrameTextSpanV1) -> Result<(), Self::Error>;
}

impl ProviderFrameTextSpanSinkV1 for ProviderFrameTextSpanValidatorV1 {
    type Error = ProviderItemValidationError;

    fn write_text_span(&mut self, span: ProviderFrameTextSpanV1) -> Result<(), Self::Error> {
        self.observe(span)
    }
}

/// Failure while streaming a validated frame to its bounded destination.
#[derive(Debug)]
pub enum ProviderFrameEncodeError<E> {
    Validation(ProviderItemValidationError),
    Sink(E),
}

impl<E> From<ProviderItemValidationError> for ProviderFrameEncodeError<E> {
    fn from(value: ProviderItemValidationError) -> Self {
        Self::Validation(value)
    }
}

/// Rejection from the explicitly bounded convenience decoder.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProviderFrameDecodeError {
    #[error("provider frame length {actual} exceeds bounded decoder limit {maximum}")]
    FrameTooLarge { maximum: usize, actual: usize },
    #[error("provider frame is truncated")]
    Truncated,
    #[error("provider frame contains trailing bytes")]
    TrailingBytes,
    #[error("provider frame has invalid {kind} tag {tag}")]
    InvalidTag { kind: &'static str, tag: u8 },
    #[error("provider frame {kind} is not valid UTF-8")]
    InvalidUtf8 { kind: &'static str },
    #[error("provider frame {kind} length cannot be represented")]
    InvalidLength { kind: &'static str },
    #[error("provider frame has an invalid exact value: {0}")]
    InvalidValue(ProviderItemValidationError),
    #[error("provider frame identity is invalid: {kind}")]
    InvalidIdentity { kind: &'static str },
    #[error("provider frame submitted-content reference is invalid")]
    InvalidContentReference,
    #[error("provider frame digest does not match its sealed expectation")]
    DigestMismatch,
}

impl From<ProviderItemValidationError> for ProviderFrameDecodeError {
    fn from(value: ProviderItemValidationError) -> Self {
        Self::InvalidValue(value)
    }
}

/// Result of one complete streaming encode.
pub type ProviderFrameEncodeResultV1 = ProviderFrameReferenceV1;

/// Generic failure from the constant-resident reader and span verifier.
#[derive(Debug)]
pub enum ProviderFrameStreamError<E> {
    Decode(ProviderFrameDecodeError),
    Read(std::io::Error),
    Span(E),
}

impl<E> From<ProviderFrameDecodeError> for ProviderFrameStreamError<E> {
    fn from(value: ProviderFrameDecodeError) -> Self {
        Self::Decode(value)
    }
}

impl<E> From<ProviderItemValidationError> for ProviderFrameStreamError<E> {
    fn from(value: ProviderItemValidationError) -> Self {
        Self::Decode(ProviderFrameDecodeError::InvalidValue(value))
    }
}

/// Lifecycle facts extracted without materializing provider strings or collections.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderFrameObservationSummaryV1 {
    Started(ProviderLifecycleTimestampMsV1),
    Delta,
    Completed(ProviderLifecycleTimestampMsV1),
}

/// Structurally validated frame identity and observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderFrameStructuralValidationV1 {
    reference: ProviderFrameReferenceV1,
    observation: ProviderFrameObservationSummaryV1,
    history_support: ProviderFrameHistorySupportV1,
}

impl ProviderFrameStructuralValidationV1 {
    #[must_use]
    pub const fn reference(&self) -> &ProviderFrameReferenceV1 {
        &self.reference
    }

    #[must_use]
    pub const fn observation(&self) -> ProviderFrameObservationSummaryV1 {
        self.observation
    }

    #[must_use]
    pub const fn history_support(&self) -> ProviderFrameHistorySupportV1 {
        self.history_support
    }
}
