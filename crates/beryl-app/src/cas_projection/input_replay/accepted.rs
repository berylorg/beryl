use beryl_backend::{
    ClientUserMessageId, StreamedInputDescriptor, StreamedInputHeader, StreamedInputSourceError,
    StreamedInputSourceIdentity, StreamedInputSourceRevision, StreamedTextPage,
    StreamedTextSourceId,
};
use beryl_home_store::{HomeGeneration, HomeHealthState, HomeStore, ReadError, SidecarError};
use beryl_model::{BerylHomeId, RuntimeMode, SyndicAcceptedInputId, SyndicContentId};
use beryl_state::{AssetOwnerHeadRecord, AssetReadError, AssetState};
use syndic_storage::{AcceptedInputRecord, SyndicReadError, SyndicStorage};
use thiserror::Error;

use super::{
    InputReplayAuthority, InputReplayContext, InputReplayFactory, InputReplayPrepareError,
    InputReplayRecord,
};
use crate::cas_projection::ProjectionCancellationToken;
use crate::cas_projection::connection::StreamedInputBrokerService;

const CORRELATION_PREFIX: &str = "beryl.accepted-input.v1:";
const CORRELATION_HEX_BYTES: usize = 32;
const CORRELATION_BYTES: usize = CORRELATION_PREFIX.len() + CORRELATION_HEX_BYTES;
const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";

/// Exact home and runtime facts used to prepare accepted-input replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct AcceptedInputReplayContext(InputReplayContext);

impl AcceptedInputReplayContext {
    pub(in crate::cas_projection) const fn new(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        runtime_mode: RuntimeMode,
    ) -> Self {
        Self(InputReplayContext::new(
            home_id,
            home_generation,
            runtime_mode,
        ))
    }
}

/// Immutable compact authority capable of opening independent replay cursors.
pub(in crate::cas_projection) struct AcceptedInputReplayFactory {
    input_id: SyndicAcceptedInputId,
    storage: SyndicStorage,
    replay: InputReplayFactory,
}

impl AcceptedInputReplayFactory {
    #[allow(
        clippy::too_many_arguments,
        reason = "preparation keeps each exact durable authority explicit"
    )]
    pub(in crate::cas_projection) fn prepare(
        store: &HomeStore,
        storage: SyndicStorage,
        assets: AssetState,
        context: AcceptedInputReplayContext,
        record: AcceptedInputRecord,
        owner_head: Option<AssetOwnerHeadRecord>,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<Self, AcceptedInputReplayError> {
        let input_id = record.id();
        let content = record.content();
        let asset_reference_set = record.asset_reference_set();
        let replay = InputReplayFactory::prepare(
            store,
            &storage,
            assets,
            context.0,
            InputReplayRecord::accepted(record),
            content,
            asset_reference_set,
            owner_head,
            cancellation,
            #[cfg(feature = "test-faults")]
            super::diagnostics::OrdinaryInputReplayDiagnostics::new(),
        )
        .map_err(AcceptedInputReplayError::from)?;
        Ok(Self {
            input_id,
            storage,
            replay,
        })
    }

    pub(in crate::cas_projection) const fn input_id(&self) -> SyndicAcceptedInputId {
        self.input_id
    }

    pub(in crate::cas_projection) fn header(&self) -> StreamedInputHeader {
        self.replay.header()
    }

    pub(in crate::cas_projection) fn fresh_source(&self) -> AcceptedInputReplaySource {
        AcceptedInputReplaySource {
            storage: self.storage.clone(),
            replay: self.replay.fresh_source(),
        }
    }
}

/// One fresh non-cloneable cursor over an accepted-input replay factory.
pub(in crate::cas_projection) struct AcceptedInputReplaySource {
    storage: SyndicStorage,
    replay: InputReplayAuthority,
}

impl AcceptedInputReplaySource {
    pub(in crate::cas_projection) fn header(&self) -> StreamedInputHeader {
        self.replay.header()
    }

    pub(in crate::cas_projection) fn begin_pass(
        &mut self,
        store: &HomeStore,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<StreamedInputHeader, StreamedInputSourceError> {
        self.replay
            .service(store, &self.storage, cancellation)
            .begin_pass()
    }

    pub(in crate::cas_projection) fn next_descriptor(
        &mut self,
        store: &HomeStore,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<Option<StreamedInputDescriptor>, StreamedInputSourceError> {
        self.replay
            .service(store, &self.storage, cancellation)
            .next_descriptor()
    }

    pub(in crate::cas_projection) fn read_text_page(
        &mut self,
        store: &HomeStore,
        cancellation: &ProjectionCancellationToken,
        source_id: StreamedTextSourceId,
        start: u64,
        max_utf8_bytes: usize,
    ) -> Result<StreamedTextPage, StreamedInputSourceError> {
        self.replay
            .service(store, &self.storage, cancellation)
            .read_text_page(source_id, start, max_utf8_bytes)
    }

    pub(in crate::cas_projection) fn service<'a>(
        &'a mut self,
        store: &'a HomeStore,
        cancellation: &'a ProjectionCancellationToken,
    ) -> super::authority::InputReplayService<'a> {
        self.replay.service(store, &self.storage, cancellation)
    }
}

