use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

use beryl_model::{CasThreadId, CasTurnId, DynamicToolCallId, DynamicToolName};
use beryl_stream::PageLease;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use thiserror::Error;

pub(crate) const DYNAMIC_TOOL_CALL_METHOD: &str = "item/tool/call";

include!("dynamic_tool/registration.rs");

/// Maximum decoded UTF-8 length of a string JSON-RPC dynamic-tool request identity.
pub const DYNAMIC_TOOL_CALL_REQUEST_ID_MAX_BYTES: usize = 256;

/// Maximum decoded UTF-8 length of one optional dynamic-tool namespace.
pub const DYNAMIC_TOOL_NAMESPACE_MAX_BYTES: usize = 128;

const UNBOUND_RESPONSE_AUTHORITY: u64 = 0;

/// One bounded JSON-RPC identity for a dynamic-tool server request.
#[derive(Eq, PartialEq)]
pub enum DynamicToolCallRequestId {
    /// A JSON-RPC integer request identity.
    Integer(i64),
    /// A bounded decoded JSON-RPC string request identity.
    String(Box<str>),
}

/// Whether the exact originating session still owns the dynamic-tool response write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DynamicToolCallResponseDisposition {
    /// The exact originating session still owes one response.
    ResponseRequired,
    /// The exact originating session successfully wrote the response.
    Responded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum DynamicToolCallIngressState {
    Building,
    Sealed,
    Abandoned,
}

struct DynamicToolCallShared {
    request_id: DynamicToolCallRequestId,
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    call_id: DynamicToolCallId,
    namespace: Option<Box<str>>,
    tool: DynamicToolName,
    response_authority_generation: u64,
    ingress_state: AtomicU8,
    response_disposition: AtomicU8,
}

/// One non-cloneable compact dynamic-tool call with exact-session response authority.
pub struct DynamicToolCall {
    shared: Arc<DynamicToolCallShared>,
}

pub(crate) struct DynamicToolCallIngress {
    shared: Arc<DynamicToolCallShared>,
}

/// Structural controls for one dynamic-tool argument value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicToolArgumentControl {
    /// Starts an object or array.
    ContainerStart(DynamicToolArgumentContainer),
    /// Ends the matching object or array.
    ContainerEnd(DynamicToolArgumentContainer),
    /// Starts one decoded object name, string, or number scalar.
    ScalarStart(DynamicToolArgumentScalarKind),
    /// Ends the matching decoded scalar.
    ScalarEnd(DynamicToolArgumentScalarKind),
    /// Emits one JSON boolean value.
    Boolean(bool),
    /// Emits one JSON null value.
    Null,
}

/// Closed JSON container kinds forwarded without feature semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicToolArgumentContainer {
    /// A JSON object.
    Object,
    /// A JSON array.
    Array,
}

/// Closed decoded scalar kinds forwarded without feature semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicToolArgumentScalarKind {
    /// A decoded JSON object member name.
    ObjectName,
    /// A decoded JSON string value.
    String,
    /// The exact lexical bytes of a JSON number.
    Number,
}

/// One decoded scalar fragment owning the connection's sole foreground page lease.
pub struct DynamicToolArgumentFragment {
    kind: DynamicToolArgumentScalarKind,
    offset: u64,
    lease: PageLease,
}

/// Why an incomplete dynamic-tool argument stream was abandoned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicToolCallAbandonReason {
    /// Envelope or argument structure was incompatible with the pinned protocol.
    SchemaFailure,
    /// The sink's local capacity was full.
    CapacityFull,
    /// Sink acknowledgement exceeded its deadline.
    Timeout,
    /// The selected sink receiver disappeared.
    ReceiverLost,
    /// The selected sink cancelled the call.
    Cancelled,
    /// The selected sink rejected an operation.
    SinkRejected,
    /// The transport ended while the call was still being decoded.
    TransportLost,
}

