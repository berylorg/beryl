use std::{error::Error, fmt};

use beryl_model::AdmittedHostPath;

const MAX_ACTIVE_THEME_ID_BYTES: usize = 256;
const MAX_DEVELOPER_INSTRUCTIONS_BYTES: usize = 60 * 1024;

/// Exact schema version of one feature-owned scalar setting.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SettingSchemaVersion(u32);

impl SettingSchemaVersion {
    /// First and only setting schema accepted by this package version.
    pub const V1: Self = Self(1);

    /// Returns the integer persisted with the setting record.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Closed set of Beryl-owned scalar preferences supported by schema V1.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SettingKey {
    ActiveThemeId,
    ContextCompactionTimeout,
    DraftAutosaveInterval,
    DeveloperInstructions,
    EndTurnSound,
}

impl SettingKey {
    pub(crate) const FIRST: Self = Self::ActiveThemeId;
    pub(crate) const LAST: Self = Self::EndTurnSound;

    /// Returns the stable feature-facing setting id.
    #[must_use]
    pub const fn stable_id(self) -> &'static str {
        match self {
            Self::ActiveThemeId => "themes.active-theme-id",
            Self::ContextCompactionTimeout => "operations.context-compaction-timeout",
            Self::DraftAutosaveInterval => "operations.draft-autosave-interval",
            Self::DeveloperInstructions => "agent.developer-instructions",
            Self::EndTurnSound => "notifications.end-turn-sound",
        }
    }

    /// Returns the exact scalar schema accepted for this key.
    #[must_use]
    pub const fn schema_version(self) -> SettingSchemaVersion {
        SettingSchemaVersion::V1
    }
}

/// One bounded value belonging to a specific closed Beryl setting key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingValue {
    pub(super) kind: SettingValueKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum SettingValueKind {
    ActiveThemeId(Box<str>),
    ContextCompactionTimeoutMillis(u64),
    DraftAutosaveIntervalSeconds(u64),
    DeveloperInstructions(Box<str>),
    EndTurnSound(Option<AdmittedHostPath>),
}

impl SettingValue {
    /// Constructs the bounded stable identity of the active installed theme.
    pub fn active_theme_id(value: impl AsRef<str>) -> Result<Self, SettingValueError> {
        bounded_text(
            SettingKey::ActiveThemeId,
            value.as_ref(),
            MAX_ACTIVE_THEME_ID_BYTES,
        )
        .map(|value| Self {
            kind: SettingValueKind::ActiveThemeId(value),
        })
    }

    /// Constructs the storage scalar for the caller-validated compaction timeout.
    #[must_use]
    pub const fn context_compaction_timeout_millis(value: u64) -> Self {
        Self {
            kind: SettingValueKind::ContextCompactionTimeoutMillis(value),
        }
    }

    /// Constructs the storage scalar for the caller-validated autosave interval.
    #[must_use]
    pub const fn draft_autosave_interval_seconds(value: u64) -> Self {
        Self {
            kind: SettingValueKind::DraftAutosaveIntervalSeconds(value),
        }
    }

    /// Constructs bounded caller-validated developer-instructions text.
    pub fn developer_instructions(value: impl AsRef<str>) -> Result<Self, SettingValueError> {
        bounded_text(
            SettingKey::DeveloperInstructions,
            value.as_ref(),
            MAX_DEVELOPER_INSTRUCTIONS_BYTES,
        )
        .map(|value| Self {
            kind: SettingValueKind::DeveloperInstructions(value),
        })
    }

    /// Constructs the optional admitted host path for end-turn sound playback.
    #[must_use]
    pub const fn end_turn_sound(path: Option<AdmittedHostPath>) -> Self {
        Self {
            kind: SettingValueKind::EndTurnSound(path),
        }
    }

    /// Returns the only setting key with which this scalar may be stored.
    #[must_use]
    pub const fn key(&self) -> SettingKey {
        match &self.kind {
            SettingValueKind::ActiveThemeId(_) => SettingKey::ActiveThemeId,
            SettingValueKind::ContextCompactionTimeoutMillis(_) => {
                SettingKey::ContextCompactionTimeout
            }
            SettingValueKind::DraftAutosaveIntervalSeconds(_) => SettingKey::DraftAutosaveInterval,
            SettingValueKind::DeveloperInstructions(_) => SettingKey::DeveloperInstructions,
            SettingValueKind::EndTurnSound(_) => SettingKey::EndTurnSound,
        }
    }

    /// Returns the exact schema carried by this scalar.
    #[must_use]
    pub const fn schema_version(&self) -> SettingSchemaVersion {
        self.key().schema_version()
    }

    #[must_use]
    pub fn as_active_theme_id(&self) -> Option<&str> {
        match &self.kind {
            SettingValueKind::ActiveThemeId(value) => Some(value),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_context_compaction_timeout_millis(&self) -> Option<u64> {
        match &self.kind {
            SettingValueKind::ContextCompactionTimeoutMillis(value) => Some(*value),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_draft_autosave_interval_seconds(&self) -> Option<u64> {
        match &self.kind {
            SettingValueKind::DraftAutosaveIntervalSeconds(value) => Some(*value),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_developer_instructions(&self) -> Option<&str> {
        match &self.kind {
            SettingValueKind::DeveloperInstructions(value) => Some(value),
            _ => None,
        }
    }

    /// Returns `None` for another value kind and the configured optional path otherwise.
    #[must_use]
    pub const fn as_end_turn_sound(&self) -> Option<&Option<AdmittedHostPath>> {
        match &self.kind {
            SettingValueKind::EndTurnSound(path) => Some(path),
            _ => None,
        }
    }

    pub(super) const fn from_kind(kind: SettingValueKind) -> Self {
        Self { kind }
    }
}

fn bounded_text(
    key: SettingKey,
    value: &str,
    max_bytes: usize,
) -> Result<Box<str>, SettingValueError> {
    if value.len() > max_bytes {
        return Err(SettingValueError::TooLong {
            key,
            max_bytes,
            actual_bytes: value.len(),
        });
    }
    Ok(value.into())
}

/// Storage-shape failure for a Beryl-owned scalar setting value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettingValueError {
    TooLong {
        key: SettingKey,
        max_bytes: usize,
        actual_bytes: usize,
    },
}

impl fmt::Display for SettingValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong {
                key,
                max_bytes,
                actual_bytes,
            } => write!(
                formatter,
                "setting `{}` must not exceed {max_bytes} UTF-8 bytes, got {actual_bytes}",
                key.stable_id()
            ),
        }
    }
}

impl Error for SettingValueError {}
