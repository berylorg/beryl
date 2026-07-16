use std::{
    collections::VecDeque,
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{ChildStderr, ChildStdin, ChildStdout},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use beryl_model::{CasThreadId, CasTurnId};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use thiserror::Error;
use tracing::{debug, warn};

use crate::{
    AccountRateLimitsResponse, ApprovalRequest, ApprovalRequestKind, ApprovalResponseDisposition,
    BackendCommandLineError, BackendConfigDefaults, BackendWebSocketEndpoint, CompatibilityError,
    CompatibilityProbe, CompatibilitySnapshot, ConfigReadOptions, ConfigReadResponse,
    DynamicToolCallRequest, DynamicToolCallResponse, HardStopCapabilityProbe,
    HardStopCapabilityProbeResult, HardStopCapabilityReport, HardStopTarget, HardStopTargetOutcome,
    InitializeResponse, JsonRpcError, ModelInfo, ModelListOptions, ModelListResponse, StartedTurn,
    SteeredTurn, ThreadBranchCapabilities, ThreadBranchCapabilityProbe,
    ThreadBranchCapabilityProbeResult, ThreadBranchCapabilityReport, ThreadInjectionBatch,
    ThreadInjectionOutcome, ThreadLoadOptions, ThreadReadMetadata, ThreadSessionResponse,
    ThreadStartOptions, ThreadSummary, ThreadUnsubscribeResponse, TurnStartOptions,
    TurnStreamEvent, UserInput,
    dynamic_tool::{is_dynamic_tool_call_method, parse_dynamic_tool_call_request},
    hard_stop::HARD_STOP_CAPABILITY_PROBES,
    thread_branch::THREAD_BRANCH_CAPABILITY_PROBES,
    thread_injection::{
        THREAD_INJECT_ITEMS_METHOD, ThreadInjectItemsParams, ThreadInjectItemsResponse,
    },
    thread_lineage::{
        FreshIdleThread, FreshLoadedThreadSession, LoadedThreadSession, ThreadForkParams,
        ThreadLineageResponse, ThreadResumeParams, ThreadRollbackParams,
    },
    thread_metadata::ThreadReadMetadataParams,
    turn::{
        ThreadStartParams, TurnStartParams, TurnStartResponseWire, TurnSteerParams,
        TurnSteerResponseWire, parse_approval_request, parse_turn_stream_event,
    },
    websocket_transport::WebSocketClientTransport,
};

const INITIALIZE_METHOD: &str = "initialize";
const INITIALIZED_METHOD: &str = "initialized";
const JSONRPC_METHOD_NOT_FOUND: i64 = -32601;
const JSONRPC_INVALID_PARAMS: i64 = -32602;
const PROBE_THREAD_ID: &str = "00000000-0000-0000-0000-000000000000";
const PROBE_TURN_ID: &str = "00000000-0000-0000-0000-000000000001";
const PROBE_COMMAND_EXEC_PROCESS_ID: &str = "beryl-hard-stop-probe";
const STDERR_LOG_LIMIT: usize = 240;
const PENDING_MESSAGE_COUNT_LIMIT: usize = 1024;
const PENDING_MESSAGE_BYTE_BUDGET: usize = 16 * 1024 * 1024;
const PENDING_DYNAMIC_TOOL_REQUEST_LIMIT: usize = 64;
const STDIO_STDOUT_LINE_BYTE_LIMIT: usize = 64 * 1024 * 1024;
const STDIO_STDERR_LINE_BYTE_LIMIT: usize = 8 * 1024;
const STDIO_MESSAGE_CHANNEL_BOUND: usize = 64;
static NEXT_APPROVAL_RESPONSE_AUTHORITY_GENERATION: AtomicU64 = AtomicU64::new(0);
const REQUEST_ONLY_NOTIFICATION_METHODS: &[&str] = &[
    "thread/started",
    "thread/status/changed",
    "thread/closed",
    "thread/name/updated",
    "thread/tokenUsage/updated",
    "account/rateLimits/updated",
    "turn/started",
    "turn/completed",
    "turn/diff/updated",
    "item/started",
    "item/completed",
    "item/agentMessage/delta",
    "item/plan/delta",
    "item/reasoning/summaryPartAdded",
    "item/reasoning/summaryTextDelta",
    "item/reasoning/textDelta",
    "item/commandExecution/outputDelta",
    "item/fileChange/outputDelta",
    "item/fileChange/patchUpdated",
    "item/mcpToolCall/progress",
    "codex/event/collab_agent_spawn_end",
];

fn probe_thread_id() -> CasThreadId {
    CasThreadId::new(PROBE_THREAD_ID).expect("fixed compatibility-probe thread id is valid")
}

fn probe_turn_id() -> CasTurnId {
    CasTurnId::new(PROBE_TURN_ID).expect("fixed compatibility-probe turn id is valid")
}

fn elapsed_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn allocate_approval_response_authority_generation() -> Result<u64, ManagedBackendError> {
    NEXT_APPROVAL_RESPONSE_AUTHORITY_GENERATION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |generation| {
            generation.checked_add(1)
        })
        .ok()
        .and_then(|generation| generation.checked_add(1))
        .ok_or(ManagedBackendError::ApprovalResponseAuthorityExhausted)
}

#[derive(Debug)]
pub struct ManagedBackendSession {
    transport: BackendClientTransport,
    initialize: Option<InitializeResponse>,
    compatibility: Option<CompatibilitySnapshot>,
    initialized_notification_profile: Option<InitializedNotificationProfile>,
    pending_messages: VecDeque<IncomingMessage>,
    pending_message_bytes: usize,
    pending_dynamic_tool_requests: usize,
    next_request_id: u64,
    approval_response_authority_generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InitializedNotificationProfile {
    FullTurnStream,
    OptedOut,
}

/// One normalized live event and its approximate pre-event-parse retained bytes.
///
/// The byte count is derived directly from the incoming JSON-RPC notification,
/// server request, or error message before its payload is parsed into a
/// [`TurnStreamEvent`]. It is suitable for applying bounded in-memory queue
/// accounting without reserializing the normalized event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnStreamEnvelope {
    event: TurnStreamEvent,
    approximate_retained_bytes: usize,
}

impl TurnStreamEnvelope {
    fn new(event: TurnStreamEvent, approximate_retained_bytes: usize) -> Self {
        Self {
            event,
            approximate_retained_bytes,
        }
    }

    /// Returns the normalized event carried by this envelope.
    #[must_use]
    pub fn event(&self) -> &TurnStreamEvent {
        &self.event
    }

    /// Consumes the envelope and returns its normalized event.
    #[must_use]
    pub fn into_event(self) -> TurnStreamEvent {
        self.event
    }

    /// Returns the incoming message's approximate pre-event-parse retained bytes.
    #[must_use]
    pub const fn approximate_retained_bytes(&self) -> usize {
        self.approximate_retained_bytes
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedBackendProbeReport {
    initialize: InitializeResponse,
    compatibility: CompatibilitySnapshot,
    method_successes: Vec<ProbeMethodSuccess>,
    thread_branch_capabilities: ThreadBranchCapabilities,
    config_defaults: BackendConfigDefaults,
    model_list: Vec<ModelInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeMethodSuccess {
    probe: CompatibilityProbe,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManagedBackendClientOptions {
    opt_out_notification_methods: Vec<String>,
}

enum ProbeMethodData {
    ConfigDefaults(BackendConfigDefaults),
    ModelList(Vec<ModelInfo>),
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
    #[error("failed to write {method} request to backend transport")]
    WriteRequest {
        method: String,
        #[source]
        source: io::Error,
    },
    #[error("backend transport read failed")]
    ReadTransport {
        #[source]
        source: io::Error,
    },
    #[error("backend transport message was not valid JSON: {line}")]
    InvalidJsonLine {
        line: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("backend returned an invalid {method} response payload")]
    DeserializeResponse {
        method: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("backend returned invalid base64 data for {method}")]
    DecodeBase64Response {
        method: String,
        #[source]
        source: base64::DecodeError,
    },
    #[error("failed to serialize {method} request payload")]
    SerializeRequest {
        method: String,
        #[source]
        source: serde_json::Error,
    },
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
    #[error("approval response for {method} was already sent")]
    ApprovalResponseAlreadySent { method: String },
    #[error("approval request for {method} does not belong to this backend session")]
    ApprovalResponseAuthorityMismatch { method: String },
    #[error("backend-session approval response authority generation is exhausted")]
    ApprovalResponseAuthorityExhausted,
    #[error("retained approval response state was lost while handling {method}")]
    PendingApprovalStateLost { method: String },
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
    #[error(
        "bounded backend resource exceeded while handling {method}: {resource} exceeded limit {limit}"
    )]
    BoundedResourceExceeded {
        method: String,
        resource: &'static str,
        limit: usize,
    },
    #[error("backend returned invalid {method} notification payload")]
    DeserializeNotification {
        method: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("backend returned invalid {method} server-request payload")]
    DeserializeServerRequest {
        method: String,
        #[source]
        source: serde_json::Error,
    },
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
                | Self::ReadTransport { .. }
                | Self::InvalidJsonLine { .. }
                | Self::DeserializeResponse { .. }
                | Self::DecodeBase64Response { .. }
                | Self::RequestTimeout { .. }
                | Self::ProcessExited { .. }
                | Self::TransportClosed { .. }
                | Self::WebSocketTransport { .. }
                | Self::ThreadResponseIdentityMismatch { .. }
                | Self::ForkResponseReusedSource { .. }
                | Self::TurnResponseIdentityMismatch { .. }
                | Self::UnexpectedMessageShape
                | Self::BoundedResourceExceeded { .. }
                | Self::DeserializeNotification { .. }
                | Self::DeserializeServerRequest { .. }
                | Self::PendingApprovalStateLost { .. }
        )
    }
}

