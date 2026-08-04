use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU8, AtomicU64, Ordering},
    },
};

use beryl_model::{CasItemId, CasThreadId, CasTurnId};
use serde::{Serialize, Serializer};
use thiserror::Error;

pub(crate) const COMMAND_EXECUTION_REQUEST_APPROVAL_METHOD: &str =
    "item/commandExecution/requestApproval";
pub(crate) const FILE_CHANGE_REQUEST_APPROVAL_METHOD: &str = "item/fileChange/requestApproval";
pub(crate) const PERMISSIONS_REQUEST_APPROVAL_METHOD: &str = "item/permissions/requestApproval";

/// Maximum decoded UTF-8 length of a string JSON-RPC approval request identity.
pub const APPROVAL_REQUEST_ID_MAX_BYTES: usize = 256;

const UNBOUND_RESPONSE_AUTHORITY: u64 = 0;

/// Closed approval method selected before any unneeded request payload is retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalRequestKind {
    /// Approval for one proposed command execution.
    CommandExecution,
    /// Approval for one proposed file change.
    FileChange,
    /// Approval for one proposed permission expansion.
    Permissions,
}

/// One bounded JSON-RPC identity for an approval server request.
#[derive(Eq, PartialEq)]
pub enum ApprovalRequestId {
    Integer(i64),
    String(Box<str>),
}

impl fmt::Debug for ApprovalRequestId {
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

impl ApprovalRequestId {
    #[must_use]
    pub const fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            Self::String(_) => None,
        }
    }

    #[must_use]
    pub const fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Integer(value) if *value >= 0 => Some(*value as u64),
            Self::Integer(_) | Self::String(_) => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            Self::Integer(_) => None,
        }
    }
}

impl Serialize for ApprovalRequestId {
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

/// Content-free structural failures from incremental approval normalization.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ApprovalRequestSchemaError {
    #[error("approval request envelope does not match the pinned JSON-RPC shape")]
    EnvelopeShape,
    #[error("approval request repeated a required envelope discriminant")]
    DuplicateDiscriminant,
    #[error("approval request repeated a route field")]
    DuplicateRoute,
    #[error("approval request is missing its JSON-RPC request identity")]
    MissingRequestIdentity,
    #[error("approval request is missing its params object")]
    MissingParams,
    #[error("approval request field has the wrong JSON type")]
    WrongType,
    #[error("approval JSON-RPC request identity is invalid")]
    InvalidRequestIdentity,
    #[error("approval route identity is invalid")]
    InvalidRouteIdentity,
    #[error("approval bounded identity exceeds its fixed capacity")]
    IdentityTooLong,
    #[error("approval discarded payload exceeds the pinned structural depth")]
    StructuredDepthExceeded,
    #[error("approval response authority was already bound to a session")]
    ResponseAuthorityAlreadyBound,
}

#[repr(u8)]
/// Whether the exact originating session still owns an approval denial write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalResponseDisposition {
    /// No denial response has been written successfully.
    ResponseRequired,
    /// The session wrote an automatic denial after ordered admission.
    AutoDenied,
    /// The idle-event caller wrote the denial through the originating session.
    Denied,
}

#[derive(Debug)]
struct ApprovalResponseState {
    authority_generation: AtomicU64,
    disposition: AtomicU8,
}

impl ApprovalResponseState {
    const fn new() -> Self {
        Self {
            authority_generation: AtomicU64::new(UNBOUND_RESPONSE_AUTHORITY),
            disposition: AtomicU8::new(ApprovalResponseDisposition::ResponseRequired as u8),
        }
    }

    fn bind(&self, generation: u64) -> Result<(), ApprovalRequestSchemaError> {
        debug_assert_ne!(generation, UNBOUND_RESPONSE_AUTHORITY);
        self.authority_generation
            .compare_exchange(
                UNBOUND_RESPONSE_AUTHORITY,
                generation,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map(|_| ())
            .map_err(|_| ApprovalRequestSchemaError::ResponseAuthorityAlreadyBound)
    }

    fn authority_generation(&self) -> u64 {
        self.authority_generation.load(Ordering::Acquire)
    }

    fn load(&self) -> ApprovalResponseDisposition {
        match self.disposition.load(Ordering::Acquire) {
            0 => ApprovalResponseDisposition::ResponseRequired,
            1 => ApprovalResponseDisposition::AutoDenied,
            2 => ApprovalResponseDisposition::Denied,
            _ => unreachable!("approval response state is internally closed"),
        }
    }

    fn store(&self, disposition: ApprovalResponseDisposition) {
        self.disposition.store(disposition as u8, Ordering::Release);
    }
}

impl PartialEq for ApprovalResponseState {
    fn eq(&self, other: &Self) -> bool {
        self.load() == other.load()
    }
}

impl Eq for ApprovalResponseState {}

struct ApprovalShared {
    request_id: ApprovalRequestId,
    kind: ApprovalRequestKind,
    thread_id: Option<CasThreadId>,
    turn_id: Option<CasTurnId>,
    item_id: Option<CasItemId>,
    response_state: ApprovalResponseState,
}

/// One non-cloneable compact approval event with read-only response-state observation.
pub struct ApprovalRequest {
    shared: Arc<ApprovalShared>,
}

pub(crate) struct ApprovalResponder {
    shared: Arc<ApprovalShared>,
}

impl fmt::Debug for ApprovalResponder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApprovalResponder")
            .field("kind", &self.kind())
            .field("response_disposition", &self.response_disposition())
            .finish_non_exhaustive()
    }
}

