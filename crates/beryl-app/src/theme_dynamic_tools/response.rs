use beryl_backend::{DynamicToolCallRequest, DynamicToolCallResponse};
use serde_json::{Value, json};

use crate::{ActiveThemeProjection, ThemeDocument, ThemeDocumentError, ThemeRepositorySnapshot};

use super::{
    MAX_THEME_ACTIVE_DOCUMENT_RESPONSE_BYTES, MAX_THEME_SCHEMA_ROLE_LIMIT,
    MAX_THEME_TOOL_ERROR_BYTES, ThemeDynamicToolError,
};

pub fn theme_repository_value(
    snapshot: &ThemeRepositorySnapshot,
    include_active_document: bool,
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
                "active": theme.is_active(),
            })
        })
        .collect();
    let active_document = if include_active_document {
        Some(active_theme_document_value(snapshot)?)
    } else {
        None
    };

    Ok(json!({
        "activeThemeId": snapshot.active_theme_id().as_str(),
        "themes": themes,
        "themeCount": snapshot.themes().len(),
        "themesTruncated": snapshot.themes().len() > MAX_THEME_SCHEMA_ROLE_LIMIT,
        "activeDocument": active_document,
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
        "activeThemeId": snapshot.active_theme_id().as_str(),
        "themeCount": snapshot.themes().len(),
        "styleRevision": snapshot.active_projection().style_revision(),
        "themes": snapshot.themes().iter().take(MAX_THEME_SCHEMA_ROLE_LIMIT).map(|theme| {
            json!({
                "id": theme.id().as_str(),
                "name": theme.name(),
                "builtIn": theme.is_built_in(),
                "active": theme.is_active(),
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

fn active_theme_document_value(
    snapshot: &ThemeRepositorySnapshot,
) -> Result<Value, ThemeDynamicToolError> {
    let name = snapshot
        .themes()
        .iter()
        .find(|theme| theme.is_active())
        .map(|theme| theme.name().to_string());
    let document_text = ThemeDocument::new(
        Some(snapshot.active_theme_id().clone()),
        name.clone(),
        snapshot.active_definition().clone(),
    )
    .and_then(|document| document.to_toml_string())
    .map_err(theme_document_error)?;
    let byte_length = document_text.len();
    let text = bounded_tool_text(document_text, MAX_THEME_ACTIVE_DOCUMENT_RESPONSE_BYTES);
    let retained_byte_length = text.len();

    Ok(json!({
        "themeId": snapshot.active_theme_id().as_str(),
        "name": name,
        "text": text,
        "byteLength": byte_length,
        "retainedByteLength": retained_byte_length,
        "omittedByteLength": byte_length.saturating_sub(retained_byte_length),
        "byteLimit": MAX_THEME_ACTIVE_DOCUMENT_RESPONSE_BYTES,
        "truncated": retained_byte_length < byte_length,
    }))
}

fn theme_document_error(error: ThemeDocumentError) -> ThemeDynamicToolError {
    ThemeDynamicToolError::new("invalid_theme_document", error.to_string())
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
