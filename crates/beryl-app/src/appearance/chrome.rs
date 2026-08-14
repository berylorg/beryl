use serde::{Deserialize, Serialize};

use super::{AppearanceSettingsError, validate_hex_color};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppearanceChromeSettings {
    pub toolbar_background: String,
    pub conversation_thread_strip_background: String,
    pub separator: String,
    pub primary_button: AppearanceButtonSettings,
    pub secondary_button: AppearanceButtonSettings,
    pub input: AppearanceInputSettings,
    pub transcript_shell: AppearanceTranscriptShellSettings,
    pub status_line: AppearanceStatusLineSettings,
    pub surfaces: AppearanceSurfaceSettings,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppearanceButtonSettings {
    #[serde(default = "default_button_font_weight")]
    pub font_weight: u16,
    pub normal: AppearanceButtonStateSettings,
    pub hover: AppearanceButtonStateSettings,
    pub active: AppearanceButtonStateSettings,
    pub disabled: AppearanceButtonStateSettings,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppearanceButtonStateSettings {
    pub background: String,
    pub border: String,
    pub foreground: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppearanceInputSettings {
    pub panel_background: String,
    pub input_background: String,
    pub input_border: String,
    pub input_foreground: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppearanceTranscriptShellSettings {
    pub background: String,
    pub foreground: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppearanceStatusLineSettings {
    pub background: String,
    pub title_foreground: String,
    pub value_foreground: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppearanceSurfaceSettings {
    pub panel_background: String,
    pub row_background: String,
    pub popup_background: String,
    pub border: String,
    pub muted_foreground: String,
}

impl Default for AppearanceChromeSettings {
    fn default() -> Self {
        Self {
            toolbar_background: "#020617".to_string(),
            conversation_thread_strip_background: "#091220".to_string(),
            separator: "#1e293b".to_string(),
            primary_button: AppearanceButtonSettings::primary(),
            secondary_button: AppearanceButtonSettings::secondary(),
            input: AppearanceInputSettings {
                panel_background: "#020617".to_string(),
                input_background: "#0f172a".to_string(),
                input_border: "#334155".to_string(),
                input_foreground: "#e2e8f0".to_string(),
            },
            transcript_shell: AppearanceTranscriptShellSettings {
                background: "#091220".to_string(),
                foreground: "#e2e8f0".to_string(),
            },
            status_line: AppearanceStatusLineSettings {
                background: "#020617".to_string(),
                title_foreground: "#94a3b8".to_string(),
                value_foreground: "#e2e8f0".to_string(),
            },
            surfaces: AppearanceSurfaceSettings {
                panel_background: "#111827".to_string(),
                row_background: "#1f2937".to_string(),
                popup_background: "#111827".to_string(),
                border: "#374151".to_string(),
                muted_foreground: "#94a3b8".to_string(),
            },
        }
    }
}

impl AppearanceButtonSettings {
    fn primary() -> Self {
        Self {
            font_weight: default_button_font_weight(),
            normal: AppearanceButtonStateSettings::new("#1d4ed8", "#3b82f6", "#eff6ff"),
            hover: AppearanceButtonStateSettings::new("#2563eb", "#60a5fa", "#ffffff"),
            active: AppearanceButtonStateSettings::new("#1e40af", "#3b82f6", "#ffffff"),
            disabled: AppearanceButtonStateSettings::new("#334155", "#475569", "#94a3b8"),
        }
    }

    fn secondary() -> Self {
        Self {
            font_weight: default_button_font_weight(),
            normal: AppearanceButtonStateSettings::new("#1e293b", "#475569", "#e2e8f0"),
            hover: AppearanceButtonStateSettings::new("#334155", "#64748b", "#f8fafc"),
            active: AppearanceButtonStateSettings::new("#0f172a", "#475569", "#f8fafc"),
            disabled: AppearanceButtonStateSettings::new("#111827", "#334155", "#64748b"),
        }
    }
}

impl AppearanceButtonStateSettings {
    pub fn new(
        background: impl Into<String>,
        border: impl Into<String>,
        foreground: impl Into<String>,
    ) -> Self {
        Self {
            background: background.into(),
            border: border.into(),
            foreground: foreground.into(),
        }
    }
}

impl AppearanceInputSettings {
    fn validated(&self) -> Result<Self, AppearanceSettingsError> {
        Ok(Self {
            panel_background: validate_hex_color(
                &self.panel_background,
                "Input",
                "panel_background",
            )?,
            input_background: validate_hex_color(
                &self.input_background,
                "Input",
                "input_background",
            )?,
            input_border: validate_hex_color(&self.input_border, "Input", "input_border")?,
            input_foreground: validate_hex_color(
                &self.input_foreground,
                "Input",
                "input_foreground",
            )?,
        })
    }
}

impl AppearanceTranscriptShellSettings {
    fn validated(&self) -> Result<Self, AppearanceSettingsError> {
        Ok(Self {
            background: validate_hex_color(&self.background, "Transcript shell", "background")?,
            foreground: validate_hex_color(&self.foreground, "Transcript shell", "foreground")?,
        })
    }
}

impl AppearanceStatusLineSettings {
    fn validated(&self) -> Result<Self, AppearanceSettingsError> {
        Ok(Self {
            background: validate_hex_color(&self.background, "Status line", "background")?,
            title_foreground: validate_hex_color(
                &self.title_foreground,
                "Status line",
                "title_foreground",
            )?,
            value_foreground: validate_hex_color(
                &self.value_foreground,
                "Status line",
                "value_foreground",
            )?,
        })
    }
}

impl AppearanceSurfaceSettings {
    fn validated(&self) -> Result<Self, AppearanceSettingsError> {
        Ok(Self {
            panel_background: validate_hex_color(
                &self.panel_background,
                "Surfaces",
                "panel_background",
            )?,
            row_background: validate_hex_color(&self.row_background, "Surfaces", "row_background")?,
            popup_background: validate_hex_color(
                &self.popup_background,
                "Surfaces",
                "popup_background",
            )?,
            border: validate_hex_color(&self.border, "Surfaces", "border")?,
            muted_foreground: validate_hex_color(
                &self.muted_foreground,
                "Surfaces",
                "muted_foreground",
            )?,
        })
    }
}

fn state_field(state: &'static str, field: &'static str) -> String {
    format!("{state}.{field}")
}

impl AppearanceChromeSettings {
    pub(super) fn validated(&self) -> Result<Self, AppearanceSettingsError> {
        Ok(Self {
            toolbar_background: validate_hex_color(
                &self.toolbar_background,
                "Chrome",
                "toolbar_background",
            )?,
            conversation_thread_strip_background: validate_hex_color(
                &self.conversation_thread_strip_background,
                "Chrome",
                "conversation_thread_strip_background",
            )?,
            separator: validate_hex_color(&self.separator, "Chrome", "separator")?,
            primary_button: self.primary_button.validated("Primary button")?,
            secondary_button: self.secondary_button.validated("Secondary button")?,
            input: self.input.validated()?,
            transcript_shell: self.transcript_shell.validated()?,
            status_line: self.status_line.validated()?,
            surfaces: self.surfaces.validated()?,
        })
    }
}

impl AppearanceButtonSettings {
    fn validated(&self, role: &'static str) -> Result<Self, AppearanceSettingsError> {
        if !(100..=900).contains(&self.font_weight) {
            return Err(AppearanceSettingsError::InvalidFontWeight { role });
        }

        Ok(Self {
            font_weight: self.font_weight,
            normal: self.normal.validated(role, "normal")?,
            hover: self.hover.validated(role, "hover")?,
            active: self.active.validated(role, "active")?,
            disabled: self.disabled.validated(role, "disabled")?,
        })
    }
}

const fn default_button_font_weight() -> u16 {
    500
}

impl AppearanceButtonStateSettings {
    fn validated(
        &self,
        role: &'static str,
        state: &'static str,
    ) -> Result<Self, AppearanceSettingsError> {
        Ok(Self {
            background: validate_hex_color(
                &self.background,
                role,
                state_field(state, "background"),
            )?,
            border: validate_hex_color(&self.border, role, state_field(state, "border"))?,
            foreground: validate_hex_color(
                &self.foreground,
                role,
                state_field(state, "foreground"),
            )?,
        })
    }
}
