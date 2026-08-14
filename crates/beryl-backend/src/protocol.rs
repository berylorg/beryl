use std::path::PathBuf;

use beryl_model::workspace::RuntimeMode;
use serde::{Deserialize, Serialize, de};
use serde_json::Value;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolPhase {
    #[serde(rename = "commentary")]
    Commentary,
    #[serde(rename = "final_answer")]
    FinalAnswer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendEvent {
    AgentMessageDelta { phase: ProtocolPhase, delta: String },
    FileChanged { path: PathBuf },
    ProtocolError(JsonRpcError),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResponse {
    pub user_agent: String,
    pub codex_home: String,
    pub platform_family: String,
    pub platform_os: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompatibilityProbe {
    ConfigRead,
    ModelList,
    ThreadList,
    ThreadCompactStart,
    ThreadLoadedList,
    ThreadNameSet,
    ThreadRead,
    ThreadResumeMetadata,
    ThreadUnsubscribe,
    ThreadTurnsList,
    TurnInterrupt,
    TurnSteer,
}

impl CompatibilityProbe {
    pub fn method(self) -> &'static str {
        match self {
            Self::ConfigRead => "config/read",
            Self::ModelList => "model/list",
            Self::ThreadList => "thread/list",
            Self::ThreadCompactStart => "thread/compact/start",
            Self::ThreadLoadedList => "thread/loaded/list",
            Self::ThreadNameSet => "thread/name/set",
            Self::ThreadRead => "thread/read",
            Self::ThreadResumeMetadata => "thread/resume",
            Self::ThreadUnsubscribe => "thread/unsubscribe",
            Self::ThreadTurnsList => "thread/turns/list",
            Self::TurnInterrupt => "turn/interrupt",
            Self::TurnSteer => "turn/steer",
        }
    }
}

const REQUIRED_COMPATIBILITY_PROBES: &[CompatibilityProbe] = &[
    CompatibilityProbe::ConfigRead,
    CompatibilityProbe::ModelList,
    CompatibilityProbe::ThreadList,
    CompatibilityProbe::ThreadCompactStart,
    CompatibilityProbe::ThreadLoadedList,
    CompatibilityProbe::ThreadNameSet,
    CompatibilityProbe::ThreadRead,
    CompatibilityProbe::ThreadResumeMetadata,
    CompatibilityProbe::ThreadUnsubscribe,
    CompatibilityProbe::ThreadTurnsList,
    CompatibilityProbe::TurnInterrupt,
    CompatibilityProbe::TurnSteer,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompatibilitySnapshot {
    platform_family: String,
    platform_os: String,
    requires_method_probes: bool,
}

impl CompatibilitySnapshot {
    pub fn from_initialize_response(response: &InitializeResponse) -> Self {
        Self {
            platform_family: response.platform_family.clone(),
            platform_os: response.platform_os.clone(),
            requires_method_probes: true,
        }
    }

    pub fn platform_family(&self) -> &str {
        &self.platform_family
    }

    pub fn platform_os(&self) -> &str {
        &self.platform_os
    }

    pub fn requires_method_probes(&self) -> bool {
        self.requires_method_probes
    }

    pub fn required_method_probes(&self) -> &'static [CompatibilityProbe] {
        REQUIRED_COMPATIBILITY_PROBES
    }

    pub fn validate_runtime_mode(
        &self,
        runtime_mode: &RuntimeMode,
    ) -> Result<(), CompatibilityError> {
        let (expected_platform_family, expected_platform_os) = match runtime_mode {
            RuntimeMode::HostWindows => ("windows", "windows"),
            RuntimeMode::WslLinux { .. } => ("unix", "linux"),
        };

        if self.platform_family != expected_platform_family {
            return Err(CompatibilityError::PlatformFamilyMismatch {
                runtime_mode: runtime_mode.display_name(),
                expected_platform_family,
                actual_platform_family: self.platform_family.clone(),
            });
        }

        if self.platform_os != expected_platform_os {
            return Err(CompatibilityError::PlatformOsMismatch {
                runtime_mode: runtime_mode.display_name(),
                expected_platform_os,
                actual_platform_os: self.platform_os.clone(),
            });
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CompatibilityError {
    #[error(
        "runtime mode {runtime_mode} requires backend platform family {expected_platform_family}, got {actual_platform_family}"
    )]
    PlatformFamilyMismatch {
        runtime_mode: String,
        expected_platform_family: &'static str,
        actual_platform_family: String,
    },
    #[error(
        "runtime mode {runtime_mode} requires backend platform os {expected_platform_os}, got {actual_platform_os}"
    )]
    PlatformOsMismatch {
        runtime_mode: String,
        expected_platform_os: &'static str,
        actual_platform_os: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadListResponse {
    pub data: Vec<ThreadSummary>,
    #[serde(default)]
    pub next_cursor: Option<String>,
    #[serde(default)]
    pub backwards_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelListResponse {
    pub data: Vec<ModelInfo>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct ConfigReadResponse {
    #[serde(default)]
    pub config: BackendConfigDefaults,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct BackendConfigDefaults {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default, alias = "modelReasoningEffort")]
    pub model_reasoning_effort: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub model: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default, deserialize_with = "deserialize_supported_reasoning_efforts")]
    pub supported_reasoning_efforts: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_default_reasoning_effort")]
    pub default_reasoning_effort: Option<String>,
    #[serde(default)]
    pub input_modalities: Vec<String>,
    #[serde(default)]
    pub supports_personality: bool,
    #[serde(default)]
    pub is_default: bool,
}

fn deserialize_supported_reasoning_efforts<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    supported_reasoning_efforts_from_value(value).map_err(de::Error::custom)
}

fn deserialize_default_reasoning_effort<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::Null => Ok(None),
        value => reasoning_effort_from_value(value)
            .map(Some)
            .map_err(de::Error::custom),
    }
}

