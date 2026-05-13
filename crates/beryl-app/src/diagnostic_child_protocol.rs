#![allow(dead_code)]

use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

pub(crate) const READ_PROCESS_COMMAND: &str = "read_process";
pub(crate) const READ_MEMORY_COMMAND: &str = "read_memory";
pub(crate) const READ_RETAINED_STATE_COMMAND: &str = "read_retained_state";
pub(crate) const READ_VISIBLE_MEDIA_COMMAND: &str = "read_visible_media";
pub(crate) const READ_MEDIA_EVENTS_COMMAND: &str = "read_media_events";
pub(crate) const READ_UI_STATE_COMMAND: &str = "read_ui_state";
pub(crate) const LIST_WORKSPACE_THREADS_COMMAND: &str = "list_workspace_threads";
pub(crate) const CREATE_NEW_THREAD_COMMAND: &str = "create_new_thread";
pub(crate) const START_TURN_COMMAND: &str = "start_turn";
pub(crate) const SOFT_STOP_TURN_COMMAND: &str = "soft_stop_turn";
pub(crate) const HARD_STOP_TURN_COMMAND: &str = "hard_stop_turn";
pub(crate) const SWITCH_WORKSPACE_COMMAND: &str = "switch_workspace";
pub(crate) const SWITCH_THREAD_COMMAND: &str = "switch_thread";
pub(crate) const SCROLL_TRANSCRIPT_COMMAND: &str = "scroll_transcript";
pub(crate) const CLOSE_POPUPS_COMMAND: &str = "close_popups";

