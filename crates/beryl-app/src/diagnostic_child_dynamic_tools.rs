use std::{path::PathBuf, time::Duration};

use beryl_backend::{DynamicToolCallRequest, DynamicToolCallResponse, DynamicToolSpec};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    BerylHomeDir,
    diagnostic_child_protocol::DiagnosticChildCommand,
    diagnostic_child_supervisor::{
        DIAGNOSTIC_CHILD_STOP_RESPONSE_TIMEOUT, DiagnosticChildIdentity,
        DiagnosticChildStartOutcome, DiagnosticChildStatus, DiagnosticChildStopOutcome,
        DiagnosticChildSupervisor, DiagnosticChildSupervisorError,
    },
    gui_control_dynamic_tools::{
        CLOSE_POPUPS_TOOL, DEFAULT_UI_VISIBLE_ROW_LIMIT, GuiControlToolRequest, MAX_SCROLL_REPEAT,
        MAX_UI_VISIBLE_ROW_LIMIT, READ_UI_STATE_TOOL, SCROLL_TRANSCRIPT_TOOL, SWITCH_THREAD_TOOL,
        SWITCH_WORKSPACE_TOOL, parse_gui_control_tool_request,
    },
};

pub const BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE: &str = "beryl_diagnostic";

pub const DIAGNOSTIC_CHILD_START_TOOL: &str = "start";
pub const DIAGNOSTIC_CHILD_STOP_TOOL: &str = "stop";
pub const DIAGNOSTIC_CHILD_STATUS_TOOL: &str = "status";
pub const DIAGNOSTIC_CHILD_READ_PROCESS_TOOL: &str = "read_process";
pub const DIAGNOSTIC_CHILD_READ_MEMORY_TOOL: &str = "read_memory";
pub const DIAGNOSTIC_CHILD_READ_UI_STATE_TOOL: &str = "read_ui_state";
pub const DIAGNOSTIC_CHILD_READ_RETAINED_STATE_TOOL: &str = "read_retained_state";
pub const DIAGNOSTIC_CHILD_READ_VISIBLE_MEDIA_TOOL: &str = "read_visible_media";
pub const DIAGNOSTIC_CHILD_READ_MEDIA_EVENTS_TOOL: &str = "read_media_events";
pub const DIAGNOSTIC_CHILD_SWITCH_WORKSPACE_TOOL: &str = "switch_workspace";
pub const DIAGNOSTIC_CHILD_SWITCH_THREAD_TOOL: &str = "switch_thread";
pub const DIAGNOSTIC_CHILD_SCROLL_TRANSCRIPT_TOOL: &str = "scroll_transcript";
pub const DIAGNOSTIC_CHILD_CLOSE_POPUPS_TOOL: &str = "close_popups";

const DIAGNOSTIC_CHILD_REQUEST_TIMEOUT: Duration = Duration::from_millis(1500);
const MAX_DIAGNOSTIC_CHILD_STRING_BYTES: usize = 1024;
const DEFAULT_CHILD_VISIBLE_MEDIA_LIMIT: usize = 32;
const MAX_CHILD_VISIBLE_MEDIA_LIMIT: usize = 64;
const DEFAULT_CHILD_MEDIA_EVENT_LIMIT: usize = 64;
const MAX_CHILD_MEDIA_EVENT_LIMIT: usize = 128;