fn supported_reasoning_efforts_from_value(value: Value) -> Result<Vec<String>, String> {
    match value {
        Value::Array(items) => items
            .into_iter()
            .map(reasoning_effort_from_value)
            .collect::<Result<Vec<_>, _>>(),
        Value::Object(map) => Ok(map
            .into_iter()
            .filter_map(|(effort, value)| {
                non_empty_string(effort).or_else(|| reasoning_effort_from_object_value(&value))
            })
            .collect()),
        Value::Null => Ok(Vec::new()),
        other => Err(format!(
            "supportedReasoningEfforts must be an array or object, got {other}"
        )),
    }
}

fn reasoning_effort_from_value(value: Value) -> Result<String, String> {
    non_empty_json_string(&value)
        .or_else(|| reasoning_effort_from_object_value(&value))
        .ok_or_else(|| {
            format!("reasoning effort entry must include a non-empty effort, got {value}")
        })
}

fn reasoning_effort_from_object_value(value: &Value) -> Option<String> {
    let Value::Object(map) = value else {
        return None;
    };
    map.get("reasoningEffort")
        .and_then(non_empty_json_string)
        .or_else(|| map.get("effort").and_then(non_empty_json_string))
        .or_else(|| map.get("id").and_then(non_empty_json_string))
        .or_else(|| map.get("name").and_then(non_empty_json_string))
}

fn non_empty_json_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .and_then(|value| non_empty_string(value.to_string()))
}

fn non_empty_string(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(non_empty_string)
}

fn subagent_source_agent_nickname(source: Option<&Value>) -> Option<String> {
    let source = source?;
    let subagent = json_field(source, &["subAgent", "subagent"])?;
    let thread_spawn = json_field(subagent, &["thread_spawn", "threadSpawn"])?;
    json_string_field(
        thread_spawn,
        &["agent_nickname", "agentNickname", "nickname"],
    )
}

fn json_field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Value> {
    let Value::Object(object) = value else {
        return None;
    };
    names.iter().find_map(|name| object.get(*name))
}

fn json_string_field(value: &Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name))
        .and_then(non_empty_json_string)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelListOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "is_false")]
    pub include_hidden: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigReadOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(skip_serializing_if = "is_false")]
    pub include_layers: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadListOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cwd: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_key: Option<ThreadSortKey>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_direction: Option<SortDirection>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadSortKey {
    CreatedAt,
    UpdatedAt,
}

impl ThreadListOptions {
    pub fn page(limit: u32) -> Self {
        Self {
            limit: Some(limit),
            ..Self::default()
        }
    }

    pub fn with_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }

    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd.push(cwd.into());
        self
    }

    pub fn with_cwds<I, P>(mut self, cwds: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.cwd.extend(cwds.into_iter().map(Into::into));
        self
    }

    pub fn updated_descending(mut self) -> Self {
        self.sort_key = Some(ThreadSortKey::UpdatedAt);
        self.sort_direction = Some(SortDirection::Desc);
        self
    }
}

impl ModelListOptions {
    pub fn page(limit: u32) -> Self {
        Self {
            limit: Some(limit),
            ..Self::default()
        }
    }

    pub fn with_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }

    pub fn include_hidden(mut self) -> Self {
        self.include_hidden = true;
        self
    }
}

impl ConfigReadOptions {
    pub fn for_cwd(cwd: impl Into<PathBuf>) -> Self {
        Self {
            cwd: Some(cwd.into()),
            ..Self::default()
        }
    }

    pub fn include_layers(mut self) -> Self {
        self.include_layers = true;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSummary {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forked_from_id: Option<String>,
    pub cwd: PathBuf,
    pub preview: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_nickname: Option<String>,
    #[serde(default)]
    pub path: Option<PathBuf>,
    pub created_at: i64,
    pub updated_at: i64,
    pub model_provider: String,
    pub ephemeral: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadSummaryWire {
    id: String,
    #[serde(default, alias = "forked_from_id")]
    forked_from_id: Option<Value>,
    cwd: PathBuf,
    preview: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default, alias = "agent_nickname")]
    agent_nickname: Option<String>,
    #[serde(default)]
    path: Option<PathBuf>,
    created_at: i64,
    updated_at: i64,
    model_provider: String,
    ephemeral: bool,
    #[serde(default)]
    source: Option<Value>,
}

impl<'de> Deserialize<'de> for ThreadSummary {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = ThreadSummaryWire::deserialize(deserializer)?;
        let agent_nickname = normalize_optional_string(wire.agent_nickname)
            .or_else(|| subagent_source_agent_nickname(wire.source.as_ref()));

        Ok(Self {
            id: wire.id,
            forked_from_id: wire.forked_from_id.as_ref().and_then(non_empty_json_string),
            cwd: wire.cwd,
            preview: wire.preview,
            name: wire.name,
            agent_nickname,
            path: wire.path,
            created_at: wire.created_at,
            updated_at: wire.updated_at,
            model_provider: wire.model_provider,
            ephemeral: wire.ephemeral,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadLoadedListResponse {
    pub data: Vec<String>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
#[error("{message}")]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

fn is_false(value: &bool) -> bool {
    !*value
}
