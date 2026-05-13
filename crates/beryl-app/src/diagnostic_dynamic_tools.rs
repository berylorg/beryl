use std::{collections::VecDeque, ops::Range};

use beryl_backend::{DynamicToolCallRequest, DynamicToolCallResponse, DynamicToolSpec};
use gpui::{
    RendererDiagnosticSnapshot as GpuiRendererDiagnosticSnapshot,
    WindowRendererDiagnosticSnapshot as GpuiWindowRendererDiagnosticSnapshot,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    dynamic_tools::BERYL_DYNAMIC_TOOL_NAMESPACE,
    memory_diagnostics::{ProcessMemorySnapshot, RetainedStateSnapshot},
};

pub const READ_PROCESS_DIAGNOSTICS_TOOL: &str = "read_process_diagnostics";
pub const READ_MEMORY_DIAGNOSTICS_TOOL: &str = "read_memory_diagnostics";
pub const READ_RENDERER_DIAGNOSTICS_TOOL: &str = "read_renderer_diagnostics";
pub const READ_RETAINED_STATE_SUMMARY_TOOL: &str = "read_retained_state_summary";
pub const READ_VISIBLE_MEDIA_TOOL: &str = "read_visible_media";
pub const READ_MEDIA_EVENTS_TOOL: &str = "read_media_events";

