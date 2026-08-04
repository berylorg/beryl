use beryl_backend::{DynamicToolCallRequest, DynamicToolCallResponse};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    diagnostic_dynamic_tools::VisibleMediaSnapshot,
    dynamic_tool_namespace::BERYL_DYNAMIC_TOOL_NAMESPACE,
};

pub(crate) const READ_UI_STATE_TOOL: &str = "read_ui_state";
pub(crate) const SWITCH_THREAD_TOOL: &str = "switch_thread";
pub(crate) const SCROLL_TRANSCRIPT_TOOL: &str = "scroll_transcript";
pub(crate) const CLOSE_POPUPS_TOOL: &str = "close_popups";

pub(crate) const SETTINGS_WINDOW_POPUP_CLOSE_REASON: &str = "settings_window_popup";
pub(crate) const MAX_UI_VISIBLE_ROW_LIMIT: usize = 64;
pub(crate) const DEFAULT_UI_VISIBLE_ROW_LIMIT: usize = 32;
pub(crate) const MAX_SCROLL_REPEAT: usize = 8;
pub(crate) const MAX_SCROLL_WHEEL_DELTA_PX: f32 = 640.0;
const MAX_CONTROL_STRING_BYTES: usize = 512;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UiStateSnapshot {
    pub shell_state: String,
    pub selected_surface: String,
    pub selected_thread_id: Option<String>,
    pub backend_unavailable: Option<BackendUnavailableUiState>,
    pub turn_state: TurnUiState,
    pub transcript: TranscriptUiState,
    pub visible_media: VisibleMediaSnapshot,
    pub markdown_cache: MarkdownCacheUiState,
    pub activity_panel: ActivityPanelUiState,
    pub popups: PopupUiState,
    pub background_work: BackgroundWorkUiState,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackendUnavailableUiState {
    pub kind: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TurnUiState {
    pub selected_thread_state: String,
    pub selected_thread_status: Option<String>,
    pub last_turn_state: String,
    pub view: TurnViewUiState,
    pub cancellable_active_turn: Option<CancellableTurnUiState>,
    pub hard_stop_target_count: usize,
    pub hard_stop_limitation_count: usize,
    pub hard_stop_request_in_flight: bool,
    pub hard_stop_hold_active: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TurnViewUiState {
    pub current: Option<usize>,
    pub total: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CancellableTurnUiState {
    pub thread_id: String,
    pub turn_id: String,
    pub kind: String,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TranscriptUiState {
    pub item_count: usize,
    pub visible_range: Option<UiRangeDiagnostic>,
    pub presentation_range: Option<UiRangeDiagnostic>,
    pub scroll_position: TranscriptScrollPositionDiagnostic,
    pub user_scrolled: bool,
    pub older_history_loading: bool,
    pub visible_rows: Vec<VisibleTranscriptRowDiagnostic>,
    pub visible_row_count: usize,
    pub visible_rows_truncated: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UiRangeDiagnostic {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TranscriptScrollPositionDiagnostic {
    pub kind: String,
    pub item_index: Option<usize>,
    pub offset_px: Option<f64>,
}

impl Default for TranscriptScrollPositionDiagnostic {
    fn default() -> Self {
        Self {
            kind: "unavailable".to_string(),
            item_index: None,
            offset_px: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VisibleTranscriptRowDiagnostic {
    pub row_index: usize,
    pub row_identity: String,
    pub source_turn_index: usize,
    pub item_count: usize,
    pub text_chars: usize,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MarkdownCacheUiState {
    pub lookups: u64,
    pub ready_hits: u64,
    pub pending_hits: u64,
    pub misses: u64,
    pub invalidations: u64,
    pub scheduled_parses: u64,
    pub completed_parses: u64,
    pub stale_completions: u64,
    pub evictions: u64,
    pub entries: usize,
    pub pending_entries: usize,
    pub source_bytes: usize,
    pub in_flight_source_bytes: usize,
    pub markdown_media_requests: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActivityPanelUiState {
    pub mode: String,
    pub visible: bool,
    pub row_count: usize,
    pub height_px: f64,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PopupUiState {
    pub transcript_branch_menu_open: bool,
    pub status_line_operations_open: bool,
    pub composer_image_popup_open: bool,
    pub transcript_image_preview_open: bool,
    pub settings_window_visible: Option<bool>,
    pub settings_window_transient_popup_open: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackgroundWorkUiState {
    pub backend_work_receivers: usize,
    pub turn_stream_pending: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SwitchThreadResult {
    pub status: String,
    pub thread_id: String,
    pub message: Option<String>,
    pub ui_state: UiStateSnapshot,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScrollTranscriptResult {
    pub status: String,
    pub command: ScrollTranscriptCommand,
    pub repeat: usize,
    pub delta_y: Option<f32>,
    pub precise: bool,
    pub message: Option<String>,
    pub ui_state: UiStateSnapshot,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClosePopupsResult {
    pub closed_count: usize,
    pub closed: Vec<String>,
    pub ui_state: UiStateSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SwitchThreadArguments {
    pub thread_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScrollTranscriptArguments {
    pub command: ScrollTranscriptCommand,
    pub repeat: usize,
    pub delta_y: Option<f32>,
    pub precise: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ScrollTranscriptCommand {
    Top,
    Bottom,
    Wheel,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GuiControlToolRequest {
    ReadUiState { visible_row_limit: usize },
    SwitchThread(SwitchThreadArguments),
    ScrollTranscript(ScrollTranscriptArguments),
    ClosePopups,
}

pub(crate) fn is_beryl_gui_control_dynamic_tool(request: &DynamicToolCallRequest) -> bool {
    request
        .namespace()
        .is_none_or(|namespace| namespace == BERYL_DYNAMIC_TOOL_NAMESPACE)
        && matches!(
            request.tool(),
            READ_UI_STATE_TOOL | SWITCH_THREAD_TOOL | SCROLL_TRANSCRIPT_TOOL | CLOSE_POPUPS_TOOL
        )
}

pub(crate) fn parse_beryl_gui_control_dynamic_tool_request(
    request: &DynamicToolCallRequest,
) -> Result<GuiControlToolRequest, GuiControlToolError> {
    validate_namespace(request)?;
    parse_gui_control_tool_request(request.tool(), request.arguments())
}

pub(crate) fn parse_gui_control_tool_request(
    tool: &str,
    arguments: &Value,
) -> Result<GuiControlToolRequest, GuiControlToolError> {
    match tool {
        READ_UI_STATE_TOOL => {
            let arguments = parse_arguments::<LimitedReadArguments>(arguments)?;
            arguments.validate_limit(MAX_UI_VISIBLE_ROW_LIMIT)?;
            Ok(GuiControlToolRequest::ReadUiState {
                visible_row_limit: arguments.limit.unwrap_or(DEFAULT_UI_VISIBLE_ROW_LIMIT),
            })
        }
        SWITCH_THREAD_TOOL => {
            let arguments = parse_arguments::<SwitchThreadSchemaArguments>(arguments)?;
            let thread_id = bounded_non_empty_argument("threadId", arguments.thread_id)?;
            Ok(GuiControlToolRequest::SwitchThread(SwitchThreadArguments {
                thread_id,
            }))
        }
        SCROLL_TRANSCRIPT_TOOL => {
            let arguments = parse_arguments::<ScrollTranscriptSchemaArguments>(arguments)?;
            let delta_y = validate_scroll_transcript_delta(arguments.command, arguments.delta_y)?;
            Ok(GuiControlToolRequest::ScrollTranscript(
                ScrollTranscriptArguments {
                    command: arguments.command,
                    repeat: arguments.repeat.unwrap_or(1).clamp(1, MAX_SCROLL_REPEAT),
                    delta_y,
                    precise: arguments.precise.unwrap_or(false),
                },
            ))
        }
        CLOSE_POPUPS_TOOL => {
            parse_arguments::<EmptyArguments>(arguments)?;
            Ok(GuiControlToolRequest::ClosePopups)
        }
        other => Err(GuiControlToolError::UnsupportedTool {
            tool: other.to_string(),
        }),
    }
}

pub(crate) fn ui_state_tool_response(
    request: &DynamicToolCallRequest,
    snapshot: UiStateSnapshot,
) -> DynamicToolCallResponse {
    gui_control_success_response(request, json!(snapshot))
}

pub(crate) fn switch_thread_tool_response(
    request: &DynamicToolCallRequest,
    result: SwitchThreadResult,
) -> DynamicToolCallResponse {
    gui_control_success_response(request, json!(result))
}

pub(crate) fn scroll_transcript_tool_response(
    request: &DynamicToolCallRequest,
    result: ScrollTranscriptResult,
) -> DynamicToolCallResponse {
    gui_control_success_response(request, json!(result))
}

pub(crate) fn close_popups_tool_response(
    request: &DynamicToolCallRequest,
    result: ClosePopupsResult,
) -> DynamicToolCallResponse {
    gui_control_success_response(request, json!(result))
}

pub(crate) fn gui_control_failure_response(
    request: &DynamicToolCallRequest,
    kind: &'static str,
    message: impl Into<String>,
) -> DynamicToolCallResponse {
    DynamicToolCallResponse::failure_text(compact_json(json!({
        "ok": false,
        "error": {
            "kind": kind,
            "message": truncate_control_string(message),
            "tool": request.tool(),
            "callId": request.call_id(),
        },
    })))
}

pub(crate) fn bounded_control_string(value: impl Into<String>) -> String {
    truncate_control_string(value)
}

fn gui_control_success_response(
    request: &DynamicToolCallRequest,
    result: Value,
) -> DynamicToolCallResponse {
    let _ = request;
    DynamicToolCallResponse::success_text(compact_json(json!({
        "ok": true,
        "result": result,
    })))
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
struct SwitchThreadSchemaArguments {
    thread_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScrollTranscriptSchemaArguments {
    command: ScrollTranscriptCommand,
    repeat: Option<usize>,
    delta_y: Option<f32>,
    precise: Option<bool>,
}

impl LimitedReadArguments {
    fn validate_limit(&self, max: usize) -> Result<(), GuiControlToolError> {
        if self.limit.is_some_and(|limit| limit > max) {
            return Err(GuiControlToolError::InvalidArguments {
                detail: format!("limit must be at most {max}"),
            });
        }
        Ok(())
    }
}

fn validate_scroll_transcript_delta(
    command: ScrollTranscriptCommand,
    delta_y: Option<f32>,
) -> Result<Option<f32>, GuiControlToolError> {
    if command != ScrollTranscriptCommand::Wheel {
        return Ok(None);
    }
    let Some(delta_y) = delta_y else {
        return Err(GuiControlToolError::InvalidArguments {
            detail: "deltaY is required for wheel transcript scrolling".to_string(),
        });
    };
    if !delta_y.is_finite() {
        return Err(GuiControlToolError::InvalidArguments {
            detail: "deltaY must be finite".to_string(),
        });
    }
    if delta_y == 0.0 {
        return Err(GuiControlToolError::InvalidArguments {
            detail: "deltaY must not be zero".to_string(),
        });
    }
    if delta_y.abs() > MAX_SCROLL_WHEEL_DELTA_PX {
        return Err(GuiControlToolError::InvalidArguments {
            detail: format!(
                "deltaY must be between -{MAX_SCROLL_WHEEL_DELTA_PX} and {MAX_SCROLL_WHEEL_DELTA_PX}"
            ),
        });
    }
    Ok(Some(delta_y))
}

fn bounded_non_empty_argument(
    name: &'static str,
    value: String,
) -> Result<String, GuiControlToolError> {
    if value.trim().is_empty() {
        return Err(GuiControlToolError::InvalidArguments {
            detail: format!("{name} must not be empty"),
        });
    }
    if value.len() > MAX_CONTROL_STRING_BYTES {
        return Err(GuiControlToolError::InvalidArguments {
            detail: format!("{name} exceeds {MAX_CONTROL_STRING_BYTES} bytes"),
        });
    }
    Ok(value)
}

fn validate_namespace(request: &DynamicToolCallRequest) -> Result<(), GuiControlToolError> {
    if let Some(namespace) = request.namespace()
        && namespace != BERYL_DYNAMIC_TOOL_NAMESPACE
    {
        return Err(GuiControlToolError::UnsupportedNamespace {
            namespace: namespace.to_string(),
        });
    }
    Ok(())
}

fn parse_arguments<T>(arguments: &Value) -> Result<T, GuiControlToolError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(arguments.clone()).map_err(|source| {
        GuiControlToolError::InvalidArguments {
            detail: source.to_string(),
        }
    })
}

fn compact_json(value: Value) -> String {
    serde_json::to_string(&value).unwrap_or_else(|_| {
        "{\"ok\":false,\"error\":{\"kind\":\"internal\",\"message\":\"could not serialize dynamic tool response\"}}"
            .to_string()
    })
}

fn truncate_control_string(value: impl Into<String>) -> String {
    let mut value = value.into();
    if value.len() <= MAX_CONTROL_STRING_BYTES {
        return value;
    }
    let mut end = MAX_CONTROL_STRING_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value
}

#[derive(Debug)]
pub(crate) enum GuiControlToolError {
    UnsupportedNamespace { namespace: String },
    UnsupportedTool { tool: String },
    InvalidArguments { detail: String },
}

impl GuiControlToolError {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::UnsupportedNamespace { .. } => "unsupported_namespace",
            Self::UnsupportedTool { .. } => "unsupported_tool",
            Self::InvalidArguments { .. } => "invalid_arguments",
        }
    }
}

impl std::fmt::Display for GuiControlToolError {
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
                    "unsupported Beryl GUI control dynamic tool {tool:?}"
                )
            }
            Self::InvalidArguments { detail } => {
                write!(formatter, "invalid GUI control tool arguments: {detail}")
            }
        }
    }
}

impl std::error::Error for GuiControlToolError {}