/// Exact normalized outcome of one non-idempotent backend request.
///
/// `turn/start` and `turn/steer` have no provider idempotency key or
/// authoritative delivery readback. Callers may retry only
/// [`Self::ProvenNotDispatched`] or apply method-specific policy to an exact
/// rejection. [`Self::CompletionUnknown`] means the request may have crossed
/// the transport and must not be replayed automatically.
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

/// Normalized outcome of one `turn/steer` request.
pub type TurnSteerOutcome = NonIdempotentRequestOutcome<SteeredTurn>;

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

    pub fn list_models(
        &mut self,
        timeout: Duration,
    ) -> Result<Vec<ModelInfo>, ManagedBackendError> {
        self.list_models_with_options(ModelListOptions::page(100), timeout)
    }

    pub fn read_config(
        &mut self,
        cwd: &Path,
        timeout: Duration,
    ) -> Result<ConfigReadResponse, ManagedBackendError> {
        self.request("config/read", &ConfigReadOptions::for_cwd(cwd), timeout)
    }

    pub fn read_account_rate_limits(
        &mut self,
        timeout: Duration,
    ) -> Result<AccountRateLimitsResponse, ManagedBackendError> {
        self.request("account/rateLimits/read", &(), timeout)
    }

    pub fn list_models_with_options(
        &mut self,
        mut options: ModelListOptions,
        timeout: Duration,
    ) -> Result<Vec<ModelInfo>, ManagedBackendError> {
        let mut models = Vec::new();

        loop {
            let response = self.list_model_page(&options, timeout)?;
            models.extend(response.data);

            if response.next_cursor.is_none() {
                break;
            }

            options.cursor = response.next_cursor;
        }

        Ok(models)
    }

    pub fn list_model_page(
        &mut self,
        options: &ModelListOptions,
        timeout: Duration,
    ) -> Result<ModelListResponse, ManagedBackendError> {
        self.request("model/list", options, timeout)
    }

    pub fn start_thread(
        &mut self,
        cwd: &Path,
        timeout: Duration,
    ) -> Result<FreshLoadedThreadSession, ManagedBackendError> {
        self.start_thread_with_options(cwd, ThreadStartOptions::persistent(), timeout)
    }

    pub fn start_thread_with_options(
        &mut self,
        cwd: &Path,
        options: ThreadStartOptions,
        timeout: Duration,
    ) -> Result<FreshLoadedThreadSession, ManagedBackendError> {
        self.request::<ThreadLineageResponse>(
            "thread/start",
            &ThreadStartParams::for_root(cwd, options),
            timeout,
        )
        .map(ThreadLineageResponse::into_fresh)
    }

    pub fn resume_thread(
        &mut self,
        thread_id: &CasThreadId,
        options: &ThreadLoadOptions,
        timeout: Duration,
    ) -> Result<LoadedThreadSession, ManagedBackendError> {
        let loaded = self
            .request::<ThreadLineageResponse>(
                "thread/resume",
                &ThreadResumeParams::new(thread_id, options),
                timeout,
            )
            .map(ThreadLineageResponse::into_loaded)?;
        if loaded.thread_id() != thread_id {
            return Err(ManagedBackendError::ThreadResponseIdentityMismatch {
                method: "thread/resume".to_string(),
                expected: thread_id.clone(),
                actual: loaded.thread_id().clone(),
            });
        }
        Ok(loaded)
    }

    fn read_thread_metadata_response(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, ManagedBackendError> {
        self.request(
            "thread/read",
            &ThreadReadMetadataParams::new(thread_id),
            timeout,
        )
    }

    pub fn read_thread_metadata(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSummary, ManagedBackendError> {
        self.read_thread_metadata_response(thread_id, timeout)
            .map(|response| response.thread.summary())
    }

    pub fn read_thread_metadata_details(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadReadMetadata, ManagedBackendError> {
        self.read_thread_metadata_response(thread_id, timeout)
            .map(ThreadReadMetadata::from_session_response)
    }

    pub fn read_file_bytes(
        &mut self,
        path: &str,
        timeout: Duration,
    ) -> Result<Vec<u8>, ManagedBackendError> {
        let response: FsReadFileResponse =
            self.request("fs/readFile", &FsReadFileParams::new(path), timeout)?;
        BASE64_STANDARD
            .decode(response.data_base64)
            .map_err(|source| ManagedBackendError::DecodeBase64Response {
                method: "fs/readFile".to_string(),
                source,
            })
    }

    pub fn fork_thread(
        &mut self,
        thread_id: &CasThreadId,
        options: &ThreadLoadOptions,
        timeout: Duration,
    ) -> Result<FreshLoadedThreadSession, ManagedBackendError> {
        let fresh = self
            .request::<ThreadLineageResponse>(
                "thread/fork",
                &ThreadForkParams::full(thread_id, options),
                timeout,
            )
            .map(ThreadLineageResponse::into_fresh)?;
        if fresh.thread_id() == thread_id {
            return Err(ManagedBackendError::ForkResponseReusedSource {
                method: "thread/fork".to_string(),
                source_thread: thread_id.clone(),
            });
        }
        Ok(fresh)
    }

    pub fn fork_thread_through_turn(
        &mut self,
        thread_id: &CasThreadId,
        last_turn_id: &CasTurnId,
        options: &ThreadLoadOptions,
        timeout: Duration,
    ) -> Result<FreshLoadedThreadSession, ManagedBackendError> {
        let fresh = self
            .request::<ThreadLineageResponse>(
                "thread/fork",
                &ThreadForkParams::through_turn(thread_id, last_turn_id, options),
                timeout,
            )
            .map(ThreadLineageResponse::into_fresh)?;
        if fresh.thread_id() == thread_id {
            return Err(ManagedBackendError::ForkResponseReusedSource {
                method: "thread/fork".to_string(),
                source_thread: thread_id.clone(),
            });
        }
        Ok(fresh)
    }

    pub fn rollback_thread(
        &mut self,
        thread_id: &CasThreadId,
        num_turns: u32,
        timeout: Duration,
    ) -> Result<LoadedThreadSession, ManagedBackendError> {
        let loaded = self
            .request::<ThreadLineageResponse>(
                "thread/rollback",
                &ThreadRollbackParams::new(thread_id, num_turns),
                timeout,
            )
            .map(ThreadLineageResponse::into_loaded)?;
        if loaded.thread_id() != thread_id {
            return Err(ManagedBackendError::ThreadResponseIdentityMismatch {
                method: "thread/rollback".to_string(),
                expected: thread_id.clone(),
                actual: loaded.thread_id().clone(),
            });
        }
        Ok(loaded)
    }

    /// Injects one validated recovery prefix into one consumed fresh-idle thread.
    ///
    /// Every outcome consumes `target`. An unsuccessful outcome never
    /// authorizes retrying injection against that same CAS thread.
    pub fn inject_thread_items(
        &mut self,
        target: FreshIdleThread,
        batch: &ThreadInjectionBatch,
        timeout: Duration,
    ) -> ThreadInjectionOutcome {
        let thread_id = target.thread_id().clone();
        let loaded = target.into_loaded();
        let outcome = self.request_json(
            THREAD_INJECT_ITEMS_METHOD,
            &ThreadInjectItemsParams::new(&thread_id, batch),
            timeout,
        );

        match outcome {
            Ok(JsonRpcRequestOutcome::Result(result)) => {
                match serde_json::from_value::<ThreadInjectItemsResponse>(result) {
                    Ok(_) => ThreadInjectionOutcome::Succeeded { thread: loaded },
                    Err(source) => ThreadInjectionOutcome::CompletionUnknown {
                        thread_id,
                        error: Box::new(ManagedBackendError::DeserializeResponse {
                            method: THREAD_INJECT_ITEMS_METHOD.to_string(),
                            source,
                        }),
                    },
                }
            }
            Ok(JsonRpcRequestOutcome::Error(error)) => ThreadInjectionOutcome::Rejected {
                thread_id,
                rejection: crate::ThreadInjectionRejection::from_json_rpc(error),
            },
            Err(error) if injection_transport_was_lost(&error) => {
                ThreadInjectionOutcome::TransportLost {
                    thread_id,
                    error: Box::new(error),
                }
            }
            Err(error) => ThreadInjectionOutcome::CompletionUnknown {
                thread_id,
                error: Box::new(error),
            },
        }
    }

    pub fn start_turn(
        &mut self,
        thread_id: &CasThreadId,
        text: &str,
        timeout: Duration,
    ) -> TurnStartOutcome {
        self.start_turn_with_options(thread_id, text, TurnStartOptions::default(), timeout)
    }

    pub fn start_turn_with_options(
        &mut self,
        thread_id: &CasThreadId,
        text: &str,
        options: TurnStartOptions,
        timeout: Duration,
    ) -> TurnStartOutcome {
        self.non_idempotent_request::<TurnStartResponseWire>(
            "turn/start",
            &TurnStartParams::text(thread_id, text, options),
            timeout,
        )
        .map_exact_response(|response| response.into_started(thread_id.clone()))
    }

    pub fn start_turn_with_user_input(
        &mut self,
        thread_id: &CasThreadId,
        input: Vec<UserInput>,
        timeout: Duration,
    ) -> TurnStartOutcome {
        self.start_turn_with_user_input_options(
            thread_id,
            input,
            TurnStartOptions::default(),
            timeout,
        )
    }

    pub fn start_turn_with_user_input_options(
        &mut self,
        thread_id: &CasThreadId,
        input: Vec<UserInput>,
        options: TurnStartOptions,
        timeout: Duration,
    ) -> TurnStartOutcome {
        self.non_idempotent_request::<TurnStartResponseWire>(
            "turn/start",
            &TurnStartParams::input(thread_id, input, options),
            timeout,
        )
        .map_exact_response(|response| response.into_started(thread_id.clone()))
    }

    pub fn steer_turn_with_user_input(
        &mut self,
        thread_id: &CasThreadId,
        expected_turn_id: &CasTurnId,
        input: Vec<UserInput>,
        timeout: Duration,
    ) -> TurnSteerOutcome {
        match self.non_idempotent_request::<TurnSteerResponseWire>(
            "turn/steer",
            &TurnSteerParams::input(thread_id, expected_turn_id, input),
            timeout,
        ) {
            NonIdempotentRequestOutcome::ExactResponse { response } => {
                let steered = response.into_steered(thread_id.clone());
                if steered.turn_id() == expected_turn_id {
                    NonIdempotentRequestOutcome::ExactResponse { response: steered }
                } else {
                    NonIdempotentRequestOutcome::CompletionUnknown {
                        error: Box::new(ManagedBackendError::TurnResponseIdentityMismatch {
                            method: "turn/steer".to_string(),
                            expected: expected_turn_id.clone(),
                            actual: steered.turn_id().clone(),
                        }),
                    }
                }
            }
            NonIdempotentRequestOutcome::ExactRejection { error } => {
                NonIdempotentRequestOutcome::ExactRejection { error }
            }
            NonIdempotentRequestOutcome::ProvenNotDispatched { error } => {
                NonIdempotentRequestOutcome::ProvenNotDispatched { error }
            }
            NonIdempotentRequestOutcome::CompletionUnknown { error } => {
                NonIdempotentRequestOutcome::CompletionUnknown { error }
            }
        }
    }

    pub fn compact_thread(
        &mut self,
        thread_id: &CasThreadId,
        timeout: Duration,
    ) -> Result<(), ManagedBackendError> {
        let _: EmptyResponse = self.request(
            "thread/compact/start",
            &ThreadCompactStartParams::new(thread_id),
            timeout,
        )?;
        Ok(())
    }

    pub fn interrupt_turn(
        &mut self,
        thread_id: &CasThreadId,
        turn_id: &CasTurnId,
        timeout: Duration,
    ) -> Result<(), ManagedBackendError> {
        let _: EmptyResponse = self.request(
            "turn/interrupt",
            &TurnInterruptParams::new(thread_id, turn_id),
            timeout,
        )?;
        Ok(())
    }

    pub fn terminate_command_execution(
        &mut self,
        process_id: &str,
        timeout: Duration,
    ) -> Result<(), ManagedBackendError> {
        let _: EmptyResponse = self.request(
            "command/exec/terminate",
            &CommandExecTerminateParams::new(process_id),
            timeout,
        )?;
        Ok(())
    }

    pub fn clean_thread_background_terminals(
        &mut self,
        thread_id: &CasThreadId,
        timeout: Duration,
    ) -> Result<(), ManagedBackendError> {
        let _: EmptyResponse = self.request(
            "thread/backgroundTerminals/clean",
            &ThreadBackgroundTerminalsCleanParams::new(thread_id),
            timeout,
        )?;
        Ok(())
    }

    pub fn request_hard_stop_target(
        &mut self,
        target: &HardStopTarget,
        timeout: Duration,
    ) -> HardStopTargetOutcome {
        let result = match target {
            HardStopTarget::Turn { thread_id, turn_id } => {
                self.interrupt_turn(thread_id, turn_id, timeout)
            }
            HardStopTarget::CommandExecution { process_id } => {
                self.terminate_command_execution(process_id, timeout)
            }
            HardStopTarget::BackgroundTerminals { thread_id } => {
                self.clean_thread_background_terminals(thread_id, timeout)
            }
        };

        match result {
            Ok(()) => HardStopTargetOutcome::succeeded(target.clone()),
            Err(error) => {
                HardStopTargetOutcome::failed(target.clone(), target.method(), error.to_string())
            }
        }
    }

    pub fn probe_hard_stop_capabilities(
        &mut self,
        timeout: Duration,
    ) -> Result<HardStopCapabilityReport, ManagedBackendError> {
        let mut results = Vec::with_capacity(HARD_STOP_CAPABILITY_PROBES.len());
        for probe in HARD_STOP_CAPABILITY_PROBES {
            results.push(self.probe_hard_stop_capability(*probe, timeout)?);
        }

        Ok(HardStopCapabilityReport::new(results))
    }

    pub fn probe_thread_branch_capabilities(
        &mut self,
        config_cwd: &Path,
        timeout: Duration,
    ) -> Result<ThreadBranchCapabilityReport, ManagedBackendError> {
        let mut results = Vec::with_capacity(THREAD_BRANCH_CAPABILITY_PROBES.len());
        for probe in THREAD_BRANCH_CAPABILITY_PROBES {
            results.push(self.probe_thread_branch_capability(*probe, config_cwd, timeout)?);
        }

        Ok(ThreadBranchCapabilityReport::new(results))
    }

    pub fn deny_approval_request(
        &mut self,
        request: &ApprovalRequest,
    ) -> Result<(), ManagedBackendError> {
        if request.response_authority_generation()
            != Some(self.approval_response_authority_generation)
        {
            return Err(ManagedBackendError::ApprovalResponseAuthorityMismatch {
                method: request.method().to_string(),
            });
        }
        if request.response_disposition() != ApprovalResponseDisposition::ResponseRequired {
            return Err(ManagedBackendError::ApprovalResponseAlreadySent {
                method: request.method().to_string(),
            });
        }
        let result = match request.kind() {
            ApprovalRequestKind::CommandExecution | ApprovalRequestKind::FileChange => {
                json!({ "decision": "cancel" })
            }
            ApprovalRequestKind::Permissions => {
                json!({
                    "permissions": {},
                    "scope": "turn",
                    "strictAutoReview": false
                })
            }
        };
        self.write_server_response(request.method(), request.request_id(), &result)?;
        request.set_response_disposition(ApprovalResponseDisposition::Denied);
        Ok(())
    }

    pub fn respond_dynamic_tool_call(
        &mut self,
        request: &DynamicToolCallRequest,
        response: &DynamicToolCallResponse,
    ) -> Result<(), ManagedBackendError> {
        self.write_server_response(request.method(), request.request_id(), response)
    }

    pub fn unsubscribe_thread(
        &mut self,
        thread_id: &CasThreadId,
        timeout: Duration,
    ) -> Result<ThreadUnsubscribeResponse, ManagedBackendError> {
        self.request(
            "thread/unsubscribe",
            &ThreadUnsubscribeParams::new(thread_id),
            timeout,
        )
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

    /// Drains at most one normalized envelope already buffered by request handling.
    ///
    /// This method never reads the transport. Unsupported buffered messages are
    /// discarded in FIFO order, while invalid notifications and server requests
    /// remain fatal errors.
    pub fn drain_buffered_turn_stream_envelope(
        &mut self,
    ) -> Result<Option<TurnStreamEnvelope>, ManagedBackendError> {
        loop {
            let Some(message) = self.pop_pending_message() else {
                return Ok(None);
            };
            if let Some(envelope) =
                normalize_turn_stream_message(message, self.approval_response_authority_generation)?
            {
                return Ok(Some(envelope));
            }
        }
    }

    /// Polls the sole session stream reader for one normalized envelope.
    ///
    /// Already-buffered messages are examined before the deadline, so a zero
    /// timeout can still drain an event retained while a request was in flight.
    /// A quiet interval returns `Ok(None)`; transport and protocol failures remain
    /// explicit errors.
    pub fn poll_turn_stream_envelope(
        &mut self,
        idle_timeout: Duration,
    ) -> Result<Option<TurnStreamEnvelope>, ManagedBackendError> {
        let deadline = Instant::now() + idle_timeout;

        loop {
            if let Some(envelope) = self.drain_buffered_turn_stream_envelope()? {
                return Ok(Some(envelope));
            }

            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Ok(None);
            };
            let Some(message) = self.recv_message_timeout("turn stream", remaining)? else {
                return Ok(None);
            };
            if let Some(envelope) =
                normalize_turn_stream_message(message, self.approval_response_authority_generation)?
            {
                return Ok(Some(envelope));
            }
        }
    }

    /// Returns the next normalized turn-stream event, if the stream stays quiet.
    ///
    /// New code that needs retained-byte accounting should use
    /// [`Self::poll_turn_stream_envelope`].
    pub fn next_turn_stream_event(
        &mut self,
        idle_timeout: Duration,
    ) -> Result<Option<TurnStreamEvent>, ManagedBackendError> {
        self.poll_turn_stream_envelope(idle_timeout)
            .map(|envelope| envelope.map(TurnStreamEnvelope::into_event))
    }

    pub fn shutdown(&mut self) -> Result<(), ManagedBackendError> {
        self.transport.close();
        Ok(())
    }

    pub(crate) fn connect_websocket_uninitialized(
        endpoint: BackendWebSocketEndpoint,
        authorization_header_value: String,
    ) -> Result<Self, ManagedBackendError> {
        let approval_response_authority_generation =
            allocate_approval_response_authority_generation()?;
        let transport = WebSocketClientTransport::connect(&endpoint, authorization_header_value)?;

        Ok(Self {
            transport: BackendClientTransport::WebSocket(transport),
            initialize: None,
            compatibility: None,
            initialized_notification_profile: None,
            pending_messages: VecDeque::new(),
            pending_message_bytes: 0,
            pending_dynamic_tool_requests: 0,
            next_request_id: 1,
            approval_response_authority_generation,
        })
    }

    pub fn connect_websocket(
        endpoint: BackendWebSocketEndpoint,
        authorization_header_value: String,
        timeout: Duration,
    ) -> Result<Self, ManagedBackendError> {
        Self::connect_websocket_with_options(
            endpoint,
            authorization_header_value,
            ManagedBackendClientOptions::foreground(),
            timeout,
        )
    }

    pub fn connect_websocket_with_options(
        endpoint: BackendWebSocketEndpoint,
        authorization_header_value: String,
        options: ManagedBackendClientOptions,
        timeout: Duration,
    ) -> Result<Self, ManagedBackendError> {
        let mut session =
            Self::connect_websocket_uninitialized(endpoint, authorization_header_value)?;
        session.initialize_client_with_options(&options, timeout)?;
        Ok(session)
    }

    /// Probes every exact 0.144.1 method shape required by Beryl.
    ///
    /// The session must already have completed its initialize handshake.
    pub fn probe_compatibility(
        &mut self,
        config_cwd: &Path,
        timeout: Duration,
    ) -> Result<ManagedBackendProbeReport, ManagedBackendError> {
        let initialize = self
            .initialize
            .clone()
            .ok_or(ManagedBackendError::ClientNotInitialized)?;
        let compatibility = self
            .compatibility
            .clone()
            .ok_or(ManagedBackendError::ClientNotInitialized)?;

        let mut method_successes = Vec::with_capacity(compatibility.required_method_probes().len());
        let mut config_defaults = BackendConfigDefaults::default();
        let mut model_list = Vec::new();
        for probe in compatibility.required_method_probes() {
            if let Some(data) = self.probe_required_method(*probe, config_cwd, timeout)? {
                match data {
                    ProbeMethodData::ConfigDefaults(defaults) => config_defaults = defaults,
                    ProbeMethodData::ModelList(models) => model_list = models,
                }
            }
            method_successes.push(ProbeMethodSuccess { probe: *probe });
        }

        let thread_branch_capabilities = ThreadBranchCapabilities::new(true, true);

        Ok(ManagedBackendProbeReport {
            initialize,
            compatibility,
            method_successes,
            thread_branch_capabilities,
            config_defaults,
            model_list,
        })
    }

    pub(crate) fn initialize_client_with_options(
        &mut self,
        options: &ManagedBackendClientOptions,
        timeout: Duration,
    ) -> Result<(InitializeResponse, CompatibilitySnapshot), ManagedBackendError> {
        let initialize = self.request(
            INITIALIZE_METHOD,
            &InitializeParams {
                client_info: ClientInfo {
                    name: "beryl",
                    version: env!("CARGO_PKG_VERSION"),
                },
                capabilities: Some(InitializeCapabilities::for_options(options)),
            },
            timeout,
        )?;

        let compatibility = CompatibilitySnapshot::from_initialize_response(&initialize);
        compatibility.validate_required_app_server_version()?;

        self.notify_initialized()?;

        self.initialize = Some(initialize.clone());
        self.compatibility = Some(compatibility.clone());
        self.initialized_notification_profile =
            Some(if options.opt_out_notification_methods.is_empty() {
                InitializedNotificationProfile::FullTurnStream
            } else {
                InitializedNotificationProfile::OptedOut
            });

        Ok((initialize, compatibility))
    }

    fn notify_initialized(&mut self) -> Result<(), ManagedBackendError> {
        self.write_message(
            INITIALIZED_METHOD,
            &JsonRpcNotification::<Value> {
                jsonrpc: "2.0",
                method: INITIALIZED_METHOD,
                params: None,
            },
        )
        .map(|_| ())
    }

    fn probe_required_method(
        &mut self,
        probe: CompatibilityProbe,
        config_cwd: &Path,
        timeout: Duration,
    ) -> Result<Option<ProbeMethodData>, ManagedBackendError> {
        match probe {
            CompatibilityProbe::ConfigRead => {
                return self
                    .read_config(config_cwd, timeout)
                    .map(|response| Some(ProbeMethodData::ConfigDefaults(response.config)));
            }
            CompatibilityProbe::ModelList => {
                return self
                    .list_models_with_options(ModelListOptions::page(100), timeout)
                    .map(|models| Some(ProbeMethodData::ModelList(models)));
            }
            CompatibilityProbe::ThreadCompactStart => {
                let thread_id = probe_thread_id();
                self.probe_request_accepts_method(
                    probe.method(),
                    &ThreadCompactStartParams::new(&thread_id),
                    timeout,
                )?;
            }
            CompatibilityProbe::ThreadFork => {
                let thread_id = probe_thread_id();
                let options = ThreadLoadOptions::for_root(config_cwd);
                self.probe_request_accepts_method(
                    probe.method(),
                    &ThreadForkParams::full(&thread_id, &options),
                    timeout,
                )?;
            }
            CompatibilityProbe::ThreadInjectItems => {
                let thread_id = probe_thread_id();
                let batch = ThreadInjectionBatch::new(vec![
                    crate::ThreadInjectionItem::user_input_text("Beryl compatibility probe")
                        .expect("fixed compatibility text is valid"),
                ])
                .expect("fixed compatibility batch is valid");
                self.probe_request_accepts_method(
                    probe.method(),
                    &ThreadInjectItemsParams::new(&thread_id, &batch),
                    timeout,
                )?;
            }
            CompatibilityProbe::ThreadResume => {
                let thread_id = probe_thread_id();
                let options = ThreadLoadOptions::for_root(config_cwd);
                self.probe_request_accepts_method(
                    probe.method(),
                    &ThreadResumeParams::new(&thread_id, &options),
                    timeout,
                )?;
            }
            CompatibilityProbe::ThreadRollback => {
                let thread_id = probe_thread_id();
                self.probe_request_accepts_method(
                    probe.method(),
                    &ThreadRollbackParams::new(&thread_id, 1),
                    timeout,
                )?;
            }
            CompatibilityProbe::ThreadUnsubscribe => {
                let thread_id = probe_thread_id();
                let _: ThreadUnsubscribeResponse = self.request(
                    probe.method(),
                    &ThreadUnsubscribeParams::new(&thread_id),
                    timeout,
                )?;
            }
            CompatibilityProbe::TurnSteer => {
                let thread_id = probe_thread_id();
                let turn_id = probe_turn_id();
                self.probe_request_accepts_method(
                    probe.method(),
                    &TurnSteerParams::input(
                        &thread_id,
                        &turn_id,
                        vec![UserInput::text("Beryl compatibility probe")],
                    ),
                    timeout,
                )?;
            }
            CompatibilityProbe::TurnStart => {
                let thread_id = probe_thread_id();
                self.probe_request_accepts_method(
                    probe.method(),
                    &TurnStartParams::text(
                        &thread_id,
                        "Beryl compatibility probe",
                        TurnStartOptions::default(),
                    ),
                    timeout,
                )?;
            }
            CompatibilityProbe::TurnInterrupt => {
                let thread_id = probe_thread_id();
                let turn_id = probe_turn_id();
                self.probe_request_accepts_method(
                    probe.method(),
                    &TurnInterruptParams::new(&thread_id, &turn_id),
                    timeout,
                )?;
            }
        }

        Ok(None)
    }

    fn probe_thread_branch_capability(
        &mut self,
        probe: ThreadBranchCapabilityProbe,
        config_cwd: &Path,
        timeout: Duration,
    ) -> Result<ThreadBranchCapabilityProbeResult, ManagedBackendError> {
        let thread_id = probe_thread_id();
        let options = ThreadLoadOptions::for_root(config_cwd);
        let params = match probe {
            ThreadBranchCapabilityProbe::ThreadFork => {
                serde_json::to_value(ThreadForkParams::full(&thread_id, &options))
            }
            ThreadBranchCapabilityProbe::ThreadRollback => {
                serde_json::to_value(ThreadRollbackParams::new(&thread_id, 1))
            }
        }
        .map_err(|source| ManagedBackendError::SerializeRequest {
            method: probe.method().to_string(),
            source,
        })?;

        match self.request_json(probe.method(), &params, timeout)? {
            JsonRpcRequestOutcome::Result(_) => Ok(
                ThreadBranchCapabilityProbeResult::for_supported_probe(probe),
            ),
            JsonRpcRequestOutcome::Error(error) if error.code == JSONRPC_METHOD_NOT_FOUND => {
                Ok(ThreadBranchCapabilityProbeResult::unsupported(probe, error))
            }
            JsonRpcRequestOutcome::Error(_) => Ok(
                ThreadBranchCapabilityProbeResult::for_supported_probe(probe),
            ),
        }
    }

    fn probe_hard_stop_capability(
        &mut self,
        probe: HardStopCapabilityProbe,
        timeout: Duration,
    ) -> Result<HardStopCapabilityProbeResult, ManagedBackendError> {
        let thread_id = probe_thread_id();
        let params = match probe {
            HardStopCapabilityProbe::CommandExecTerminate => serde_json::to_value(
                CommandExecTerminateParams::new(PROBE_COMMAND_EXEC_PROCESS_ID),
            ),
            HardStopCapabilityProbe::ThreadBackgroundTerminalsClean => {
                serde_json::to_value(ThreadBackgroundTerminalsCleanParams::new(&thread_id))
            }
        }
        .map_err(|source| ManagedBackendError::SerializeRequest {
            method: probe.method().to_string(),
            source,
        })?;

        match self.request_json(probe.method(), &params, timeout)? {
            JsonRpcRequestOutcome::Result(_) => {
                Ok(HardStopCapabilityProbeResult::for_supported_probe(probe))
            }
            JsonRpcRequestOutcome::Error(error) if error.code == JSONRPC_METHOD_NOT_FOUND => {
                Ok(HardStopCapabilityProbeResult::unsupported(probe, error))
            }
            JsonRpcRequestOutcome::Error(_) => {
                Ok(HardStopCapabilityProbeResult::for_supported_probe(probe))
            }
        }
    }

    fn probe_request_accepts_method(
        &mut self,
        method: &str,
        params: &impl Serialize,
        timeout: Duration,
    ) -> Result<(), ManagedBackendError> {
        match self.request_json(method, params, timeout)? {
            JsonRpcRequestOutcome::Result(_) => Ok(()),
            JsonRpcRequestOutcome::Error(error)
                if error.code == JSONRPC_METHOD_NOT_FOUND
                    || error.code == JSONRPC_INVALID_PARAMS =>
            {
                Err(ManagedBackendError::RequestFailed {
                    method: method.to_string(),
                    error: Box::new(error),
                })
            }
            JsonRpcRequestOutcome::Error(_) => Ok(()),
        }
    }

    fn request<R: DeserializeOwned>(
        &mut self,
        method: &str,
        params: &impl Serialize,
        timeout: Duration,
    ) -> Result<R, ManagedBackendError> {
        let request_started = Instant::now();
        match self.request_json(method, params, timeout)? {
            JsonRpcRequestOutcome::Result(result) => {
                let deserialize_started = Instant::now();
                let response = serde_json::from_value(result).map_err(|source| {
                    ManagedBackendError::DeserializeResponse {
                        method: method.to_string(),
                        source,
                    }
                })?;
                let typed_deserialize = deserialize_started.elapsed();
                let typed_request_total = request_started.elapsed();
                debug!(
                    method,
                    typed_deserialize_ms = elapsed_ms(typed_deserialize),
                    typed_request_total_ms = elapsed_ms(typed_request_total),
                    "deserialized backend JSON-RPC response"
                );
                Ok(response)
            }
            JsonRpcRequestOutcome::Error(error) => Err(ManagedBackendError::RequestFailed {
                method: method.to_string(),
                error: Box::new(error),
            }),
        }
    }

    fn non_idempotent_request<R: DeserializeOwned>(
        &mut self,
        method: &str,
        params: &impl Serialize,
        timeout: Duration,
    ) -> NonIdempotentRequestOutcome<R> {
        match self.request_json_with_dispatch_evidence(method, params, timeout) {
            Ok(JsonRpcRequestOutcome::Result(result)) => {
                match serde_json::from_value(result).map_err(|source| {
                    ManagedBackendError::DeserializeResponse {
                        method: method.to_string(),
                        source,
                    }
                }) {
                    Ok(response) => NonIdempotentRequestOutcome::ExactResponse { response },
                    Err(error) => NonIdempotentRequestOutcome::CompletionUnknown {
                        error: Box::new(error),
                    },
                }
            }
            Ok(JsonRpcRequestOutcome::Error(error)) => {
                NonIdempotentRequestOutcome::ExactRejection { error }
            }
            Err(RequestAttemptFailure::ProvenNotDispatched(error)) => {
                NonIdempotentRequestOutcome::ProvenNotDispatched {
                    error: Box::new(error),
                }
            }
            Err(RequestAttemptFailure::CompletionUnknown(error)) => {
                NonIdempotentRequestOutcome::CompletionUnknown {
                    error: Box::new(error),
                }
            }
        }
    }

    fn request_json(
        &mut self,
        method: &str,
        params: &impl Serialize,
        timeout: Duration,
    ) -> Result<JsonRpcRequestOutcome, ManagedBackendError> {
        self.request_json_with_dispatch_evidence(method, params, timeout)
            .map_err(RequestAttemptFailure::into_error)
    }

    fn request_json_with_dispatch_evidence(
        &mut self,
        method: &str,
        params: &impl Serialize,
        timeout: Duration,
    ) -> Result<JsonRpcRequestOutcome, RequestAttemptFailure> {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        let request_started = Instant::now();

        let write_metrics = self.write_message_with_dispatch_evidence(
            method,
            &JsonRpcRequest {
                jsonrpc: "2.0",
                id: request_id,
                method,
                params,
            },
        )?;

        self.wait_for_json_rpc_response(method, request_id, timeout, request_started, write_metrics)
            .map_err(RequestAttemptFailure::CompletionUnknown)
    }

    fn wait_for_json_rpc_response(
        &mut self,
        method: &str,
        request_id: u64,
        timeout: Duration,
        request_started: Instant,
        write_metrics: MessageWriteMetrics,
    ) -> Result<JsonRpcRequestOutcome, ManagedBackendError> {
        let deadline = Instant::now() + timeout;
        let response_wait_started = Instant::now();
        let mut interleaved_notification_count = 0_usize;
        let mut interleaved_server_request_count = 0_usize;
        let mut denied_approval_request_count = 0_usize;
        let mut deferred_dynamic_tool_request_count = 0_usize;
        let mut out_of_order_response_count = 0_usize;
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(ManagedBackendError::RequestTimeout {
                    method: method.to_string(),
                    timeout,
                });
            };

            let message = match self.recv_message_timeout(method, remaining)? {
                Some(message) => message,
                None => {
                    return Err(ManagedBackendError::RequestTimeout {
                        method: method.to_string(),
                        timeout,
                    });
                }
            };

            match message {
                IncomingMessage::Response { id, result } if id == request_id => {
                    let response_wait = response_wait_started.elapsed();
                    let request_total = request_started.elapsed();
                    debug!(
                        method,
                        request_id,
                        outcome = "result",
                        request_bytes = write_metrics.bytes,
                        request_serialize_ms = elapsed_ms(write_metrics.serialize),
                        request_send_ms = elapsed_ms(write_metrics.transport),
                        response_wait_ms = elapsed_ms(response_wait),
                        request_total_ms = elapsed_ms(request_total),
                        interleaved_notification_count,
                        interleaved_server_request_count,
                        denied_approval_request_count,
                        deferred_dynamic_tool_request_count,
                        out_of_order_response_count,
                        "backend JSON-RPC request completed"
                    );
                    return Ok(JsonRpcRequestOutcome::Result(result));
                }
                IncomingMessage::Error { id, error } if id == Some(request_id) => {
                    let response_wait = response_wait_started.elapsed();
                    let request_total = request_started.elapsed();
                    debug!(
                        method,
                        request_id,
                        outcome = "error",
                        error_code = error.code,
                        request_bytes = write_metrics.bytes,
                        request_serialize_ms = elapsed_ms(write_metrics.serialize),
                        request_send_ms = elapsed_ms(write_metrics.transport),
                        response_wait_ms = elapsed_ms(response_wait),
                        request_total_ms = elapsed_ms(request_total),
                        interleaved_notification_count,
                        interleaved_server_request_count,
                        denied_approval_request_count,
                        deferred_dynamic_tool_request_count,
                        out_of_order_response_count,
                        "backend JSON-RPC request completed"
                    );
                    return Ok(JsonRpcRequestOutcome::Error(error));
                }
                IncomingMessage::Notification {
                    method: notification_method,
                    params,
                } => {
                    interleaved_notification_count += 1;
                    self.push_pending_message(
                        method,
                        IncomingMessage::Notification {
                            method: notification_method.clone(),
                            params,
                        },
                    )?;
                    debug!(
                        request_method = method,
                        notification_method,
                        "deferring backend notification while waiting for request response"
                    );
                }
                IncomingMessage::ServerRequest {
                    id,
                    method: request_method,
                    params,
                    approval_response_disposition: _,
                } => {
                    interleaved_server_request_count += 1;
                    if let Some(mut request) =
                        parse_approval_request(id.clone(), &request_method, params.clone())
                    {
                        request.bind_response_authority(
                            self.approval_response_authority_generation,
                            ApprovalResponseDisposition::ResponseRequired,
                        );
                        denied_approval_request_count += 1;
                        self.push_pending_message(
                            method,
                            IncomingMessage::ServerRequest {
                                id: id.clone(),
                                method: request_method.clone(),
                                params,
                                approval_response_disposition:
                                    ApprovalResponseDisposition::ResponseRequired,
                            },
                        )?;
                        warn!(
                            approval = %request.summary(),
                            approval_payload = %request.pretty_params(),
                            "denying and retaining backend approval request received while waiting for another response"
                        );
                        self.deny_approval_request(&request)?;
                        self.mark_last_pending_approval_auto_denied(method, &id)?;
                    } else if is_dynamic_tool_call_method(&request_method) {
                        deferred_dynamic_tool_request_count += 1;
                        self.push_pending_message(
                            method,
                            IncomingMessage::ServerRequest {
                                id,
                                method: request_method.clone(),
                                params,
                                approval_response_disposition:
                                    ApprovalResponseDisposition::ResponseRequired,
                            },
                        )?;
                        warn!(
                            request_method = method,
                            server_request_method = request_method,
                            "deferring backend dynamic tool-call request while waiting for request response"
                        );
                    } else {
                        warn!(
                            request_method = method,
                            server_request_method = request_method,
                            "ignoring unsupported backend server request while waiting for request response"
                        );
                    }
                }
                IncomingMessage::Response { id, .. } => {
                    out_of_order_response_count += 1;
                    warn!(
                        request_method = method,
                        response_id = id,
                        expected_id = request_id,
                        "ignoring out-of-order backend response during sequential probe"
                    );
                }
                IncomingMessage::Error { id, error } => {
                    out_of_order_response_count += 1;
                    warn!(
                        request_method = method,
                        ?id,
                        code = error.code,
                        message = %error.message,
                        "ignoring unrelated backend error response during sequential probe"
                    );
                }
            }
        }
    }

    fn pop_pending_message(&mut self) -> Option<IncomingMessage> {
        let message = self.pending_messages.pop_front()?;
        self.pending_message_bytes = self
            .pending_message_bytes
            .saturating_sub(message.approximate_retained_bytes());
        if message.is_dynamic_tool_request() {
            self.pending_dynamic_tool_requests =
                self.pending_dynamic_tool_requests.saturating_sub(1);
        }
        Some(message)
    }

    fn mark_last_pending_approval_auto_denied(
        &mut self,
        method: &str,
        request_id: &Value,
    ) -> Result<(), ManagedBackendError> {
        match self.pending_messages.back_mut() {
            Some(IncomingMessage::ServerRequest {
                id,
                approval_response_disposition,
                ..
            }) if id == request_id => {
                *approval_response_disposition = ApprovalResponseDisposition::AutoDenied;
                Ok(())
            }
            _ => Err(ManagedBackendError::PendingApprovalStateLost {
                method: method.to_string(),
            }),
        }
    }

    fn push_pending_message(
        &mut self,
        method: &str,
        message: IncomingMessage,
    ) -> Result<(), ManagedBackendError> {
        if self.pending_messages.len() >= PENDING_MESSAGE_COUNT_LIMIT {
            return Err(bounded_resource_exceeded(
                method,
                "pending message queue count",
                PENDING_MESSAGE_COUNT_LIMIT,
            ));
        }

        let dynamic_tool_request = message.is_dynamic_tool_request();
        if dynamic_tool_request
            && self.pending_dynamic_tool_requests >= PENDING_DYNAMIC_TOOL_REQUEST_LIMIT
        {
            return Err(bounded_resource_exceeded(
                method,
                "dynamic tool-call request queue count",
                PENDING_DYNAMIC_TOOL_REQUEST_LIMIT,
            ));
        }

        let message_bytes = message.approximate_retained_bytes();
        if self.pending_message_bytes.saturating_add(message_bytes) > PENDING_MESSAGE_BYTE_BUDGET {
            return Err(bounded_resource_exceeded(
                method,
                "pending message queue byte budget",
                PENDING_MESSAGE_BYTE_BUDGET,
            ));
        }

        self.pending_message_bytes = self.pending_message_bytes.saturating_add(message_bytes);
        if dynamic_tool_request {
            self.pending_dynamic_tool_requests += 1;
        }
        self.pending_messages.push_back(message);
        Ok(())
    }

    fn write_message(
        &mut self,
        method: &str,
        message: &impl Serialize,
    ) -> Result<MessageWriteMetrics, ManagedBackendError> {
        self.write_message_with_dispatch_evidence(method, message)
            .map_err(RequestAttemptFailure::into_error)
    }

    fn write_message_with_dispatch_evidence(
        &mut self,
        method: &str,
        message: &impl Serialize,
    ) -> Result<MessageWriteMetrics, RequestAttemptFailure> {
        let serialize_started = Instant::now();
        let line = serde_json::to_string(message).map_err(|source| {
            RequestAttemptFailure::ProvenNotDispatched(ManagedBackendError::SerializeRequest {
                method: method.to_string(),
                source,
            })
        })?;
        let serialize = serialize_started.elapsed();
        let bytes = line.len();
        let transport_started = Instant::now();
        self.transport
            .write_message(method, &line)
            .map_err(|failure| match failure {
                TransportWriteFailure::ProvenNotDispatched(error) => {
                    RequestAttemptFailure::ProvenNotDispatched(error)
                }
                TransportWriteFailure::MayHaveDispatched(error) => {
                    RequestAttemptFailure::CompletionUnknown(error)
                }
            })?;
        Ok(MessageWriteMetrics {
            serialize,
            transport: transport_started.elapsed(),
            bytes,
        })
    }

    fn write_server_response<T: Serialize + ?Sized>(
        &mut self,
        method: &str,
        request_id: &Value,
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
        .map(|_| ())
    }

    fn recv_message_timeout(
        &mut self,
        method: &str,
        timeout: Duration,
    ) -> Result<Option<IncomingMessage>, ManagedBackendError> {
        self.transport.recv_message_timeout(method, timeout)
    }
}