pub fn beryl_diagnostic_child_dynamic_tool_specs() -> Vec<DynamicToolSpec> {
    vec![
        diagnostic_child_tool_spec(
            DIAGNOSTIC_CHILD_START_TOOL,
            "Start one isolated diagnostic child Beryl process with an explicit Beryl home directory.",
            start_schema(),
        ),
        diagnostic_child_tool_spec(
            DIAGNOSTIC_CHILD_STOP_TOOL,
            "Stop the running diagnostic child Beryl process, if any.",
            empty_object_schema(),
        ),
        diagnostic_child_tool_spec(
            DIAGNOSTIC_CHILD_STATUS_TOOL,
            "Read diagnostic child process lifecycle status.",
            empty_object_schema(),
        ),
        diagnostic_child_tool_spec(
            DIAGNOSTIC_CHILD_READ_PROCESS_TOOL,
            "Read a bounded process identity snapshot from the diagnostic child Beryl.",
            empty_object_schema(),
        ),
        diagnostic_child_tool_spec(
            DIAGNOSTIC_CHILD_READ_MEMORY_TOOL,
            "Read bounded process memory counters from the diagnostic child Beryl.",
            empty_object_schema(),
        ),
        diagnostic_child_tool_spec(
            DIAGNOSTIC_CHILD_READ_UI_STATE_TOOL,
            "Read bounded selected workspace, thread, transcript, popup, and background-work UI state from the diagnostic child Beryl.",
            limited_read_schema(MAX_UI_VISIBLE_ROW_LIMIT, DEFAULT_UI_VISIBLE_ROW_LIMIT),
        ),
        diagnostic_child_tool_spec(
            DIAGNOSTIC_CHILD_READ_RETAINED_STATE_TOOL,
            "Read bounded retained-state counters from the diagnostic child Beryl.",
            empty_object_schema(),
        ),
        diagnostic_child_tool_spec(
            DIAGNOSTIC_CHILD_READ_VISIBLE_MEDIA_TOOL,
            "Read bounded metadata for media currently retained in the diagnostic child's visible transcript projection.",
            limited_read_schema(
                MAX_CHILD_VISIBLE_MEDIA_LIMIT,
                DEFAULT_CHILD_VISIBLE_MEDIA_LIMIT,
            ),
        ),
        diagnostic_child_tool_spec(
            DIAGNOSTIC_CHILD_READ_MEDIA_EVENTS_TOOL,
            "Read a bounded metadata-only ring of recent transcript media lifecycle events from the diagnostic child Beryl.",
            media_events_schema(),
        ),
        diagnostic_child_tool_spec(
            DIAGNOSTIC_CHILD_SWITCH_WORKSPACE_TOOL,
            "Switch the diagnostic child Beryl to an exact child-known workspace id through the ordinary workspace activation path.",
            switch_workspace_schema(),
        ),
        diagnostic_child_tool_spec(
            DIAGNOSTIC_CHILD_SWITCH_THREAD_TOOL,
            "Switch the diagnostic child Beryl to an exact child-known backend thread id through the ordinary thread activation path.",
            switch_thread_schema(),
        ),
        diagnostic_child_tool_spec(
            DIAGNOSTIC_CHILD_SCROLL_TRANSCRIPT_TOOL,
            "Scroll the diagnostic child transcript by a bounded command.",
            scroll_transcript_schema(),
        ),
        diagnostic_child_tool_spec(
            DIAGNOSTIC_CHILD_CLOSE_POPUPS_TOOL,
            "Close transient popups in the diagnostic child Beryl.",
            empty_object_schema(),
        ),
    ]
}

pub fn is_beryl_diagnostic_child_dynamic_tool(request: &DynamicToolCallRequest) -> bool {
    request.namespace() == Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE)
        && matches!(
            request.tool(),
            DIAGNOSTIC_CHILD_START_TOOL
                | DIAGNOSTIC_CHILD_STOP_TOOL
                | DIAGNOSTIC_CHILD_STATUS_TOOL
                | DIAGNOSTIC_CHILD_READ_PROCESS_TOOL
                | DIAGNOSTIC_CHILD_READ_MEMORY_TOOL
                | DIAGNOSTIC_CHILD_READ_UI_STATE_TOOL
                | DIAGNOSTIC_CHILD_READ_RETAINED_STATE_TOOL
                | DIAGNOSTIC_CHILD_READ_VISIBLE_MEDIA_TOOL
                | DIAGNOSTIC_CHILD_READ_MEDIA_EVENTS_TOOL
                | DIAGNOSTIC_CHILD_SWITCH_WORKSPACE_TOOL
                | DIAGNOSTIC_CHILD_SWITCH_THREAD_TOOL
                | DIAGNOSTIC_CHILD_SCROLL_TRANSCRIPT_TOOL
                | DIAGNOSTIC_CHILD_CLOSE_POPUPS_TOOL
        )
}