/// Strictly encodes one accepted-input identity for CAS steering correlation.
pub(in crate::cas_projection) fn encode_accepted_input_steering_correlation(
    input_id: SyndicAcceptedInputId,
) -> ClientUserMessageId {
    let mut value = String::with_capacity(CORRELATION_BYTES);
    value.push_str(CORRELATION_PREFIX);
    for byte in input_id.as_bytes() {
        value.push(char::from(LOWER_HEX[usize::from(byte >> 4)]));
        value.push(char::from(LOWER_HEX[usize::from(byte & 0x0f)]));
    }
    ClientUserMessageId::try_new(&value)
        .expect("canonical accepted-input correlation fits the protocol identity bound")
}

/// Strictly decodes only the canonical V1 accepted-input steering correlation.
pub(in crate::cas_projection) fn decode_accepted_input_steering_correlation(
    correlation: &ClientUserMessageId,
) -> Result<SyndicAcceptedInputId, AcceptedInputSteeringCorrelationError> {
    let value = correlation.as_str();
    let payload = value
        .strip_prefix(CORRELATION_PREFIX)
        .ok_or(AcceptedInputSteeringCorrelationError::WrongPrefix)?;
    if payload.len() != CORRELATION_HEX_BYTES {
        return Err(AcceptedInputSteeringCorrelationError::WrongLength {
            actual: value.len(),
        });
    }
    let mut decoded = [0_u8; 16];
    for (index, pair) in payload.as_bytes().chunks_exact(2).enumerate() {
        let high = decode_nibble(pair[0], index * 2)?;
        let low = decode_nibble(pair[1], index * 2 + 1)?;
        decoded[index] = (high << 4) | low;
    }
    Ok(SyndicAcceptedInputId::from_bytes(decoded))
}

fn decode_nibble(byte: u8, index: usize) -> Result<u8, AcceptedInputSteeringCorrelationError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(AcceptedInputSteeringCorrelationError::InvalidHex { index }),
    }
}

