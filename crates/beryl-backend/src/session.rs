use std::{
    collections::{HashSet, VecDeque},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[cfg(feature = "lifecycle-test-support")]
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use thiserror::Error;
use tracing::{debug, warn};

use crate::{
    AccountRateLimitsResponse, ApprovalRequest, ApprovalRequestKind, BackendCommandLineError,
    BackendConfigDefaults, BackendLaunchSpec, BackendWebSocketEndpoint, CompatibilityError,
    CompatibilityProbe, CompatibilitySnapshot, ConfigReadOptions, ConfigReadResponse,
    DynamicToolCallRequest, DynamicToolCallResponse, HardStopCapabilityProbe,
    HardStopCapabilityProbeResult, HardStopCapabilityReport, HardStopTarget, HardStopTargetOutcome,
    InitializeResponse, JsonRpcError, ManagedBackendLaunchOptionsError, ModelInfo,
    ModelListOptions, ModelListResponse, ThreadBranchCapabilities, ThreadBranchCapabilityProbe,
    ThreadBranchCapabilityProbeResult, ThreadBranchCapabilityReport, ThreadForkOptions,
    ThreadForkResponse, ThreadListBudget, ThreadListCollection, ThreadListCollectionStatus,
    ThreadListResponse, ThreadListTruncationReason, ThreadLoadedListResponse, ThreadReadMetadata,
    ThreadReadOptions, ThreadReadResponse, ThreadResumeOptions, ThreadRollbackResponse,
    ThreadSessionResponse, ThreadStartOptions, ThreadSummary, ThreadTurnsListOptions,
    ThreadTurnsListResponse, ThreadUnsubscribeResponse, TurnStartOptions, TurnStartResponse,
    TurnSteerResponse, TurnStreamEvent, UserInput,
    dynamic_tool::{is_dynamic_tool_call_method, parse_dynamic_tool_call_request},
    hard_stop::HARD_STOP_CAPABILITY_PROBES,
    managed_process::SupervisedBackendProcess,
    protocol::{SortDirection, ThreadListOptions, ThreadSortKey},
    response_sanitizer::{response_sanitizer_kind, sanitize_json_rpc_message},
    thread_branch::{THREAD_BRANCH_CAPABILITY_PROBES, ThreadForkParams, ThreadRollbackParams},
    thread_history::{ThreadReadParams, ThreadResumeParams, ThreadTurnsListParams},
    thread_lifecycle::{
        ThreadArchiveParams, ThreadDeleteParams, ThreadLifecycleEmptyResponse,
        ThreadUnarchiveParams, ThreadUnarchiveResponse,
    },
    turn::{
        ThreadStartParams, TurnStartParams, TurnSteerParams, parse_approval_request,
        parse_turn_stream_event,
    },
    websocket_transport::{WebSocketClientTransport, WebSocketReceiveMode},
};

const INITIALIZE_METHOD: &str = "initialize";
const INITIALIZED_METHOD: &str = "initialized";
const JSONRPC_METHOD_NOT_FOUND: i64 = -32601;
const JSONRPC_INVALID_PARAMS: i64 = -32602;
const PROBE_THREAD_ID: &str = "00000000-0000-0000-0000-000000000000";
const PROBE_TURN_ID: &str = "00000000-0000-0000-0000-000000000001";
const PROBE_COMMAND_EXEC_PROCESS_ID: &str = "beryl-hard-stop-probe";
const STDERR_LOG_LIMIT: usize = 240;
const INVALID_JSON_ERROR_LINE_LIMIT: usize = 4 * 1024;
const PENDING_MESSAGE_COUNT_LIMIT: usize = 1024;
const PENDING_MESSAGE_BYTE_BUDGET: usize = 16 * 1024 * 1024;
const PENDING_DYNAMIC_TOOL_REQUEST_LIMIT: usize = 64;
const STDIO_STDOUT_LINE_BYTE_LIMIT: usize = 64 * 1024 * 1024;
const STDIO_STDERR_LINE_BYTE_LIMIT: usize = 8 * 1024;
const STDIO_MESSAGE_CHANNEL_BOUND: usize = 64;
const STDIO_PROCESS_CLOSE_GRACE_TIMEOUT: Duration = Duration::from_millis(250);
const MANAGED_PROCESS_KILL_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_ONLY_NOTIFICATION_METHODS: &[&str] = &[
    "thread/started",
    "thread/archived",
    "thread/unarchived",
    "thread/deleted",
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
    "item/reasoning/summaryPartAdded",
    "item/reasoning/summaryTextDelta",
    "item/reasoning/textDelta",
    "item/commandExecution/outputDelta",
    "item/fileChange/outputDelta",
    "item/mcpToolCall/progress",
    "codex/event/collab_agent_spawn_end",
];

fn elapsed_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn sanitized_value_byte_len(value: &Value) -> Option<usize> {
    if !backend_metrics_debug_enabled() {
        return None;
    }
    serde_json::to_vec(value).ok().map(|bytes| bytes.len())
}

fn backend_metrics_debug_enabled() -> bool {
    tracing::enabled!(
        target: "beryl_backend::backend_metrics",
        tracing::Level::DEBUG
    )
}

const STARTUP_STAGES: &[ManagedBackendStartupStage] = &[
    ManagedBackendStartupStage::LaunchProcess,
    ManagedBackendStartupStage::InitializeHandshake,
    ManagedBackendStartupStage::ValidateRuntime,
    ManagedBackendStartupStage::VerifyRequiredMethods,
    ManagedBackendStartupStage::Ready,
];

#[derive(Debug)]
pub struct ManagedBackendSession {
    launch_spec: BackendLaunchSpec,
    process: Option<SupervisedBackendProcess>,
    transport: BackendClientTransport,
    pending_messages: VecDeque<IncomingMessage>,
    pending_message_bytes: usize,
    pending_dynamic_tool_requests: usize,
    next_request_id: u64,
    last_request_dispatched: bool,
    poisoned: bool,
    terminal_cleanup: Option<StdioTerminalCleanup>,
    #[cfg(feature = "lifecycle-test-support")]
    transport_close_classification_pause: Option<TransportCloseClassificationPause>,
}

#[cfg(feature = "lifecycle-test-support")]
#[derive(Debug)]
struct TransportCloseClassificationPause {
    entered: SyncSender<()>,
    release: Receiver<()>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManagedBackendStartupStage {
    LaunchProcess,
    InitializeHandshake,
    ValidateRuntime,
    VerifyRequiredMethods,
    Ready,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedBackendStartupProgress {
    stage: ManagedBackendStartupStage,
    detail: Option<String>,
}

#[derive(Debug, Error)]
pub enum ManagedBackendError {
    #[error("managed backend launch options are invalid")]
    InvalidLaunchOptions {
        #[from]
        source: ManagedBackendLaunchOptionsError,
    },
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
    #[error("backend returned an invalid {method} response during streaming sanitization")]
    SanitizeResponse {
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
    #[error(
        "backend session is terminal after an indeterminate transport write while handling {method}"
    )]
    SessionPoisoned { method: String },
    #[error("supervised backend stdio writer stopped before acknowledging {method}")]
    StdioWriterStopped { method: String },
    #[error("supervised backend stdio writer panicked during cleanup")]
    StdioWriterPanicked,
    #[error("supervised backend stdio cleanup reported multiple failures: {failures:?}")]
    StdioCleanupFailures { failures: Vec<ManagedBackendError> },
    #[error("backend returned {returned} thread-list rows after a page limit of {requested_limit}")]
    ThreadListPageLimitExceeded {
        requested_limit: u32,
        returned: usize,
    },
    #[error("backend repeated thread-list cursor {cursor:?} without progress")]
    ThreadListCursorRepeated { cursor: String },
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
    #[error("failed to clean up managed backend WebSocket token file {path}: {source}")]
    CleanUpWebSocketTokenFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "managed backend shutdown failed for both process supervision ({process}) and auth cleanup ({auth})"
    )]
    ShutdownProcessAndAuth {
        process: Box<ManagedBackendError>,
        auth: Box<ManagedBackendError>,
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
    RequestFailed { method: String, error: JsonRpcError },
    #[error("backend response line did not match JSON-RPC response or notification shape")]
    UnexpectedMessageShape,
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

/// A failed bounded thread-list collection with all prior trustworthy pages.
#[derive(Debug, Error)]
#[error(
    "bounded thread listing failed after {pages_collected} successfully collected page(s): {source}"
)]
pub struct ThreadListCollectionError {
    pub data: Vec<ThreadSummary>,
    pub next_cursor: Option<String>,
    pub pages_collected: usize,
    #[source]
    pub source: ManagedBackendError,
}

/// The commitment status of a failed `thread/fork` request.
///
/// [`Self::NotCommitted`] is returned only when Beryl serialized no request or
/// when app-server explicitly rejected the request. Every other failure is
/// [`Self::Indeterminate`], because a child may have been created without its
/// exact id reaching this client.
#[derive(Debug, Error)]
pub enum ThreadForkFailure {
    #[error("thread/fork definitively did not commit: {source}")]
    NotCommitted {
        #[source]
        source: ManagedBackendError,
    },
    #[error("thread/fork may have committed but no child id was returned: {source}")]
    Indeterminate {
        #[source]
        source: ManagedBackendError,
    },
}

impl ThreadForkFailure {
    fn classify(source: ManagedBackendError, dispatched: bool) -> Self {
        match source {
            ManagedBackendError::SerializeRequest { .. }
            | ManagedBackendError::RequestFailed { .. } => Self::NotCommitted { source },
            _ if !dispatched => Self::NotCommitted { source },
            _ => Self::Indeterminate { source },
        }
    }

    /// Returns the backend error that caused this fork failure.
    pub fn source_error(&self) -> &ManagedBackendError {
        match self {
            Self::NotCommitted { source } | Self::Indeterminate { source } => source,
        }
    }
}

#[derive(Debug)]
pub struct ManagedWebSocketError {
    message: String,
    io_error_kind: Option<io::ErrorKind>,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
    kind: ManagedWebSocketErrorKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManagedWebSocketErrorKind {
    Other,
    DeadlineExpired,
    ReadDeadlineExpiredAfterProgress,
    WriteDeadlineExpired,
    WriteFailedAfterCommit,
}

impl ManagedWebSocketError {
    pub fn protocol(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            io_error_kind: None,
            source: None,
            kind: ManagedWebSocketErrorKind::Other,
        }
    }

    pub fn io_error_kind(&self) -> Option<io::ErrorKind> {
        self.io_error_kind
    }

    pub(crate) fn deadline_expired() -> Self {
        Self {
            message: "request deadline expired".to_string(),
            io_error_kind: Some(io::ErrorKind::TimedOut),
            source: None,
            kind: ManagedWebSocketErrorKind::DeadlineExpired,
        }
    }

    pub(crate) fn is_deadline_expired(&self) -> bool {
        self.kind == ManagedWebSocketErrorKind::DeadlineExpired
    }

