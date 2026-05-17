use std::path::PathBuf;

use beryl_backend::DynamicToolCallRequest;
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};

use crate::{
    OperationPreferences, parse_context_compaction_timeout_seconds_text,
    parse_notification_sound_path_text,
};

use super::{
    AgentSettingsUpdate, GuiSettingsUpdate, MAX_SETTINGS_TOOL_STRING_BYTES,
    MAX_SETTINGS_TOOL_TIMEOUT_STRING_BYTES, NotificationSettingsUpdate, OperationSettingsUpdate,
    READ_GUI_SETTINGS_TOOL, SettingsDynamicToolError, SettingsDynamicToolRequest,
    UPDATE_GUI_SETTINGS_TOOL, VALIDATE_GUI_SETTINGS_UPDATE_TOOL,
};

use crate::dynamic_tools::BERYL_DYNAMIC_TOOL_NAMESPACE;

pub fn parse_beryl_settings_dynamic_tool_request(
    request: &DynamicToolCallRequest,
) -> Result<SettingsDynamicToolRequest, SettingsDynamicToolError> {
    validate_namespace(request)?;
    match request.tool() {
        READ_GUI_SETTINGS_TOOL => {
            parse_arguments::<EmptyArguments>(request.arguments())?;
            Ok(SettingsDynamicToolRequest::Read)
        }
        VALIDATE_GUI_SETTINGS_UPDATE_TOOL => {
            let update = parse_settings_update(request.arguments())?;
            Ok(SettingsDynamicToolRequest::Validate { update })
        }
        UPDATE_GUI_SETTINGS_TOOL => {
            let update = parse_settings_update(request.arguments())?;
            Ok(SettingsDynamicToolRequest::Update { update })
        }
        tool => Err(SettingsDynamicToolError::new(
            "unsupported_tool",
            format!("unsupported Beryl settings dynamic tool {tool:?}"),
        )),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArguments {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SettingsUpdateArguments {
    operations: Option<OperationSettingsUpdateArguments>,
    notifications: Option<NotificationSettingsUpdateArguments>,
    agent: Option<AgentSettingsUpdateArguments>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OperationSettingsUpdateArguments {
    #[serde(default, deserialize_with = "deserialize_setting_value_field")]
    context_compaction_timeout_seconds: SettingValueField,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NotificationSettingsUpdateArguments {
    #[serde(default, deserialize_with = "deserialize_setting_value_field")]
    end_turn_sound_path: SettingValueField,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentSettingsUpdateArguments {
    #[serde(default, deserialize_with = "deserialize_setting_value_field")]
    developer_instructions: SettingValueField,
}

#[derive(Debug)]
enum SettingValueField {
    Missing,
    Present(Value),
}

impl Default for SettingValueField {
    fn default() -> Self {
        Self::Missing
    }
}

fn validate_namespace(request: &DynamicToolCallRequest) -> Result<(), SettingsDynamicToolError> {
    if let Some(namespace) = request.namespace()
        && namespace != BERYL_DYNAMIC_TOOL_NAMESPACE
    {
        return Err(SettingsDynamicToolError::new(
            "unsupported_namespace",
            format!("unsupported Beryl dynamic tool namespace {namespace:?}"),
        ));
    }
    Ok(())
}

fn parse_settings_update(arguments: &Value) -> Result<GuiSettingsUpdate, SettingsDynamicToolError> {
    let arguments = parse_arguments::<SettingsUpdateArguments>(arguments)?;
    Ok(GuiSettingsUpdate {
        operations: arguments.operations.map(operation_update).transpose()?,
        notifications: arguments
            .notifications
            .map(notification_update)
            .transpose()?,
        agent: arguments.agent.map(agent_update).transpose()?,
    })
}

fn operation_update(
    arguments: OperationSettingsUpdateArguments,
) -> Result<OperationSettingsUpdate, SettingsDynamicToolError> {
    let context_compaction_timeout_seconds = match arguments.context_compaction_timeout_seconds {
        SettingValueField::Missing => None,
        SettingValueField::Present(value) => Some(parse_timeout_seconds_value(value)?),
    };
    Ok(OperationSettingsUpdate {
        context_compaction_timeout_seconds,
    })
}

fn notification_update(
    arguments: NotificationSettingsUpdateArguments,
) -> Result<NotificationSettingsUpdate, SettingsDynamicToolError> {
    let end_turn_sound_path = match arguments.end_turn_sound_path {
        SettingValueField::Missing => None,
        SettingValueField::Present(value) => Some(parse_notification_path_value(value)?),
    };
    Ok(NotificationSettingsUpdate {
        end_turn_sound_path,
    })
}

fn agent_update(
    arguments: AgentSettingsUpdateArguments,
) -> Result<AgentSettingsUpdate, SettingsDynamicToolError> {
    let developer_instructions = match arguments.developer_instructions {
        SettingValueField::Missing => None,
        SettingValueField::Present(value) => Some(parse_optional_bounded_string(value)?),
    };
    Ok(AgentSettingsUpdate {
        developer_instructions,
    })
}

fn deserialize_setting_value_field<'de, D>(deserializer: D) -> Result<SettingValueField, D::Error>
where
    D: Deserializer<'de>,
{
    Value::deserialize(deserializer).map(SettingValueField::Present)
}

fn parse_timeout_seconds_value(value: Value) -> Result<u64, SettingsDynamicToolError> {
    match value {
        Value::Number(number) if number.is_u64() => {
            let seconds = number.as_u64().expect("number reports u64");
            OperationPreferences::with_context_compaction_timeout_seconds(seconds)
                .map(|preferences| preferences.context_compaction_timeout_seconds)
                .map_err(|source| {
                    SettingsDynamicToolError::invalid_field(
                        "operations.contextCompactionTimeoutSeconds",
                        source.to_string(),
                    )
                })
        }
        Value::String(value) => {
            if value.len() > MAX_SETTINGS_TOOL_TIMEOUT_STRING_BYTES {
                return Err(SettingsDynamicToolError::invalid_field(
                    "operations.contextCompactionTimeoutSeconds",
                    format!("value exceeds {MAX_SETTINGS_TOOL_TIMEOUT_STRING_BYTES} bytes"),
                ));
            }
            parse_context_compaction_timeout_seconds_text(&value).map_err(|source| {
                SettingsDynamicToolError::invalid_field(
                    "operations.contextCompactionTimeoutSeconds",
                    source.to_string(),
                )
            })
        }
        _ => Err(SettingsDynamicToolError::invalid_field(
            "operations.contextCompactionTimeoutSeconds",
            "expected integer seconds",
        )),
    }
}

fn parse_notification_path_value(
    value: Value,
) -> Result<Option<PathBuf>, SettingsDynamicToolError> {
    match value {
        Value::Null => Ok(None),
        Value::String(value) => {
            if value.len() > MAX_SETTINGS_TOOL_STRING_BYTES {
                return Err(SettingsDynamicToolError::invalid_field(
                    "notifications.endTurnSoundPath",
                    format!("value exceeds {MAX_SETTINGS_TOOL_STRING_BYTES} bytes"),
                ));
            }
            parse_notification_sound_path_text(&value).map_err(|source| {
                SettingsDynamicToolError::invalid_field(
                    "notifications.endTurnSoundPath",
                    source.to_string(),
                )
            })
        }
        _ => Err(SettingsDynamicToolError::invalid_field(
            "notifications.endTurnSoundPath",
            "expected string path or null",
        )),
    }
}

fn parse_optional_bounded_string(value: Value) -> Result<Option<String>, SettingsDynamicToolError> {
    match value {
        Value::Null => Ok(None),
        Value::String(value) => {
            if value.len() > MAX_SETTINGS_TOOL_STRING_BYTES {
                return Err(SettingsDynamicToolError::invalid_field(
                    "agent.developerInstructions",
                    format!("value exceeds {MAX_SETTINGS_TOOL_STRING_BYTES} bytes"),
                ));
            }
            Ok(Some(value))
        }
        _ => Err(SettingsDynamicToolError::invalid_field(
            "agent.developerInstructions",
            "expected string or null",
        )),
    }
}

fn parse_arguments<T>(arguments: &Value) -> Result<T, SettingsDynamicToolError>
where
    T: for<'de> Deserialize<'de>,
{
    let arguments = if arguments.is_null() {
        json!({})
    } else {
        arguments.clone()
    };
    serde_json::from_value(arguments).map_err(|source| {
        SettingsDynamicToolError::invalid_arguments(format!(
            "invalid settings tool arguments: {source}"
        ))
    })
}