pub(crate) const MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES: usize = 256 * 1024;
const MAX_DIAGNOSTIC_PROTOCOL_MESSAGE_BYTES: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiagnosticProtocolRequest {
    id: String,
    command: DiagnosticChildCommand,
    params: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiagnosticChildCommand {
    ReadProcess,
    ReadMemory,
    ReadRetainedState,
    ReadVisibleMedia,
    ReadMediaEvents,
    ReadUiState,
    ListWorkspaceThreads,
    CreateNewThread,
    StartTurn,
    SoftStopTurn,
    HardStopTurn,
    SwitchWorkspace,
    SwitchThread,
    ScrollTranscript,
    ClosePopups,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDiagnosticProtocolRequest {
    id: String,
    command: String,
    #[serde(default = "empty_object")]
    params: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DiagnosticProtocolResponse {
    id: Option<String>,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<DiagnosticProtocolErrorBody>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DiagnosticProtocolErrorBody {
    kind: String,
    message: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum DiagnosticProtocolError {
    #[error("diagnostic protocol frame exceeded {limit} bytes")]
    FrameTooLarge { limit: usize },
    #[error("diagnostic protocol frame was not valid UTF-8: {message}")]
    InvalidUtf8 { message: String },
    #[error("diagnostic protocol frame was not valid JSON: {message}")]
    InvalidJson { message: String },
    #[error("diagnostic protocol request id must not be empty")]
    EmptyRequestId,
    #[error("unsupported diagnostic child command {command:?}")]
    UnsupportedCommand { command: String },
    #[error("diagnostic protocol response exceeded {limit} bytes")]
    ResponseTooLarge { limit: usize },
}

pub(crate) enum BoundedLineRead {
    Eof,
    Line(Vec<u8>),
    LineTooLong { prefix: Vec<u8> },
}

impl DiagnosticProtocolRequest {
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn command(&self) -> DiagnosticChildCommand {
        self.command
    }

    pub(crate) fn params(&self) -> &Value {
        &self.params
    }
}

impl DiagnosticChildCommand {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ReadProcess => READ_PROCESS_COMMAND,
            Self::ReadMemory => READ_MEMORY_COMMAND,
            Self::ReadRetainedState => READ_RETAINED_STATE_COMMAND,
            Self::ReadVisibleMedia => READ_VISIBLE_MEDIA_COMMAND,
            Self::ReadMediaEvents => READ_MEDIA_EVENTS_COMMAND,
            Self::ReadUiState => READ_UI_STATE_COMMAND,
            Self::ListWorkspaceThreads => LIST_WORKSPACE_THREADS_COMMAND,
            Self::CreateNewThread => CREATE_NEW_THREAD_COMMAND,
            Self::StartTurn => START_TURN_COMMAND,
            Self::SoftStopTurn => SOFT_STOP_TURN_COMMAND,
            Self::HardStopTurn => HARD_STOP_TURN_COMMAND,
            Self::SwitchWorkspace => SWITCH_WORKSPACE_COMMAND,
            Self::SwitchThread => SWITCH_THREAD_COMMAND,
            Self::ScrollTranscript => SCROLL_TRANSCRIPT_COMMAND,
            Self::ClosePopups => CLOSE_POPUPS_COMMAND,
        }
    }
}

impl TryFrom<&str> for DiagnosticChildCommand {
    type Error = DiagnosticProtocolError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            READ_PROCESS_COMMAND => Ok(Self::ReadProcess),
            READ_MEMORY_COMMAND => Ok(Self::ReadMemory),
            READ_RETAINED_STATE_COMMAND => Ok(Self::ReadRetainedState),
            READ_VISIBLE_MEDIA_COMMAND => Ok(Self::ReadVisibleMedia),
            READ_MEDIA_EVENTS_COMMAND => Ok(Self::ReadMediaEvents),
            READ_UI_STATE_COMMAND => Ok(Self::ReadUiState),
            LIST_WORKSPACE_THREADS_COMMAND => Ok(Self::ListWorkspaceThreads),
            CREATE_NEW_THREAD_COMMAND => Ok(Self::CreateNewThread),
            START_TURN_COMMAND => Ok(Self::StartTurn),
            SOFT_STOP_TURN_COMMAND => Ok(Self::SoftStopTurn),
            HARD_STOP_TURN_COMMAND => Ok(Self::HardStopTurn),
            SWITCH_WORKSPACE_COMMAND => Ok(Self::SwitchWorkspace),
            SWITCH_THREAD_COMMAND => Ok(Self::SwitchThread),
            SCROLL_TRANSCRIPT_COMMAND => Ok(Self::ScrollTranscript),
            CLOSE_POPUPS_COMMAND => Ok(Self::ClosePopups),
            command => Err(DiagnosticProtocolError::UnsupportedCommand {
                command: command.to_string(),
            }),
        }
    }
}

impl DiagnosticProtocolResponse {
    pub(crate) fn success(id: impl Into<String>, result: Value) -> Self {
        Self {
            id: Some(id.into()),
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub(crate) fn error(
        id: Option<String>,
        kind: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id,
            ok: false,
            result: None,
            error: Some(DiagnosticProtocolErrorBody {
                kind: kind.into(),
                message: truncate_protocol_message(message),
            }),
        }
    }

    pub(crate) fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub(crate) fn into_result(self) -> Result<Value, DiagnosticProtocolErrorBody> {
        if self.ok {
            Ok(self.result.unwrap_or(Value::Null))
        } else {
            Err(self.error.unwrap_or_else(|| DiagnosticProtocolErrorBody {
                kind: "remote_error".to_string(),
                message: "diagnostic child returned an error without details".to_string(),
            }))
        }
    }
}

impl DiagnosticProtocolErrorBody {
    pub(crate) fn kind(&self) -> &str {
        &self.kind
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

impl DiagnosticProtocolError {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::FrameTooLarge { .. } => "frame_too_large",
            Self::InvalidUtf8 { .. } => "invalid_utf8",
            Self::InvalidJson { .. } => "invalid_json",
            Self::EmptyRequestId => "invalid_request_id",
            Self::UnsupportedCommand { .. } => "unsupported_command",
            Self::ResponseTooLarge { .. } => "response_too_large",
        }
    }
}

pub(crate) fn parse_request_frame(
    frame: &[u8],
) -> Result<Option<DiagnosticProtocolRequest>, DiagnosticProtocolError> {
    let frame = trim_ascii_line_end(frame);
    if frame.is_empty() {
        return Ok(None);
    }
    let text =
        std::str::from_utf8(frame).map_err(|source| DiagnosticProtocolError::InvalidUtf8 {
            message: source.to_string(),
        })?;
    let raw = serde_json::from_str::<RawDiagnosticProtocolRequest>(text).map_err(|source| {
        DiagnosticProtocolError::InvalidJson {
            message: source.to_string(),
        }
    })?;
    if raw.id.trim().is_empty() {
        return Err(DiagnosticProtocolError::EmptyRequestId);
    }
    let command = DiagnosticChildCommand::try_from(raw.command.as_str())?;
    Ok(Some(DiagnosticProtocolRequest {
        id: raw.id,
        command,
        params: raw.params,
    }))
}

pub(crate) fn parse_response_frame(
    frame: &[u8],
) -> Result<Option<DiagnosticProtocolResponse>, DiagnosticProtocolError> {
    let frame = trim_ascii_line_end(frame);
    if frame.is_empty() {
        return Ok(None);
    }
    let text =
        std::str::from_utf8(frame).map_err(|source| DiagnosticProtocolError::InvalidUtf8 {
            message: source.to_string(),
        })?;
    serde_json::from_str::<DiagnosticProtocolResponse>(text)
        .map(Some)
        .map_err(|source| DiagnosticProtocolError::InvalidJson {
            message: source.to_string(),
        })
}

pub(crate) fn request_frame(
    id: &str,
    command: DiagnosticChildCommand,
    params: Value,
) -> Result<Vec<u8>, DiagnosticProtocolError> {
    let frame = json!({
        "id": id,
        "command": command.as_str(),
        "params": params,
    });
    serialize_frame(&frame)
}

pub(crate) fn write_response_frame(
    writer: &mut impl Write,
    response: DiagnosticProtocolResponse,
) -> io::Result<()> {
    let frame = response_frame(response);
    writer.write_all(&frame)?;
    writer.flush()
}

pub(crate) fn response_frame(mut response: DiagnosticProtocolResponse) -> Vec<u8> {
    match serialize_frame(&response) {
        Ok(frame) => frame,
        Err(error) => {
            response = DiagnosticProtocolResponse::error(
                response.id.take(),
                error.kind(),
                error.to_string(),
            );
            serialize_frame(&response).unwrap_or_else(|_| {
                b"{\"id\":null,\"ok\":false,\"error\":{\"kind\":\"internal\",\"message\":\"could not serialize diagnostic protocol response\"}}\n".to_vec()
            })
        }
    }
}

pub(crate) fn read_bounded_line_bytes(
    reader: &mut impl BufRead,
    limit: usize,
) -> io::Result<BoundedLineRead> {
    let mut line = Vec::new();
    let mut over_limit = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if line.is_empty() && !over_limit {
                Ok(BoundedLineRead::Eof)
            } else if over_limit {
                Ok(BoundedLineRead::LineTooLong { prefix: line })
            } else {
                Ok(BoundedLineRead::Line(line))
            };
        }

        let newline_index = available.iter().position(|byte| *byte == b'\n');
        let take = newline_index.map_or(available.len(), |index| index + 1);
        if !over_limit {
            let remaining_budget = limit.saturating_sub(line.len());
            if take > remaining_budget {
                line.extend_from_slice(&available[..remaining_budget]);
                over_limit = true;
            } else {
                line.extend_from_slice(&available[..take]);
            }
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

fn serialize_frame<T: Serialize>(value: &T) -> Result<Vec<u8>, DiagnosticProtocolError> {
    let mut frame =
        serde_json::to_vec(value).map_err(|source| DiagnosticProtocolError::InvalidJson {
            message: source.to_string(),
        })?;
    if frame.len().saturating_add(1) > MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES {
        return Err(DiagnosticProtocolError::ResponseTooLarge {
            limit: MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES,
        });
    }
    frame.push(b'\n');
    Ok(frame)
}

fn empty_object() -> Value {
    json!({})
}

fn trim_ascii_line_end(mut frame: &[u8]) -> &[u8] {
    while frame
        .last()
        .is_some_and(|byte| *byte == b'\n' || *byte == b'\r')
    {
        frame = &frame[..frame.len() - 1];
    }
    frame
}

fn truncate_protocol_message(value: impl Into<String>) -> String {
    let mut value = value.into();
    if value.len() <= MAX_DIAGNOSTIC_PROTOCOL_MESSAGE_BYTES {
        return value;
    }
    let mut end = MAX_DIAGNOSTIC_PROTOCOL_MESSAGE_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value
}