impl Drop for ManagedBackendSession {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            warn!(%error, "failed to shut down managed backend session");
        }
    }
}

impl ManagedBackendProbeReport {
    pub fn initialize(&self) -> &InitializeResponse {
        &self.initialize
    }

    pub fn compatibility(&self) -> &CompatibilitySnapshot {
        &self.compatibility
    }

    pub fn method_successes(&self) -> &[ProbeMethodSuccess] {
        &self.method_successes
    }

    pub fn thread_branch_capabilities(&self) -> &ThreadBranchCapabilities {
        &self.thread_branch_capabilities
    }

    pub fn model_list(&self) -> &[ModelInfo] {
        &self.model_list
    }

    pub fn config_defaults(&self) -> &BackendConfigDefaults {
        &self.config_defaults
    }
}

impl ProbeMethodSuccess {
    pub fn probe(&self) -> CompatibilityProbe {
        self.probe
    }
}

impl ManagedBackendClientOptions {
    pub fn foreground() -> Self {
        Self::default()
    }

    pub fn request_only() -> Self {
        Self::default()
            .with_opt_out_notification_methods(REQUEST_ONLY_NOTIFICATION_METHODS.iter().copied())
    }

    pub fn with_opt_out_notification_methods<I, S>(mut self, methods: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.opt_out_notification_methods.clear();
        for method in methods {
            let method = method.into();
            let method = method.trim();
            if method.is_empty()
                || self
                    .opt_out_notification_methods
                    .iter()
                    .any(|existing| existing == method)
            {
                continue;
            }
            self.opt_out_notification_methods.push(method.to_string());
        }
        self
    }