pub fn beryl_diagnostic_child_dynamic_tool_shell_response_timeout(
    request: &DynamicToolCallRequest,
    default_timeout: Duration,
) -> Duration {
    if request.namespace() == Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE)
        && request.tool() == DIAGNOSTIC_CHILD_STOP_TOOL
        && default_timeout < DIAGNOSTIC_CHILD_STOP_RESPONSE_TIMEOUT
    {
        return DIAGNOSTIC_CHILD_STOP_RESPONSE_TIMEOUT;
    }
    default_timeout
}

pub(crate) fn dispatch_beryl_diagnostic_child_dynamic_tool_call(
    supervisor: &mut DiagnosticChildSupervisor,
    supervisor_home: &BerylHomeDir,
    request: &DynamicToolCallRequest,
) -> DynamicToolCallResponse {
    match diagnostic_child_tool_result(supervisor, supervisor_home, request) {
        Ok(value) => diagnostic_child_success_response(value),
        Err(error) => diagnostic_child_failure_response(request, error.kind, error.message),
    }
}

pub(crate) fn diagnostic_child_failure_response(
    request: &DynamicToolCallRequest,
    kind: impl Into<String>,
    message: impl Into<String>,
) -> DynamicToolCallResponse {
    DynamicToolCallResponse::failure_text(compact_json(json!({
        "ok": false,
        "error": {
            "kind": kind.into(),
            "message": bounded_child_string(message),
            "tool": request.tool(),
            "callId": request.call_id(),
        },
    })))
}

fn diagnostic_child_tool_spec(
    name: &str,
    description: &str,
    input_schema: Value,
) -> DynamicToolSpec {
    DynamicToolSpec::new(name, description, input_schema)
        .with_namespace(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE)
        .with_defer_loading(false)
}

fn diagnostic_child_tool_result(
    supervisor: &mut DiagnosticChildSupervisor,
    supervisor_home: &BerylHomeDir,
    request: &DynamicToolCallRequest,
) -> Result<Value, DiagnosticChildDynamicToolError> {
    validate_namespace(request)?;
    match request.tool() {
        DIAGNOSTIC_CHILD_START_TOOL => {
            let arguments = parse_arguments::<StartArguments>(request.arguments())?;
            let child_home = bounded_non_empty_argument("berylHomeDir", arguments.beryl_home_dir)?;
            let outcome = supervisor
                .start(supervisor_home, PathBuf::from(child_home))
                .map_err(map_supervisor_error)?;
            Ok(start_outcome_result(outcome))
        }
        DIAGNOSTIC_CHILD_STOP_TOOL => {
            parse_arguments::<EmptyArguments>(request.arguments())?;
            let outcome = supervisor.stop().map_err(map_supervisor_error)?;
            Ok(stop_outcome_result(outcome))
        }
        DIAGNOSTIC_CHILD_STATUS_TOOL => {
            parse_arguments::<EmptyArguments>(request.arguments())?;
            let status = supervisor.status().map_err(map_supervisor_error)?;
            Ok(status_result(status))
        }
        DIAGNOSTIC_CHILD_READ_PROCESS_TOOL
        | DIAGNOSTIC_CHILD_READ_MEMORY_TOOL
        | DIAGNOSTIC_CHILD_READ_UI_STATE_TOOL
        | DIAGNOSTIC_CHILD_READ_RETAINED_STATE_TOOL
        | DIAGNOSTIC_CHILD_READ_VISIBLE_MEDIA_TOOL
        | DIAGNOSTIC_CHILD_READ_MEDIA_EVENTS_TOOL
        | DIAGNOSTIC_CHILD_SWITCH_WORKSPACE_TOOL
        | DIAGNOSTIC_CHILD_SWITCH_THREAD_TOOL
        | DIAGNOSTIC_CHILD_SCROLL_TRANSCRIPT_TOOL
        | DIAGNOSTIC_CHILD_CLOSE_POPUPS_TOOL => {
            ensure_child_running(supervisor)?;
            let (command, params) = child_command_and_params(request)?;
            supervisor
                .request(command, params, DIAGNOSTIC_CHILD_REQUEST_TIMEOUT)
                .map_err(map_supervisor_error)
        }
        other => Err(DiagnosticChildDynamicToolError::new(
            "unsupported_tool",
            format!("unsupported Beryl diagnostic child dynamic tool {other:?}"),
        )),
    }
}

