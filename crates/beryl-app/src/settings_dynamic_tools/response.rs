use beryl_backend::{DynamicToolCallRequest, DynamicToolCallResponse};
use serde_json::{Value, json};

use super::MAX_SETTINGS_TOOL_ERROR_BYTES;

pub fn settings_tool_success_response(
    _request: &DynamicToolCallRequest,
    result: Value,
) -> DynamicToolCallResponse {
    DynamicToolCallResponse::success_text(compact_json(json!({
        "ok": true,
        "result": result,
    })))
}

pub fn settings_tool_failure_response(
    request: &DynamicToolCallRequest,
    kind: &'static str,
    message: impl Into<String>,
) -> DynamicToolCallResponse {
    DynamicToolCallResponse::failure_text(compact_json(json!({
        "ok": false,
        "error": {
            "kind": kind,
            "message": bounded_tool_string(message),
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

pub(super) fn bounded_tool_string(value: impl Into<String>) -> String {
    let mut value = value.into();
    if value.len() <= MAX_SETTINGS_TOOL_ERROR_BYTES {
        return value;
    }
    let mut end = MAX_SETTINGS_TOOL_ERROR_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value
}
