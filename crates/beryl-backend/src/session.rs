use std::{
    io,
    path::PathBuf,
    process::ChildStdin,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, RecvTimeoutError},
    },
    time::Duration,
};

use beryl_model::{CasThreadId, CasTurnId};
use serde::Serialize;
use thiserror::Error;
use tracing::warn;

mod bounded_request;
mod compaction;
mod incoming;
mod interruption;
mod ordered_turn_stream;
pub(crate) mod outbound;
mod pre_bind;
mod streamed_turn_start;
mod streamed_turn_steer;
mod thread_injection;

pub use compaction::{
    CompactThreadDisposition, CompactThreadOutcome, CompactThreadRequest,
    CompactionAttemptCorrelation, ExactForegroundThread, ExactForegroundThreadAuthorization,
};

use incoming::{IncomingMessage, ReceiveOutcome};
use pre_bind::PreBindApprovalPrefix;

use crate::{
    ApprovalInterruption, ApprovalRequest, ApprovalRequestKind, ApprovalRequestSchemaError,
    ApprovalResponseDisposition, BackendCommandLineError, BackendConfigDefaults,
    CompatibilityError, DynamicToolCall, DynamicToolCallError, DynamicToolCallResponse,
    DynamicToolCallResponseDisposition, InitializeResponse, JsonRpcError, StartedTurn,
    ThreadInjectionSourceError, TurnStartOptions,
    thread_lineage::LoadedThreadSession,
    turn::{
        ApprovalResponder, StreamedInputSource, StreamedInputSourceError,
        StreamedUserMessageCorrelationError, StreamedUserMessageVerifierHandle,
        StreamedUserMessageVerifierSlot,
    },
    websocket_transport::{ForegroundWebSocketTransport, RequestOnlyWebSocketTransport},
};

use outbound::{DispatchProgress, OutboundWriteFailure, StdioJsonWriter, write_json};

static NEXT_APPROVAL_RESPONSE_AUTHORITY_GENERATION: AtomicU64 = AtomicU64::new(0);

fn allocate_approval_response_authority_generation() -> Result<u64, ManagedBackendError> {
    NEXT_APPROVAL_RESPONSE_AUTHORITY_GENERATION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |generation| {
            generation.checked_add(1)
        })
        .ok()
        .and_then(|generation| generation.checked_add(1))
        .ok_or(ManagedBackendError::ApprovalResponseAuthorityExhausted)
}