    pub fn opt_out_notification_methods(&self) -> &[String] {
        &self.opt_out_notification_methods
    }
}

#[derive(Debug)]
enum IncomingMessage {
    Response {
        id: u64,
        result: Value,
    },
    Error {
        id: Option<u64>,
        error: JsonRpcError,
    },
    Notification {
        method: String,
        params: Option<Value>,
    },
    ServerRequest {
        id: Value,
        method: String,
        params: Option<Value>,
        approval_response_disposition: ApprovalResponseDisposition,
    },
}

impl IncomingMessage {
    fn approximate_retained_bytes(&self) -> usize {
        match self {
            Self::Response { result, .. } => json_value_retained_byte_len(result),
            Self::Error { id, error } => std::mem::size_of::<i64>()
                .saturating_add(id.map(|_| std::mem::size_of::<u64>()).unwrap_or_default())
                .saturating_add(error.message.len())
                .saturating_add(optional_json_value_retained_byte_len(error.data.as_ref())),
            Self::Notification { method, params } => method
                .len()
                .saturating_add(optional_json_value_retained_byte_len(params.as_ref())),
            Self::ServerRequest {
                id, method, params, ..
            } => json_value_retained_byte_len(id)
                .saturating_add(method.len())
                .saturating_add(optional_json_value_retained_byte_len(params.as_ref())),
        }
    }

