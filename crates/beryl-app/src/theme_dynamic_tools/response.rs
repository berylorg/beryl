use beryl_backend::{DynamicToolCallRequest, DynamicToolCallResponse};
use serde_json::{Value, json};

use crate::{ActiveThemeProjection, ThemeRepositorySnapshot};

use super::{MAX_THEME_SCHEMA_ROLE_LIMIT, MAX_THEME_TOOL_ERROR_BYTES, ThemeDynamicToolError};

pub fn theme_repository_value(
    snapshot: &ThemeRepositorySnapshot,
    _include_active_document: bool,
) -> Result<Value, ThemeDynamicToolError> {
    let themes: Vec<Value> = snapshot
        .themes()
        .iter()
        .take(MAX_THEME_SCHEMA_ROLE_LIMIT)
        .map(|theme| {
            json!({
                "id": theme.id().as_str(),
                "name": theme.name(),
                "builtIn": theme.is_built_in(),
            })
        })
        .collect();

    Ok(json!({
        "themes": themes,
        "themeCount": snapshot.themes().len(),
        "themesTruncated": snapshot.themes().len() > MAX_THEME_SCHEMA_ROLE_LIMIT,
        "activeDocument": Value::Null,
    }))
}

pub fn theme_preview_value(
    projection: &ActiveThemeProjection,
    name: Option<&str>,
    installed: bool,
) -> Value {
    json!({
        "previewActive": true,
        "installed": installed,
        "name": name,
        "styleRevision": projection.style_revision(),
    })
}

pub fn theme_mutation_value(snapshot: &ThemeRepositorySnapshot, changed: bool) -> Value {
    json!({
        "changed": changed,
        "themeCount": snapshot.themes().len(),
        "themes": snapshot.themes().iter().take(MAX_THEME_SCHEMA_ROLE_LIMIT).map(|theme| {
            json!({
                "id": theme.id().as_str(),
                "name": theme.name(),
                "builtIn": theme.is_built_in(),
            })
        }).collect::<Vec<_>>(),
        "themesTruncated": snapshot.themes().len() > MAX_THEME_SCHEMA_ROLE_LIMIT,
    })
}

pub fn theme_tool_success_response(
    _request: &DynamicToolCallRequest,
    result: Value,
) -> DynamicToolCallResponse {
    DynamicToolCallResponse::success_text(compact_json(json!({
        "ok": true,
        "result": result,
    })))
}

pub fn theme_tool_failure_response(
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

pub(super) fn compact_json(value: Value) -> String {
    serde_json::to_string(&value).unwrap_or_else(|_| {
        "{\"ok\":false,\"error\":{\"kind\":\"internal\",\"message\":\"could not serialize dynamic tool response\"}}"
            .to_string()
    })
}

pub(super) fn bounded_tool_string(value: impl Into<String>) -> String {
    bounded_tool_text(value.into(), MAX_THEME_TOOL_ERROR_BYTES)
}

fn bounded_tool_text(mut value: String, byte_limit: usize) -> String {
    if value.len() <= byte_limit {
        return value;
    }
    let mut end = byte_limit;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value
}