/// Content-free structural failures from incremental dynamic-tool normalization.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum DynamicToolCallSchemaError {
    /// The JSON-RPC envelope did not match the pinned structural shape.
    #[error("dynamic-tool request envelope does not match the pinned JSON-RPC shape")]
    EnvelopeShape,
    /// A discriminating envelope field appeared more than once.
    #[error("dynamic-tool request repeated a discriminating field")]
    DuplicateField,
    /// Envelope fields did not follow the pinned producer order.
    #[error("dynamic-tool request fields did not follow the pinned order")]
    ReorderedField,
    /// A required envelope field was absent.
    #[error("dynamic-tool request omitted a required field")]
    MissingField,
    /// An envelope field used an incompatible JSON scalar or container kind.
    #[error("dynamic-tool request field has the wrong JSON type")]
    WrongType,
    /// The JSON-RPC request identity was not a bounded string or exact integer.
    #[error("dynamic-tool request identity is invalid")]
    InvalidRequestIdentity,
    /// A route, call, namespace, or tool identity was invalid.
    #[error("dynamic-tool route, call, namespace, or tool identity is invalid")]
    InvalidIdentity,
    /// A bounded identity exceeded its fixed decoded capacity.
    #[error("dynamic-tool bounded identity exceeds its fixed capacity")]
    IdentityTooLong,
    /// Argument structure exceeded the supported nesting depth.
    #[error("dynamic-tool arguments exceeded depth 128")]
    StructuredDepthExceeded,
    /// A dynamic request arrived before an ordered sink was installed.
    #[error("dynamic-tool request arrived before its ordered registry sink was bound")]
    OrderedSinkUnbound,
}

/// Failure while decoding or forwarding one dynamic-tool call.
#[derive(Debug, Error)]
pub enum DynamicToolCallError {
    /// Incremental envelope or argument normalization failed.
    #[error(transparent)]
    Schema(#[from] DynamicToolCallSchemaError),
    /// The ordered sink failed or rejected one submitted operation.
    #[error(transparent)]
    Submit(#[from] crate::OrderedTurnStreamSubmitCause),
    /// The ordered sink returned a completion for a different operation kind.
    #[error("ordered turn-stream sink returned the wrong dynamic-tool completion kind")]
    UnexpectedCompletion,
}

include!("dynamic_tool/response.rs");

impl DynamicToolCallRequestId {
    /// Returns the integer identity, or `None` for a string identity.
    #[must_use]
    pub const fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            Self::String(_) => None,
        }
    }

    /// Returns the string identity, or `None` for an integer identity.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Integer(_) => None,
            Self::String(value) => Some(value),
        }
    }
}

impl fmt::Debug for DynamicToolCallRequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Integer(_) => formatter.write_str("Integer([redacted])"),
            Self::String(value) => formatter
                .debug_struct("String")
                .field("bytes", &value.len())
                .finish_non_exhaustive(),
        }
    }
}

impl Serialize for DynamicToolCallRequestId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Integer(value) => serializer.serialize_i64(*value),
            Self::String(value) => serializer.serialize_str(value),
        }
    }
}

impl DynamicToolCall {
    pub(crate) fn decoded(
        request_id: DynamicToolCallRequestId,
        thread_id: CasThreadId,
        turn_id: CasTurnId,
        call_id: DynamicToolCallId,
        namespace: Option<Box<str>>,
        tool: DynamicToolName,
        response_authority_generation: u64,
    ) -> (Self, DynamicToolCallIngress) {
        debug_assert_ne!(response_authority_generation, UNBOUND_RESPONSE_AUTHORITY);
        let shared = Arc::new(DynamicToolCallShared {
            request_id,
            thread_id,
            turn_id,
            call_id,
            namespace,
            tool,
            response_authority_generation,
            ingress_state: AtomicU8::new(DynamicToolCallIngressState::Building as u8),
            response_disposition: AtomicU8::new(
                DynamicToolCallResponseDisposition::ResponseRequired as u8,
            ),
        });
        (
            Self {
                shared: Arc::clone(&shared),
            },
            DynamicToolCallIngress { shared },
        )
    }

    /// Returns the exact JSON-RPC request identity used for the response.
    #[must_use]
    pub fn request_id(&self) -> &DynamicToolCallRequestId {
        &self.shared.request_id
    }