fn child_command_and_params(
    request: &DynamicToolCallRequest,
) -> Result<(DiagnosticChildCommand, Value), DiagnosticChildDynamicToolError> {
    match request.tool() {
        DIAGNOSTIC_CHILD_READ_PROCESS_TOOL => {
            parse_arguments::<EmptyArguments>(request.arguments())?;
            Ok((DiagnosticChildCommand::ReadProcess, json!({})))
        }
        DIAGNOSTIC_CHILD_READ_MEMORY_TOOL => {
            parse_arguments::<EmptyArguments>(request.arguments())?;
            Ok((DiagnosticChildCommand::ReadMemory, json!({})))
        }
        DIAGNOSTIC_CHILD_READ_RETAINED_STATE_TOOL => {
            parse_arguments::<EmptyArguments>(request.arguments())?;
            Ok((DiagnosticChildCommand::ReadRetainedState, json!({})))
        }
        DIAGNOSTIC_CHILD_READ_VISIBLE_MEDIA_TOOL => Ok((
            DiagnosticChildCommand::ReadVisibleMedia,
            limited_read_params(
                request.arguments(),
                DEFAULT_CHILD_VISIBLE_MEDIA_LIMIT,
                MAX_CHILD_VISIBLE_MEDIA_LIMIT,
            )?,
        )),
        DIAGNOSTIC_CHILD_READ_MEDIA_EVENTS_TOOL => Ok((
            DiagnosticChildCommand::ReadMediaEvents,
            media_events_params(request.arguments())?,
        )),
        DIAGNOSTIC_CHILD_READ_UI_STATE_TOOL => gui_control_child_command(
            DiagnosticChildCommand::ReadUiState,
            READ_UI_STATE_TOOL,
            request.arguments(),
        ),
        DIAGNOSTIC_CHILD_SWITCH_WORKSPACE_TOOL => gui_control_child_command(
            DiagnosticChildCommand::SwitchWorkspace,
            SWITCH_WORKSPACE_TOOL,
            request.arguments(),
        ),
        DIAGNOSTIC_CHILD_SWITCH_THREAD_TOOL => gui_control_child_command(
            DiagnosticChildCommand::SwitchThread,
            SWITCH_THREAD_TOOL,
            request.arguments(),
        ),
        DIAGNOSTIC_CHILD_SCROLL_TRANSCRIPT_TOOL => gui_control_child_command(
            DiagnosticChildCommand::ScrollTranscript,
            SCROLL_TRANSCRIPT_TOOL,
            request.arguments(),
        ),
        DIAGNOSTIC_CHILD_CLOSE_POPUPS_TOOL => gui_control_child_command(
            DiagnosticChildCommand::ClosePopups,
            CLOSE_POPUPS_TOOL,
            request.arguments(),
        ),
        other => Err(DiagnosticChildDynamicToolError::new(
            "unsupported_tool",
            format!("unsupported Beryl diagnostic child dynamic tool {other:?}"),
        )),
    }
}

fn gui_control_child_command(
    command: DiagnosticChildCommand,
    tool: &str,
    arguments: &Value,
) -> Result<(DiagnosticChildCommand, Value), DiagnosticChildDynamicToolError> {
    let parsed = parse_gui_control_tool_request(tool, arguments)
        .map_err(|error| DiagnosticChildDynamicToolError::new(error.kind(), error.to_string()))?;
    let params = match parsed {
        GuiControlToolRequest::ReadUiState { visible_row_limit } => {
            json!({ "limit": visible_row_limit })
        }
        GuiControlToolRequest::SwitchWorkspace(arguments) => {
            json!({ "workspaceId": arguments.workspace_id })
        }
        GuiControlToolRequest::SwitchThread(arguments) => {
            json!({ "threadId": arguments.thread_id })
        }
        GuiControlToolRequest::ScrollTranscript(arguments) => {
            json!({ "command": arguments.command, "repeat": arguments.repeat })
        }
        GuiControlToolRequest::ClosePopups => json!({}),
    };
    Ok((command, params))
}