    fn is_dynamic_tool_request(&self) -> bool {
        matches!(
            self,
            Self::ServerRequest { method, .. } if is_dynamic_tool_call_method(method)
        )
    }
}

fn normalize_turn_stream_message(
    message: IncomingMessage,
    approval_response_authority_generation: u64,
) -> Result<Option<TurnStreamEnvelope>, ManagedBackendError> {
    let approximate_retained_bytes = message.approximate_retained_bytes();
    let event = match message {
        IncomingMessage::Notification { method, params } => {
            match parse_turn_stream_event(&method, params) {
                Ok(Some(event)) => Some(event),
                Ok(None) => {
                    warn!(
                        notification_method = method,
                        "ignoring unsupported backend notification during turn stream"
                    );
                    None
                }
                Err(source) => {
                    return Err(ManagedBackendError::DeserializeNotification { method, source });
                }
            }
        }
        IncomingMessage::ServerRequest {
            id,
            method,
            params,
            approval_response_disposition,
        } => {
            if let Some(mut request) = parse_approval_request(id.clone(), &method, params.clone()) {
                request.bind_response_authority(
                    approval_response_authority_generation,
                    approval_response_disposition,
                );
                Some(TurnStreamEvent::ApprovalRequested(request))
            } else {
                match parse_dynamic_tool_call_request(id, &method, params) {
                    Ok(Some(request)) => Some(TurnStreamEvent::DynamicToolCallRequested(request)),
                    Ok(None) => {
                        warn!(
                            request_method = method,
                            "ignoring unsupported backend server request during turn stream"
                        );
                        None
                    }
                    Err(source) => {
                        return Err(ManagedBackendError::DeserializeServerRequest {
                            method,
                            source,
                        });
                    }
                }
            }
        }
        IncomingMessage::Error { error, .. } => Some(TurnStreamEvent::ProtocolError { error }),
        IncomingMessage::Response { id, .. } => {
            warn!(
                response_id = id,
                "ignoring unexpected backend response during turn stream"
            );
            None
        }
    };

    Ok(event.map(|event| TurnStreamEnvelope::new(event, approximate_retained_bytes)))
}