pub struct ManagedBackendSession {
    transport: BackendClientTransport,
    managed_launch_provenance: Option<crate::server::ManagedLaunchProvenance>,
    initialize: Option<InitializeResponse>,
    initialized_notification_profile: Option<InitializedNotificationProfile>,
    next_request_id: u64,
    approval_response_authority_generation: u64,
    streamed_user_message_verifier: StreamedUserMessageVerifierSlot,
    response_expectation: crate::incoming_json::ResponseExpectationSlot,
    foreground_authorization_epoch: u64,
    exact_foreground_thread: Option<ExactForegroundThread>,
    exact_foreground_turn: Option<crate::ExactForegroundTurn>,
    pre_bind_approvals: PreBindApprovalPrefix,
    ordered_turn_stream_sink: Option<Box<dyn crate::OrderedTurnStreamSink>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InitializedNotificationProfile {
    FullTurnStream,
    OptedOut,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ManagedBackendReleaseAdmission {
    initialize: InitializeResponse,
    config_defaults: BackendConfigDefaults,
    launch_provenance: crate::server::ManagedLaunchProvenance,
}

#[derive(Debug, Error)]
pub enum ManagedBackendError {
    #[error("failed to build backend command line")]
    BuildCommandLine {
        #[from]
        source: BackendCommandLineError,
    },
    #[error("failed to spawn backend process {program}")]
    Spawn {
        program: String,
        #[source]
        source: io::Error,
    },
    #[error("backend process did not expose redirected {stream_name}")]
    MissingPipe { stream_name: &'static str },
    #[error("failed to start the managed backend stderr reader")]
    SpawnStderrReader {
        #[source]
        source: io::Error,
    },
    #[error("the managed backend process-generation domain is exhausted")]
    ProcessGenerationExhausted,
    #[error("the managed backend stderr reader panicked")]
    StderrReaderPanicked,
    #[error("managed backend launch identity does not match release admission")]
    ManagedLaunchIdentityMismatch,
    #[error("failed to write {method} request to backend transport")]
    WriteRequest {
        method: String,
        #[source]
        source: io::Error,
    },
    #[error("backend transport message was not valid JSON: {line}")]
    InvalidJsonLine {
        line: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to serialize {method} request payload")]
    SerializeRequest {
        method: String,
        #[source]
        source: serde_json::Error,
    },
    /// A replayable descriptor source failed while the specialized request was encoded.
    #[error("streamed input source failed while serializing {method} request")]
    StreamedInputSource {
        /// Constant JSON-RPC method whose request was being encoded.
        method: String,
        /// Exact typed source or page-validation failure.
        #[source]
        source: StreamedInputSourceError,
        /// Whether any underlying transport byte was successfully written first.
        transport_bytes_written: bool,
    },
    /// A recovery source failed or disagreed while injection was encoded.
    #[error("thread-injection source failed while serializing {method} request")]
    ThreadInjectionSource {
        /// Constant JSON-RPC method whose request was being encoded.
        method: String,
        /// Exact typed source or replay-validation failure.
        #[source]
        source: ThreadInjectionSourceError,
        /// Whether any underlying transport byte was successfully written first.
        transport_bytes_written: bool,
    },
    /// This transport cannot stream a recovery source into one injection request.
    #[error("transport {transport} does not support thread injection for {method}")]
    ThreadInjectionTransportUnsupported {
        method: String,
        transport: &'static str,
    },
    /// This transport cannot provide request-scoped streamed echo verification.
    #[error("transport {transport} does not support streamed input for {method}")]
    StreamedInputTransportUnsupported {
        method: String,
        transport: &'static str,
    },
    /// An incremental echoed user-message disagreed with its frozen request.
    #[error("streamed user-message correlation failed while handling {method}")]
    StreamedUserMessageCorrelation {
        method: String,
        #[source]
        source: StreamedUserMessageCorrelationError,
        /// Whether transport bytes could already have been dispatched.
        transport_bytes_written: bool,
    },
    #[error("provider observation failed while handling {method}")]
    ProviderObservation {
        method: String,
        #[source]
        source: crate::ProviderObservationError,
    },
    #[error("delayed steering user-message failed while handling {method}")]
    SteeringUserMessage {
        method: String,
        #[source]
        source: crate::SteeringUserMessageError,
    },
    #[error("dynamic-tool call failed while handling {method}")]
    DynamicToolCall {
        method: String,
        #[source]
        source: DynamicToolCallError,
    },
    #[error("foreground JSON ingress failed while handling {method}")]
    ForegroundIngress {
        method: String,
        #[source]
        source: crate::ForegroundIngressError,
    },
    #[error("pre-bind compact-control capacity {capacity} was exhausted while handling {method}")]
    PreBindControlCapacityExceeded { method: String, capacity: usize },
    #[error("ordered turn-stream submission failed while handling {method}")]
    OrderedTurnStream {
        method: String,
        #[source]
        source: Box<crate::OrderedTurnStreamSubmitError>,
    },
    #[error("ordered turn-stream sink returned the wrong completion while handling {method}")]
    OrderedTurnStreamUnexpectedCompletion { method: String },
    #[error("ordered turn-stream progress requires a bound sink")]
    OrderedTurnStreamSinkUnbound,
    /// A permission denial cannot be written before an ordered durable stop owner exists.
    #[error("permission approval arrived before its ordered durable stop owner was bound")]
    PermissionApprovalStopOwnerUnbound,
    #[error("backend request {method} timed out after {timeout:?}")]
    RequestTimeout { method: String, timeout: Duration },
    #[error("backend process exited while waiting for {method}")]
    ProcessExited { method: String },
    #[error("failed to query managed backend process status for {launch}")]
    QueryProcessStatus {
        launch: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to terminate managed backend process {launch}")]
    TerminateProcess {
        launch: String,
        #[source]
        source: io::Error,
    },
    #[error("managed backend process {launch} did not exit within {timeout:?}")]
    ShutdownTimeout { launch: String, timeout: Duration },
    #[error("failed to create managed backend process job for {launch}")]
    CreateProcessJob {
        launch: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to configure managed backend process job for {launch}")]
    ConfigureProcessJob {
        launch: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to assign managed backend process to job for {launch}")]
    AssignProcessToJob {
        launch: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to terminate managed backend process job for {launch}")]
    TerminateProcessJob {
        launch: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to spawn WSL process-group cleanup in distro {distro_name}")]
    SpawnWslProcessGroupCleanup {
        distro_name: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to query WSL process-group cleanup status in distro {distro_name}")]
    QueryWslProcessGroupCleanupStatus {
        distro_name: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to terminate WSL process-group cleanup in distro {distro_name}")]
    TerminateWslProcessGroupCleanup {
        distro_name: String,
        #[source]
        source: io::Error,
    },
    #[error("WSL process-group cleanup in distro {distro_name} did not finish within {timeout:?}")]
    WslProcessGroupCleanupTimeout {
        distro_name: String,
        timeout: Duration,
    },
    #[error(
        "WSL process-group cleanup in distro {distro_name} exited unsuccessfully with {status}"
    )]
    WslProcessGroupCleanupFailed {
        distro_name: String,
        status: std::process::ExitStatus,
    },
    #[error("backend transport closed while waiting for {method}")]
    TransportClosed { method: String },
    #[error("failed to choose a loopback WebSocket port for the managed backend")]
    SelectWebSocketPort {
        #[source]
        source: io::Error,
    },
    #[error("failed to generate managed backend WebSocket capability token")]
    GenerateWebSocketToken {
        #[source]
        source: getrandom::Error,
    },
    #[error("failed to create managed backend WebSocket token file {path}")]
    CreateWebSocketTokenFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to write managed backend WebSocket token file {path}")]
    WriteWebSocketTokenFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to clean up managed backend WebSocket token file {path}")]
    CleanUpWebSocketTokenFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to connect to managed backend WebSocket endpoint {endpoint}")]
    ConnectWebSocket {
        endpoint: String,
        #[source]
        source: ManagedWebSocketError,
    },
    #[error("managed backend WebSocket transport failed while handling {method}: {source}")]
    WebSocketTransport {
        method: String,
        endpoint: String,
        #[source]
        source: ManagedWebSocketError,
    },
    #[error("backend returned a JSON-RPC error for {method}: {error}")]
    RequestFailed {
        method: String,
        error: Box<JsonRpcError>,
    },
    #[error("backend client session has already completed initialization")]
    ClientAlreadyInitialized,
    #[error("backend {profile} initialization requires its immutable {profile} transport profile")]
    InitializationProfileMismatch { profile: &'static str },
    #[error("backend request identity space is exhausted before {method}")]
    RequestIdExhausted { method: &'static str },
    #[error("backend response expectation is unavailable before {method}")]
    ResponseExpectationUnavailable { method: &'static str },
    #[error("exact foreground authorization belongs to a different backend session")]
    ExactForegroundAuthorizationMismatch,
    #[error("exact foreground authorization was revoked by a later target cut")]
    ExactForegroundAuthorizationStale,
    #[error("the backend session has no exact foreground turn bound")]
    ExactForegroundTurnUnbound,
    #[error("the backend session already has an exact foreground turn bound")]
    ExactForegroundTurnAlreadyBound,
    #[error("the requested exact foreground turn does not match the backend session binding")]
    ExactForegroundTurnMismatch,
    #[error("the backend session has no exact foreground thread bound")]
    ExactForegroundThreadUnbound,
    #[error("the backend session already has an exact foreground thread bound")]
    ExactForegroundThreadAlreadyBound,
    #[error("the requested exact foreground thread does not match the backend session binding")]
    ExactForegroundThreadMismatch,
    #[error("exact foreground authorization epoch is exhausted")]
    ExactForegroundAuthorizationEpochExhausted,
    #[error("backend request {method} requires the immutable {required_profile} transport profile")]
    RequestProfileMismatch {
        method: &'static str,
        required_profile: &'static str,
    },
    #[error("backend {method} response did not match its closed result family")]
    UnexpectedBoundedResponse { method: &'static str },
    #[error("release admission requires production managed-launch provenance")]
    ReleaseAdmissionManagedLaunchProvenanceMissing,
    #[error("release admission config/read did not prove both required sessionFlags settings")]
    ReleaseAdmissionEffectiveConfigUnproven,
    /// The method-owned response decoder has not yet been restored after ordinary ingress removal.
    #[error("backend response family {method} is unavailable before dispatch")]
    ResponseFamilyUnavailable { method: &'static str },
    #[error("{kind} approval response was already sent")]
    ApprovalResponseAlreadySent { kind: ApprovalRequestKind },
    #[error("{kind} approval request does not belong to this backend session")]
    ApprovalResponseAuthorityMismatch { kind: ApprovalRequestKind },
    #[error("backend-session approval response authority generation is exhausted")]
    ApprovalResponseAuthorityExhausted,
    #[error("dynamic-tool response was already sent")]
    DynamicToolResponseAlreadySent,
    #[error("dynamic-tool call does not belong to this backend session")]
    DynamicToolResponseAuthorityMismatch,
    #[error("dynamic-tool call arguments were not sealed successfully")]
    DynamicToolResponseBeforeSeal,
    #[error("invalid compact {kind} approval request")]
    InvalidApprovalRequest {
        kind: ApprovalRequestKind,
        #[source]
        source: ApprovalRequestSchemaError,
    },
    #[error("{kind} approval routing acknowledged an incompatible interruption fact {actual:?}")]
    ApprovalInterruptionMismatch {
        kind: ApprovalRequestKind,
        actual: ApprovalInterruption,
    },
    #[error("exact approval target failed locally: {cause}")]
    ApprovalTargetFailed {
        request: ApprovalRequest,
        cause: crate::OrderedTurnStreamSubmitCause,
    },
    #[error("failed to write the {kind} approval denial response")]
    ApprovalDenialWrite {
        kind: ApprovalRequestKind,
        #[source]
        source: Box<ManagedBackendError>,
    },
    #[error(
        "backend {method} response named CAS thread {actual:?} instead of expected {expected:?}"
    )]
    ThreadResponseIdentityMismatch {
        method: String,
        expected: CasThreadId,
        actual: CasThreadId,
    },
    #[error(
        "backend {method} response reused source CAS thread {source_thread:?} as its fork result"
    )]
    ForkResponseReusedSource {
        method: String,
        source_thread: CasThreadId,
    },
    #[error("backend {method} response named CAS turn {actual:?} instead of expected {expected:?}")]
    TurnResponseIdentityMismatch {
        method: String,
        expected: CasTurnId,
        actual: CasTurnId,
    },
    #[error("backend response line did not match JSON-RPC response or notification shape")]
    UnexpectedMessageShape,
    #[error("backend client session has not completed its initialize handshake")]
    ClientNotInitialized,
    #[error(transparent)]
    Compatibility(#[from] CompatibilityError),
}

impl ManagedBackendError {
    /// Returns whether this failure proves that the exact client connection can no longer
    /// authorize loaded-thread subscriptions.
    #[must_use]
    pub const fn invalidates_connection_authority(&self) -> bool {
        matches!(
            self,
            Self::WriteRequest { .. }
                | Self::InvalidJsonLine { .. }
                | Self::RequestTimeout { .. }
                | Self::ProcessExited { .. }
                | Self::TransportClosed { .. }
                | Self::WebSocketTransport { .. }
                | Self::ThreadResponseIdentityMismatch { .. }
                | Self::ForkResponseReusedSource { .. }
                | Self::TurnResponseIdentityMismatch { .. }
                | Self::UnexpectedMessageShape
                | Self::InvalidApprovalRequest { .. }
                | Self::ApprovalInterruptionMismatch { .. }
                | Self::ApprovalDenialWrite { .. }
                | Self::ProviderObservation { .. }
                | Self::SteeringUserMessage { .. }
                | Self::DynamicToolCall { .. }
                | Self::ForegroundIngress { .. }
                | Self::PreBindControlCapacityExceeded { .. }
                | Self::PermissionApprovalStopOwnerUnbound
                | Self::OrderedTurnStream { .. }
                | Self::OrderedTurnStreamUnexpectedCompletion { .. }
                | Self::UnexpectedBoundedResponse { .. }
        ) || matches!(
            self,
            Self::StreamedInputSource {
                transport_bytes_written: true,
                ..
            } | Self::StreamedUserMessageCorrelation {
                transport_bytes_written: true,
                ..
            } | Self::ThreadInjectionSource {
                transport_bytes_written: true,
                ..
            }
        )
    }
}

/// Exact normalized outcome of one non-idempotent backend request.
///
/// `turn/start`, `turn/steer`, and `thread/compact/start` have no provider idempotency key or
/// authoritative delivery readback. Only [`Self::ProvenNotDispatched`] can
/// ever be eligible for caller-owned retry policy, and that variant proves
/// dispatch state rather than a transient cause; callers must classify its
/// error separately. A method-specific exact rejection may have its own
/// policy. [`Self::CompletionUnknown`] means the request may have crossed the
/// transport and must not be replayed automatically.
#[must_use = "non-idempotent request outcomes must be classified"]
#[derive(Debug)]
pub enum NonIdempotentRequestOutcome<T> {
    /// CAS returned one matching response that decoded and passed identity checks.
    ExactResponse { response: T },
    /// CAS returned one matching structured JSON-RPC rejection.
    ExactRejection { error: JsonRpcError },
    /// Local evidence proves no request bytes were offered to the transport.
    ProvenNotDispatched { error: Box<ManagedBackendError> },
    /// The request may have been dispatched, but no authoritative outcome survived.
    CompletionUnknown { error: Box<ManagedBackendError> },
}

/// Normalized outcome of one `turn/start` request.
pub type TurnStartOutcome = NonIdempotentRequestOutcome<StartedTurn>;
/// Normalized outcome of one specialized streamed `turn/steer` request.
pub type TurnSteerOutcome = NonIdempotentRequestOutcome<crate::SteeredTurn>;

impl<T> NonIdempotentRequestOutcome<T> {
    fn map_exact_response<U>(self, map: impl FnOnce(T) -> U) -> NonIdempotentRequestOutcome<U> {
        match self {
            Self::ExactResponse { response } => NonIdempotentRequestOutcome::ExactResponse {
                response: map(response),
            },
            Self::ExactRejection { error } => NonIdempotentRequestOutcome::ExactRejection { error },
            Self::ProvenNotDispatched { error } => {
                NonIdempotentRequestOutcome::ProvenNotDispatched { error }
            }
            Self::CompletionUnknown { error } => {
                NonIdempotentRequestOutcome::CompletionUnknown { error }
            }
        }
    }
}

#[derive(Debug)]
pub struct ManagedWebSocketError {
    message: String,
    io_error_kind: Option<io::ErrorKind>,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl ManagedWebSocketError {
    pub fn protocol(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            io_error_kind: None,
            source: None,
        }
    }

    pub fn io_error_kind(&self) -> Option<io::ErrorKind> {
        self.io_error_kind
    }

    pub(crate) fn from_io(source: io::Error) -> Self {
        let io_error_kind = Some(source.kind());
        Self {
            message: format!("i/o error: {source}"),
            io_error_kind,
            source: Some(Box::new(source)),
        }
    }

    pub(crate) fn from_handshake(source: soketto::handshake::Error) -> Self {
        let io_error_kind = match &source {
            soketto::handshake::Error::Io(error) => Some(error.kind()),
            _ => None,
        };
        Self {
            message: format!("handshake failed: {source}"),
            io_error_kind,
            source: Some(Box::new(source)),
        }
    }

    pub(crate) fn from_frame(source: soketto::base::Error) -> Self {
        let io_error_kind = match &source {
            soketto::base::Error::Io(error) => Some(error.kind()),
            _ => None,
        };
        Self {
            message: format!("frame error: {source}"),
            io_error_kind,
            source: Some(Box::new(source)),
        }
    }

    pub(crate) fn from_mask_generation(source: getrandom::Error) -> Self {
        Self {
            message: format!("failed to generate WebSocket mask: {source}"),
            io_error_kind: None,
            source: Some(Box::new(source)),
        }
    }
}

impl std::fmt::Display for ManagedWebSocketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ManagedWebSocketError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|source| source as _)
    }
}

impl ManagedBackendSession {
    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    pub fn last_websocket_ingress_test_metrics(
        &self,
    ) -> Option<(usize, usize, usize, usize, bool)> {
        self.transport.last_websocket_ingress_test_metrics()
    }

    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    pub fn websocket_diagnostics_for_lifecycle_test(&self) -> Option<crate::WebSocketDiagnostics> {
        self.transport.websocket_diagnostics()
    }

    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    pub fn bound_approval_request_for_lifecycle_test(
        &self,
        kind: ApprovalRequestKind,
    ) -> ApprovalRequest {
        let decoded =
            ApprovalRequest::decoded(crate::ApprovalRequestId::Integer(1), kind, None, None, None);
        let (request, responder) = decoded.into_parts();
        responder
            .bind_response_authority(self.approval_response_authority_generation)
            .expect("fresh lifecycle approval authority is unbound");
        request
    }

    /// Constructs only the detached stdio capability-gate fixture.
    ///
    /// This test-support seam has no stdout reader and cannot execute requests;
    /// it exists solely to prove streamed input is rejected before source reads
    /// or writes while the session remains locally reusable.
    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    pub fn stdio_streamed_input_gate_for_lifecycle_test() -> Result<Self, ManagedBackendError> {
        let (_sender, messages) = std::sync::mpsc::sync_channel(1);
        Ok(Self {
            transport: BackendClientTransport::Stdio {
                stdin: None,
                messages,
            },
            managed_launch_provenance: None,
            initialize: None,
            initialized_notification_profile: None,
            next_request_id: 1,
            approval_response_authority_generation:
                allocate_approval_response_authority_generation()?,
            streamed_user_message_verifier: StreamedUserMessageVerifierSlot::default(),
            response_expectation: crate::incoming_json::ResponseExpectationSlot::default(),
            foreground_authorization_epoch: 1,
            exact_foreground_thread: None,
            exact_foreground_turn: None,
            pre_bind_approvals: PreBindApprovalPrefix::disabled(),
            ordered_turn_stream_sink: None,
        })
    }

    /// Poisons the streamed user-message verifier slot for fail-closed tests.
    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    pub fn poison_streamed_user_message_verifier_for_lifecycle_test(&self) {
        self.streamed_user_message_verifier
            .poison_for_lifecycle_test();
    }

    /// Returns request-sequence, response-slot, and verifier-poison facts without exposing slots.
    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    #[must_use]
    pub fn predispatch_state_for_lifecycle_test(&self) -> (u64, bool, bool) {
        (
            self.next_request_id,
            self.response_expectation.is_idle(),
            self.streamed_user_message_verifier.active_handle().is_err(),
        )
    }

    /// Moves the serialized request sequence to its terminal value without dispatching.
    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    pub fn exhaust_request_ids_for_lifecycle_test(&mut self) {
        assert!(
            self.response_expectation.is_idle(),
            "request-id exhaustion requires an idle response expectation",
        );
        self.next_request_id = u64::MAX;
    }

    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    pub fn prepare_pre_bind_response_wait_for_lifecycle_test(&mut self, request_id: u64) {
        assert!(
            matches!(
                self.transport,
                BackendClientTransport::ForegroundWebSocket(_)
            ),
            "pre-bind lifecycle wait requires a foreground candidate",
        );
        self.initialized_notification_profile =
            Some(InitializedNotificationProfile::FullTurnStream);
        self.response_expectation
            .install_fixed(request_id, crate::incoming_json::ResponseFamily::Initialize)
            .expect("fresh lifecycle candidate accepts one response expectation");
    }

    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    pub fn occupy_response_expectation_for_lifecycle_test(&mut self, request_id: u64) {
        self.response_expectation
            .install_fixed(request_id, crate::incoming_json::ResponseFamily::Initialize)
            .expect("fresh lifecycle session accepts one response expectation");
    }

    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    pub fn poison_response_expectation_for_lifecycle_test(&mut self) {
        self.response_expectation.poison_for_test();
    }

    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    pub fn enable_full_turn_stream_for_lifecycle_test(&mut self) {
        assert!(
            matches!(
                self.transport,
                BackendClientTransport::ForegroundWebSocket(_)
            ),
            "full-profile lifecycle seam requires a foreground candidate",
        );
        self.initialized_notification_profile =
            Some(InitializedNotificationProfile::FullTurnStream);
    }

    /// Replaces an initialized foreground fixture's notification profile with opted-out state.
    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    pub fn opt_out_full_turn_stream_for_lifecycle_test(&mut self) {
        assert!(
            self.initialize.is_some()
                && matches!(
                    self.transport,
                    BackendClientTransport::ForegroundWebSocket(_)
                ),
            "opted-out lifecycle seam requires an initialized foreground candidate",
        );
        self.initialized_notification_profile = Some(InitializedNotificationProfile::OptedOut);
    }

    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    pub fn poll_pre_bind_response_wait_for_lifecycle_test(
        &mut self,
        timeout: Duration,
    ) -> Result<crate::OrderedTurnStreamProgress, ManagedBackendError> {
        match self.recv_message_timeout("pre-bind lifecycle wait", timeout)? {
            ReceiveOutcome::Quiet => Ok(crate::OrderedTurnStreamProgress::Quiet),
            ReceiveOutcome::OrderedProgress => Ok(crate::OrderedTurnStreamProgress::Progress),
            ReceiveOutcome::Message(_) => Err(ManagedBackendError::UnexpectedMessageShape),
            ReceiveOutcome::Response { .. } | ReceiveOutcome::Rejection(_) => {
                self.retire_connection();
                Err(ManagedBackendError::ForegroundIngress {
                    method: "pre-bind lifecycle wait".to_string(),
                    source: crate::ForegroundIngressError::IdleResponse,
                })
            }
        }
    }

    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    #[must_use]
    pub fn pre_bind_prefix_is_empty_for_lifecycle_test(&self) -> bool {
        self.pre_bind_approvals.is_empty()
    }

    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    #[must_use]
    pub fn transport_is_closed_for_lifecycle_test(&self) -> bool {
        self.transport.is_closed()
    }

    #[cfg(feature = "lifecycle-test-support")]
    #[doc(hidden)]
    pub fn fail_next_write_before_dispatch_for_lifecycle_test(&mut self) {
        match &mut self.transport {
            BackendClientTransport::ForegroundWebSocket(transport) => {
                transport.fail_next_write_before_dispatch_for_lifecycle_test();
            }
            BackendClientTransport::RequestOnlyWebSocket(transport) => {
                transport.fail_next_write_before_dispatch_for_lifecycle_test();
            }
            BackendClientTransport::Stdio { .. } => {
                panic!("write-failure lifecycle seam requires a WebSocket candidate");
            }
        }
    }

    pub fn rollback_thread(
        &mut self,
        _thread_id: &CasThreadId,
        _num_turns: u32,
        _timeout: Duration,
    ) -> Result<LoadedThreadSession, ManagedBackendError> {
        Err(ManagedBackendError::ResponseFamilyUnavailable {
            method: "thread/rollback",
        })
    }

    /// Starts a turn from one owned replayable submitted-input descriptor source.
    pub fn start_turn_with_streamed_input(
        &mut self,
        thread_id: &CasThreadId,
        source: Box<dyn StreamedInputSource>,
        timeout: Duration,
    ) -> TurnStartOutcome {
        self.start_turn_with_streamed_input_options(
            thread_id,
            source,
            TurnStartOptions::default(),
            timeout,
        )
    }

    /// Starts a turn through the bounded specialized streamed-input encoder.
    ///
    /// Every text source is replayed from absolute offset zero as one logical
    /// JSON string. A source failure preserves the same exact dispatch evidence
    /// as a transport failure and never causes automatic replay.
    pub fn start_turn_with_streamed_input_options(
        &mut self,
        thread_id: &CasThreadId,
        source: Box<dyn StreamedInputSource>,
        options: TurnStartOptions,
        timeout: Duration,
    ) -> TurnStartOutcome {
        self.non_idempotent_streamed_turn_start(thread_id, source, &options, timeout)
            .map_exact_response(|response| response.into_started(thread_id.clone()))
    }

    /// Steers one exact active turn from an owned replayable submitted-input source.
    ///
    /// The successful response is returned as soon as CAS names the expected active turn. Its
    /// later correlation-bearing `UserMessage` is checked independently through the already-bound
    /// ordered sink and a fresh replay source selected by that sink.
    pub fn steer_turn_with_streamed_input(
        &mut self,
        thread_id: &CasThreadId,
        expected_turn_id: &CasTurnId,
        client_user_message_id: &crate::ClientUserMessageId,
        source: Box<dyn StreamedInputSource>,
        timeout: Duration,
    ) -> TurnSteerOutcome {
        self.non_idempotent_streamed_turn_steer(
            thread_id,
            expected_turn_id,
            client_user_message_id,
            source,
            timeout,
        )
        .map_exact_response(crate::TurnSteerResponseWire::into_steered)
    }

    pub fn deny_approval_request(
        &mut self,
        request: &ApprovalRequest,
    ) -> Result<(), ManagedBackendError> {
        let kind = request.kind();
        if request.response_authority_generation() != self.approval_response_authority_generation {
            return Err(ManagedBackendError::ApprovalResponseAuthorityMismatch { kind });
        }
        if request.response_disposition() != ApprovalResponseDisposition::ResponseRequired {
            return Err(ManagedBackendError::ApprovalResponseAlreadySent { kind });
        }
        self.write_approval_denial(kind, request.request_id())?;
        request.mark_response_disposition(ApprovalResponseDisposition::Denied);
        Ok(())
    }

    fn auto_deny_approval_through(
        transport: &mut BackendClientTransport,
        approval_response_authority_generation: u64,
        responder: &ApprovalResponder,
    ) -> Result<(), ManagedBackendError> {
        let kind = responder.kind();
        if responder.response_authority_generation() != approval_response_authority_generation {
            return Err(ManagedBackendError::ApprovalResponseAuthorityMismatch { kind });
        }
        if responder.response_disposition() != ApprovalResponseDisposition::ResponseRequired {
            return Err(ManagedBackendError::ApprovalResponseAlreadySent { kind });
        }
        Self::write_approval_denial_through(transport, kind, responder.request_id())?;
        responder.mark_response_disposition(ApprovalResponseDisposition::AutoDenied);
        Ok(())
    }

    fn write_approval_denial(
        &mut self,
        kind: ApprovalRequestKind,
        request_id: &crate::ApprovalRequestId,
    ) -> Result<(), ManagedBackendError> {
        Self::write_approval_denial_through(&mut self.transport, kind, request_id)
    }

    fn write_approval_denial_through(
        transport: &mut BackendClientTransport,
        kind: ApprovalRequestKind,
        request_id: &crate::ApprovalRequestId,
    ) -> Result<(), ManagedBackendError> {
        let result = match kind {
            ApprovalRequestKind::CommandExecution | ApprovalRequestKind::FileChange => {
                write_server_response_through(
                    transport,
                    kind.method(),
                    request_id,
                    &ApprovalCancelResponse { decision: "cancel" },
                )
            }
            ApprovalRequestKind::Permissions => write_server_response_through(
                transport,
                kind.method(),
                request_id,
                &PermissionApprovalResponse {
                    permissions: EmptyPermissions {},
                    scope: "turn",
                    strict_auto_review: false,
                },
            ),
        };
        result.map_err(|source| ManagedBackendError::ApprovalDenialWrite {
            kind,
            source: Box::new(source),
        })
    }

    /// Writes one response through the exact session that originated a sealed dynamic-tool call.
    ///
    /// The call remains response-required if the transport write fails. A foreign session,
    /// incomplete or abandoned call, or second response attempt is rejected before writing.
    pub fn respond_dynamic_tool_call(
        &mut self,
        call: &DynamicToolCall,
        response: &DynamicToolCallResponse,
    ) -> Result<(), ManagedBackendError> {
        if call.response_authority_generation() != self.approval_response_authority_generation {
            return Err(ManagedBackendError::DynamicToolResponseAuthorityMismatch);
        }
        if !call.is_sealed() {
            return Err(ManagedBackendError::DynamicToolResponseBeforeSeal);
        }
        if call.response_disposition() != DynamicToolCallResponseDisposition::ResponseRequired {
            return Err(ManagedBackendError::DynamicToolResponseAlreadySent);
        }
        self.write_server_response(call.method(), call.request_id(), response)?;
        call.mark_responded();
        Ok(())
    }

    /// Returns exact proof that initialization retained the full turn-stream profile.
    ///
    /// An uninitialized session, a request-only session, or a session initialized
    /// with any custom notification opt-out returns `false`.
    #[must_use]
    pub fn has_full_turn_stream(&self) -> bool {
        matches!(
            self.initialized_notification_profile,
            Some(InitializedNotificationProfile::FullTurnStream)
        )
    }

    /// Returns local pre-bind control diagnostics for a foreground session.
    #[must_use]
    pub fn pre_bind_control_diagnostics(&self) -> Option<crate::PreBindControlDiagnostics> {
        matches!(
            self.transport,
            BackendClientTransport::ForegroundWebSocket(_)
        )
        .then(|| self.pre_bind_approvals.diagnostics())
    }

    pub fn shutdown(&mut self) -> Result<(), ManagedBackendError> {
        self.retire_connection();
        Ok(())
    }

    fn retire_connection(&mut self) {
        self.pre_bind_approvals.clear();
        self.response_expectation.poison();
        self.exact_foreground_thread = None;
        self.exact_foreground_turn = None;
        self.transport.close();
    }

    pub(crate) fn from_foreground_websocket(
        transport: ForegroundWebSocketTransport,
    ) -> Result<Self, ManagedBackendError> {
        let approval_response_authority_generation =
            allocate_approval_response_authority_generation()?;
        let pre_bind_control_capacity = transport.config().pre_bind_control_capacity().get();

        Ok(Self {
            transport: BackendClientTransport::ForegroundWebSocket(transport),
            managed_launch_provenance: None,
            initialize: None,
            initialized_notification_profile: None,
            next_request_id: 1,
            approval_response_authority_generation,
            streamed_user_message_verifier: StreamedUserMessageVerifierSlot::default(),
            response_expectation: crate::incoming_json::ResponseExpectationSlot::default(),
            foreground_authorization_epoch: 1,
            exact_foreground_thread: None,
            exact_foreground_turn: None,
            pre_bind_approvals: PreBindApprovalPrefix::new(pre_bind_control_capacity),
            ordered_turn_stream_sink: None,
        })
    }

    pub(crate) fn from_request_only_websocket(
        transport: RequestOnlyWebSocketTransport,
    ) -> Result<Self, ManagedBackendError> {
        let approval_response_authority_generation =
            allocate_approval_response_authority_generation()?;

        Ok(Self {
            transport: BackendClientTransport::RequestOnlyWebSocket(transport),
            managed_launch_provenance: None,
            initialize: None,
            initialized_notification_profile: None,
            next_request_id: 1,
            approval_response_authority_generation,
            streamed_user_message_verifier: StreamedUserMessageVerifierSlot::default(),
            response_expectation: crate::incoming_json::ResponseExpectationSlot::default(),
            foreground_authorization_epoch: 1,
            exact_foreground_thread: None,
            exact_foreground_turn: None,
            pre_bind_approvals: PreBindApprovalPrefix::disabled(),
            ordered_turn_stream_sink: None,
        })
    }

    pub(crate) fn bind_managed_launch_provenance(
        &mut self,
        provenance: crate::server::ManagedLaunchProvenance,
    ) {
        debug_assert!(self.managed_launch_provenance.is_none());
        self.managed_launch_provenance = Some(provenance);
    }

    pub(crate) fn has_production_managed_launch_provenance(&self) -> bool {
        matches!(
            self.managed_launch_provenance,
            Some(crate::server::ManagedLaunchProvenance::Production(_))
        )
    }

    fn write_message(
        &mut self,
        method: &str,
        message: &impl Serialize,
    ) -> Result<(), ManagedBackendError> {
        self.transport
            .write_message(method, message)
            .map(|_| ())
            .map_err(|failure| match failure {
                TransportWriteFailure::ProvenNotDispatched(error)
                | TransportWriteFailure::MayHaveDispatched(error) => error,
            })
    }

    fn write_server_response<I: Serialize + ?Sized, T: Serialize + ?Sized>(
        &mut self,
        method: &str,
        request_id: &I,
        result: &T,
    ) -> Result<(), ManagedBackendError> {
        self.write_message(
            method,
            &JsonRpcServerResponse {
                jsonrpc: "2.0",
                id: request_id,
                result,
            },
        )
    }
}

fn write_server_response_through<I: Serialize + ?Sized, T: Serialize + ?Sized>(
    transport: &mut BackendClientTransport,
    method: &str,
    request_id: &I,
    result: &T,
) -> Result<(), ManagedBackendError> {
    transport
        .write_message(
            method,
            &JsonRpcServerResponse {
                jsonrpc: "2.0",
                id: request_id,
                result,
            },
        )
        .map(|_| ())
        .map_err(|failure| match failure {
            TransportWriteFailure::ProvenNotDispatched(error)
            | TransportWriteFailure::MayHaveDispatched(error) => error,
        })
}

impl Drop for ManagedBackendSession {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            warn!(%error, "failed to shut down managed backend session");
        }
    }
}

impl std::fmt::Debug for ManagedBackendSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ManagedBackendSession")
            .field("transport", &self.transport)
            .field("initialize", &self.initialize)
            .field(
                "ordered_turn_stream_sink_bound",
                &self.ordered_turn_stream_sink.is_some(),
            )
            .finish_non_exhaustive()
    }
}

impl ManagedBackendReleaseAdmission {
    pub(crate) fn new(
        initialize: InitializeResponse,
        config_defaults: BackendConfigDefaults,
        launch_provenance: crate::server::ManagedLaunchProvenance,
    ) -> Self {
        Self {
            initialize,
            config_defaults,
            launch_provenance,
        }
    }

    pub fn initialize(&self) -> &InitializeResponse {
        &self.initialize
    }

    pub fn config_defaults(&self) -> &BackendConfigDefaults {
        &self.config_defaults
    }

    pub fn launch_identity(&self) -> &crate::ManagedBackendLaunchIdentity {
        match &self.launch_provenance {
            crate::server::ManagedLaunchProvenance::Production(identity) => identity,
            #[cfg(feature = "lifecycle-test-support")]
            crate::server::ManagedLaunchProvenance::LifecycleTest => {
                unreachable!("lifecycle-test provenance cannot create release admission")
            }
        }
    }
}

pub(crate) enum TransportWriteFailure {
    ProvenNotDispatched(ManagedBackendError),
    MayHaveDispatched(ManagedBackendError),
}

impl TransportWriteFailure {
    pub(crate) fn from_progress(progress: DispatchProgress, error: ManagedBackendError) -> Self {
        if progress.some_bytes() {
            Self::MayHaveDispatched(error)
        } else {
            Self::ProvenNotDispatched(error)
        }
    }
}

enum BackendClientTransport {
    Stdio {
        stdin: Option<ChildStdin>,
        messages: Receiver<Result<IncomingMessage, ManagedBackendError>>,
    },
    ForegroundWebSocket(ForegroundWebSocketTransport),
    RequestOnlyWebSocket(RequestOnlyWebSocketTransport),
}

impl BackendClientTransport {
    fn write_message<T: Serialize + ?Sized>(
        &mut self,
        method: &str,
        message: &T,
    ) -> Result<outbound::OutboundWriteMetrics, TransportWriteFailure> {
        match self {
            Self::Stdio { stdin, .. } => {
                let Some(sink) = stdin.as_mut() else {
                    return Err(TransportWriteFailure::ProvenNotDispatched(
                        ManagedBackendError::TransportClosed {
                            method: method.to_string(),
                        },
                    ));
                };
                let result = {
                    let mut writer = StdioJsonWriter::new(sink);
                    write_json(&mut writer, message)
                };
                match result {
                    Ok(metrics) => Ok(metrics),
                    Err(failure) => {
                        let progress = failure.progress();
                        let error = match failure {
                            OutboundWriteFailure::Serialize { source, .. }
                                if progress.some_bytes() =>
                            {
                                ManagedBackendError::WriteRequest {
                                    method: method.to_string(),
                                    source: io::Error::other(source),
                                }
                            }
                            OutboundWriteFailure::Serialize { source, .. } => {
                                ManagedBackendError::SerializeRequest {
                                    method: method.to_string(),
                                    source,
                                }
                            }
                            OutboundWriteFailure::Transport { source, .. } => {
                                ManagedBackendError::WriteRequest {
                                    method: method.to_string(),
                                    source,
                                }
                            }
                        };
                        if progress.some_bytes() {
                            drop(stdin.take());
                        }
                        Err(TransportWriteFailure::from_progress(progress, error))
                    }
                }
            }
            Self::ForegroundWebSocket(transport) => transport.write_message(method, message),
            Self::RequestOnlyWebSocket(transport) => transport.write_message(method, message),
        }
    }

    fn write_streamed_message<T: Serialize + ?Sized>(
        &mut self,
        method: &str,
        message: &T,
        source_failure: &crate::turn::StreamedInputSourceFailureSlot,
    ) -> Result<outbound::OutboundWriteMetrics, TransportWriteFailure> {
        match self {
            Self::ForegroundWebSocket(transport) => {
                transport.write_streamed_message(method, message, source_failure)
            }
            Self::Stdio { .. } => Err(TransportWriteFailure::ProvenNotDispatched(
                ManagedBackendError::StreamedInputTransportUnsupported {
                    method: method.to_string(),
                    transport: "stdio",
                },
            )),
            Self::RequestOnlyWebSocket(_) => Err(TransportWriteFailure::ProvenNotDispatched(
                ManagedBackendError::StreamedInputTransportUnsupported {
                    method: method.to_string(),
                    transport: "request-only websocket",
                },
            )),
        }
    }

    fn write_injection_message<T: Serialize + ?Sized>(
        &mut self,
        method: &str,
        message: &T,
        source_failure: &crate::thread_injection::ThreadInjectionSourceFailureSlot,
    ) -> Result<outbound::OutboundWriteMetrics, TransportWriteFailure> {
        match self {
            Self::ForegroundWebSocket(transport) => {
                transport.write_injection_message(method, message, source_failure)
            }
            Self::Stdio { .. } => Err(TransportWriteFailure::ProvenNotDispatched(
                ManagedBackendError::ThreadInjectionTransportUnsupported {
                    method: method.to_string(),
                    transport: "stdio",
                },
            )),
            Self::RequestOnlyWebSocket(_) => Err(TransportWriteFailure::ProvenNotDispatched(
                ManagedBackendError::ThreadInjectionTransportUnsupported {
                    method: method.to_string(),
                    transport: "request-only websocket",
                },
            )),
        }
    }

    fn recv_message_timeout<'a>(
        &mut self,
        method: &str,
        timeout: Duration,
        verifier: Option<StreamedUserMessageVerifierHandle<'a>>,
        ordered_sink: Option<&'a mut dyn crate::OrderedTurnStreamSink>,
        response_authority_generation: u64,
        response_expectation: &mut crate::incoming_json::ResponseExpectationSlot,
    ) -> Result<ReceiveOutcome, ManagedBackendError> {
        match self {
            Self::Stdio { messages, .. } if verifier.is_some() => {
                Err(ManagedBackendError::StreamedInputTransportUnsupported {
                    method: method.to_string(),
                    transport: "stdio",
                })
            }
            Self::Stdio { messages, .. } => match messages.recv_timeout(timeout) {
                Ok(message) => message.map(ReceiveOutcome::Message),
                Err(RecvTimeoutError::Timeout) => Ok(ReceiveOutcome::Quiet),
                Err(RecvTimeoutError::Disconnected) => Err(ManagedBackendError::TransportClosed {
                    method: method.to_string(),
                }),
            },
            Self::ForegroundWebSocket(transport) => {
                match transport.recv_json_value_timeout(
                    method,
                    timeout,
                    verifier,
                    ordered_sink,
                    response_authority_generation,
                    response_expectation,
                )? {
                    Some(crate::incoming_json::DecodedIncoming::Approval(approval)) => {
                        let (request, responder) = approval.into_parts();
                        Ok(ReceiveOutcome::Message(IncomingMessage::Approval {
                            request,
                            responder,
                        }))
                    }
                    Some(crate::incoming_json::DecodedIncoming::OrderedHandled) => {
                        Ok(ReceiveOutcome::OrderedProgress)
                    }
                    Some(crate::incoming_json::DecodedIncoming::DiscardedNotification) => {
                        Ok(ReceiveOutcome::OrderedProgress)
                    }
                    Some(crate::incoming_json::DecodedIncoming::Response { result, .. }) => {
                        Ok(ReceiveOutcome::Response(result))
                    }
                    Some(crate::incoming_json::DecodedIncoming::Rejection { error, .. }) => {
                        Ok(ReceiveOutcome::Rejection(error))
                    }
                    None => Ok(ReceiveOutcome::Quiet),
                }
            }
            Self::RequestOnlyWebSocket(transport) => {
                match transport.recv_json_value_timeout(method, timeout, response_expectation)? {
                    Some(crate::incoming_json::DecodedIncoming::Response { result, .. }) => {
                        Ok(ReceiveOutcome::Response(result))
                    }
                    Some(crate::incoming_json::DecodedIncoming::Rejection { error, .. }) => {
                        Ok(ReceiveOutcome::Rejection(error))
                    }
                    Some(crate::incoming_json::DecodedIncoming::DiscardedNotification) => {
                        Ok(ReceiveOutcome::OrderedProgress)
                    }
                    Some(
                        crate::incoming_json::DecodedIncoming::Approval(_)
                        | crate::incoming_json::DecodedIncoming::OrderedHandled,
                    ) => {
                        transport.close();
                        Err(ManagedBackendError::ForegroundIngress {
                            method: method.to_string(),
                            source: crate::ForegroundIngressError::KnownControlUnavailable,
                        })
                    }
                    None => Ok(ReceiveOutcome::Quiet),
                }
            }
        }
    }

    #[cfg(feature = "lifecycle-test-support")]
    fn last_websocket_ingress_test_metrics(&self) -> Option<(usize, usize, usize, usize, bool)> {
        let Self::ForegroundWebSocket(transport) = self else {
            return None;
        };
        transport.last_ingress_stats().map(|stats| {
            (
                stats.message_bytes,
                stats.maximum_transport_chunk_bytes,
                stats.maximum_parser_buffer_bytes,
                stats.discarded_image_result_bytes,
                stats.retained_item_result_present,
            )
        })
    }

    #[cfg(feature = "lifecycle-test-support")]
    fn websocket_diagnostics(&self) -> Option<crate::WebSocketDiagnostics> {
        let Self::ForegroundWebSocket(transport) = self else {
            return None;
        };
        Some(transport.diagnostics())
    }

    fn close(&mut self) {
        match self {
            Self::Stdio { stdin, .. } => {
                drop(stdin.take());
            }
            Self::ForegroundWebSocket(transport) => {
                transport.close();
            }
            Self::RequestOnlyWebSocket(transport) => {
                transport.close();
            }
        }
    }

    fn is_closed(&self) -> bool {
        match self {
            Self::Stdio { stdin, .. } => stdin.is_none(),
            Self::ForegroundWebSocket(transport) => transport.is_closed(),
            Self::RequestOnlyWebSocket(transport) => transport.is_closed(),
        }
    }
}

impl std::fmt::Debug for BackendClientTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stdio { .. } => f.write_str("BackendClientTransport::Stdio"),
            Self::ForegroundWebSocket(transport) => f
                .debug_struct("BackendClientTransport::ForegroundWebSocket")
                .field("endpoint", &transport.endpoint())
                .finish(),
            Self::RequestOnlyWebSocket(transport) => f
                .debug_struct("BackendClientTransport::RequestOnlyWebSocket")
                .field("endpoint", &transport.endpoint())
                .finish(),
        }
    }
}

#[derive(Serialize)]
struct JsonRpcServerResponse<'a, I: Serialize + ?Sized, T: Serialize + ?Sized> {
    jsonrpc: &'static str,
    id: &'a I,
    result: &'a T,
}

#[derive(Serialize)]
struct ApprovalCancelResponse {
    decision: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PermissionApprovalResponse {
    permissions: EmptyPermissions,
    scope: &'static str,
    strict_auto_review: bool,
}

#[derive(Serialize)]
struct EmptyPermissions {}