fn ensure_child_running(
    supervisor: &mut DiagnosticChildSupervisor,
) -> Result<(), DiagnosticChildDynamicToolError> {
    match supervisor.status().map_err(map_supervisor_error)? {
        DiagnosticChildStatus::Running(_) => Ok(()),
        DiagnosticChildStatus::NotRunning => Err(DiagnosticChildDynamicToolError::not_running()),
    }
}

fn start_outcome_result(outcome: DiagnosticChildStartOutcome) -> Value {
    match outcome {
        DiagnosticChildStartOutcome::Started(identity) => json!({
            "status": "started",
            "child": identity_result(identity),
        }),
        DiagnosticChildStartOutcome::AlreadyRunning(identity) => json!({
            "status": "already_running",
            "child": identity_result(identity),
        }),
    }
}

fn stop_outcome_result(outcome: DiagnosticChildStopOutcome) -> Value {
    match outcome {
        DiagnosticChildStopOutcome::Stopped(identity) => json!({
            "status": "stopped",
            "child": identity_result(identity),
        }),
        DiagnosticChildStopOutcome::NotRunning => json!({
            "status": "not_running",
        }),
    }
}

fn status_result(status: DiagnosticChildStatus) -> Value {
    match status {
        DiagnosticChildStatus::Running(identity) => json!({
            "status": "running",
            "child": identity_result(identity),
        }),
        DiagnosticChildStatus::NotRunning => json!({
            "status": "not_running",
        }),
    }
}

fn identity_result(identity: DiagnosticChildIdentity) -> Value {
    json!({
        "pid": identity.pid,
        "home": bounded_child_string(identity.home_dir.display().to_string()),
        "executablePath": bounded_child_string(identity.executable_path.display().to_string()),
    })
}

fn limited_read_params(
    arguments: &Value,
    default: usize,
    max: usize,
) -> Result<Value, DiagnosticChildDynamicToolError> {
    let arguments = parse_arguments::<LimitedReadArguments>(arguments)?;
    Ok(json!({
        "limit": arguments.limit.unwrap_or(default).min(max),
    }))
}

fn media_events_params(arguments: &Value) -> Result<Value, DiagnosticChildDynamicToolError> {
    let arguments = parse_arguments::<MediaEventsArguments>(arguments)?;
    let mut params = json!({
        "limit": arguments
            .limit
            .unwrap_or(DEFAULT_CHILD_MEDIA_EVENT_LIMIT)
            .min(MAX_CHILD_MEDIA_EVENT_LIMIT),
    });
    if let Some(after_sequence) = arguments.after_sequence {
        params["afterSequence"] = json!(after_sequence);
    }
    Ok(params)
}

fn validate_namespace(
    request: &DynamicToolCallRequest,
) -> Result<(), DiagnosticChildDynamicToolError> {
    if request.namespace() != Some(BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE) {
        return Err(DiagnosticChildDynamicToolError::new(
            "unsupported_namespace",
            format!(
                "unsupported Beryl diagnostic child dynamic tool namespace {:?}",
                request.namespace().unwrap_or("<none>")
            ),
        ));
    }
    Ok(())
}

fn parse_arguments<T>(arguments: &Value) -> Result<T, DiagnosticChildDynamicToolError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(arguments.clone()).map_err(|source| {
        DiagnosticChildDynamicToolError::new(
            "invalid_arguments",
            format!("invalid diagnostic child tool arguments: {source}"),
        )
    })
}

