use std::path::Path;

use beryl_backend::{DynamicToolCallRequest, DynamicToolCallResponse};
use serde_json::{Value, json};

use crate::{GuiPreferences, ThemeRepositorySnapshot};

use super::{
    GuiSettingsUpdate, MAX_SETTINGS_TOOL_ERROR_BYTES, MAX_SETTINGS_TOOL_INSTALLED_THEME_COUNT,
    SettingsDynamicToolError,
};

pub fn gui_settings_snapshot_value(
    preferences: &GuiPreferences,
    themes: &ThemeRepositorySnapshot,
) -> Value {
    let installed_themes = themes
        .themes()
        .iter()
        .take(MAX_SETTINGS_TOOL_INSTALLED_THEME_COUNT)
        .map(|theme| {
            json!({
                "id": theme.id().as_str(),
                "name": theme.name(),
                "builtIn": theme.is_built_in(),
                "active": theme.is_active(),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "operations": {
            "contextCompactionTimeoutSeconds": preferences.operations.context_compaction_timeout_seconds,
        },
        "notifications": {
            "endTurnSoundPath": notification_path_metadata(
                preferences.notifications.end_turn_sound_path.as_deref()
            ),
        },
        "agent": {
            "developerInstructions": developer_instructions_metadata(
                preferences.agent.developer_instructions.as_deref()
            ),
        },
        "aiControl": {
            "available": false,
            "writable": false,
        },
        "appearance": {
            "activeThemeId": themes.active_theme_id().as_str(),
            "installedThemes": installed_themes,
            "installedThemeCount": themes.themes().len(),
            "installedThemesTruncated": themes.themes().len() > MAX_SETTINGS_TOOL_INSTALLED_THEME_COUNT,
        },
    })
}

pub fn settings_validation_value(
    current: &GuiPreferences,
    update: &GuiSettingsUpdate,
) -> Result<Value, SettingsDynamicToolError> {
    let updated = update.apply_to(current)?;
    Ok(json!({
        "valid": true,
        "changed": &updated != current,
        "resultingSettings": gui_preferences_summary_value(&updated),
    }))
}

pub fn settings_update_value(
    preferences: &GuiPreferences,
    changed: bool,
    save_pending: bool,
) -> Value {
    json!({
        "changed": changed,
        "savePending": save_pending,
        "settings": gui_preferences_summary_value(preferences),
    })
}

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

fn gui_preferences_summary_value(preferences: &GuiPreferences) -> Value {
    json!({
        "operations": {
            "contextCompactionTimeoutSeconds": preferences.operations.context_compaction_timeout_seconds,
        },
        "notifications": {
            "endTurnSoundPath": notification_path_metadata(
                preferences.notifications.end_turn_sound_path.as_deref()
            ),
        },
        "agent": {
            "developerInstructions": developer_instructions_metadata(
                preferences.agent.developer_instructions.as_deref()
            ),
        },
    })
}

fn notification_path_metadata(path: Option<&Path>) -> Value {
    let Some(path) = path else {
        return json!({
            "configured": false,
            "extension": Value::Null,
            "pathByteLength": 0,
        });
    };
    let display = path.display().to_string();
    json!({
        "configured": true,
        "extension": path.extension().and_then(|extension| extension.to_str()).map(str::to_ascii_lowercase),
        "pathByteLength": display.len(),
    })
}

fn developer_instructions_metadata(value: Option<&str>) -> Value {
    let Some(value) = value else {
        return json!({
            "enabled": false,
            "characterCount": 0,
            "lineCount": 0,
            "fingerprint": Value::Null,
        });
    };
    json!({
        "enabled": true,
        "characterCount": value.chars().count(),
        "lineCount": value.lines().count().max(1),
        "fingerprint": stable_fingerprint(value.as_bytes()),
    })
}

fn stable_fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
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