/// Typed failure while stabilizing one accepted-input replay factory.
#[derive(Debug, Error)]
pub(in crate::cas_projection) enum AcceptedInputReplayError {
    #[error("accepted-input replay was cancelled before preparation completed")]
    Cancelled,
    #[error("accepted-input replay requires a healthy Beryl home, got {state:?}")]
    HomeNotHealthy {
        state: HomeHealthState,
        expected_home_id: BerylHomeId,
        actual_home_id: BerylHomeId,
        expected_generation: HomeGeneration,
        actual_generation: Option<HomeGeneration>,
    },
    #[error("healthy Beryl home has no generation")]
    HealthyHomeGenerationMissing,
    #[error("accepted-input replay home identity changed")]
    HomeIdentityMismatch {
        expected: BerylHomeId,
        actual: BerylHomeId,
    },
    #[error("accepted-input replay home generation changed")]
    HomeGenerationMismatch {
        expected: HomeGeneration,
        actual: Option<HomeGeneration>,
        state: HomeHealthState,
    },
    #[error("Beryl-home state could not be read while preparing accepted input")]
    HomeRead(#[source] ReadError),
    #[error("Syndic state could not be read while preparing accepted input")]
    SyndicRead(#[source] SyndicReadError),
    #[error("asset state could not be read while preparing accepted input")]
    AssetRead(#[source] AssetReadError),
    #[error("accepted-input image sidecar could not be verified")]
    Sidecar(#[source] SidecarError),
    #[error("accepted input {input_id:?} is missing")]
    AcceptedInputMissing { input_id: SyndicAcceptedInputId },
    #[error("accepted input {input_id:?} changed after stabilization")]
    AcceptedInputChanged { input_id: SyndicAcceptedInputId },
    #[error("accepted input {input_id:?} disagrees with its replay content")]
    AcceptedInputContentMismatch { input_id: SyndicAcceptedInputId },
    #[error("accepted-input content {content_id:?} is missing")]
    ContentMissing { content_id: SyndicContentId },
    #[error("accepted-input content {content_id:?} changed after stabilization")]
    ContentChanged { content_id: SyndicContentId },
    #[error("marker-bearing accepted input has no sealed asset-reference proof")]
    AssetReferenceSetMissing,
    #[error("marker-bearing accepted input has no exact asset owner head")]
    AssetOwnerHeadMissing,
    #[error("accepted-input asset owner, proof, or sealed set disagrees")]
    AssetReferenceSetMismatch,
    #[error("accepted-input replay durable authority is unavailable")]
    ReadUnavailable,
    #[error("accepted-input descriptor evidence is invalid")]
    DescriptorInvalid,
    #[error("accepted input is empty")]
    EmptyInput,
    #[error("accepted-input source identity changed during preparation")]
    SourceIdentityMismatch {
        expected: StreamedInputSourceIdentity,
        actual: StreamedInputSourceIdentity,
    },
    #[error("accepted-input content revision changed during preparation")]
    RevisionDrift {
        expected: StreamedInputSourceRevision,
        actual: StreamedInputSourceRevision,
    },
    #[error("accepted-input image path is not Unicode")]
    RuntimePathNotUnicode,
    #[error("accepted-input image path cannot be projected into the selected runtime")]
    RuntimePathUnmappable,
}

impl From<InputReplayPrepareError> for AcceptedInputReplayError {
    fn from(error: InputReplayPrepareError) -> Self {
        match error {
            InputReplayPrepareError::Cancelled => Self::Cancelled,
            InputReplayPrepareError::HomeNotHealthy {
                state,
                expected_home_id,
                actual_home_id,
                expected_generation,
                actual_generation,
            } => Self::HomeNotHealthy {
                state,
                expected_home_id,
                actual_home_id,
                expected_generation,
                actual_generation,
            },
            InputReplayPrepareError::HealthyHomeGenerationMissing => {
                Self::HealthyHomeGenerationMissing
            }
            InputReplayPrepareError::HomeIdentityMismatch { expected, actual } => {
                Self::HomeIdentityMismatch { expected, actual }
            }
            InputReplayPrepareError::HomeGenerationMismatch {
                expected,
                actual,
                state,
            } => Self::HomeGenerationMismatch {
                expected,
                actual,
                state,
            },
            InputReplayPrepareError::HomeRead(source) => Self::HomeRead(source),
            InputReplayPrepareError::SyndicRead(source) => Self::SyndicRead(source),
            InputReplayPrepareError::AssetRead(source) => Self::AssetRead(source),
            InputReplayPrepareError::Sidecar(source) => Self::Sidecar(source),
            InputReplayPrepareError::AcceptedInputMissing { input_id } => {
                Self::AcceptedInputMissing { input_id }
            }
            InputReplayPrepareError::AcceptedInputChanged { input_id } => {
                Self::AcceptedInputChanged { input_id }
            }
            InputReplayPrepareError::AcceptedInputContentMismatch { input_id } => {
                Self::AcceptedInputContentMismatch { input_id }
            }
            InputReplayPrepareError::ContentMissing { content_id } => {
                Self::ContentMissing { content_id }
            }
            InputReplayPrepareError::ContentChanged { content_id } => {
                Self::ContentChanged { content_id }
            }
            InputReplayPrepareError::AssetReferenceSetMissing => Self::AssetReferenceSetMissing,
            InputReplayPrepareError::AssetOwnerHeadMissing => Self::AssetOwnerHeadMissing,
            InputReplayPrepareError::AssetReferenceSetMismatch => Self::AssetReferenceSetMismatch,
            InputReplayPrepareError::ReadUnavailable => Self::ReadUnavailable,
            InputReplayPrepareError::DescriptorInvalid => Self::DescriptorInvalid,
            InputReplayPrepareError::EmptyInput => Self::EmptyInput,
            InputReplayPrepareError::SourceIdentityMismatch { expected, actual } => {
                Self::SourceIdentityMismatch { expected, actual }
            }
            InputReplayPrepareError::RevisionDrift { expected, actual } => {
                Self::RevisionDrift { expected, actual }
            }
            InputReplayPrepareError::RuntimePathNotUnicode => Self::RuntimePathNotUnicode,
            InputReplayPrepareError::RuntimePathUnmappable => Self::RuntimePathUnmappable,
        }
    }
}

/// Strict parse error for the canonical accepted-input steering correlation.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub(in crate::cas_projection) enum AcceptedInputSteeringCorrelationError {
    #[error("steering correlation has the wrong prefix")]
    WrongPrefix,
    #[error("steering correlation has length {actual}, expected {CORRELATION_BYTES}")]
    WrongLength { actual: usize },
    #[error("steering correlation has a non-lowercase-hex byte at payload index {index}")]
    InvalidHex { index: usize },
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/accepted_input_replay.rs"
    ));
}