enum JsonRpcRequestOutcome {
    Result(Value),
    Error(JsonRpcError),
}

enum RequestAttemptFailure {
    ProvenNotDispatched(ManagedBackendError),
    CompletionUnknown(ManagedBackendError),
}

impl RequestAttemptFailure {
    fn into_error(self) -> ManagedBackendError {
        match self {
            Self::ProvenNotDispatched(error) | Self::CompletionUnknown(error) => error,
        }
    }
}

pub(crate) enum TransportWriteFailure {
    ProvenNotDispatched(ManagedBackendError),
    MayHaveDispatched(ManagedBackendError),
}

struct MessageWriteMetrics {
    serialize: Duration,
    transport: Duration,
    bytes: usize,
}

enum BackendClientTransport {
    Stdio {
        stdin: Option<ChildStdin>,
        messages: Receiver<Result<IncomingMessage, ManagedBackendError>>,
    },
    WebSocket(WebSocketClientTransport),
}

impl BackendClientTransport {
    fn write_message(&mut self, method: &str, line: &str) -> Result<(), TransportWriteFailure> {
        match self {
            Self::Stdio { stdin, .. } => {
                let Some(stdin) = stdin.as_mut() else {
                    return Err(TransportWriteFailure::ProvenNotDispatched(
                        ManagedBackendError::TransportClosed {
                            method: method.to_string(),
                        },
                    ));
                };
                let mut bytes = line.as_bytes().to_vec();
                bytes.push(b'\n');
                stdin
                    .write_all(&bytes)
                    .and_then(|()| stdin.flush())
                    .map_err(|source| {
                        TransportWriteFailure::MayHaveDispatched(
                            ManagedBackendError::WriteRequest {
                                method: method.to_string(),
                                source,
                            },
                        )
                    })
            }
            Self::WebSocket(transport) => transport.write_message(method, line),
        }
    }

