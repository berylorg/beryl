use std::{env, io, path::PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

mod chrome;
mod runtime;
mod theme;

pub use chrome::{
    AppearanceButtonSettings, AppearanceButtonStateSettings, AppearanceChromeSettings,
    AppearanceInputSettings, AppearanceStatusLineSettings, AppearanceSurfaceSettings,
    AppearanceTranscriptShellSettings,
};
pub use theme::{
    ActiveThemeProjection, BUILT_IN_INSTALLED_THEME_ID, BUILT_IN_THEME_ROLE_INVENTORY,
    BerylThemeProperty, BerylThemeRole, InstalledThemeId, InstalledThemeMetadata,
    MAX_THEME_DIAGNOSTIC_MESSAGE_BYTES, MAX_THEME_FONT_FAMILY_BYTES,
    MAX_THEME_VALIDATION_DIAGNOSTICS, ResolvedStyle, StylePropertyId, StylePropertyKind,
    StylePropertySource, StylePropertyValue, StyleRoleId, ThemeDefinition, ThemeDiagnostic,
    ThemeDiagnosticKind, ThemeDocument, ThemeDocumentError, ThemePropertySchema,
    ThemeRepositoryError, ThemeRepositorySnapshot, ThemeRepositoryStore, ThemeResolutionContext,
    ThemeResolutionError, ThemeResolver, ThemeRoleDefinition, ThemeRoleSchema, ThemeSchema,
    ThemeValidationDiagnostics, built_in_theme_definition, built_in_theme_resolver,
    built_in_theme_schema,
};

const APP_ROOT_DIR_NAME: &str = ".beryl";
const THEME_FILE_NAME: &str = "theme.toml";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppearanceSettings {
    pub general_ui: AppearanceRoleSettings,
    pub conversation_text: AppearanceRoleSettings,
    pub transcript_reasoning: AppearanceForegroundSettings,
    pub transcript_commentary: AppearanceForegroundSettings,
    pub markdown_header: AppearanceRoleSettings,
    pub code: AppearanceRoleSettings,
    pub emphasis: AppearanceRoleSettings,
    pub strong_emphasis: AppearanceRoleSettings,
    pub chrome: AppearanceChromeSettings,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppearanceRoleSettings {
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: u16,
    pub foreground: String,
    pub background: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppearanceForegroundSettings {
    pub foreground: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedHexColor {
    red: u8,
    green: u8,
    blue: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppearanceSettingsStore {
    root_dir: PathBuf,
}

#[derive(Debug, Error)]
pub enum AppearanceSettingsError {
    #[error("could not determine the current user's home directory")]
    MissingHomeDirectory,
    #[error("failed to create settings directory {path}")]
    CreateDirectory {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to read appearance settings from {path}")]
    ReadSettings {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to write appearance settings to {path}")]
    WriteSettings {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to serialize appearance settings")]
    SerializeSettings {
        #[source]
        source: toml::ser::Error,
    },
    #[error("failed to parse appearance settings from {path}")]
    ParseSettings {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("{role} font family must not be empty")]
    EmptyFontFamily { role: &'static str },
    #[error("{role} font family must be at most {max_bytes} bytes")]
    FontFamilyTooLong {
        role: &'static str,
        max_bytes: usize,
    },
    #[error("{role} font size must be between 8 and 48 points")]
    InvalidFontSize { role: &'static str },
    #[error("{role} font weight must be between 100 and 900")]
    InvalidFontWeight { role: &'static str },
    #[error("{role} {field} must use #RRGGBB hex color syntax")]
    InvalidColor { role: &'static str, field: String },
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            general_ui: AppearanceRoleSettings::new("Inter", 14.0, 400, "#e2e8f0", "#020617"),
            conversation_text: AppearanceRoleSettings::new(
                "Inter", 14.0, 400, "#e2e8f0", "#091220",
            ),
            transcript_reasoning: AppearanceForegroundSettings::new("#e2e8f0"),
            transcript_commentary: AppearanceForegroundSettings::new("#cbd5e1"),
            markdown_header: AppearanceRoleSettings::new("Inter", 18.0, 600, "#93c5fd", "#091220"),
            code: AppearanceRoleSettings::new("Consolas", 13.0, 400, "#e2e8f0", "#0f172a"),
            emphasis: AppearanceRoleSettings::new("Inter", 14.0, 400, "#bfdbfe", "#091220"),
            strong_emphasis: AppearanceRoleSettings::new("Inter", 14.0, 700, "#f8fafc", "#091220"),
            chrome: AppearanceChromeSettings::default(),
        }
    }
}

impl AppearanceSettings {
    pub fn validated(&self) -> Result<Self, AppearanceSettingsError> {
        Ok(Self {
            general_ui: self.general_ui.validated("General UI")?,
            conversation_text: self.conversation_text.validated("Conversation text")?,
            transcript_reasoning: self
                .transcript_reasoning
                .validated("Transcript reasoning")?,
            transcript_commentary: self
                .transcript_commentary
                .validated("Transcript commentary")?,
            markdown_header: self.markdown_header.validated("Markdown header")?,
            code: self.code.validated("Code")?,
            emphasis: self.emphasis.validated("Emphasis")?,
            strong_emphasis: self.strong_emphasis.validated("Strong emphasis")?,
            chrome: self.chrome.validated()?,
        })
    }
}

impl AppearanceRoleSettings {
    pub fn new(
        font_family: impl Into<String>,
        font_size: f32,
        font_weight: u16,
        foreground: impl Into<String>,
        background: impl Into<String>,
    ) -> Self {
        Self {
            font_family: font_family.into(),
            font_size,
            font_weight,
            foreground: foreground.into(),
            background: background.into(),
        }
    }

    pub fn parsed_foreground(&self) -> Option<ParsedHexColor> {
        ParsedHexColor::parse(&self.foreground)
    }

    pub fn parsed_background(&self) -> Option<ParsedHexColor> {
        ParsedHexColor::parse(&self.background)
    }

    fn validated(&self, role: &'static str) -> Result<Self, AppearanceSettingsError> {
        let font_family = self.font_family.trim().to_string();
        if font_family.is_empty() {
            return Err(AppearanceSettingsError::EmptyFontFamily { role });
        }
        if font_family.len() > MAX_THEME_FONT_FAMILY_BYTES {
            return Err(AppearanceSettingsError::FontFamilyTooLong {
                role,
                max_bytes: MAX_THEME_FONT_FAMILY_BYTES,
            });
        }
        if !(8.0..=48.0).contains(&self.font_size) {
            return Err(AppearanceSettingsError::InvalidFontSize { role });
        }
        if !(100..=900).contains(&self.font_weight) {
            return Err(AppearanceSettingsError::InvalidFontWeight { role });
        }
        let foreground =
            normalize_hex_color(&self.foreground).ok_or(AppearanceSettingsError::InvalidColor {
                role,
                field: "foreground".to_string(),
            })?;
        let background =
            normalize_hex_color(&self.background).ok_or(AppearanceSettingsError::InvalidColor {
                role,
                field: "background".to_string(),
            })?;

        Ok(Self {
            font_family,
            font_size: self.font_size,
            font_weight: self.font_weight,
            foreground,
            background,
        })
    }
}

impl AppearanceForegroundSettings {
    pub fn new(foreground: impl Into<String>) -> Self {
        Self {
            foreground: foreground.into(),
        }
    }

    pub fn parsed_foreground(&self) -> Option<ParsedHexColor> {
        ParsedHexColor::parse(&self.foreground)
    }

    fn validated(&self, role: &'static str) -> Result<Self, AppearanceSettingsError> {
        Ok(Self {
            foreground: validate_hex_color(&self.foreground, role, "foreground")?,
        })
    }
}

impl ParsedHexColor {
    pub fn parse(value: &str) -> Option<Self> {
        let normalized = normalize_hex_color(value)?;
        let red = u8::from_str_radix(&normalized[1..3], 16).ok()?;
        let green = u8::from_str_radix(&normalized[3..5], 16).ok()?;
        let blue = u8::from_str_radix(&normalized[5..7], 16).ok()?;
        Some(Self { red, green, blue })
    }

    pub fn red(&self) -> u8 {
        self.red
    }

    pub fn green(&self) -> u8 {
        self.green
    }

    pub fn blue(&self) -> u8 {
        self.blue
    }
}

impl AppearanceSettingsStore {
    pub fn from_environment() -> Result<Self, AppearanceSettingsError> {
        let home = env::var_os("USERPROFILE")
            .or_else(|| env::var_os("HOME"))
            .map(PathBuf::from)
            .ok_or(AppearanceSettingsError::MissingHomeDirectory)?;
        Ok(Self::new(home.join(APP_ROOT_DIR_NAME)))
    }

    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    pub fn theme_path(&self) -> PathBuf {
        self.root_dir.join(THEME_FILE_NAME)
    }

    pub fn load_or_default(&self) -> Result<AppearanceSettings, AppearanceSettingsError> {
        Ok(AppearanceSettings::default())
    }

    pub fn save(&self, settings: &AppearanceSettings) -> Result<(), AppearanceSettingsError> {
        settings.validated()?;
        Ok(())
    }
}

fn validate_hex_color(
    value: &str,
    role: &'static str,
    field: impl Into<String>,
) -> Result<String, AppearanceSettingsError> {
    normalize_hex_color(value).ok_or_else(|| AppearanceSettingsError::InvalidColor {
        role,
        field: field.into(),
    })
}

fn normalize_hex_color(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let hex = trimmed.strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("#{}", hex.to_ascii_lowercase()))
}