pub(crate) const DEFAULT_VISIBLE_MEDIA_LIMIT: usize = 32;
pub(crate) const MAX_VISIBLE_MEDIA_LIMIT: usize = 64;
pub(crate) const DEFAULT_MEDIA_EVENT_LIMIT: usize = 64;
pub(crate) const MAX_MEDIA_EVENT_LIMIT: usize = 128;
const MAX_RENDERER_DIAGNOSTIC_WINDOWS: usize = 16;
const MEDIA_EVENT_RING_CAPACITY: usize = 256;
const MAX_DIAGNOSTIC_STRING_BYTES: usize = 512;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticToolSnapshot {
    pub process: ProcessDiagnosticSnapshot,
    pub memory: MemoryDiagnosticSnapshot,
    pub renderer: RendererDiagnosticSnapshot,
    pub retained_state: RetainedStateSnapshot,
    pub visible_media: VisibleMediaSnapshot,
    pub media_events: MediaEventSnapshot,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProcessDiagnosticSnapshot {
    pub pid: u32,
    pub executable_path: Option<String>,
    pub beryl_home: Option<String>,
    pub selected_workspace_id: Option<String>,
    pub selected_thread_id: Option<String>,
    pub selected_runtime_target: Option<RuntimeTargetDiagnostic>,
    pub managed_backend_child_pids: Vec<ManagedBackendProcessDiagnostic>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeTargetDiagnostic {
    pub runtime: String,
    pub canonical_path: String,
    pub display_label: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ManagedBackendProcessDiagnostic {
    pub pid: u32,
    pub runtime_target: RuntimeTargetDiagnostic,
    pub selected: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemoryDiagnosticSnapshot {
    pub counters: Option<ProcessMemorySnapshot>,
    pub unavailable_reason: Option<String>,
    pub ui: MemoryDiagnosticUiCorrelation,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemoryDiagnosticUiCorrelation {
    pub selected_workspace_id: Option<String>,
    pub selected_thread_id: Option<String>,
    pub selected_runtime_target: Option<RuntimeTargetDiagnostic>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RendererDiagnosticSnapshot {
    pub target: ProcessDiagnosticSnapshot,
    pub shell_window: ShellWindowRendererDiagnostic,
    pub renderer: GpuiRendererDiagnosticSnapshot,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ShellWindowRendererDiagnostic {
    pub window_id: u64,
    pub matched_renderer_window: bool,
    pub active: Option<bool>,
    pub logical_width: Option<f64>,
    pub logical_height: Option<f64>,
    pub device_width: Option<u32>,
    pub device_height: Option<u32>,
    pub scale_factor: Option<f32>,
    pub surface_usable: Option<bool>,
    pub renderer_attribution_ready: bool,
    pub unready_reason: Option<String>,
}

impl ShellWindowRendererDiagnostic {
    pub(crate) fn from_renderer_window(
        window_id: u64,
        window: Option<&GpuiWindowRendererDiagnosticSnapshot>,
    ) -> Self {
        let Some(window) = window else {
            return Self {
                window_id,
                matched_renderer_window: false,
                active: None,
                logical_width: None,
                logical_height: None,
                device_width: None,
                device_height: None,
                scale_factor: None,
                surface_usable: None,
                renderer_attribution_ready: false,
                unready_reason: Some("shell_window_not_in_renderer_snapshot".to_string()),
            };
        };
        let unready_reason = shell_renderer_unready_reason(window);
        Self {
            window_id,
            matched_renderer_window: true,
            active: Some(window.active),
            logical_width: Some(window.logical_width),
            logical_height: Some(window.logical_height),
            device_width: Some(window.device_width),
            device_height: Some(window.device_height),
            scale_factor: Some(window.scale_factor),
            surface_usable: Some(window.surface_usable),
            renderer_attribution_ready: unready_reason.is_none(),
            unready_reason,
        }
    }
}

pub(crate) fn renderer_snapshot_with_shell_window(
    target: ProcessDiagnosticSnapshot,
    mut renderer: GpuiRendererDiagnosticSnapshot,
    shell_window_snapshot: GpuiWindowRendererDiagnosticSnapshot,
) -> RendererDiagnosticSnapshot {
    let shell_window_id = shell_window_snapshot.window_id;
    if let Some(window) = renderer
        .windows
        .iter_mut()
        .find(|window| window.window_id == shell_window_id)
    {
        *window = shell_window_snapshot;
    } else {
        renderer.window_count = renderer.window_count.saturating_add(1);
        renderer.windows.insert(0, shell_window_snapshot);
        if renderer.windows.len() > MAX_RENDERER_DIAGNOSTIC_WINDOWS {
            renderer.windows.truncate(MAX_RENDERER_DIAGNOSTIC_WINDOWS);
            renderer.truncated = true;
        }
    }
    let shell_window = ShellWindowRendererDiagnostic::from_renderer_window(
        shell_window_id,
        renderer
            .windows
            .iter()
            .find(|window| window.window_id == shell_window_id),
    );
    RendererDiagnosticSnapshot {
        target,
        shell_window,
        renderer,
    }
}

fn shell_renderer_unready_reason(window: &GpuiWindowRendererDiagnosticSnapshot) -> Option<String> {
    if !window.surface_usable {
        return Some(
            window
                .surface_unusable_reason
                .clone()
                .unwrap_or_else(|| "surface_unusable".to_string()),
        );
    }
    if window.device_width == 0 || window.device_height == 0 {
        return Some("zero_device_size".to_string());
    }
    if window.logical_width <= 0.0 || window.logical_height <= 0.0 {
        return Some("zero_logical_size".to_string());
    }
    if !window.active {
        return Some("shell_window_inactive".to_string());
    }
    None
}

impl MemoryDiagnosticUiCorrelation {
    pub(crate) fn from_process(process: &ProcessDiagnosticSnapshot) -> Self {
        Self {
            selected_workspace_id: process.selected_workspace_id.clone(),
            selected_thread_id: process.selected_thread_id.clone(),
            selected_runtime_target: process.selected_runtime_target.clone(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VisibleMediaSnapshot {
    pub frame_generation: u64,
    pub selected_thread_id: Option<String>,
    pub presentation_range: Option<PresentationRangeDiagnostic>,
    pub items: Vec<VisibleMediaItemDiagnostic>,
    pub item_count: usize,
    pub truncated: bool,
    pub stale: bool,
    pub preview: PreviewDiagnostic,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PresentationRangeDiagnostic {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VisibleMediaItemDiagnostic {
    pub row_identity: Option<String>,
    pub key: String,
    pub source_kind: String,
    pub outcome: String,
    pub format: Option<String>,
    pub compressed_bytes: Option<usize>,
    pub decoded_bytes_estimate: Option<usize>,
    pub natural_width: Option<u32>,
    pub natural_height: Option<u32>,
    pub displayed_width: f64,
    pub displayed_height: f64,
    pub image_id: Option<u64>,
    pub image_asset_key_hash: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewDiagnostic {
    pub transcript_image_preview: Option<PreviewStateDiagnostic>,
    pub composer_image_preview: Option<PreviewStateDiagnostic>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewStateDiagnostic {
    pub state: String,
    pub compressed_bytes: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaEventSnapshot {
    pub events: Vec<MediaDiagnosticEvent>,
    pub event_count: usize,
    pub truncated: bool,
    pub next_sequence: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaDiagnosticEvent {
    pub sequence: u64,
    pub kind: String,
    pub row_identity: Option<String>,
    pub key: Option<String>,
    pub source_kind: Option<String>,
    pub outcome: Option<String>,
    pub format: Option<String>,
    pub compressed_bytes: Option<usize>,
    pub decoded_bytes_estimate: Option<usize>,
    pub natural_width: Option<u32>,
    pub natural_height: Option<u32>,
    pub image_id: Option<u64>,
    pub image_asset_key_hash: Option<u64>,
    pub image_count: Option<usize>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct VisibleMediaDiagnostics {
    frame_generation: u64,
    selected_thread_id: Option<String>,
    presentation_range: Option<Range<usize>>,
    items: Vec<VisibleMediaItemDiagnostic>,
    truncated: bool,
    stale: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct MediaDiagnosticLog {
    next_sequence: u64,
    events: VecDeque<MediaDiagnosticEvent>,
    capacity: usize,
}

impl VisibleMediaDiagnostics {
    pub fn begin_frame(&mut self, selected_thread_id: Option<String>, range: Range<usize>) {
        self.frame_generation = self.frame_generation.saturating_add(1);
        self.selected_thread_id = selected_thread_id.map(truncate_diagnostic_string);
        self.presentation_range = Some(range);
        self.items.clear();
        self.truncated = false;
        self.stale = false;
    }

    pub fn clear(&mut self) {
        self.frame_generation = self.frame_generation.saturating_add(1);
        self.selected_thread_id = None;
        self.presentation_range = None;
        self.items.clear();
        self.truncated = false;
        self.stale = true;
    }

    pub fn record_item(&mut self, item: VisibleMediaItemDiagnostic) {
        if self.items.len() >= MAX_VISIBLE_MEDIA_LIMIT {
            self.truncated = true;
            return;
        }
        self.items.push(item.truncated());
    }

    pub fn snapshot(&self) -> VisibleMediaSnapshot {
        VisibleMediaSnapshot {
            frame_generation: self.frame_generation,
            selected_thread_id: self.selected_thread_id.clone(),
            presentation_range: self.presentation_range.as_ref().map(|range| {
                PresentationRangeDiagnostic {
                    start: range.start,
                    end: range.end,
                }
            }),
            items: self.items.clone(),
            item_count: self.items.len(),
            truncated: self.truncated,
            stale: self.stale,
            preview: PreviewDiagnostic::default(),
        }
    }
}

impl Default for MediaDiagnosticLog {
    fn default() -> Self {
        Self {
            next_sequence: 1,
            events: VecDeque::with_capacity(MEDIA_EVENT_RING_CAPACITY),
            capacity: MEDIA_EVENT_RING_CAPACITY,
        }
    }
}

impl MediaDiagnosticLog {
    pub fn record(&mut self, mut event: MediaDiagnosticEvent) {
        event.sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        event.truncate_strings();
        if self.events.len() >= self.capacity {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    pub fn snapshot(&self) -> MediaEventSnapshot {
        MediaEventSnapshot {
            events: self.events.iter().cloned().collect(),
            event_count: self.events.len(),
            truncated: false,
            next_sequence: self.next_sequence,
        }
    }
}

impl VisibleMediaItemDiagnostic {
    fn truncated(mut self) -> Self {
        self.row_identity = self.row_identity.map(truncate_diagnostic_string);
        self.key = truncate_diagnostic_string(self.key);
        self.source_kind = truncate_diagnostic_string(self.source_kind);
        self.outcome = truncate_diagnostic_string(self.outcome);
        self.format = self.format.map(truncate_diagnostic_string);
        self
    }
}

impl MediaDiagnosticEvent {
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            sequence: 0,
            kind: kind.into(),
            row_identity: None,
            key: None,
            source_kind: None,
            outcome: None,
            format: None,
            compressed_bytes: None,
            decoded_bytes_estimate: None,
            natural_width: None,
            natural_height: None,
            image_id: None,
            image_asset_key_hash: None,
            image_count: None,
            detail: None,
        }
    }

    fn truncate_strings(&mut self) {
        self.kind = truncate_diagnostic_string(std::mem::take(&mut self.kind));
        self.row_identity = self.row_identity.take().map(truncate_diagnostic_string);
        self.key = self.key.take().map(truncate_diagnostic_string);
        self.source_kind = self.source_kind.take().map(truncate_diagnostic_string);
        self.outcome = self.outcome.take().map(truncate_diagnostic_string);
        self.format = self.format.take().map(truncate_diagnostic_string);
        self.detail = self.detail.take().map(truncate_diagnostic_string);
    }
}

pub fn beryl_diagnostic_dynamic_tool_specs() -> Vec<DynamicToolSpec> {
    vec![
        DynamicToolSpec::new(
            READ_PROCESS_DIAGNOSTICS_TOOL,
            "Read a bounded Beryl GUI process identity snapshot.",
            empty_object_schema(),
        ),
        DynamicToolSpec::new(
            READ_MEMORY_DIAGNOSTICS_TOOL,
            "Read bounded Beryl GUI process memory counters and related UI labels.",
            empty_object_schema(),
        ),
        DynamicToolSpec::new(
            READ_RENDERER_DIAGNOSTICS_TOOL,
            "Read bounded Beryl GUI renderer resource counters and byte estimates.",
            empty_object_schema(),
        ),
        DynamicToolSpec::new(
            READ_RETAINED_STATE_SUMMARY_TOOL,
            "Read bounded retained-state counters for Beryl GUI projections and caches.",
            empty_object_schema(),
        ),
        DynamicToolSpec::new(
            READ_VISIBLE_MEDIA_TOOL,
            "Read bounded metadata for media currently retained in the visible transcript projection.",
            limited_read_schema(MAX_VISIBLE_MEDIA_LIMIT, DEFAULT_VISIBLE_MEDIA_LIMIT),
        ),
        DynamicToolSpec::new(
            READ_MEDIA_EVENTS_TOOL,
            "Read a bounded metadata-only ring of recent transcript media lifecycle events.",
            media_events_schema(),
        ),
    ]
    .into_iter()
    .map(|tool| {
        tool.with_namespace(BERYL_DYNAMIC_TOOL_NAMESPACE)
            .with_defer_loading(false)
    })
    .collect()
}

pub fn is_beryl_diagnostic_dynamic_tool(request: &DynamicToolCallRequest) -> bool {
    request
        .namespace()
        .is_none_or(|namespace| namespace == BERYL_DYNAMIC_TOOL_NAMESPACE)
        && matches!(
            request.tool(),
            READ_PROCESS_DIAGNOSTICS_TOOL
                | READ_MEMORY_DIAGNOSTICS_TOOL
                | READ_RENDERER_DIAGNOSTICS_TOOL
                | READ_RETAINED_STATE_SUMMARY_TOOL
                | READ_VISIBLE_MEDIA_TOOL
                | READ_MEDIA_EVENTS_TOOL
        )
}

pub(crate) fn dispatch_beryl_diagnostic_dynamic_tool_call(
    request: &DynamicToolCallRequest,
    snapshot: DiagnosticToolSnapshot,
) -> DynamicToolCallResponse {
    match diagnostic_tool_result(request, snapshot) {
        Ok(value) => DynamicToolCallResponse::success_text(compact_json(json!({
            "ok": true,
            "result": value,
        }))),
        Err(error) => diagnostic_failure_response(request, error.kind(), error.to_string()),
    }
}

pub fn diagnostic_bridge_unavailable_response(
    request: &DynamicToolCallRequest,
    message: impl Into<String>,
) -> DynamicToolCallResponse {
    diagnostic_failure_response(request, "shell_unavailable", message.into())
}

pub(crate) fn bounded_diagnostic_string(value: impl Into<String>) -> String {
    truncate_diagnostic_string(value)
}

fn diagnostic_tool_result(
    request: &DynamicToolCallRequest,
    snapshot: DiagnosticToolSnapshot,
) -> Result<Value, DynamicDiagnosticToolError> {
    validate_namespace(request)?;
    match request.tool() {
        READ_PROCESS_DIAGNOSTICS_TOOL => {
            parse_arguments::<EmptyArguments>(request.arguments())?;
            Ok(json!(snapshot.process))
        }
        READ_MEMORY_DIAGNOSTICS_TOOL => {
            parse_arguments::<EmptyArguments>(request.arguments())?;
            Ok(json!(snapshot.memory))
        }
        READ_RENDERER_DIAGNOSTICS_TOOL => {
            parse_arguments::<EmptyArguments>(request.arguments())?;
            Ok(json!(snapshot.renderer))
        }
        READ_RETAINED_STATE_SUMMARY_TOOL => {
            parse_arguments::<EmptyArguments>(request.arguments())?;
            Ok(json!({ "retainedState": snapshot.retained_state }))
        }
        READ_VISIBLE_MEDIA_TOOL => {
            let arguments = parse_arguments::<LimitedReadArguments>(request.arguments())?;
            Ok(json!(visible_media_result(
                snapshot.visible_media,
                arguments.limit_or_default(DEFAULT_VISIBLE_MEDIA_LIMIT, MAX_VISIBLE_MEDIA_LIMIT),
            )))
        }
        READ_MEDIA_EVENTS_TOOL => {
            let arguments = parse_arguments::<MediaEventsArguments>(request.arguments())?;
            Ok(json!(media_events_result(
                snapshot.media_events,
                arguments.after_sequence,
                arguments.limit_or_default(DEFAULT_MEDIA_EVENT_LIMIT, MAX_MEDIA_EVENT_LIMIT),
            )))
        }
        other => Err(DynamicDiagnosticToolError::UnsupportedTool {
            tool: other.to_string(),
        }),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArguments {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LimitedReadArguments {
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MediaEventsArguments {
    limit: Option<usize>,
    after_sequence: Option<u64>,
}

impl LimitedReadArguments {
    fn limit_or_default(self, default: usize, max: usize) -> usize {
        self.limit.unwrap_or(default).min(max)
    }
}

impl MediaEventsArguments {
    fn limit_or_default(&self, default: usize, max: usize) -> usize {
        self.limit.unwrap_or(default).min(max)
    }
}

pub(crate) fn visible_media_result(
    mut snapshot: VisibleMediaSnapshot,
    limit: usize,
) -> VisibleMediaSnapshot {
    if snapshot.items.len() > limit {
        snapshot.items.truncate(limit);
        snapshot.truncated = true;
    }
    snapshot.item_count = snapshot.items.len();
    snapshot
}

pub(crate) fn media_events_result(
    mut snapshot: MediaEventSnapshot,
    after_sequence: Option<u64>,
    limit: usize,
) -> MediaEventSnapshot {
    if let Some(after_sequence) = after_sequence {
        snapshot
            .events
            .retain(|event| event.sequence > after_sequence);
    }
    if snapshot.events.len() > limit {
        snapshot.events.truncate(limit);
        snapshot.truncated = true;
    }
    snapshot.event_count = snapshot.events.len();
    snapshot
}

fn validate_namespace(request: &DynamicToolCallRequest) -> Result<(), DynamicDiagnosticToolError> {
    if let Some(namespace) = request.namespace()
        && namespace != BERYL_DYNAMIC_TOOL_NAMESPACE
    {
        return Err(DynamicDiagnosticToolError::UnsupportedNamespace {
            namespace: namespace.to_string(),
        });
    }
    Ok(())
}

fn parse_arguments<T>(arguments: &Value) -> Result<T, DynamicDiagnosticToolError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(arguments.clone()).map_err(|source| {
        DynamicDiagnosticToolError::InvalidArguments {
            detail: source.to_string(),
        }
    })
}

fn empty_object_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

fn limited_read_schema(max: usize, default: usize) -> Value {
    json!({
        "type": "object",
        "properties": {
            "limit": {
                "type": "integer",
                "minimum": 0,
                "maximum": max,
                "default": default
            }
        },
        "additionalProperties": false
    })
}

fn media_events_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "limit": {
                "type": "integer",
                "minimum": 0,
                "maximum": MAX_MEDIA_EVENT_LIMIT,
                "default": DEFAULT_MEDIA_EVENT_LIMIT
            },
            "afterSequence": {
                "type": "integer",
                "minimum": 0,
                "description": "Return events with sequence numbers greater than this value."
            }
        },
        "additionalProperties": false
    })
}

fn diagnostic_failure_response(
    request: &DynamicToolCallRequest,
    kind: &'static str,
    message: String,
) -> DynamicToolCallResponse {
    DynamicToolCallResponse::failure_text(compact_json(json!({
        "ok": false,
        "error": {
            "kind": kind,
            "message": truncate_diagnostic_string(message),
            "tool": request.tool(),
            "callId": request.call_id(),
        },
    })))
}

fn compact_json(value: Value) -> String {
    serde_json::to_string(&value).unwrap_or_else(|_| {
        "{\"ok\":false,\"error\":{\"kind\":\"internal\",\"message\":\"could not serialize dynamic tool response\"}}"
            .to_string()
    })
}

fn truncate_diagnostic_string(value: impl Into<String>) -> String {
    let mut value = value.into();
    if value.len() <= MAX_DIAGNOSTIC_STRING_BYTES {
        return value;
    }
    let mut end = MAX_DIAGNOSTIC_STRING_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value
}

#[derive(Debug)]
enum DynamicDiagnosticToolError {
    UnsupportedNamespace { namespace: String },
    UnsupportedTool { tool: String },
    InvalidArguments { detail: String },
}

impl DynamicDiagnosticToolError {
    fn kind(&self) -> &'static str {
        match self {
            Self::UnsupportedNamespace { .. } => "unsupported_namespace",
            Self::UnsupportedTool { .. } => "unsupported_tool",
            Self::InvalidArguments { .. } => "invalid_arguments",
        }
    }
}

impl std::fmt::Display for DynamicDiagnosticToolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedNamespace { namespace } => {
                write!(
                    formatter,
                    "unsupported Beryl dynamic tool namespace {namespace:?}"
                )
            }
            Self::UnsupportedTool { tool } => {
                write!(
                    formatter,
                    "unsupported Beryl diagnostic dynamic tool {tool:?}"
                )
            }
            Self::InvalidArguments { detail } => {
                write!(formatter, "invalid diagnostic tool arguments: {detail}")
            }
        }
    }
}

impl std::error::Error for DynamicDiagnosticToolError {}