fn bounded_non_empty_argument(
    name: &'static str,
    value: String,
) -> Result<String, DiagnosticChildDynamicToolError> {
    if value.trim().is_empty() {
        return Err(DiagnosticChildDynamicToolError::new(
            "invalid_arguments",
            format!("{name} must not be empty"),
        ));
    }
    if value.len() > MAX_DIAGNOSTIC_CHILD_STRING_BYTES {
        return Err(DiagnosticChildDynamicToolError::new(
            "invalid_arguments",
            format!("{name} exceeds {MAX_DIAGNOSTIC_CHILD_STRING_BYTES} bytes"),
        ));
    }
    Ok(value)
}

fn map_supervisor_error(error: DiagnosticChildSupervisorError) -> DiagnosticChildDynamicToolError {
    let message = error.to_string();
    match error {
        DiagnosticChildSupervisorError::BerylHomeDir(_)
        | DiagnosticChildSupervisorError::HomeCollidesWithSupervisor { .. } => {
            DiagnosticChildDynamicToolError::new("invalid_arguments", message)
        }
        DiagnosticChildSupervisorError::ProtocolEof => {
            DiagnosticChildDynamicToolError::not_running()
        }
        DiagnosticChildSupervisorError::RequestTimeout { .. } => {
            DiagnosticChildDynamicToolError::new("diagnostic_child_timeout", message)
        }
        DiagnosticChildSupervisorError::ChildError { kind, message } => {
            DiagnosticChildDynamicToolError::new(kind, message)
        }
        DiagnosticChildSupervisorError::Protocol(protocol_error) => {
            DiagnosticChildDynamicToolError::new(protocol_error.kind(), message)
        }
        _ => DiagnosticChildDynamicToolError::new("diagnostic_child_lifecycle_error", message),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArguments {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartArguments {
    beryl_home_dir: String,
}

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

#[derive(Debug)]
struct DiagnosticChildDynamicToolError {
    kind: String,
    message: String,
}

impl DiagnosticChildDynamicToolError {
    fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: bounded_child_string(message),
        }
    }

    fn not_running() -> Self {
        Self::new(
            "diagnostic_child_not_running",
            "The diagnostic child Beryl process is not running.",
        )
    }
}

fn diagnostic_child_success_response(result: Value) -> DynamicToolCallResponse {
    DynamicToolCallResponse::success_text(compact_json(json!({
        "ok": true,
        "result": result,
    })))
}

fn empty_object_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

fn start_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "berylHomeDir": {
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_DIAGNOSTIC_CHILD_STRING_BYTES,
                "description": "Explicit isolated Beryl home directory for the diagnostic child."
            }
        },
        "required": ["berylHomeDir"],
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
                "maximum": MAX_CHILD_MEDIA_EVENT_LIMIT,
                "default": DEFAULT_CHILD_MEDIA_EVENT_LIMIT
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

fn switch_workspace_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "workspaceId": {
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_DIAGNOSTIC_CHILD_STRING_BYTES,
                "description": "Exact child-known Beryl workspace id."
            }
        },
        "required": ["workspaceId"],
        "additionalProperties": false
    })
}

fn switch_thread_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "threadId": {
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_DIAGNOSTIC_CHILD_STRING_BYTES,
                "description": "Exact child-known backend thread id."
            }
        },
        "required": ["threadId"],
        "additionalProperties": false
    })
}

fn scroll_transcript_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "command": {
                "type": "string",
                "enum": ["top", "bottom", "page_up", "page_down"]
            },
            "repeat": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_SCROLL_REPEAT,
                "default": 1
            }
        },
        "required": ["command"],
        "additionalProperties": false
    })
}

fn compact_json(value: Value) -> String {
    serde_json::to_string(&value).unwrap_or_else(|_| {
        "{\"ok\":false,\"error\":{\"kind\":\"internal\",\"message\":\"could not serialize dynamic tool response\"}}"
            .to_string()
    })
}

fn bounded_child_string(value: impl Into<String>) -> String {
    let mut value = value.into();
    if value.len() <= MAX_DIAGNOSTIC_CHILD_STRING_BYTES {
        return value;
    }
    let mut end = MAX_DIAGNOSTIC_CHILD_STRING_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value
}