    fn recv_message_timeout(
        &mut self,
        method: &str,
        timeout: Duration,
    ) -> Result<Option<IncomingMessage>, ManagedBackendError> {
        match self {
            Self::Stdio { messages, .. } => match messages.recv_timeout(timeout) {
                Ok(message) => message.map(Some),
                Err(RecvTimeoutError::Timeout) => Ok(None),
                Err(RecvTimeoutError::Disconnected) => Err(ManagedBackendError::TransportClosed {
                    method: method.to_string(),
                }),
            },
            Self::WebSocket(transport) => {
                match transport.recv_json_value_timeout(method, timeout)? {
                    Some(value) => parse_incoming_value(value).map(Some),
                    None => Ok(None),
                }
            }
        }
    }

    #[cfg(feature = "lifecycle-test-support")]
    fn last_websocket_ingress_test_metrics(&self) -> Option<(usize, usize, usize, usize, bool)> {
        let Self::WebSocket(transport) = self else {
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

    fn close(&mut self) {
        match self {
            Self::Stdio { stdin, .. } => {
                drop(stdin.take());
            }
            Self::WebSocket(transport) => {
                transport.close();
            }
        }
    }
}

impl std::fmt::Debug for BackendClientTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stdio { .. } => f.write_str("BackendClientTransport::Stdio"),
            Self::WebSocket(transport) => f
                .debug_struct("BackendClientTransport::WebSocket")
                .field("endpoint", &transport.endpoint())
                .finish(),
        }
    }
}

#[derive(Serialize)]
struct JsonRpcRequest<'a, T> {
    jsonrpc: &'static str,
    id: u64,
    method: &'a str,
    params: &'a T,
}

#[derive(Serialize)]
struct JsonRpcNotification<'a, T> {
    jsonrpc: &'static str,
    method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<T>,
}

#[derive(Serialize)]
struct JsonRpcServerResponse<'a, T: Serialize + ?Sized> {
    jsonrpc: &'static str,
    id: &'a Value,
    result: &'a T,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InitializeParams<'a> {
    client_info: ClientInfo<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capabilities: Option<InitializeCapabilities>,
}

#[derive(Serialize)]
struct ClientInfo<'a> {
    name: &'a str,
    version: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InitializeCapabilities {
    experimental_api: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    opt_out_notification_methods: Option<Vec<String>>,
}