    /// Returns the pinned JSON-RPC method for dynamic-tool calls.
    #[must_use]
    pub const fn method(&self) -> &'static str {
        DYNAMIC_TOOL_CALL_METHOD
    }

    /// Returns the bounded CAS thread identity carried by the request.
    #[must_use]
    pub fn thread_id(&self) -> &CasThreadId {
        &self.shared.thread_id
    }

    /// Returns the bounded CAS turn identity carried by the request.
    #[must_use]
    pub fn turn_id(&self) -> &CasTurnId {
        &self.shared.turn_id
    }

    /// Returns the bounded call identity carried by the request.
    #[must_use]
    pub fn call_id(&self) -> &DynamicToolCallId {
        &self.shared.call_id
    }

    /// Returns the bounded installed tool name selected before arguments.
    #[must_use]
    pub fn tool(&self) -> &DynamicToolName {
        &self.shared.tool
    }

    /// Returns the optional bounded installed namespace selected before arguments.
    #[must_use]
    pub fn namespace(&self) -> Option<&str> {
        self.shared.namespace.as_deref()
    }

    /// Returns whether the exact owning session still owes a response.
    #[must_use]
    pub fn response_disposition(&self) -> DynamicToolCallResponseDisposition {
        load_response_disposition(&self.shared)
    }

    /// Returns whether the complete argument stream passed structural validation and was sealed.
    #[must_use]
    pub fn is_sealed(&self) -> bool {
        load_ingress_state(&self.shared) == DynamicToolCallIngressState::Sealed
    }

    pub(crate) fn response_authority_generation(&self) -> u64 {
        self.shared.response_authority_generation
    }

    pub(crate) fn mark_responded(&self) {
        self.shared.response_disposition.store(
            DynamicToolCallResponseDisposition::Responded as u8,
            Ordering::Release,
        );
    }
}

impl fmt::Debug for DynamicToolCall {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DynamicToolCall")
            .field("thread_id", self.thread_id())
            .field("turn_id", self.turn_id())
            .field("call_id", self.call_id())
            .field("namespace", &self.namespace())
            .field("tool", self.tool())
            .field("sealed", &self.is_sealed())
            .field("response_disposition", &self.response_disposition())
            .finish()
    }
}

impl DynamicToolCallIngress {
    pub(crate) fn seal(&self) {
        self.shared
            .ingress_state
            .store(DynamicToolCallIngressState::Sealed as u8, Ordering::Release);
    }

    pub(crate) fn abandon(&self) {
        self.shared.ingress_state.store(
            DynamicToolCallIngressState::Abandoned as u8,
            Ordering::Release,
        );
    }
}

impl DynamicToolArgumentFragment {
    pub(crate) const fn new(
        kind: DynamicToolArgumentScalarKind,
        offset: u64,
        lease: PageLease,
    ) -> Self {
        Self {
            kind,
            offset,
            lease,
        }
    }

    /// Returns the decoded scalar kind associated with these bytes.
    #[must_use]
    pub const fn kind(&self) -> DynamicToolArgumentScalarKind {
        self.kind
    }

    /// Returns the byte offset within the current decoded scalar.
    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }

    /// Returns the committed bytes in the owned page lease.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.lease.as_slice()
    }

    /// Consumes the fragment and returns its page lease for same-page exchange.
    #[must_use]
    pub fn into_lease(self) -> PageLease {
        self.lease
    }
}

fn load_ingress_state(shared: &DynamicToolCallShared) -> DynamicToolCallIngressState {
    match shared.ingress_state.load(Ordering::Acquire) {
        0 => DynamicToolCallIngressState::Building,
        1 => DynamicToolCallIngressState::Sealed,
        2 => DynamicToolCallIngressState::Abandoned,
        _ => unreachable!("dynamic-tool ingress state is internally closed"),
    }
}

fn load_response_disposition(shared: &DynamicToolCallShared) -> DynamicToolCallResponseDisposition {
    match shared.response_disposition.load(Ordering::Acquire) {
        0 => DynamicToolCallResponseDisposition::ResponseRequired,
        1 => DynamicToolCallResponseDisposition::Responded,
        _ => unreachable!("dynamic-tool response state is internally closed"),
    }
}

pub(crate) fn dynamic_abandon_reason(
    error: crate::OrderedTurnStreamSubmitCause,
) -> DynamicToolCallAbandonReason {
    match error {
        crate::OrderedTurnStreamSubmitCause::Unavailable
        | crate::OrderedTurnStreamSubmitCause::ReceiverLost => {
            DynamicToolCallAbandonReason::ReceiverLost
        }
        crate::OrderedTurnStreamSubmitCause::CapacityFull => {
            DynamicToolCallAbandonReason::CapacityFull
        }
        crate::OrderedTurnStreamSubmitCause::Timeout => DynamicToolCallAbandonReason::Timeout,
        crate::OrderedTurnStreamSubmitCause::Cancelled => DynamicToolCallAbandonReason::Cancelled,
        crate::OrderedTurnStreamSubmitCause::Rejected(_) => {
            DynamicToolCallAbandonReason::SinkRejected
        }
    }
}