pub(crate) struct DecodedApproval {
    request: ApprovalRequest,
    responder: ApprovalResponder,
}

impl ApprovalRequestKind {
    #[must_use]
    pub const fn method(self) -> &'static str {
        match self {
            Self::CommandExecution => COMMAND_EXECUTION_REQUEST_APPROVAL_METHOD,
            Self::FileChange => FILE_CHANGE_REQUEST_APPROVAL_METHOD,
            Self::Permissions => PERMISSIONS_REQUEST_APPROVAL_METHOD,
        }
    }

    #[must_use]
    pub const fn denial_response_interrupts_turn(self) -> bool {
        matches!(self, Self::CommandExecution | Self::FileChange)
    }

    #[must_use]
    pub const fn separate_interruption_required(self) -> bool {
        !self.denial_response_interrupts_turn()
    }
}

impl fmt::Display for ApprovalRequestKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::CommandExecution => "command-execution",
            Self::FileChange => "file-change",
            Self::Permissions => "permission-expansion",
        })
    }
}

impl ApprovalRequest {
    pub(crate) fn decoded(
        request_id: ApprovalRequestId,
        kind: ApprovalRequestKind,
        thread_id: Option<CasThreadId>,
        turn_id: Option<CasTurnId>,
        item_id: Option<CasItemId>,
    ) -> DecodedApproval {
        let shared = Arc::new(ApprovalShared {
            request_id,
            kind,
            thread_id,
            turn_id,
            item_id,
            response_state: ApprovalResponseState::new(),
        });
        DecodedApproval {
            request: Self {
                shared: Arc::clone(&shared),
            },
            responder: ApprovalResponder { shared },
        }
    }

    #[must_use]
    /// Returns the bounded identity used only for the sole JSON-RPC response.
    pub fn request_id(&self) -> &ApprovalRequestId {
        &self.shared.request_id
    }

    #[must_use]
    /// Returns the closed approval method kind.
    pub fn kind(&self) -> ApprovalRequestKind {
        self.shared.kind
    }

    /// Returns whether this request still needs a JSON-RPC response.
    #[must_use]
    pub fn response_disposition(&self) -> ApprovalResponseDisposition {
        self.shared.response_state.load()
    }

    pub(crate) fn response_authority_generation(&self) -> u64 {
        self.shared.response_state.authority_generation()
    }

    pub(crate) fn mark_response_disposition(&self, disposition: ApprovalResponseDisposition) {
        self.shared.response_state.store(disposition);
    }

    #[must_use]
    /// Returns the optional exact thread route retained from the request.
    pub fn thread_id(&self) -> Option<&CasThreadId> {
        self.shared.thread_id.as_ref()
    }

    #[must_use]
    /// Returns the optional exact turn route retained from the request.
    pub fn turn_id(&self) -> Option<&CasTurnId> {
        self.shared.turn_id.as_ref()
    }

    #[must_use]
    /// Returns the optional exact item route retained from the request.
    pub fn item_id(&self) -> Option<&CasItemId> {
        self.shared.item_id.as_ref()
    }
}

impl PartialEq for ApprovalRequest {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.shared, &other.shared)
    }
}

impl Eq for ApprovalRequest {}

impl fmt::Debug for ApprovalRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApprovalRequest")
            .field("kind", &self.kind())
            .field("thread_route_present", &self.thread_id().is_some())
            .field("turn_route_present", &self.turn_id().is_some())
            .field("item_route_present", &self.item_id().is_some())
            .field("response_disposition", &self.response_disposition())
            .finish()
    }
}

impl ApprovalResponder {
    pub(crate) fn matches(&self, request: &ApprovalRequest) -> bool {
        Arc::ptr_eq(&self.shared, &request.shared)
    }

    pub(crate) fn bind_response_authority(
        &self,
        generation: u64,
    ) -> Result<(), ApprovalRequestSchemaError> {
        self.shared.response_state.bind(generation)
    }

    pub(crate) fn request_id(&self) -> &ApprovalRequestId {
        &self.shared.request_id
    }

    pub(crate) fn kind(&self) -> ApprovalRequestKind {
        self.shared.kind
    }

    pub(crate) fn thread_id(&self) -> Option<&CasThreadId> {
        self.shared.thread_id.as_ref()
    }

    pub(crate) fn turn_id(&self) -> Option<&CasTurnId> {
        self.shared.turn_id.as_ref()
    }

    pub(crate) fn response_authority_generation(&self) -> u64 {
        self.shared.response_state.authority_generation()
    }

    pub(crate) fn response_disposition(&self) -> ApprovalResponseDisposition {
        self.shared.response_state.load()
    }

    pub(crate) fn mark_response_disposition(&self, disposition: ApprovalResponseDisposition) {
        self.shared.response_state.store(disposition);
    }
}

impl DecodedApproval {
    pub(crate) fn into_parts(self) -> (ApprovalRequest, ApprovalResponder) {
        (self.request, self.responder)
    }
}
