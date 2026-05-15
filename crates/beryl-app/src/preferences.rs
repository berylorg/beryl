use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

const APP_ROOT_DIR_NAME: &str = ".beryl";
const GUI_PREFERENCES_FILE_NAME: &str = "preferences.toml";
pub const DEFAULT_CONTEXT_COMPACTION_TIMEOUT_SECONDS: u64 = 180;
pub const MIN_CONTEXT_COMPACTION_TIMEOUT_SECONDS: u64 = 1;
pub const MAX_CONTEXT_COMPACTION_TIMEOUT_SECONDS: u64 = 86_400;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiPreferences {
    #[serde(default)]
    pub notifications: NotificationPreferences,
    #[serde(default)]
    pub agent: AgentPreferences,
    #[serde(default)]
    pub operations: OperationPreferences,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationPreferences {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_turn_sound_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPreferences {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub developer_instructions: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationPreferences {
    #[serde(
        default = "default_context_compaction_timeout_seconds",
        deserialize_with = "deserialize_context_compaction_timeout_seconds"
    )]
    pub context_compaction_timeout_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuiPreferencesStore {
    root_dir: PathBuf,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum NotificationSoundPathError {
    #[error("notification sound path must be absolute")]
    NotAbsolute,
    #[error("notification sound path must point to a .wav file")]
    NotWav,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ContextCompactionTimeoutError {
    #[error("context compaction timeout must be a whole number of seconds")]
    NotInteger,
    #[error("context compaction timeout must be at least {min} second")]
    TooSmall { min: u64 },
    #[error("context compaction timeout must be at most {max} seconds")]
    TooLarge { max: u64 },
}

#[derive(Debug, Error)]
pub enum GuiPreferencesError {
    #[error("could not determine the current user's home directory")]
    MissingHomeDirectory,
    #[error("failed to create preferences directory {path}")]
    CreateDirectory {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to read GUI preferences from {path}")]
    ReadPreferences {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to write GUI preferences to {path}")]
    WritePreferences {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to serialize GUI preferences")]
    SerializePreferences {
        #[source]
        source: toml::ser::Error,
    },
    #[error("failed to parse GUI preferences from {path}")]
    ParsePreferences {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("invalid notification sound path {path}")]
    InvalidNotificationSoundPath {
        path: String,
        #[source]
        source: NotificationSoundPathError,
    },
}

impl GuiPreferences {
    pub fn validated(&self) -> Result<Self, GuiPreferencesError> {
        Ok(Self {
            notifications: self.notifications.validated()?,
            agent: self.agent.validated(),
            operations: self.operations.validated(),
        })
    }

    pub fn context_compaction_timeout(&self) -> Duration {
        self.operations.context_compaction_timeout()
    }
}

impl NotificationPreferences {
    pub fn with_end_turn_sound_path(
        end_turn_sound_path: Option<PathBuf>,
    ) -> Result<Self, NotificationSoundPathError> {
        if let Some(path) = end_turn_sound_path.as_ref() {
            validate_notification_sound_path(path)?;
        }
        Ok(Self {
            end_turn_sound_path,
        })
    }

    pub fn end_turn_sound_path(&self) -> Option<&Path> {
        self.end_turn_sound_path.as_deref()
    }

    pub fn validated(&self) -> Result<Self, GuiPreferencesError> {
        if let Some(path) = self.end_turn_sound_path.as_ref() {
            validate_notification_sound_path(path).map_err(|source| {
                GuiPreferencesError::InvalidNotificationSoundPath {
                    path: path.display().to_string(),
                    source,
                }
            })?;
        }

        Ok(self.clone())
    }
}

impl AgentPreferences {
    pub fn with_developer_instructions(developer_instructions: Option<String>) -> Self {
        Self {
            developer_instructions: normalize_developer_instructions(developer_instructions),
        }
    }

    pub fn developer_instructions(&self) -> Option<&str> {
        self.developer_instructions.as_deref()
    }

    pub fn validated(&self) -> Self {
        Self::with_developer_instructions(self.developer_instructions.clone())
    }
}

impl Default for OperationPreferences {
    fn default() -> Self {
        Self {
            context_compaction_timeout_seconds: DEFAULT_CONTEXT_COMPACTION_TIMEOUT_SECONDS,
        }
    }
}

impl OperationPreferences {
    pub fn with_context_compaction_timeout_seconds(
        context_compaction_timeout_seconds: u64,
    ) -> Result<Self, ContextCompactionTimeoutError> {
        validate_context_compaction_timeout_seconds(context_compaction_timeout_seconds)?;
        Ok(Self {
            context_compaction_timeout_seconds,
        })
    }

    pub fn context_compaction_timeout(&self) -> Duration {
        Duration::from_secs(self.context_compaction_timeout_seconds)
    }

    pub fn validated(&self) -> Self {
        if validate_context_compaction_timeout_seconds(self.context_compaction_timeout_seconds)
            .is_err()
        {
            return Self::default();
        }

        self.clone()
    }
}

impl GuiPreferencesStore {
    pub fn from_environment() -> Result<Self, GuiPreferencesError> {
        let home = env::var_os("USERPROFILE")
            .or_else(|| env::var_os("HOME"))
            .map(PathBuf::from)
            .ok_or(GuiPreferencesError::MissingHomeDirectory)?;
        Ok(Self::new(home.join(APP_ROOT_DIR_NAME)))
    }

    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    pub fn preferences_path(&self) -> PathBuf {
        self.root_dir.join(GUI_PREFERENCES_FILE_NAME)
    }

    pub fn load_or_default(&self) -> Result<GuiPreferences, GuiPreferencesError> {
        let path = self.preferences_path();
        if !path.exists() {
            return Ok(GuiPreferences::default());
        }

        let text =
            fs::read_to_string(&path).map_err(|source| GuiPreferencesError::ReadPreferences {
                path: path.display().to_string(),
                source,
            })?;
        let preferences: GuiPreferences =
            toml::from_str(&text).map_err(|source| GuiPreferencesError::ParsePreferences {
                path: path.display().to_string(),
                source,
            })?;
        preferences.validated()
    }

    pub fn save(&self, preferences: &GuiPreferences) -> Result<(), GuiPreferencesError> {
        let preferences = preferences.validated()?;
        ensure_directory(&self.root_dir)?;
        let path = self.preferences_path();
        let text = toml::to_string_pretty(&preferences)
            .map_err(|source| GuiPreferencesError::SerializePreferences { source })?;
        let mut temp_file = tempfile::NamedTempFile::new_in(&self.root_dir).map_err(|source| {
            GuiPreferencesError::WritePreferences {
                path: path.display().to_string(),
                source,
            }
        })?;
        temp_file.write_all(text.as_bytes()).map_err(|source| {
            GuiPreferencesError::WritePreferences {
                path: temp_file.path().display().to_string(),
                source,
            }
        })?;
        temp_file.persist(&path).map_err(|error| {
            let tempfile::PersistError { error: source, .. } = error;
            GuiPreferencesError::WritePreferences {
                path: path.display().to_string(),
                source,
            }
        })?;
        Ok(())
    }
}

pub fn parse_notification_sound_path_text(
    value: &str,
) -> Result<Option<PathBuf>, NotificationSoundPathError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }

    let path = PathBuf::from(value);
    validate_notification_sound_path(&path)?;
    Ok(Some(path))
}

pub fn normalize_developer_instructions_text(value: &str) -> Option<String> {
    if value.trim().is_empty() {
        return None;
    }

    Some(value.to_string())
}

pub fn parse_context_compaction_timeout_seconds_text(
    value: &str,
) -> Result<u64, ContextCompactionTimeoutError> {
    let seconds = value
        .trim()
        .parse::<u64>()
        .map_err(|_| ContextCompactionTimeoutError::NotInteger)?;
    validate_context_compaction_timeout_seconds(seconds)?;
    Ok(seconds)
}

pub fn validate_notification_sound_path(path: &Path) -> Result<(), NotificationSoundPathError> {
    if !path.is_absolute() {
        return Err(NotificationSoundPathError::NotAbsolute);
    }

    let is_wav = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"));
    if !is_wav {
        return Err(NotificationSoundPathError::NotWav);
    }

    Ok(())
}

pub fn validate_context_compaction_timeout_seconds(
    seconds: u64,
) -> Result<(), ContextCompactionTimeoutError> {
    if seconds < MIN_CONTEXT_COMPACTION_TIMEOUT_SECONDS {
        return Err(ContextCompactionTimeoutError::TooSmall {
            min: MIN_CONTEXT_COMPACTION_TIMEOUT_SECONDS,
        });
    }
    if seconds > MAX_CONTEXT_COMPACTION_TIMEOUT_SECONDS {
        return Err(ContextCompactionTimeoutError::TooLarge {
            max: MAX_CONTEXT_COMPACTION_TIMEOUT_SECONDS,
        });
    }
    Ok(())
}

fn ensure_directory(path: &Path) -> Result<(), GuiPreferencesError> {
    fs::create_dir_all(path).map_err(|source| GuiPreferencesError::CreateDirectory {
        path: path.display().to_string(),
        source,
    })
}

fn normalize_developer_instructions(value: Option<String>) -> Option<String> {
    value.and_then(|value| normalize_developer_instructions_text(&value))
}

fn default_context_compaction_timeout_seconds() -> u64 {
    DEFAULT_CONTEXT_COMPACTION_TIMEOUT_SECONDS
}

fn deserialize_context_compaction_timeout_seconds<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = toml::Value::deserialize(deserializer)?;
    Ok(context_compaction_timeout_seconds_from_toml_value(value))
}

fn context_compaction_timeout_seconds_from_toml_value(value: toml::Value) -> u64 {
    let toml::Value::Integer(seconds) = value else {
        return DEFAULT_CONTEXT_COMPACTION_TIMEOUT_SECONDS;
    };
    let Ok(seconds) = u64::try_from(seconds) else {
        return DEFAULT_CONTEXT_COMPACTION_TIMEOUT_SECONDS;
    };
    if validate_context_compaction_timeout_seconds(seconds).is_err() {
        return DEFAULT_CONTEXT_COMPACTION_TIMEOUT_SECONDS;
    }
    seconds
}
