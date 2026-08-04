use std::io::Read;

use thiserror::Error;

mod provider;
mod response;

pub(crate) use response::{ResponseExpectation, ResponseExpectationSlot, ResponseFamily};

use crate::turn::{StreamedUserMessageCorrelationError, StreamedUserMessageVerifierHandle};

const REDACTED_INVALID_JSON: &str = "[redacted incoming JSON]";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DecodeStats {
    pub(crate) discarded_image_result_bytes: usize,
    pub(crate) verified_user_text_wire_bytes: usize,
    pub(crate) maximum_buffered_input_bytes: usize,
    pub(crate) input_bytes: usize,
}

pub(crate) enum DecodedIncoming {
    Approval(crate::turn::DecodedApproval),
    OrderedHandled,
    DiscardedNotification,
    Response {
        id: u64,
        result: crate::BoundedResponseResult,
    },
    Rejection {
        id: u64,
        error: crate::JsonRpcError,
    },
}

pub(crate) struct DecodedValue {
    pub(crate) incoming: DecodedIncoming,
    pub(crate) stats: DecodeStats,
}

pub(crate) enum DecodeReaderError {
    Json(serde_json::Error),
    Correlation(StreamedUserMessageCorrelationError),
    Steering(crate::SteeringUserMessageError),
    Approval {
        kind: crate::ApprovalRequestKind,
        source: crate::ApprovalRequestSchemaError,
    },
    Provider(crate::ProviderObservationError),
    DynamicTool(crate::DynamicToolCallError),
    Ordered(Box<crate::OrderedTurnStreamSubmitError>),
    OrderedUnexpectedCompletion,
    Envelope(ForegroundIngressError),
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ForegroundIngressError {
    #[error("incoming foreground envelope entered fixed quarantine")]
    Quarantined,
    #[error("unsupported server request was structurally consumed")]
    UnsupportedServerRequest,
    #[error("known foreground control family is unavailable in this cutover phase")]
    KnownControlUnavailable,
    #[error("normal turn terminal did not match the pinned field order and shape")]
    MalformedNormalTurnTerminal,
    #[error("thread status change did not match the pinned loaded-thread shape")]
    MalformedThreadStatusChanged,
    #[error("thread close did not match the pinned field order and shape")]
    MalformedThreadClosed,
    #[error("turn start did not match the pinned field order and in-progress shape")]
    MalformedTurnStarted,
    #[error("a response arrived while no response expectation was installed")]
    IdleResponse,
    #[error("response id did not match the installed exact expectation")]
    ResponseIdMismatch { expected: u64, actual: Option<u64> },
    #[error("response envelope did not match the pinned field order and shape")]
    MalformedResponse,
    #[error("response family {method} is unavailable in this cutover phase")]
    ResponseFamilyUnavailable { method: &'static str },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KnownControlFamily {
    Compact,
    Approval,
    DynamicTool,
    Provider,
}

pub(crate) fn decode_reader_with_provider<'a, R>(
    reader: R,
    input_buffer_bytes: usize,
    verifier: Option<StreamedUserMessageVerifierHandle<'a>>,
    sink: Option<&'a mut dyn crate::OrderedTurnStreamSink>,
    response_authority_generation: u64,
    response_expectation: &mut ResponseExpectationSlot,
) -> Result<DecodedValue, DecodeReaderError>
where
    R: Read,
{
    provider::decode(
        reader,
        input_buffer_bytes,
        verifier,
        sink,
        response_authority_generation,
        response_expectation,
    )
}

pub(crate) fn decode_reader<R>(
    reader: R,
    input_buffer_bytes: usize,
    response_expectation: &mut ResponseExpectationSlot,
) -> Result<DecodedValue, DecodeReaderError>
where
    R: Read,
{
    decode_reader_with_provider(
        reader,
        input_buffer_bytes,
        None,
        None,
        0,
        response_expectation,
    )
}

pub(crate) fn redacted_invalid_json() -> String {
    REDACTED_INVALID_JSON.to_string()
}
