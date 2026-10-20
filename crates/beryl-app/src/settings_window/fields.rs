use beryl_state::SettingKey;
use gpui_settings_window::SettingsFieldKind;

use super::DEVELOPER_INSTRUCTIONS_DRAFT_LIMIT;

pub(super) const KEYS: [SettingKey; 5] = [
    SettingKey::ActiveThemeId,
    SettingKey::ContextCompactionTimeout,
    SettingKey::DraftAutosaveInterval,
    SettingKey::DeveloperInstructions,
    SettingKey::EndTurnSound,
];

pub(super) fn key(field: &str) -> Option<SettingKey> {
    KEYS.into_iter().find(|key| key.stable_id() == field)
}

pub(super) fn metadata(key: SettingKey) -> (&'static str, SettingsFieldKind, usize) {
    match key {
        SettingKey::ActiveThemeId => ("Active theme", SettingsFieldKind::Text, 256),
        SettingKey::ContextCompactionTimeout => (
            "Context compaction timeout (seconds)",
            SettingsFieldKind::Number,
            20,
        ),
        SettingKey::DraftAutosaveInterval => (
            "Draft autosave interval (seconds)",
            SettingsFieldKind::Number,
            20,
        ),
        SettingKey::DeveloperInstructions => (
            "Developer Instructions",
            SettingsFieldKind::MultilineText,
            DEVELOPER_INSTRUCTIONS_DRAFT_LIMIT,
        ),
        SettingKey::EndTurnSound => ("End-turn sound", SettingsFieldKind::Text, 32 * 1024),
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum Route {
    #[default]
    Themes,
    Operations,
    Notifications,
    Agent,
    ThemeEditor,
}

impl Route {
    pub(super) fn from_id(id: &str) -> Option<Self> {
        match id {
            "themes" => Some(Self::Themes),
            "operations" => Some(Self::Operations),
            "notifications" => Some(Self::Notifications),
            "agent" => Some(Self::Agent),
            "theme-editor" => Some(Self::ThemeEditor),
            _ => None,
        }
    }

    pub(super) const fn id(self) -> &'static str {
        match self {
            Self::Themes => "themes",
            Self::Operations => "operations",
            Self::Notifications => "notifications",
            Self::Agent => "agent",
            Self::ThemeEditor => "theme-editor",
        }
    }

    pub(super) const fn binding(self) -> u64 {
        match self {
            Self::Themes => 1,
            Self::Operations => 2,
            Self::Notifications => 3,
            Self::Agent => 4,
            Self::ThemeEditor => 5,
        }
    }
}
