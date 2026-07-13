use std::path::PathBuf;

use serde::{Deserialize, Serialize, de};
use serde_json::Value;
use thiserror::Error;

pub const REQUIRED_CODEX_APP_SERVER_VERSION: &str = "0.137.0";

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
    #[serde(default)]
    pub user_agent: String,
    pub codex_home: String,
    pub platform_family: String,
    pub platform_os: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompatibilityProbe {
    ConfigRead,
    ModelList,
    ThreadCompactStart,
    ThreadResumeMetadata,
    ThreadUnsubscribe,
    TurnInterrupt,
    TurnSteer,
}

impl CompatibilityProbe {
    pub fn method(self) -> &'static str {
        match self {
            Self::ConfigRead => "config/read",
            Self::ModelList => "model/list",
            Self::ThreadCompactStart => "thread/compact/start",
            Self::ThreadResumeMetadata => "thread/resume",
            Self::ThreadUnsubscribe => "thread/unsubscribe",
            Self::TurnInterrupt => "turn/interrupt",
            Self::TurnSteer => "turn/steer",
        }
    }
}

const REQUIRED_COMPATIBILITY_PROBES: &[CompatibilityProbe] = &[
    CompatibilityProbe::ConfigRead,
    CompatibilityProbe::ModelList,
    CompatibilityProbe::ThreadCompactStart,
    CompatibilityProbe::ThreadResumeMetadata,
    CompatibilityProbe::ThreadUnsubscribe,
    CompatibilityProbe::TurnInterrupt,
    CompatibilityProbe::TurnSteer,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompatibilitySnapshot {
    user_agent: String,
    platform_family: String,
    platform_os: String,
    requires_method_probes: bool,
}

impl CompatibilitySnapshot {
    pub fn from_initialize_response(response: &InitializeResponse) -> Self {
        Self {
            user_agent: response.user_agent.clone(),
            platform_family: response.platform_family.clone(),
            platform_os: response.platform_os.clone(),
            requires_method_probes: true,
        }
    }

    pub fn user_agent(&self) -> &str {
        &self.user_agent
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

    pub(crate) fn validate_required_app_server_version(&self) -> Result<(), CompatibilityError> {
        self.validate_required_app_server_version_value()
    }

    fn validate_required_app_server_version_value(&self) -> Result<(), CompatibilityError> {
        let Some(actual_version) = parse_codex_app_server_user_agent(&self.user_agent) else {
            if self.user_agent.trim().is_empty() {
                return Err(CompatibilityError::AppServerVersionMissing {
                    required_version: REQUIRED_CODEX_APP_SERVER_VERSION,
                });
            }

            return Err(CompatibilityError::AppServerVersionUnrecognized {
                required_version: REQUIRED_CODEX_APP_SERVER_VERSION,
                user_agent: self.user_agent.clone(),
            });
        };
        let required_version = CodexAppServerVersion::parse(REQUIRED_CODEX_APP_SERVER_VERSION)
            .expect("required version is valid");

        if actual_version != required_version {
            return Err(CompatibilityError::AppServerVersionMismatch {
                required_version: REQUIRED_CODEX_APP_SERVER_VERSION,
                actual_version: actual_version.to_string(),
                user_agent: self.user_agent.clone(),
            });
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CodexAppServerVersion {
    major: u16,
    minor: u16,
    patch: u16,
}

impl CodexAppServerVersion {
    fn parse(value: &str) -> Option<Self> {
        let mut parts = value.split('.');
        let major = parse_version_component(parts.next()?)?;
        let minor = parse_version_component(parts.next()?)?;
        let patch = parse_version_component(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }

        Some(Self {
            major,
            minor,
            patch,
        })
    }
}

impl std::fmt::Display for CodexAppServerVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

fn parse_codex_app_server_user_agent(user_agent: &str) -> Option<CodexAppServerVersion> {
    let product = user_agent.split_whitespace().next()?;
    let (name, version) = product.split_once('/')?;
    if name != "beryl" {
        return None;
    }
    CodexAppServerVersion::parse(version)
}

fn parse_version_component(value: &str) -> Option<u16> {
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return None;
    }

    value.parse().ok()
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CompatibilityError {
    #[error(
        "backend initialize response did not include a Codex App Server version; required exactly {required_version}"
    )]
    AppServerVersionMissing { required_version: &'static str },
    #[error(
        "backend userAgent {user_agent:?} did not start with `beryl/<major.minor.patch>`; required exactly {required_version}"
    )]
    AppServerVersionUnrecognized {
        required_version: &'static str,
        user_agent: String,
    },
    #[error(
        "backend Codex App Server version {actual_version} does not match required {required_version} from userAgent {user_agent:?}"
    )]
    AppServerVersionMismatch {
        required_version: &'static str,
        actual_version: String,
        user_agent: String,
    },
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