    pub(crate) fn read_deadline_expired_after_progress() -> Self {
        Self {
            message: "receive deadline expired after inbound framing progress".to_string(),
            io_error_kind: Some(io::ErrorKind::TimedOut),
            source: None,
            kind: ManagedWebSocketErrorKind::ReadDeadlineExpiredAfterProgress,
        }
    }

    pub(crate) fn is_read_deadline_expired_after_progress(&self) -> bool {
        self.kind == ManagedWebSocketErrorKind::ReadDeadlineExpiredAfterProgress
    }

    pub(crate) fn write_deadline_expired() -> Self {
        Self {
            message: "request deadline expired during a possibly partial WebSocket write"
                .to_string(),
            io_error_kind: Some(io::ErrorKind::TimedOut),
            source: None,
            kind: ManagedWebSocketErrorKind::WriteDeadlineExpired,
        }
    }

    pub(crate) fn is_write_deadline_expired(&self) -> bool {
        self.kind == ManagedWebSocketErrorKind::WriteDeadlineExpired
    }

    pub(crate) fn is_write_failed_after_commit(&self) -> bool {
        self.kind == ManagedWebSocketErrorKind::WriteFailedAfterCommit
    }

    pub(crate) fn from_io(source: io::Error) -> Self {
        let io_error_kind = Some(source.kind());
        Self {
            message: format!("i/o error: {source}"),
            io_error_kind,
            source: Some(Box::new(source)),
            kind: ManagedWebSocketErrorKind::Other,
        }
    }