impl InitializeCapabilities {
    fn for_options(options: &ManagedBackendClientOptions) -> Self {
        let opt_out_notification_methods = (!options.opt_out_notification_methods.is_empty())
            .then(|| options.opt_out_notification_methods.clone());
        Self {
            experimental_api: true,
            opt_out_notification_methods,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FsReadFileParams<'a> {
    path: &'a str,
}

impl<'a> FsReadFileParams<'a> {
    fn new(path: &'a str) -> Self {
        Self { path }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FsReadFileResponse {
    data_base64: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadCompactStartParams<'a> {
    thread_id: &'a CasThreadId,
}

impl<'a> ThreadCompactStartParams<'a> {
    fn new(thread_id: &'a CasThreadId) -> Self {
        Self { thread_id }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnInterruptParams<'a> {
    thread_id: &'a CasThreadId,
    turn_id: &'a CasTurnId,
}

impl<'a> TurnInterruptParams<'a> {
    fn new(thread_id: &'a CasThreadId, turn_id: &'a CasTurnId) -> Self {
        Self { thread_id, turn_id }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandExecTerminateParams<'a> {
    process_id: &'a str,
}

impl<'a> CommandExecTerminateParams<'a> {
    fn new(process_id: &'a str) -> Self {
        Self { process_id }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadBackgroundTerminalsCleanParams<'a> {
    thread_id: &'a CasThreadId,
}

impl<'a> ThreadBackgroundTerminalsCleanParams<'a> {
    fn new(thread_id: &'a CasThreadId) -> Self {
        Self { thread_id }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadUnsubscribeParams<'a> {
    thread_id: &'a CasThreadId,
}

impl<'a> ThreadUnsubscribeParams<'a> {
    fn new(thread_id: &'a CasThreadId) -> Self {
        Self { thread_id }
    }
}

#[derive(serde::Deserialize)]
struct EmptyResponse {}

enum BoundedLineRead {
    Eof,
    Line(Vec<u8>),
    LineTooLong { prefix: Vec<u8> },
}

fn read_bounded_line_bytes(reader: &mut impl BufRead, limit: usize) -> io::Result<BoundedLineRead> {
    let mut line = Vec::new();
    let mut over_limit = false;
    let mut saw_bytes = false;

    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if !saw_bytes {
                Ok(BoundedLineRead::Eof)
            } else if over_limit {
                Ok(BoundedLineRead::LineTooLong { prefix: line })
            } else {
                Ok(BoundedLineRead::Line(line))
            };
        }

        saw_bytes = true;
        let newline_index = available.iter().position(|byte| *byte == b'\n');
        let take = newline_index.map_or(available.len(), |index| index + 1);

        if over_limit {
            reader.consume(take);
            if newline_index.is_some() {
                return Ok(BoundedLineRead::LineTooLong { prefix: line });
            }
            continue;
        }

        let remaining_budget = limit.saturating_sub(line.len());
        if take > remaining_budget {
            line.extend_from_slice(&available[..remaining_budget]);
            over_limit = true;
        } else {
            line.extend_from_slice(&available[..take]);
        }

        reader.consume(take);

        if newline_index.is_some() {
            return if over_limit {
                Ok(BoundedLineRead::LineTooLong { prefix: line })
            } else {
                Ok(BoundedLineRead::Line(line))
            };
        }
    }
}

fn spawn_stdout_reader(
    stdout: ChildStdout,
) -> Receiver<Result<IncomingMessage, ManagedBackendError>> {
    let (sender, receiver) = mpsc::sync_channel(STDIO_MESSAGE_CHANNEL_BOUND);
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_bounded_line_bytes(&mut reader, STDIO_STDOUT_LINE_BYTE_LIMIT) {
                Ok(BoundedLineRead::Eof) => break,
                Ok(BoundedLineRead::Line(line)) => {
                    let line = match std::str::from_utf8(&line) {
                        Ok(line) => line,
                        Err(source) => {
                            let error = io::Error::new(io::ErrorKind::InvalidData, source);
                            let _ = sender
                                .send(Err(ManagedBackendError::ReadTransport { source: error }));
                            break;
                        }
                    };
                    let json_line = line.trim();
                    if json_line.is_empty() {
                        continue;
                    }

                    let message = parse_incoming_message(json_line);
                    if sender.send(message).is_err() {
                        break;
                    }
                }
                Ok(BoundedLineRead::LineTooLong { .. }) => {
                    let error = bounded_resource_exceeded(
                        "stdio stdout",
                        "stdio stdout line byte length",
                        STDIO_STDOUT_LINE_BYTE_LIMIT,
                    );
                    let _ = sender.send(Err(error));
                    break;
                }
                Err(source) => {
                    let _ = sender.send(Err(ManagedBackendError::ReadTransport { source }));
                    break;
                }
            }
        }
    });
    receiver
}

pub(crate) fn spawn_stderr_logger(stderr: ChildStderr, launch_label: String) {
    thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        loop {
            match read_bounded_line_bytes(&mut reader, STDIO_STDERR_LINE_BYTE_LIMIT) {
                Ok(BoundedLineRead::Eof) => break,
                Ok(BoundedLineRead::Line(line))
                | Ok(BoundedLineRead::LineTooLong { prefix: line, .. }) => {
                    let line = String::from_utf8_lossy(&line);
                    if line.trim().is_empty() {
                        continue;
                    }
                    let message = truncate_for_log(&line, STDERR_LOG_LIMIT);
                    debug!(
                        launch = %launch_label,
                        message = %message,
                        "backend stderr"
                    );
                }
                Err(error) => {
                    warn!(
                        launch = %launch_label,
                        %error,
                        "failed to read backend stderr"
                    );
                    break;
                }
            }
        }
    });
}

fn truncate_for_log(line: &str, limit: usize) -> String {
    if line.chars().count() <= limit {
        return line.to_string();
    }

    let truncated: String = line.chars().take(limit).collect();
    format!("{truncated}...")
}

fn injection_transport_was_lost(error: &ManagedBackendError) -> bool {
    matches!(
        error,
        ManagedBackendError::WriteRequest { .. }
            | ManagedBackendError::ReadTransport { .. }
            | ManagedBackendError::ProcessExited { .. }
            | ManagedBackendError::TransportClosed { .. }
            | ManagedBackendError::WebSocketTransport { .. }
    )
}

fn bounded_resource_exceeded(
    method: &str,
    resource: &'static str,
    limit: usize,
) -> ManagedBackendError {
    ManagedBackendError::BoundedResourceExceeded {
        method: method.to_string(),
        resource,
        limit,
    }
}

fn optional_json_value_retained_byte_len(value: Option<&Value>) -> usize {
    value.map(json_value_retained_byte_len).unwrap_or_default()
}

fn json_value_retained_byte_len(value: &Value) -> usize {
    match value {
        Value::Null | Value::Bool(_) => 0,
        Value::Number(_) => std::mem::size_of::<serde_json::Number>(),
        Value::String(text) => text.len(),
        Value::Array(values) => values
            .iter()
            .fold(values.len() * std::mem::size_of::<Value>(), {
                |total, value| total.saturating_add(json_value_retained_byte_len(value))
            }),
        Value::Object(entries) => entries.iter().fold(
            entries.len() * (std::mem::size_of::<String>() + std::mem::size_of::<Value>()),
            |total, (key, value)| {
                total
                    .saturating_add(key.len())
                    .saturating_add(json_value_retained_byte_len(value))
            },
        ),
    }
}

fn parse_incoming_message(line: &str) -> Result<IncomingMessage, ManagedBackendError> {
    let value = crate::incoming_json::decode_value(line).map_err(|source| {
        ManagedBackendError::InvalidJsonLine {
            line: crate::incoming_json::redacted_invalid_json(),
            source,
        }
    })?;

    parse_incoming_value(value)
}

fn parse_incoming_value(value: Value) -> Result<IncomingMessage, ManagedBackendError> {
    let Some(object) = value.as_object() else {
        return Err(ManagedBackendError::UnexpectedMessageShape);
    };

    if let Some(method) = object.get("method").and_then(Value::as_str) {
        if let Some(id) = object.get("id").cloned().filter(|id| !id.is_null()) {
            return Ok(IncomingMessage::ServerRequest {
                id,
                method: method.to_string(),
                params: object.get("params").cloned(),
                approval_response_disposition: ApprovalResponseDisposition::ResponseRequired,
            });
        }
        return Ok(IncomingMessage::Notification {
            method: method.to_string(),
            params: object.get("params").cloned(),
        });
    }

    if let Some(error) = object.get("error") {
        let id = object.get("id").and_then(Value::as_u64);
        let error = serde_json::from_value(error.clone()).map_err(|source| {
            ManagedBackendError::DeserializeResponse {
                method: "error".to_string(),
                source,
            }
        })?;
        return Ok(IncomingMessage::Error { id, error });
    }

    if let (Some(id), Some(result)) = (
        object.get("id").and_then(Value::as_u64),
        object.get("result").cloned(),
    ) {
        return Ok(IncomingMessage::Response { id, result });
    }

    Err(ManagedBackendError::UnexpectedMessageShape)
}