    pub(crate) fn from_io_after_write_commit(source: io::Error) -> Self {
        let mut error = Self::from_io(source);
        error.kind = ManagedWebSocketErrorKind::WriteFailedAfterCommit;
        error
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
            kind: ManagedWebSocketErrorKind::Other,
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
            kind: ManagedWebSocketErrorKind::Other,
        }
    }

    pub(crate) fn from_mask_generation(source: getrandom::Error) -> Self {
        Self {
            message: format!("failed to generate WebSocket mask: {source}"),
            io_error_kind: None,
            source: Some(Box::new(source)),
            kind: ManagedWebSocketErrorKind::Other,
        }
    }

    pub(crate) fn from_utf8(source: std::string::FromUtf8Error) -> Self {
        Self {
            message: format!("text message was not valid UTF-8: {source}"),
            io_error_kind: None,
            source: Some(Box::new(source)),
            kind: ManagedWebSocketErrorKind::Other,
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
    pub fn launch_and_probe(
        launch_spec: BackendLaunchSpec,
        timeout: Duration,
    ) -> Result<(Self, ManagedBackendProbeReport), ManagedBackendError> {
        Self::launch_and_probe_with_progress(launch_spec, timeout, |_| {})
    }

    pub fn launch_and_probe_with_progress<F>(
        launch_spec: BackendLaunchSpec,
        timeout: Duration,
        mut on_progress: F,
    ) -> Result<(Self, ManagedBackendProbeReport), ManagedBackendError>
    where
        F: FnMut(ManagedBackendStartupProgress),
    {
        on_progress(ManagedBackendStartupProgress::new(
            ManagedBackendStartupStage::LaunchProcess,
            None,
        ));

        let mut session = Self::launch(launch_spec)?;
        let report = session.probe_compatibility(timeout, &mut on_progress)?;

        on_progress(ManagedBackendStartupProgress::new(
            ManagedBackendStartupStage::Ready,
            None,
        ));

        Ok((session, report))
    }

    pub fn launch_spec(&self) -> &BackendLaunchSpec {
        &self.launch_spec
    }

    pub fn process_id(&self) -> Option<u32> {
        self.process
            .as_ref()
            .and_then(SupervisedBackendProcess::process_id)
    }

    pub fn is_process_alive(&mut self) -> bool {
        !self.child_exited()
    }

    pub fn list_threads(
        &mut self,
        timeout: Duration,
    ) -> Result<Vec<ThreadSummary>, ManagedBackendError> {
        self.list_threads_with_options(ThreadListOptions::page(100), timeout)
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

    pub fn list_threads_with_options(
        &mut self,
        mut options: ThreadListOptions,
        timeout: Duration,
    ) -> Result<Vec<ThreadSummary>, ManagedBackendError> {
        let mut threads = Vec::new();

        loop {
            let response = self.list_thread_page(&options, timeout)?;
            threads.extend(response.data);

            if response.next_cursor.is_none() {
                break;
            }

            options.cursor = response.next_cursor;
        }

        Ok(threads)
    }

    /// Collects `thread/list` pages within one aggregate time, page, and row budget.
    ///
    /// Successfully received pages are retained in [`ThreadListCollectionError`]
    /// if a later request or protocol validation fails.
    pub fn list_threads_bounded(
        &mut self,
        mut options: ThreadListOptions,
        budget: ThreadListBudget,
    ) -> Result<ThreadListCollection, ThreadListCollectionError> {
        let deadline = RequestDeadline::from_timeout(budget.aggregate_timeout());
        let mut threads = Vec::new();
        let mut pages_collected = 0;
        let mut seen_cursors = HashSet::new();
        if let Some(cursor) = options.cursor.as_ref() {
            seen_cursors.insert(cursor.clone());
        }

        loop {
            if deadline.remaining().is_none() {
                return Ok(ThreadListCollection {
                    data: threads,
                    next_cursor: options.cursor,
                    pages_collected,
                    status: ThreadListCollectionStatus::Truncated(
                        ThreadListTruncationReason::ElapsedTime,
                    ),
                });
            }

            let remaining_results = budget.max_results() - threads.len();
            let remaining_limit = remaining_results.min(u32::MAX as usize) as u32;
            let requested_limit = options
                .limit
                .unwrap_or(remaining_limit)
                .min(remaining_limit);
            options.limit = Some(requested_limit);
            let request_cursor = options.cursor.clone();

            let response = match self.list_thread_page_until(&options, deadline) {
                Ok(response) => response,
                Err(source) => {
                    return Err(ThreadListCollectionError {
                        data: threads,
                        next_cursor: request_cursor,
                        pages_collected,
                        source,
                    });
                }
            };

            if response.data.len() > requested_limit as usize {
                return Err(ThreadListCollectionError {
                    data: threads,
                    next_cursor: request_cursor,
                    pages_collected,
                    source: ManagedBackendError::ThreadListPageLimitExceeded {
                        requested_limit,
                        returned: response.data.len(),
                    },
                });
            }

            if let Some(next_cursor) = response.next_cursor.as_ref()
                && !seen_cursors.insert(next_cursor.clone())
            {
                return Err(ThreadListCollectionError {
                    data: threads,
                    next_cursor: request_cursor,
                    pages_collected,
                    source: ManagedBackendError::ThreadListCursorRepeated {
                        cursor: next_cursor.clone(),
                    },
                });
            }

            threads.extend(response.data);
            pages_collected += 1;
            options.cursor = response.next_cursor;

            if options.cursor.is_none() {
                return Ok(ThreadListCollection {
                    data: threads,
                    next_cursor: None,
                    pages_collected,
                    status: ThreadListCollectionStatus::Complete,
                });
            }
            if threads.len() == budget.max_results() {
                return Ok(ThreadListCollection {
                    data: threads,
                    next_cursor: options.cursor,
                    pages_collected,
                    status: ThreadListCollectionStatus::Truncated(
                        ThreadListTruncationReason::ResultLimit,
                    ),
                });
            }
            if pages_collected == budget.max_pages() {
                return Ok(ThreadListCollection {
                    data: threads,
                    next_cursor: options.cursor,
                    pages_collected,
                    status: ThreadListCollectionStatus::Truncated(
                        ThreadListTruncationReason::PageLimit,
                    ),
                });
            }
        }
    }

    pub fn list_thread_page(
        &mut self,
        options: &ThreadListOptions,
        timeout: Duration,
    ) -> Result<ThreadListResponse, ManagedBackendError> {
        self.request("thread/list", options, timeout)
    }

    fn list_thread_page_until(
        &mut self,
        options: &ThreadListOptions,
        deadline: RequestDeadline,
    ) -> Result<ThreadListResponse, ManagedBackendError> {
        self.request_until("thread/list", options, deadline)
    }

    pub fn start_thread(
        &mut self,
        cwd: &Path,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, ManagedBackendError> {
        self.start_thread_with_options(cwd, ThreadStartOptions::persistent(), timeout)
    }

    pub fn start_thread_with_options(
        &mut self,
        cwd: &Path,
        options: ThreadStartOptions,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, ManagedBackendError> {
        self.request(
            "thread/start",
            &ThreadStartParams::for_workspace(cwd, options),
            timeout,
        )
    }

    pub fn resume_thread(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, ManagedBackendError> {
        self.resume_thread_with_options(thread_id, ThreadResumeOptions::default(), timeout)
    }

    pub fn resume_thread_metadata(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, ManagedBackendError> {
        self.resume_thread_with_options(thread_id, ThreadResumeOptions::metadata_only(), timeout)
    }

    pub fn resume_thread_with_options(
        &mut self,
        thread_id: &str,
        options: ThreadResumeOptions,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, ManagedBackendError> {
        self.request(
            "thread/resume",
            &ThreadResumeParams::new(thread_id, options),
            timeout,
        )
    }

    pub fn read_thread(
        &mut self,
        thread_id: &str,
        options: ThreadReadOptions,
        timeout: Duration,
    ) -> Result<ThreadReadResponse, ManagedBackendError> {
        self.request(
            "thread/read",
            &ThreadReadParams::new(thread_id, options),
            timeout,
        )
    }

    pub fn read_thread_metadata(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSummary, ManagedBackendError> {
        self.read_thread(thread_id, ThreadReadOptions::metadata_only(), timeout)
            .map(|response| response.thread.summary())
    }

    pub fn read_thread_metadata_details(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadReadMetadata, ManagedBackendError> {
        self.read_thread(thread_id, ThreadReadOptions::metadata_only(), timeout)
            .map(|response| response.read_metadata())
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

    pub fn list_thread_turns(
        &mut self,
        thread_id: &str,
        options: &ThreadTurnsListOptions,
        timeout: Duration,
    ) -> Result<ThreadTurnsListResponse, ManagedBackendError> {
        self.request(
            "thread/turns/list",
            &ThreadTurnsListParams::new(thread_id, options),
            timeout,
        )
    }

    pub fn fork_thread(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadForkResponse, ManagedBackendError> {
        self.fork_thread_with_options(thread_id, ThreadForkOptions::default(), timeout)
    }

    pub fn fork_thread_with_options(
        &mut self,
        thread_id: &str,
        options: ThreadForkOptions,
        timeout: Duration,
    ) -> Result<ThreadForkResponse, ManagedBackendError> {
        self.request(
            "thread/fork",
            &ThreadForkParams::new(thread_id, options),
            timeout,
        )
    }

    /// Forks a thread while classifying whether a failed request could have
    /// created an unidentified backend child.
    pub fn fork_thread_with_commitment(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadForkResponse, ThreadForkFailure> {
        self.fork_thread_with_options_and_commitment(
            thread_id,
            ThreadForkOptions::default(),
            timeout,
        )
    }

    /// The commitment-aware counterpart to [`Self::fork_thread_with_options`].
    ///
    /// A successful response contains the exact child id. On failure, callers
    /// must treat [`ThreadForkFailure::Indeterminate`] as a possible committed
    /// child and must not infer that id from thread inventory results.
    pub fn fork_thread_with_options_and_commitment(
        &mut self,
        thread_id: &str,
        options: ThreadForkOptions,
        timeout: Duration,
    ) -> Result<ThreadForkResponse, ThreadForkFailure> {
        self.fork_thread_with_options(thread_id, options, timeout)
            .map_err(|source| ThreadForkFailure::classify(source, self.last_request_dispatched))
    }

    pub fn rollback_thread(
        &mut self,
        thread_id: &str,
        num_turns: u32,
        timeout: Duration,
    ) -> Result<ThreadRollbackResponse, ManagedBackendError> {
        self.request(
            "thread/rollback",
            &ThreadRollbackParams::new(thread_id, num_turns),
            timeout,
        )
    }

    pub fn start_turn(
        &mut self,
        thread_id: &str,
        text: &str,
        timeout: Duration,
    ) -> Result<TurnStartResponse, ManagedBackendError> {
        self.start_turn_with_options(thread_id, text, TurnStartOptions::default(), timeout)
    }

    pub fn start_turn_with_options(
        &mut self,
        thread_id: &str,
        text: &str,
        options: TurnStartOptions,
        timeout: Duration,
    ) -> Result<TurnStartResponse, ManagedBackendError> {
        self.request(
            "turn/start",
            &TurnStartParams::text(thread_id, text, options),
            timeout,
        )
    }

    pub fn start_turn_with_user_input(
        &mut self,
        thread_id: &str,
        input: Vec<UserInput>,
        timeout: Duration,
    ) -> Result<TurnStartResponse, ManagedBackendError> {
        self.start_turn_with_user_input_options(
            thread_id,
            input,
            TurnStartOptions::default(),
            timeout,
        )
    }

    pub fn start_turn_with_user_input_options(
        &mut self,
        thread_id: &str,
        input: Vec<UserInput>,
        options: TurnStartOptions,
        timeout: Duration,
    ) -> Result<TurnStartResponse, ManagedBackendError> {
        self.request(
            "turn/start",
            &TurnStartParams::input(thread_id, input, options),
            timeout,
        )
    }

    pub fn steer_turn_with_user_input(
        &mut self,
        thread_id: &str,
        expected_turn_id: &str,
        input: Vec<UserInput>,
        timeout: Duration,
    ) -> Result<TurnSteerResponse, ManagedBackendError> {
        self.request(
            "turn/steer",
            &TurnSteerParams::input(thread_id, expected_turn_id, input),
            timeout,
        )
    }

    pub fn set_thread_name(
        &mut self,
        thread_id: &str,
        name: &str,
        timeout: Duration,
    ) -> Result<(), ManagedBackendError> {
        let _: EmptyResponse = self.request(
            "thread/name/set",
            &ThreadSetNameParams::new(thread_id, name),
            timeout,
        )?;
        Ok(())
    }

    pub fn compact_thread(
        &mut self,
        thread_id: &str,
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
        thread_id: &str,
        turn_id: &str,
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
        thread_id: &str,
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
        timeout: Duration,
    ) -> Result<ThreadBranchCapabilityReport, ManagedBackendError> {
        let mut results = Vec::with_capacity(THREAD_BRANCH_CAPABILITY_PROBES.len());
        for probe in THREAD_BRANCH_CAPABILITY_PROBES {
            results.push(self.probe_thread_branch_capability(*probe, timeout)?);
        }

        Ok(ThreadBranchCapabilityReport::new(results))
    }

    pub fn deny_approval_request(
        &mut self,
        request: &ApprovalRequest,
    ) -> Result<(), ManagedBackendError> {
        match self.deny_approval_request_with_deadline(request, None) {
            Ok(()) => Ok(()),
            Err(DeadlineTransportError::Backend(error)) => Err(error),
            Err(DeadlineTransportError::StdioWriteFailed(error)) => {
                self.poison_after_stdio_write_timeout();
                Err(error)
            }
            Err(DeadlineTransportError::WebSocketWriteFailed(error)) => {
                self.poison_after_websocket_write_timeout();
                Err(error)
            }
            Err(
                DeadlineTransportError::DeadlineExpired
                | DeadlineTransportError::StdioWriteExpired
                | DeadlineTransportError::WebSocketWriteExpired
                | DeadlineTransportError::WebSocketReadExpiredAfterProgress(_),
            ) => {
                unreachable!("unbounded response expired")
            }
        }
    }

    fn deny_approval_request_until(
        &mut self,
        request: &ApprovalRequest,
        deadline: Instant,
    ) -> Result<(), DeadlineTransportError> {
        self.deny_approval_request_with_deadline(request, Some(deadline))
    }

    fn deny_approval_request_with_deadline(
        &mut self,
        request: &ApprovalRequest,
        deadline: Option<Instant>,
    ) -> Result<(), DeadlineTransportError> {
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
        self.write_server_response_with_deadline(
            request.method(),
            request.request_id(),
            &result,
            deadline,
        )
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
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadUnsubscribeResponse, ManagedBackendError> {
        self.request(
            "thread/unsubscribe",
            &ThreadUnsubscribeParams::new(thread_id),
            timeout,
        )
    }

    /// Moves one persistent thread into app-server archived storage, attempting
    /// to archive its spawned descendants as well.
    pub fn archive_thread(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<(), ManagedBackendError> {
        let _: ThreadLifecycleEmptyResponse = self.request(
            "thread/archive",
            &ThreadArchiveParams::new(thread_id),
            timeout,
        )?;
        Ok(())
    }

    /// Restores one archived persistent thread to active storage and returns its
    /// [`crate::ThreadInfo`] value without resuming it or starting a turn.
    pub fn unarchive_thread(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<crate::ThreadInfo, ManagedBackendError> {
        self.request::<ThreadUnarchiveResponse>(
            "thread/unarchive",
            &ThreadUnarchiveParams::new(thread_id),
            timeout,
        )
        .map(|response| response.thread)
    }

    /// Physically removes one persistent active or archived thread and its
    /// spawned descendants.
    pub fn delete_thread(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<(), ManagedBackendError> {
        let _: ThreadLifecycleEmptyResponse = self.request(
            "thread/delete",
            &ThreadDeleteParams::new(thread_id),
            timeout,
        )?;
        Ok(())
    }

    pub fn next_turn_stream_event(
        &mut self,
        idle_timeout: Duration,
    ) -> Result<Option<TurnStreamEvent>, ManagedBackendError> {
        self.ensure_usable("turn stream")?;
        let deadline = Instant::now() + idle_timeout;

        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Ok(None);
            };
            if remaining.is_zero() {
                return Ok(None);
            }

            let message = if let Some(message) = self.pop_pending_message() {
                message
            } else {
                match self.recv_message_until("turn stream", deadline, None)? {
                    Some(message) => message,
                    None => return Ok(None),
                }
            };

            match message {
                IncomingMessage::Notification { method, params } => {
                    match parse_turn_stream_event(&method, params) {
                        Ok(Some(event)) => return Ok(Some(event)),
                        Ok(None) => {
                            warn!(
                                notification_method = method,
                                "ignoring unsupported backend notification during turn stream"
                            );
                        }
                        Err(source) => {
                            return Err(ManagedBackendError::DeserializeNotification {
                                method,
                                source,
                            });
                        }
                    }
                }
                IncomingMessage::ServerRequest { id, method, params } => {
                    if let Some(request) =
                        parse_approval_request(id.clone(), &method, params.clone())
                    {
                        return Ok(Some(TurnStreamEvent::ApprovalRequested(request)));
                    }
                    match parse_dynamic_tool_call_request(id, &method, params) {
                        Ok(Some(request)) => {
                            return Ok(Some(TurnStreamEvent::DynamicToolCallRequested(request)));
                        }
                        Ok(None) => {}
                        Err(source) => {
                            return Err(ManagedBackendError::DeserializeServerRequest {
                                method,
                                source,
                            });
                        }
                    }
                    warn!(
                        request_method = method,
                        "ignoring unsupported backend server request during turn stream"
                    );
                }
                IncomingMessage::Error { error, .. } => {
                    return Ok(Some(TurnStreamEvent::ProtocolError { error }));
                }
                IncomingMessage::Response { id, .. } => {
                    warn!(
                        response_id = id,
                        "ignoring unexpected backend response during turn stream"
                    );
                }
            }

            if self.child_exited() {
                return Err(ManagedBackendError::ProcessExited {
                    method: "turn stream".to_string(),
                });
            }
        }
    }

    pub fn shutdown(&mut self) -> Result<(), ManagedBackendError> {
        if let Some(cleanup) = self.terminal_cleanup.as_mut() {
            return cleanup.finish();
        }

        self.transport.close();
        let mut failures = Vec::new();
        if let Some(process) = self.process.as_mut() {
            if let Err(error) = process.shutdown(
                STDIO_PROCESS_CLOSE_GRACE_TIMEOUT,
                MANAGED_PROCESS_KILL_TIMEOUT,
            ) {
                failures.push(error);
            }
        }
        if let Err(error) = self.transport.join_stdio_writer() {
            failures.push(error);
        }
        finish_stdio_cleanup(failures)
    }

    fn launch(launch_spec: BackendLaunchSpec) -> Result<Self, ManagedBackendError> {
        let command_line = launch_spec.command_line()?;
        let mut command = Command::new(command_line.program());
        command.args(command_line.args());
        if let Some(cwd) = command_line.cwd() {
            command.current_dir(cwd);
        }
        Self::launch_command(launch_spec, command)
    }

    fn launch_command(
        launch_spec: BackendLaunchSpec,
        mut command: Command,
    ) -> Result<Self, ManagedBackendError> {
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        let program = command.get_program().to_string_lossy().into_owned();

        let child = command
            .spawn()
            .map_err(|source| ManagedBackendError::Spawn { program, source })?;
        let mut process = SupervisedBackendProcess::new(launch_spec.clone(), child)?;

        let stdin = process
            .take_stdin()
            .ok_or(ManagedBackendError::MissingPipe {
                stream_name: "stdin",
            })?;
        let stdout = process
            .take_stdout()
            .ok_or(ManagedBackendError::MissingPipe {
                stream_name: "stdout",
            })?;
        let stderr = process
            .take_stderr()
            .ok_or(ManagedBackendError::MissingPipe {
                stream_name: "stderr",
            })?;

        let messages = spawn_stdout_reader(stdout);
        spawn_stderr_logger(stderr, launch_spec.clone());

        Ok(Self {
            launch_spec,
            process: Some(process),
            transport: BackendClientTransport::Stdio {
                writer: Some(SupervisedStdioWriter::spawn(stdin)),
                messages,
            },
            pending_messages: VecDeque::new(),
            pending_message_bytes: 0,
            pending_dynamic_tool_requests: 0,
            next_request_id: 1,
            last_request_dispatched: false,
            poisoned: false,
            terminal_cleanup: None,
            #[cfg(feature = "lifecycle-test-support")]
            transport_close_classification_pause: None,
        })
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn launch_test_command(
        launch_spec: BackendLaunchSpec,
        command: Command,
    ) -> Result<Self, ManagedBackendError> {
        Self::launch_command(launch_spec, command)
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn stdio_cleanup_finished_for_test(&self) -> bool {
        self.terminal_cleanup
            .as_ref()
            .is_some_and(StdioTerminalCleanup::is_finished)
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn stdio_cleanup_retained_for_test(&self) -> bool {
        self.terminal_cleanup
            .as_ref()
            .is_some_and(StdioTerminalCleanup::is_retained)
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn stdio_writer_finished_for_test(&self) -> bool {
        self.terminal_cleanup
            .as_ref()
            .is_some_and(StdioTerminalCleanup::writer_finished)
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn next_request_id_for_test(&self) -> u64 {
        self.next_request_id
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn websocket_close_frame_attempts_for_test(&self) -> Option<usize> {
        self.transport.websocket_close_frame_attempts()
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn fail_next_websocket_write_after_header_for_test(
        &mut self,
        kind: io::ErrorKind,
    ) -> bool {
        match &mut self.transport {
            BackendClientTransport::WebSocket(transport) => {
                transport.fail_next_write_after_header(kind);
                true
            }
            BackendClientTransport::Stdio { .. } => false,
        }
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn stdio_write_count_for_test(&self) -> usize {
        self.terminal_cleanup
            .as_ref()
            .map_or(0, StdioTerminalCleanup::write_count)
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn inject_process_termination_failures_for_test(&mut self, count: usize) -> bool {
        let Some(process) = self.process.as_mut() else {
            return false;
        };
        process.inject_termination_failures(count);
        true
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn fail_next_stdio_write_after_bytes_for_test(&mut self, bytes: usize) -> bool {
        self.transport.fail_next_stdio_write_after_bytes(bytes)
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn pause_websocket_after_next_write_header_for_test(
        &mut self,
        entered: SyncSender<()>,
        release: Receiver<()>,
    ) -> bool {
        match &mut self.transport {
            BackendClientTransport::WebSocket(transport) => {
                transport.pause_after_next_write_header(entered, release);
                true
            }
            BackendClientTransport::Stdio { .. } => false,
        }
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn pause_websocket_after_next_read_frame_byte_for_test(
        &mut self,
        entered: SyncSender<()>,
        release: Receiver<()>,
    ) -> bool {
        match &mut self.transport {
            BackendClientTransport::WebSocket(transport) => {
                transport.pause_after_next_read_frame_byte(entered, release);
                true
            }
            BackendClientTransport::Stdio { .. } => false,
        }
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn pause_websocket_after_next_read_payload_for_test(
        &mut self,
        entered: SyncSender<()>,
        release: Receiver<()>,
    ) -> bool {
        match &mut self.transport {
            BackendClientTransport::WebSocket(transport) => {
                transport.pause_after_next_read_payload(entered, release);
                true
            }
            BackendClientTransport::Stdio { .. } => false,
        }
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn pause_websocket_before_next_write_for_test(
        &mut self,
        entered: SyncSender<()>,
        release: Receiver<()>,
    ) -> bool {
        match &mut self.transport {
            BackendClientTransport::WebSocket(transport) => {
                transport.pause_before_next_write(entered, release);
                true
            }
            BackendClientTransport::Stdio { .. } => false,
        }
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn pause_websocket_before_write_after_for_test(
        &mut self,
        skipped_writes: usize,
        entered: SyncSender<()>,
        release: Receiver<()>,
    ) -> bool {
        match &mut self.transport {
            BackendClientTransport::WebSocket(transport) => {
                transport.pause_before_write_after(skipped_writes, entered, release);
                true
            }
            BackendClientTransport::Stdio { .. } => false,
        }
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn pause_websocket_after_next_control_write_header_for_test(
        &mut self,
        entered: SyncSender<()>,
        release: Receiver<()>,
    ) -> bool {
        match &mut self.transport {
            BackendClientTransport::WebSocket(transport) => {
                transport.pause_after_next_control_write_header(entered, release);
                true
            }
            BackendClientTransport::Stdio { .. } => false,
        }
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn pause_before_next_transport_close_classification_for_test(
        &mut self,
        entered: SyncSender<()>,
        release: Receiver<()>,
    ) -> bool {
        if !self.transport.is_websocket() {
            return false;
        }
        self.transport_close_classification_pause =
            Some(TransportCloseClassificationPause { entered, release });
        true
    }

    pub(crate) fn connect_websocket_uninitialized(
        launch_spec: BackendLaunchSpec,
        endpoint: BackendWebSocketEndpoint,
        authorization_header_value: String,
    ) -> Result<Self, ManagedBackendError> {
        let transport = WebSocketClientTransport::connect(&endpoint, authorization_header_value)?;

        Ok(Self {
            launch_spec,
            process: None,
            transport: BackendClientTransport::WebSocket(transport),
            pending_messages: VecDeque::new(),
            pending_message_bytes: 0,
            pending_dynamic_tool_requests: 0,
            next_request_id: 1,
            last_request_dispatched: false,
            poisoned: false,
            terminal_cleanup: None,
            #[cfg(feature = "lifecycle-test-support")]
            transport_close_classification_pause: None,
        })
    }

    pub fn connect_websocket(
        launch_spec: BackendLaunchSpec,
        endpoint: BackendWebSocketEndpoint,
        authorization_header_value: String,
        timeout: Duration,
    ) -> Result<Self, ManagedBackendError> {
        Self::connect_websocket_with_options(
            launch_spec,
            endpoint,
            authorization_header_value,
            ManagedBackendClientOptions::foreground(),
            timeout,
        )
    }

    pub fn connect_websocket_with_options(
        launch_spec: BackendLaunchSpec,
        endpoint: BackendWebSocketEndpoint,
        authorization_header_value: String,
        options: ManagedBackendClientOptions,
        timeout: Duration,
    ) -> Result<Self, ManagedBackendError> {
        let mut session = Self::connect_websocket_uninitialized(
            launch_spec,
            endpoint,
            authorization_header_value,
        )?;
        session.initialize_client_with_options(&options, timeout)?;
        Ok(session)
    }

    pub(crate) fn probe_compatibility<F>(
        &mut self,
        timeout: Duration,
        on_progress: &mut F,
    ) -> Result<ManagedBackendProbeReport, ManagedBackendError>
    where
        F: FnMut(ManagedBackendStartupProgress),
    {
        let (initialize, compatibility) =
            self.initialize_client_with_progress(timeout, on_progress)?;

        let mut method_successes = Vec::with_capacity(compatibility.required_method_probes().len());
        let mut config_defaults = BackendConfigDefaults::default();
        let mut model_list = Vec::new();
        for probe in compatibility.required_method_probes() {
            on_progress(ManagedBackendStartupProgress::new(
                ManagedBackendStartupStage::VerifyRequiredMethods,
                Some(probe.method().to_string()),
            ));
            if let Some(data) = self.probe_required_method(*probe, timeout)? {
                match data {
                    ProbeMethodData::ConfigDefaults(defaults) => config_defaults = defaults,
                    ProbeMethodData::ModelList(models) => model_list = models,
                }
            }
            method_successes.push(ProbeMethodSuccess { probe: *probe });
        }

        let thread_branch_capabilities = self
            .probe_thread_branch_capabilities(timeout)?
            .capabilities();

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
        self.initialize_client_with_progress_and_options(timeout, options, &mut |_| {})
    }

    fn initialize_client_with_progress<F>(
        &mut self,
        timeout: Duration,
        on_progress: &mut F,
    ) -> Result<(InitializeResponse, CompatibilitySnapshot), ManagedBackendError>
    where
        F: FnMut(ManagedBackendStartupProgress),
    {
        self.initialize_client_with_progress_and_options(
            timeout,
            &ManagedBackendClientOptions::foreground(),
            on_progress,
        )
    }

    fn initialize_client_with_progress_and_options<F>(
        &mut self,
        timeout: Duration,
        options: &ManagedBackendClientOptions,
        on_progress: &mut F,
    ) -> Result<(InitializeResponse, CompatibilitySnapshot), ManagedBackendError>
    where
        F: FnMut(ManagedBackendStartupProgress),
    {
        on_progress(ManagedBackendStartupProgress::new(
            ManagedBackendStartupStage::InitializeHandshake,
            None,
        ));

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

        on_progress(ManagedBackendStartupProgress::new(
            ManagedBackendStartupStage::ValidateRuntime,
            None,
        ));

        let compatibility = CompatibilitySnapshot::from_initialize_response(&initialize);
        compatibility.validate_runtime_mode(self.launch_spec.runtime_mode())?;

        self.notify_initialized()?;

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
        timeout: Duration,
    ) -> Result<Option<ProbeMethodData>, ManagedBackendError> {
        match probe {
            CompatibilityProbe::ConfigRead => {
                let cwd = self.launch_spec.cwd().to_path_buf();
                return self
                    .read_config(&cwd, timeout)
                    .map(|response| Some(ProbeMethodData::ConfigDefaults(response.config)));
            }
            CompatibilityProbe::ModelList => {
                return self
                    .list_models_with_options(ModelListOptions::page(100), timeout)
                    .map(|models| Some(ProbeMethodData::ModelList(models)));
            }
            CompatibilityProbe::ThreadList => {
                let _: ThreadListResponse = self.request(
                    probe.method(),
                    &ThreadListProbeParams {
                        cursor: None,
                        limit: Some(1),
                        cwd: Vec::new(),
                        sort_key: Some(ThreadSortKey::UpdatedAt),
                        sort_direction: Some(SortDirection::Desc),
                    },
                    timeout,
                )?;
            }
            CompatibilityProbe::ThreadCompactStart => {
                self.probe_request_accepts_method(
                    probe.method(),
                    &ThreadCompactStartParams::new(PROBE_THREAD_ID),
                    timeout,
                )?;
            }
            CompatibilityProbe::ThreadLoadedList => {
                let _: ThreadLoadedListResponse = self.request(
                    probe.method(),
                    &ThreadLoadedListProbeParams { limit: Some(1) },
                    timeout,
                )?;
            }
            CompatibilityProbe::ThreadNameSet => {
                self.probe_request_accepts_method(
                    probe.method(),
                    &ThreadSetNameParams::new(PROBE_THREAD_ID, "Beryl compatibility probe"),
                    timeout,
                )?;
            }
            CompatibilityProbe::ThreadRead => {
                self.probe_request_accepts_method(
                    probe.method(),
                    &ThreadReadParams::new(PROBE_THREAD_ID, ThreadReadOptions::metadata_only()),
                    timeout,
                )?;
            }
            CompatibilityProbe::ThreadResumeMetadata => {
                self.probe_request_accepts_method(
                    probe.method(),
                    &ThreadResumeParams::new(PROBE_THREAD_ID, ThreadResumeOptions::metadata_only()),
                    timeout,
                )?;
            }
            CompatibilityProbe::ThreadUnsubscribe => {
                let _: ThreadUnsubscribeResponse = self.request(
                    probe.method(),
                    &ThreadUnsubscribeParams::new(PROBE_THREAD_ID),
                    timeout,
                )?;
            }
            CompatibilityProbe::ThreadTurnsList => {
                let options =
                    ThreadTurnsListOptions::page(1).with_sort_direction(SortDirection::Desc);
                self.probe_request_accepts_method(
                    probe.method(),
                    &ThreadTurnsListParams::new(PROBE_THREAD_ID, &options),
                    timeout,
                )?;
            }
            CompatibilityProbe::TurnSteer => {
                self.probe_request_accepts_method(
                    probe.method(),
                    &TurnSteerParams::input(
                        PROBE_THREAD_ID,
                        PROBE_TURN_ID,
                        vec![UserInput::text("Beryl compatibility probe")],
                    ),
                    timeout,
                )?;
            }
            CompatibilityProbe::TurnInterrupt => {
                self.probe_request_accepts_method(
                    probe.method(),
                    &TurnInterruptParams::new(PROBE_THREAD_ID, PROBE_TURN_ID),
                    timeout,
                )?;
            }
        }

        Ok(None)
    }

    fn probe_thread_branch_capability(
        &mut self,
        probe: ThreadBranchCapabilityProbe,
        timeout: Duration,
    ) -> Result<ThreadBranchCapabilityProbeResult, ManagedBackendError> {
        let params = match probe {
            ThreadBranchCapabilityProbe::ThreadFork => serde_json::to_value(ThreadForkParams::new(
                PROBE_THREAD_ID,
                ThreadForkOptions::default(),
            )),
            ThreadBranchCapabilityProbe::ThreadRollback => {
                serde_json::to_value(ThreadRollbackParams::new(PROBE_THREAD_ID, 1))
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
        let params = match probe {
            HardStopCapabilityProbe::CommandExecTerminate => serde_json::to_value(
                CommandExecTerminateParams::new(PROBE_COMMAND_EXEC_PROCESS_ID),
            ),
            HardStopCapabilityProbe::ThreadBackgroundTerminalsClean => {
                serde_json::to_value(ThreadBackgroundTerminalsCleanParams::new(PROBE_THREAD_ID))
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
                    error,
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
        self.request_until(method, params, RequestDeadline::from_timeout(timeout))
    }

    fn request_until<R: DeserializeOwned>(
        &mut self,
        method: &str,
        params: &impl Serialize,
        deadline: RequestDeadline,
    ) -> Result<R, ManagedBackendError> {
        let request_started = Instant::now();
        match self.request_json_until(method, params, deadline)? {
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
                if backend_metrics_debug_enabled() && response_sanitizer_kind(method).is_some() {
                    debug!(
                        target: "beryl_backend::backend_metrics",
                        method,
                        typed_deserialize_ms = elapsed_ms(typed_deserialize),
                        typed_request_total_ms = elapsed_ms(typed_request_total),
                        "deserialized backend JSON-RPC response metrics"
                    );
                }
                Ok(response)
            }
            JsonRpcRequestOutcome::Error(error) => Err(ManagedBackendError::RequestFailed {
                method: method.to_string(),
                error,
            }),
        }
    }

    fn request_json(
        &mut self,
        method: &str,
        params: &impl Serialize,
        timeout: Duration,
    ) -> Result<JsonRpcRequestOutcome, ManagedBackendError> {
        self.request_json_until(method, params, RequestDeadline::from_timeout(timeout))
    }

    fn request_json_until(
        &mut self,
        method: &str,
        params: &impl Serialize,
        deadline: RequestDeadline,
    ) -> Result<JsonRpcRequestOutcome, ManagedBackendError> {
        self.last_request_dispatched = false;
        self.ensure_usable(method)?;
        let request_id = self.next_request_id;
        let request_started = Instant::now();

        let write_metrics = match self.write_message_until(
            method,
            &JsonRpcRequest {
                jsonrpc: "2.0",
                id: request_id,
                method,
                params,
            },
            deadline.expires_at,
        ) {
            Ok(metrics) => {
                self.next_request_id += 1;
                self.last_request_dispatched = true;
                metrics
            }
            Err(DeadlineTransportError::StdioWriteExpired) => {
                self.next_request_id += 1;
                self.last_request_dispatched = true;
                self.poison_after_stdio_write_timeout();
                return Err(deadline.timeout_error(method));
            }
            Err(DeadlineTransportError::StdioWriteFailed(error)) => {
                self.next_request_id += 1;
                self.last_request_dispatched = true;
                self.poison_after_stdio_write_timeout();
                return Err(error);
            }
            Err(DeadlineTransportError::WebSocketWriteExpired) => {
                self.next_request_id += 1;
                self.last_request_dispatched = true;
                self.poison_after_websocket_write_timeout();
                return Err(deadline.timeout_error(method));
            }
            Err(DeadlineTransportError::WebSocketWriteFailed(error)) => {
                self.next_request_id += 1;
                self.last_request_dispatched = true;
                self.poison_after_websocket_write_timeout();
                return Err(error);
            }
            Err(DeadlineTransportError::DeadlineExpired) => {
                return Err(deadline.timeout_error(method));
            }
            Err(DeadlineTransportError::Backend(error)) => return Err(error),
            Err(DeadlineTransportError::WebSocketReadExpiredAfterProgress(_)) => {
                unreachable!("write path cannot report a read deadline")
            }
        };

        let response_wait_started = Instant::now();
        let mut interleaved_notification_count = 0_usize;
        let mut interleaved_server_request_count = 0_usize;
        let mut denied_approval_request_count = 0_usize;
        let mut deferred_dynamic_tool_request_count = 0_usize;
        let mut out_of_order_response_count = 0_usize;
        loop {
            if deadline.remaining().is_none() {
                return Err(deadline.timeout_error(method));
            }

            let message = match self.recv_message_until(
                method,
                deadline.expires_at,
                Some(ExpectedJsonRpcResponse { method, request_id }),
            )? {
                Some(message) => message,
                None => return Err(deadline.timeout_error(method)),
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
                    if backend_metrics_debug_enabled() && response_sanitizer_kind(method).is_some()
                    {
                        debug!(
                            target: "beryl_backend::backend_metrics",
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
                            "backend JSON-RPC request completed metrics"
                        );
                    }
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
                    if backend_metrics_debug_enabled() && response_sanitizer_kind(method).is_some()
                    {
                        debug!(
                            target: "beryl_backend::backend_metrics",
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
                            "backend JSON-RPC request completed metrics"
                        );
                    }
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
                } => {
                    interleaved_server_request_count += 1;
                    if let Some(request) =
                        parse_approval_request(id.clone(), &request_method, params.clone())
                    {
                        denied_approval_request_count += 1;
                        warn!(
                            approval = %request.summary(),
                            approval_payload = %request.pretty_params(),
                            "denying backend approval request received while waiting for another response"
                        );
                        match self.deny_approval_request_until(&request, deadline.expires_at) {
                            Ok(()) => {}
                            Err(DeadlineTransportError::StdioWriteExpired) => {
                                self.poison_after_stdio_write_timeout();
                                return Err(deadline.timeout_error(method));
                            }
                            Err(DeadlineTransportError::StdioWriteFailed(error)) => {
                                self.poison_after_stdio_write_timeout();
                                return Err(error);
                            }
                            Err(DeadlineTransportError::WebSocketWriteExpired) => {
                                self.poison_after_websocket_write_timeout();
                                return Err(deadline.timeout_error(method));
                            }
                            Err(DeadlineTransportError::WebSocketWriteFailed(error)) => {
                                self.poison_after_websocket_write_timeout();
                                return Err(error);
                            }
                            Err(DeadlineTransportError::DeadlineExpired) => {
                                return Err(deadline.timeout_error(method));
                            }
                            Err(DeadlineTransportError::Backend(error)) => return Err(error),
                            Err(DeadlineTransportError::WebSocketReadExpiredAfterProgress(_)) => {
                                unreachable!("write path cannot report a read deadline")
                            }
                        }
                    } else if is_dynamic_tool_call_method(&request_method) {
                        deferred_dynamic_tool_request_count += 1;
                        self.push_pending_message(
                            method,
                            IncomingMessage::ServerRequest {
                                id,
                                method: request_method.clone(),
                                params,
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

            if self.child_exited() {
                return Err(ManagedBackendError::ProcessExited {
                    method: method.to_string(),
                });
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
        match self.write_message_with_deadline(method, message, None) {
            Ok(metrics) => Ok(metrics),
            Err(DeadlineTransportError::Backend(error)) => Err(error),
            Err(DeadlineTransportError::StdioWriteFailed(error)) => {
                self.poison_after_stdio_write_timeout();
                Err(error)
            }
            Err(DeadlineTransportError::WebSocketWriteFailed(error)) => {
                self.poison_after_websocket_write_timeout();
                Err(error)
            }
            Err(
                DeadlineTransportError::DeadlineExpired
                | DeadlineTransportError::StdioWriteExpired
                | DeadlineTransportError::WebSocketWriteExpired
                | DeadlineTransportError::WebSocketReadExpiredAfterProgress(_),
            ) => {
                unreachable!("unbounded write expired")
            }
        }
    }

    fn write_message_until(
        &mut self,
        method: &str,
        message: &impl Serialize,
        deadline: Instant,
    ) -> Result<MessageWriteMetrics, DeadlineTransportError> {
        self.write_message_with_deadline(method, message, Some(deadline))
    }

    fn write_message_with_deadline(
        &mut self,
        method: &str,
        message: &impl Serialize,
        deadline: Option<Instant>,
    ) -> Result<MessageWriteMetrics, DeadlineTransportError> {
        let serialize_started = Instant::now();
        let line = serde_json::to_string(message).map_err(|source| {
            DeadlineTransportError::Backend(ManagedBackendError::SerializeRequest {
                method: method.to_string(),
                source,
            })
        })?;
        let serialize = serialize_started.elapsed();
        let bytes = line.len();
        let transport_started = Instant::now();
        self.transport.write_message(method, &line, deadline)?;
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
        match self.write_server_response_with_deadline(method, request_id, result, None) {
            Ok(()) => Ok(()),
            Err(DeadlineTransportError::Backend(error)) => Err(error),
            Err(DeadlineTransportError::StdioWriteFailed(error)) => {
                self.poison_after_stdio_write_timeout();
                Err(error)
            }
            Err(DeadlineTransportError::WebSocketWriteFailed(error)) => {
                self.poison_after_websocket_write_timeout();
                Err(error)
            }
            Err(
                DeadlineTransportError::DeadlineExpired
                | DeadlineTransportError::StdioWriteExpired
                | DeadlineTransportError::WebSocketWriteExpired
                | DeadlineTransportError::WebSocketReadExpiredAfterProgress(_),
            ) => {
                unreachable!("unbounded response expired")
            }
        }
    }

    fn write_server_response_with_deadline<T: Serialize + ?Sized>(
        &mut self,
        method: &str,
        request_id: &Value,
        result: &T,
        deadline: Option<Instant>,
    ) -> Result<(), DeadlineTransportError> {
        self.write_message_with_deadline(
            method,
            &JsonRpcServerResponse {
                jsonrpc: "2.0",
                id: request_id,
                result,
            },
            deadline,
        )
        .map(|_| ())
    }

    fn recv_message_until(
        &mut self,
        method: &str,
        deadline: Instant,
        expected_response: Option<ExpectedJsonRpcResponse<'_>>,
    ) -> Result<Option<IncomingMessage>, ManagedBackendError> {
        let received = self
            .transport
            .recv_message_until(method, deadline, expected_response);
        #[cfg(feature = "lifecycle-test-support")]
        self.pause_before_transport_close_classification_for_test(&received);
        let received = match received {
            Ok(received) => received,
            Err(DeadlineTransportError::DeadlineExpired) => return Ok(None),
            Err(
                DeadlineTransportError::StdioWriteExpired
                | DeadlineTransportError::StdioWriteFailed(_),
            ) => {
                unreachable!("receive path cannot report a write deadline")
            }
            Err(DeadlineTransportError::WebSocketReadExpiredAfterProgress(error)) => {
                self.poison_after_websocket_write_timeout();
                if expected_response.is_some() {
                    return Ok(None);
                }
                return Err(error);
            }
            Err(DeadlineTransportError::WebSocketWriteExpired) => {
                self.poison_after_websocket_write_timeout();
                return Ok(None);
            }
            Err(DeadlineTransportError::WebSocketWriteFailed(error)) => {
                self.poison_after_websocket_write_timeout();
                return Err(error);
            }
            Err(DeadlineTransportError::Backend(ManagedBackendError::TransportClosed {
                ..
            })) if self.child_exited() => {
                return Err(ManagedBackendError::ProcessExited {
                    method: method.to_string(),
                });
            }
            Err(DeadlineTransportError::Backend(error)) => return Err(error),
        };

        match received {
            Some(message) => Ok(Some(message)),
            None => {
                if self.child_exited() {
                    return Err(ManagedBackendError::ProcessExited {
                        method: method.to_string(),
                    });
                }
                Ok(None)
            }
        }
    }

    fn child_exited(&mut self) -> bool {
        self.process
            .as_mut()
            .is_some_and(SupervisedBackendProcess::has_exited)
    }

    fn ensure_usable(&self, method: &str) -> Result<(), ManagedBackendError> {
        if self.poisoned {
            Err(ManagedBackendError::SessionPoisoned {
                method: method.to_string(),
            })
        } else {
            Ok(())
        }
    }

    fn poison_after_stdio_write_timeout(&mut self) {
        let Some(writer) = self.transport.take_stdio_writer() else {
            return;
        };
        self.poisoned = true;
        let mut failures = Vec::new();
        let Some(mut process) = self.process.take() else {
            failures.push(ManagedBackendError::ProcessExited {
                method: "stdio write cleanup".to_string(),
            });
            self.terminal_cleanup = Some(StdioTerminalCleanup::spawn(None, writer, failures));
            return;
        };
        if let Err(error) = process.terminate_child_now() {
            failures.push(error);
        }
        self.terminal_cleanup = Some(StdioTerminalCleanup::spawn(Some(process), writer, failures));
    }

    fn poison_after_websocket_write_timeout(&mut self) {
        if self.transport.is_websocket() {
            self.poisoned = true;
            self.transport.abort_websocket();
        }
    }

    #[cfg(feature = "lifecycle-test-support")]
    fn pause_before_transport_close_classification_for_test(
        &mut self,
        received: &Result<Option<IncomingMessage>, DeadlineTransportError>,
    ) {
        if !matches!(
            received,
            Err(DeadlineTransportError::Backend(
                ManagedBackendError::TransportClosed { .. }
            ))
        ) {
            return;
        }
        let Some(pause) = self.transport_close_classification_pause.take() else {
            return;
        };
        let _ = pause.entered.send(());
        let _ = pause.release.recv();
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

impl ManagedBackendStartupStage {
    pub fn ordered() -> &'static [Self] {
        STARTUP_STAGES
    }

    pub fn display_label(self) -> &'static str {
        match self {
            Self::LaunchProcess => "Launch managed backend",
            Self::InitializeHandshake => "Send initialize handshake",
            Self::ValidateRuntime => "Confirm backend runtime",
            Self::VerifyRequiredMethods => "Verify required backend methods",
            Self::Ready => "Keep backend ready for this window",
        }
    }

    pub fn display_description(self) -> &'static str {
        match self {
            Self::LaunchProcess => {
                "Start the managed codex app-server process for the selected workspace."
            }
            Self::InitializeHandshake => {
                "Wait for the initialize response and complete the startup handshake."
            }
            Self::ValidateRuntime => {
                "Confirm that the backend runtime matches host-Windows or WSL-Linux."
            }
            Self::VerifyRequiredMethods => {
                "Call the required compatibility methods before the workspace can open."
            }
            Self::Ready => "Leave the verified managed backend running for this Beryl window.",
        }
    }
}

impl ManagedBackendStartupProgress {
    pub fn new(stage: ManagedBackendStartupStage, detail: Option<String>) -> Self {
        Self { stage, detail }
    }

    pub fn stage(&self) -> ManagedBackendStartupStage {
        self.stage
    }

    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
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
    },
}

impl IncomingMessage {
    fn approximate_retained_bytes(&self) -> usize {
        match self {
            Self::Response { result, .. } => json_value_retained_byte_len(result),
            Self::Error { id, error } => id
                .map(|_| std::mem::size_of::<u64>())
                .unwrap_or_default()
                .saturating_add(error.message.len())
                .saturating_add(optional_json_value_retained_byte_len(error.data.as_ref())),
            Self::Notification { method, params } => method
                .len()
                .saturating_add(optional_json_value_retained_byte_len(params.as_ref())),
            Self::ServerRequest { id, method, params } => json_value_retained_byte_len(id)
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

enum JsonRpcRequestOutcome {
    Result(Value),
    Error(JsonRpcError),
}

#[derive(Clone, Copy)]
struct ExpectedJsonRpcResponse<'a> {
    method: &'a str,
    request_id: u64,
}

struct MessageWriteMetrics {
    serialize: Duration,
    transport: Duration,
    bytes: usize,
}

#[derive(Clone, Copy)]
struct RequestDeadline {
    expires_at: Instant,
    timeout: Duration,
}

impl RequestDeadline {
    fn from_timeout(timeout: Duration) -> Self {
        Self {
            expires_at: Instant::now() + timeout,
            timeout,
        }
    }

    fn remaining(self) -> Option<Duration> {
        self.expires_at
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
    }

    fn timeout_error(self, method: &str) -> ManagedBackendError {
        ManagedBackendError::RequestTimeout {
            method: method.to_string(),
            timeout: self.timeout,
        }
    }
}

enum DeadlineTransportError {
    DeadlineExpired,
    StdioWriteExpired,
    StdioWriteFailed(ManagedBackendError),
    WebSocketReadExpiredAfterProgress(ManagedBackendError),
    WebSocketWriteExpired,
    WebSocketWriteFailed(ManagedBackendError),
    Backend(ManagedBackendError),
}

impl From<ManagedBackendError> for DeadlineTransportError {
    fn from(error: ManagedBackendError) -> Self {
        Self::Backend(error)
    }
}

#[derive(Debug)]
struct SupervisedStdioWriter {
    commands: Option<SyncSender<StdioWriterCommand>>,
    join: Option<JoinHandle<()>>,
    #[cfg(feature = "lifecycle-test-support")]
    finished: Arc<AtomicBool>,
    #[cfg(feature = "lifecycle-test-support")]
    write_count: Arc<AtomicUsize>,
    #[cfg(feature = "lifecycle-test-support")]
    fail_after_bytes: Arc<AtomicUsize>,
}

enum StdioWriterCommand {
    Write {
        method: String,
        bytes: Vec<u8>,
        acknowledgment: SyncSender<Result<(), StdioWriterFailure>>,
    },
}

struct StdioWriterFailure {
    error: ManagedBackendError,
    committed: bool,
}

impl SupervisedStdioWriter {
    fn spawn(mut stdin: ChildStdin) -> Self {
        let (commands, receiver) = mpsc::sync_channel(1);
        #[cfg(feature = "lifecycle-test-support")]
        let finished = Arc::new(AtomicBool::new(false));
        #[cfg(feature = "lifecycle-test-support")]
        let thread_finished = Arc::clone(&finished);
        #[cfg(feature = "lifecycle-test-support")]
        let write_count = Arc::new(AtomicUsize::new(0));
        #[cfg(feature = "lifecycle-test-support")]
        let thread_write_count = Arc::clone(&write_count);
        #[cfg(feature = "lifecycle-test-support")]
        let fail_after_bytes = Arc::new(AtomicUsize::new(usize::MAX));
        #[cfg(feature = "lifecycle-test-support")]
        let thread_fail_after_bytes = Arc::clone(&fail_after_bytes);
        let join = thread::spawn(move || {
            while let Ok(StdioWriterCommand::Write {
                method,
                bytes,
                acknowledgment,
            }) = receiver.recv()
            {
                #[cfg(feature = "lifecycle-test-support")]
                thread_write_count.fetch_add(1, Ordering::SeqCst);
                let result = write_stdio_message(
                    &mut stdin,
                    &method,
                    &bytes,
                    #[cfg(feature = "lifecycle-test-support")]
                    &thread_fail_after_bytes,
                );
                let _ = acknowledgment.send(result);
            }
            #[cfg(feature = "lifecycle-test-support")]
            thread_finished.store(true, Ordering::SeqCst);
        });
        Self {
            commands: Some(commands),
            join: Some(join),
            #[cfg(feature = "lifecycle-test-support")]
            finished,
            #[cfg(feature = "lifecycle-test-support")]
            write_count,
            #[cfg(feature = "lifecycle-test-support")]
            fail_after_bytes,
        }
    }

    fn write_message(
        &mut self,
        method: &str,
        line: &str,
        deadline: Option<Instant>,
    ) -> Result<(), DeadlineTransportError> {
        let Some(commands) = self.commands.as_ref() else {
            return Err(ManagedBackendError::SessionPoisoned {
                method: method.to_string(),
            }
            .into());
        };
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(DeadlineTransportError::DeadlineExpired);
        }

        let mut bytes = line.as_bytes().to_vec();
        bytes.push(b'\n');
        let (acknowledgment, result) = mpsc::sync_channel(1);
        match commands.try_send(StdioWriterCommand::Write {
            method: method.to_string(),
            bytes,
            acknowledgment,
        }) {
            Ok(()) => {}
            Err(TrySendError::Disconnected(_)) => {
                return Err(ManagedBackendError::StdioWriterStopped {
                    method: method.to_string(),
                }
                .into());
            }
            Err(TrySendError::Full(_)) => {
                return Err(ManagedBackendError::SessionPoisoned {
                    method: method.to_string(),
                }
                .into());
            }
        }

        match deadline {
            Some(deadline) => {
                let Some(remaining) = deadline
                    .checked_duration_since(Instant::now())
                    .filter(|remaining| !remaining.is_zero())
                else {
                    self.poison();
                    return Err(DeadlineTransportError::StdioWriteExpired);
                };
                match result.recv_timeout(remaining) {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(failure)) if failure.committed => {
                        Err(DeadlineTransportError::StdioWriteFailed(failure.error))
                    }
                    Ok(Err(failure)) => Err(DeadlineTransportError::Backend(failure.error)),
                    Err(RecvTimeoutError::Timeout) => {
                        self.poison();
                        Err(DeadlineTransportError::StdioWriteExpired)
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        Err(ManagedBackendError::StdioWriterStopped {
                            method: method.to_string(),
                        }
                        .into())
                    }
                }
            }
            None => match result
                .recv()
                .map_err(|_| ManagedBackendError::StdioWriterStopped {
                    method: method.to_string(),
                })? {
                Ok(()) => Ok(()),
                Err(failure) if failure.committed => {
                    Err(DeadlineTransportError::StdioWriteFailed(failure.error))
                }
                Err(failure) => Err(DeadlineTransportError::Backend(failure.error)),
            },
        }
    }

    fn poison(&mut self) {
        drop(self.commands.take());
    }

    fn close(&mut self) {
        drop(self.commands.take());
    }

    fn join(&mut self) -> Result<(), ManagedBackendError> {
        self.close();
        let Some(join) = self.join.take() else {
            return Ok(());
        };
        join.join()
            .map_err(|_| ManagedBackendError::StdioWriterPanicked)
    }

    #[cfg(feature = "lifecycle-test-support")]
    fn is_finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }

    #[cfg(feature = "lifecycle-test-support")]
    fn write_count(&self) -> usize {
        self.write_count.load(Ordering::SeqCst)
    }

    #[cfg(feature = "lifecycle-test-support")]
    fn fail_next_write_after_bytes(&self, bytes: usize) {
        self.fail_after_bytes.store(bytes, Ordering::SeqCst);
    }
}

fn write_stdio_message(
    stdin: &mut ChildStdin,
    method: &str,
    mut bytes: &[u8],
    #[cfg(feature = "lifecycle-test-support")] fail_after_bytes: &AtomicUsize,
) -> Result<(), StdioWriterFailure> {
    let mut committed = false;
    while !bytes.is_empty() {
        #[cfg(feature = "lifecycle-test-support")]
        let write_bytes = {
            let remaining = fail_after_bytes.load(Ordering::SeqCst);
            if remaining == 0 {
                return Err(StdioWriterFailure {
                    error: ManagedBackendError::WriteRequest {
                        method: method.to_string(),
                        source: io::Error::new(
                            io::ErrorKind::BrokenPipe,
                            "injected stdio write failure after byte commitment",
                        ),
                    },
                    committed,
                });
            }
            &bytes[..bytes.len().min(remaining)]
        };
        #[cfg(not(feature = "lifecycle-test-support"))]
        let write_bytes = bytes;
        match stdin.write(write_bytes) {
            Ok(0) => {
                return Err(StdioWriterFailure {
                    error: ManagedBackendError::WriteRequest {
                        method: method.to_string(),
                        source: io::Error::new(
                            io::ErrorKind::WriteZero,
                            "failed to write complete stdio request",
                        ),
                    },
                    committed,
                });
            }
            Ok(written) => {
                committed = true;
                #[cfg(feature = "lifecycle-test-support")]
                if fail_after_bytes.load(Ordering::SeqCst) != usize::MAX {
                    fail_after_bytes.fetch_sub(written, Ordering::SeqCst);
                }
                bytes = &bytes[written..];
            }
            Err(source) => {
                return Err(StdioWriterFailure {
                    error: ManagedBackendError::WriteRequest {
                        method: method.to_string(),
                        source,
                    },
                    committed,
                });
            }
        }
    }
    stdin.flush().map_err(|source| StdioWriterFailure {
        error: ManagedBackendError::WriteRequest {
            method: method.to_string(),
            source,
        },
        committed,
    })
}

impl Drop for SupervisedStdioWriter {
    fn drop(&mut self) {
        if let Err(error) = self.join() {
            warn!(%error, "failed to join supervised backend stdio writer");
        }
    }
}

#[derive(Debug)]
struct StdioTerminalCleanup {
    state: Option<StdioTerminalCleanupState>,
}

#[derive(Debug)]
enum StdioTerminalCleanupState {
    Running(JoinHandle<StdioCleanupOutcome>),
    Retained {
        process: Option<SupervisedBackendProcess>,
        writer: Option<SupervisedStdioWriter>,
    },
    Finished,
}

#[derive(Debug)]
enum StdioCleanupOutcome {
    Complete(Vec<ManagedBackendError>),
    Retry {
        process: Option<SupervisedBackendProcess>,
        writer: SupervisedStdioWriter,
        failures: Vec<ManagedBackendError>,
    },
}

impl StdioTerminalCleanup {
    fn spawn(
        process: Option<SupervisedBackendProcess>,
        writer: SupervisedStdioWriter,
        failures: Vec<ManagedBackendError>,
    ) -> Self {
        Self {
            state: Some(StdioTerminalCleanupState::Running(
                spawn_stdio_cleanup_attempt(process, writer, failures),
            )),
        }
    }

    fn finish(&mut self) -> Result<(), ManagedBackendError> {
        loop {
            match self
                .state
                .take()
                .unwrap_or(StdioTerminalCleanupState::Finished)
            {
                StdioTerminalCleanupState::Running(join) => {
                    let outcome = join
                        .join()
                        .map_err(|_| ManagedBackendError::StdioWriterPanicked)?;
                    return self.accept_outcome(outcome);
                }
                StdioTerminalCleanupState::Retained {
                    process,
                    writer: Some(writer),
                } => {
                    self.state = Some(StdioTerminalCleanupState::Running(
                        spawn_stdio_cleanup_attempt(process, writer, Vec::new()),
                    ));
                }
                StdioTerminalCleanupState::Retained { writer: None, .. }
                | StdioTerminalCleanupState::Finished => {
                    self.state = Some(StdioTerminalCleanupState::Finished);
                    return Ok(());
                }
            }
        }
    }

    fn accept_outcome(&mut self, outcome: StdioCleanupOutcome) -> Result<(), ManagedBackendError> {
        match outcome {
            StdioCleanupOutcome::Complete(failures) => {
                self.state = Some(StdioTerminalCleanupState::Finished);
                finish_stdio_cleanup(failures)
            }
            StdioCleanupOutcome::Retry {
                process,
                writer,
                failures,
            } => {
                self.state = Some(StdioTerminalCleanupState::Retained {
                    process,
                    writer: Some(writer),
                });
                finish_stdio_cleanup(failures)
            }
        }
    }

    #[cfg(feature = "lifecycle-test-support")]
    fn is_finished(&self) -> bool {
        matches!(self.state, Some(StdioTerminalCleanupState::Finished))
    }

    #[cfg(feature = "lifecycle-test-support")]
    fn is_retained(&self) -> bool {
        matches!(
            self.state,
            Some(StdioTerminalCleanupState::Retained {
                writer: Some(_),
                ..
            })
        )
    }

    #[cfg(feature = "lifecycle-test-support")]
    fn writer_finished(&self) -> bool {
        match self.state.as_ref() {
            Some(StdioTerminalCleanupState::Retained {
                writer: Some(writer),
                ..
            }) => writer.is_finished(),
            Some(StdioTerminalCleanupState::Finished) => true,
            Some(StdioTerminalCleanupState::Running(_))
            | Some(StdioTerminalCleanupState::Retained { writer: None, .. })
            | None => false,
        }
    }

    #[cfg(feature = "lifecycle-test-support")]
    fn write_count(&self) -> usize {
        match self.state.as_ref() {
            Some(StdioTerminalCleanupState::Retained {
                writer: Some(writer),
                ..
            }) => writer.write_count(),
            Some(StdioTerminalCleanupState::Running(_))
            | Some(StdioTerminalCleanupState::Retained { writer: None, .. })
            | Some(StdioTerminalCleanupState::Finished)
            | None => 0,
        }
    }
}

impl Drop for StdioTerminalCleanup {
    fn drop(&mut self) {
        let mut state = self
            .state
            .take()
            .unwrap_or(StdioTerminalCleanupState::Finished);
        if let StdioTerminalCleanupState::Running(join) = state {
            state = match join.join() {
                Ok(StdioCleanupOutcome::Complete(failures)) => {
                    if let Err(error) = finish_stdio_cleanup(failures) {
                        warn!(%error, "terminal backend stdio cleanup failed during drop");
                    }
                    return;
                }
                Ok(StdioCleanupOutcome::Retry {
                    process,
                    writer,
                    failures,
                }) => {
                    if let Err(error) = finish_stdio_cleanup(failures) {
                        warn!(%error, "terminal backend stdio cleanup attempt failed during drop");
                    }
                    StdioTerminalCleanupState::Retained {
                        process,
                        writer: Some(writer),
                    }
                }
                Err(_) => {
                    warn!("terminal backend stdio cleanup worker panicked during drop");
                    return;
                }
            };
        }

        if let StdioTerminalCleanupState::Retained {
            mut process,
            mut writer,
        } = state
        {
            if let Some(process) = process.as_mut()
                && let Err(error) = process.shutdown(Duration::ZERO, MANAGED_PROCESS_KILL_TIMEOUT)
            {
                warn!(%error, "final bounded backend process cleanup attempt failed");
            }
            // Keep the exact supervisor ownership live while the writer joins.
            // If the OS repeatedly refuses termination, final destruction may
            // block here rather than detach either the child or the writer.
            if let Some(writer) = writer.as_mut()
                && let Err(error) = writer.join()
            {
                warn!(%error, "failed to join terminal backend stdio writer during drop");
            }
            drop(process);
        }
    }
}

fn spawn_stdio_cleanup_attempt(
    mut process: Option<SupervisedBackendProcess>,
    mut writer: SupervisedStdioWriter,
    mut failures: Vec<ManagedBackendError>,
) -> JoinHandle<StdioCleanupOutcome> {
    thread::spawn(move || {
        if let Some(process_ref) = process.as_mut()
            && let Err(error) = process_ref.shutdown(Duration::ZERO, MANAGED_PROCESS_KILL_TIMEOUT)
        {
            failures.push(error);
            return StdioCleanupOutcome::Retry {
                process,
                writer,
                failures,
            };
        }
        drop(process);
        if let Err(error) = writer.join() {
            failures.push(error);
        }
        StdioCleanupOutcome::Complete(failures)
    })
}

fn finish_stdio_cleanup(mut failures: Vec<ManagedBackendError>) -> Result<(), ManagedBackendError> {
    match failures.len() {
        0 => Ok(()),
        1 => Err(failures.pop().expect("one cleanup failure must exist")),
        _ => Err(ManagedBackendError::StdioCleanupFailures { failures }),
    }
}

enum BackendClientTransport {
    Stdio {
        writer: Option<SupervisedStdioWriter>,
        messages: Receiver<Result<IncomingMessage, ManagedBackendError>>,
    },
    WebSocket(WebSocketClientTransport),
}

impl BackendClientTransport {
    fn write_message(
        &mut self,
        method: &str,
        line: &str,
        deadline: Option<Instant>,
    ) -> Result<(), DeadlineTransportError> {
        match self {
            Self::Stdio { writer, .. } => {
                let Some(writer) = writer.as_mut() else {
                    return Err(ManagedBackendError::SessionPoisoned {
                        method: method.to_string(),
                    }
                    .into());
                };
                writer.write_message(method, line, deadline)
            }
            Self::WebSocket(transport) => match transport.write_message(method, line, deadline) {
                Ok(()) => Ok(()),
                Err(ManagedBackendError::WebSocketTransport { source, .. })
                    if source.is_write_deadline_expired() =>
                {
                    Err(DeadlineTransportError::WebSocketWriteExpired)
                }
                Err(ManagedBackendError::WebSocketTransport { source, .. })
                    if source.is_deadline_expired() =>
                {
                    Err(DeadlineTransportError::DeadlineExpired)
                }
                Err(error @ ManagedBackendError::WebSocketTransport { .. })
                    if matches!(
                        &error,
                        ManagedBackendError::WebSocketTransport { source, .. }
                            if source.is_write_failed_after_commit()
                    ) =>
                {
                    Err(DeadlineTransportError::WebSocketWriteFailed(error))
                }
                Err(error) => Err(DeadlineTransportError::Backend(error)),
            },
        }
    }

    fn recv_message_until(
        &mut self,
        method: &str,
        deadline: Instant,
        expected_response: Option<ExpectedJsonRpcResponse<'_>>,
    ) -> Result<Option<IncomingMessage>, DeadlineTransportError> {
        match self {
            Self::Stdio { messages, .. } => {
                let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                    return Err(DeadlineTransportError::DeadlineExpired);
                };
                if remaining.is_zero() {
                    return Err(DeadlineTransportError::DeadlineExpired);
                }
                match messages.recv_timeout(remaining) {
                    Ok(message) => message.map(Some).map_err(DeadlineTransportError::Backend),
                    Err(RecvTimeoutError::Timeout) => Err(DeadlineTransportError::DeadlineExpired),
                    Err(RecvTimeoutError::Disconnected) => {
                        Err(ManagedBackendError::TransportClosed {
                            method: method.to_string(),
                        }
                        .into())
                    }
                }
            }
            Self::WebSocket(transport) => {
                if let Some((expected, sanitizer_kind)) = expected_response.and_then(|expected| {
                    response_sanitizer_kind(expected.method).map(|kind| (expected, kind))
                }) {
                    return match transport.recv_text_message_until_with_parser(
                        method,
                        deadline,
                        |reader| {
                            sanitize_json_rpc_message(sanitizer_kind, expected.request_id, reader)
                        },
                    ) {
                        Err(error @ ManagedBackendError::WebSocketTransport { .. })
                            if matches!(
                                &error,
                                ManagedBackendError::WebSocketTransport { source, .. }
                                    if source.is_read_deadline_expired_after_progress()
                            ) =>
                        {
                            Err(DeadlineTransportError::WebSocketReadExpiredAfterProgress(
                                error,
                            ))
                        }
                        Err(ManagedBackendError::WebSocketTransport { source, .. })
                            if source.is_write_deadline_expired() =>
                        {
                            Err(DeadlineTransportError::WebSocketWriteExpired)
                        }
                        Err(ManagedBackendError::WebSocketTransport { source, .. })
                            if source.is_deadline_expired() =>
                        {
                            Err(DeadlineTransportError::DeadlineExpired)
                        }
                        Err(error @ ManagedBackendError::WebSocketTransport { .. })
                            if matches!(
                                &error,
                                ManagedBackendError::WebSocketTransport { source, .. }
                                    if source.is_write_failed_after_commit()
                            ) =>
                        {
                            Err(DeadlineTransportError::WebSocketWriteFailed(error))
                        }
                        Err(error) => Err(DeadlineTransportError::Backend(error)),
                        Ok(received) => match received {
                            Some(Ok(sanitized)) => {
                                let sanitized_response_bytes =
                                    sanitized_value_byte_len(&sanitized.value);
                                let parse_started = Instant::now();
                                let incoming = parse_incoming_value(sanitized.value).map(Some);
                                let sanitized_response_parse = parse_started.elapsed();
                                debug!(
                                    target: "beryl_backend::backend_metrics",
                                    method = expected.method,
                                    request_id = expected.request_id,
                                    sanitizer_kind = ?sanitizer_kind,
                                    sanitized_response_bytes,
                                    sanitizer_turn_count = sanitized.stats.turn_count,
                                    sanitizer_item_count = sanitized.stats.item_count,
                                    sanitizer_image_result_removed_count =
                                        sanitized.stats.image_result_removed_count,
                                    sanitizer_total_ms =
                                        elapsed_ms(sanitized.stats.total_sanitize),
                                    sanitizer_result_ms =
                                        elapsed_ms(sanitized.stats.result_sanitize),
                                    sanitizer_turn_array_ms =
                                        elapsed_ms(sanitized.stats.turn_array_sanitize),
                                    sanitizer_item_array_ms =
                                        elapsed_ms(sanitized.stats.item_array_sanitize),
                                    sanitizer_image_result_skip_ms =
                                        elapsed_ms(sanitized.stats.image_result_skip),
                                    sanitizer_image_result_skip_max_ms =
                                        elapsed_ms(sanitized.stats.image_result_skip_max),
                                    sanitized_response_parse_ms =
                                        elapsed_ms(sanitized_response_parse),
                                    "parsed sanitized backend JSON-RPC response metrics"
                                );
                                incoming.map_err(DeadlineTransportError::Backend)
                            }
                            Some(Err(source)) => Err(DeadlineTransportError::Backend(
                                ManagedBackendError::SanitizeResponse {
                                    method: expected.method.to_string(),
                                    source,
                                },
                            )),
                            None => Ok(None),
                        },
                    };
                }

                match transport.recv_text_message_until(
                    method,
                    deadline,
                    if expected_response.is_none() {
                        WebSocketReceiveMode::StreamPoll
                    } else {
                        WebSocketReceiveMode::Request
                    },
                ) {
                    Err(error @ ManagedBackendError::WebSocketTransport { .. })
                        if matches!(
                            &error,
                            ManagedBackendError::WebSocketTransport { source, .. }
                                if source.is_read_deadline_expired_after_progress()
                        ) =>
                    {
                        Err(DeadlineTransportError::WebSocketReadExpiredAfterProgress(
                            error,
                        ))
                    }
                    Err(ManagedBackendError::WebSocketTransport { source, .. })
                        if source.is_write_deadline_expired() =>
                    {
                        Err(DeadlineTransportError::WebSocketWriteExpired)
                    }
                    Err(ManagedBackendError::WebSocketTransport { source, .. })
                        if source.is_deadline_expired() =>
                    {
                        Err(DeadlineTransportError::DeadlineExpired)
                    }
                    Err(error @ ManagedBackendError::WebSocketTransport { .. })
                        if matches!(
                            &error,
                            ManagedBackendError::WebSocketTransport { source, .. }
                                if source.is_write_failed_after_commit()
                        ) =>
                    {
                        Err(DeadlineTransportError::WebSocketWriteFailed(error))
                    }
                    Err(error) => Err(DeadlineTransportError::Backend(error)),
                    Ok(Some(text)) => {
                        let parse_started = Instant::now();
                        let incoming = parse_incoming_message(&text)
                            .map(Some)
                            .map_err(DeadlineTransportError::Backend);
                        debug!(
                            method,
                            response_bytes = text.len(),
                            response_parse_ms = elapsed_ms(parse_started.elapsed()),
                            "parsed backend JSON-RPC response"
                        );
                        incoming
                    }
                    Ok(None) => Ok(None),
                }
            }
        }
    }

    fn close(&mut self) {
        match self {
            Self::Stdio { writer, .. } => {
                if let Some(writer) = writer.as_mut() {
                    writer.close();
                }
            }
            Self::WebSocket(transport) => {
                transport.close();
            }
        }
    }

    fn abort_websocket(&mut self) {
        if let Self::WebSocket(transport) = self {
            transport.abort();
        }
    }

    fn take_stdio_writer(&mut self) -> Option<SupervisedStdioWriter> {
        match self {
            Self::Stdio { writer, .. } => writer.take(),
            Self::WebSocket(_) => None,
        }
    }

    fn join_stdio_writer(&mut self) -> Result<(), ManagedBackendError> {
        match self {
            Self::Stdio { writer, .. } => match writer.as_mut() {
                Some(writer) => writer.join(),
                None => Ok(()),
            },
            Self::WebSocket(_) => Ok(()),
        }
    }

    fn is_websocket(&self) -> bool {
        matches!(self, Self::WebSocket(_))
    }

    #[cfg(feature = "lifecycle-test-support")]
    fn websocket_close_frame_attempts(&self) -> Option<usize> {
        match self {
            Self::WebSocket(transport) => Some(transport.close_frame_attempts()),
            Self::Stdio { .. } => None,
        }
    }

    #[cfg(feature = "lifecycle-test-support")]
    fn fail_next_stdio_write_after_bytes(&mut self, bytes: usize) -> bool {
        match self {
            Self::Stdio {
                writer: Some(writer),
                ..
            } => {
                writer.fail_next_write_after_bytes(bytes);
                true
            }
            Self::Stdio { writer: None, .. } | Self::WebSocket(_) => false,
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
struct ThreadListProbeParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
    cwd: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort_key: Option<ThreadSortKey>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort_direction: Option<SortDirection>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadLoadedListProbeParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadSetNameParams<'a> {
    thread_id: &'a str,
    name: &'a str,
}

impl<'a> ThreadSetNameParams<'a> {
    fn new(thread_id: &'a str, name: &'a str) -> Self {
        Self { thread_id, name }
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
    thread_id: &'a str,
}

impl<'a> ThreadCompactStartParams<'a> {
    fn new(thread_id: &'a str) -> Self {
        Self { thread_id }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnInterruptParams<'a> {
    thread_id: &'a str,
    turn_id: &'a str,
}

impl<'a> TurnInterruptParams<'a> {
    fn new(thread_id: &'a str, turn_id: &'a str) -> Self {
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
    thread_id: &'a str,
}

impl<'a> ThreadBackgroundTerminalsCleanParams<'a> {
    fn new(thread_id: &'a str) -> Self {
        Self { thread_id }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadUnsubscribeParams<'a> {
    thread_id: &'a str,
}

impl<'a> ThreadUnsubscribeParams<'a> {
    fn new(thread_id: &'a str) -> Self {
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

pub(crate) fn spawn_stderr_logger(stderr: ChildStderr, launch_spec: BackendLaunchSpec) {
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
                        workspace = %launch_spec.display_label(),
                        message = %message,
                        "backend stderr"
                    );
                }
                Err(error) => {
                    warn!(
                        workspace = %launch_spec.display_label(),
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
    let value: Value =
        serde_json::from_str(line).map_err(|source| ManagedBackendError::InvalidJsonLine {
            line: truncate_for_log(line, INVALID_JSON_ERROR_LINE_LIMIT),
            source,
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
