use std::collections::HashMap;

use gpui_settings_window::{
    RgbColor, SettingsFieldId, SettingsFieldKind, SettingsRow, SettingsSection,
};

use super::fields::{
    AppearanceField, AppearanceFieldSpec, AppearanceSection, SECTIONS, field_specs, field_value,
    foreground_settings_mut, role_settings_mut,
};
use crate::{AppearanceForegroundSettings, AppearanceRoleSettings, AppearanceSettings};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AppearanceSettingsDraft {
    values: HashMap<SettingsFieldId, String>,
}

impl AppearanceSettingsDraft {
    pub(crate) fn from_settings(settings: &AppearanceSettings) -> Self {
        let mut values = HashMap::new();
        for spec in field_specs() {
            values.insert(spec.field_id(), field_value(settings, spec));
        }
        Self { values }
    }

    fn field_value(&self, spec: AppearanceFieldSpec) -> String {
        self.values
            .get(&spec.field_id())
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn set_field_value(&mut self, field_id: &SettingsFieldId, value: String) -> bool {
        if !field_specs().any(|spec| spec.field_id() == *field_id) {
            return false;
        }
        self.values.insert(field_id.clone(), value);
        true
    }

    pub(crate) fn to_settings(
        &self,
    ) -> Result<AppearanceSettings, HashMap<SettingsFieldId, String>> {
        let mut settings = AppearanceSettings::default();
        let mut errors = HashMap::new();

        for spec in field_specs() {
            let field_id = spec.field_id();
            let value = self.values.get(&field_id).map(String::as_str).unwrap_or("");
            if let Some(value) = validate_field_value(spec, value, &mut errors) {
                assign_field_value(&mut settings, spec, value);
            }
        }

        if errors.is_empty() {
            Ok(settings)
        } else {
            Err(errors)
        }
    }
}

pub(crate) fn settings_sections(
    draft: &AppearanceSettingsDraft,
    errors: &HashMap<SettingsFieldId, String>,
) -> Vec<SettingsSection> {
    SECTIONS
        .into_iter()
        .map(|section| {
            section.fields().iter().copied().fold(
                SettingsSection::new(section.section_id(), section.label()),
                |section_model, field| {
                    let spec = AppearanceFieldSpec { section, field };
                    let field_id = spec.field_id();
                    let row = SettingsRow::new(
                        field_id.clone(),
                        field.label(),
                        draft.field_value(spec),
                        field.kind(),
                    );
                    section_model.with_row(match errors.get(&field_id) {
                        Some(error) => row.with_error(error.clone()),
                        None => row,
                    })
                },
            )
        })
        .collect()
}

enum ValidatedFieldValue {
    Text(String),
    FontSize(f32),
    FontWeight(u16),
}

fn validate_field_value(
    spec: AppearanceFieldSpec,
    value: &str,
    errors: &mut HashMap<SettingsFieldId, String>,
) -> Option<ValidatedFieldValue> {
    match spec.field {
        AppearanceField::FontFamily => validate_font_family(spec, value, errors),
        AppearanceField::FontSize => validate_font_size(spec, value, errors),
        AppearanceField::FontWeight => validate_font_weight(spec, value, errors),
        _ => validate_color(spec, value, errors),
    }
}

fn validate_font_family(
    spec: AppearanceFieldSpec,
    value: &str,
    errors: &mut HashMap<SettingsFieldId, String>,
) -> Option<ValidatedFieldValue> {
    let value = value.trim().to_string();
    if value.is_empty() {
        errors.insert(
            spec.field_id(),
            "Font family must not be empty.".to_string(),
        );
        return None;
    }
    Some(ValidatedFieldValue::Text(value))
}

fn validate_font_size(
    spec: AppearanceFieldSpec,
    value: &str,
    errors: &mut HashMap<SettingsFieldId, String>,
) -> Option<ValidatedFieldValue> {
    let Ok(size) = value.trim().parse::<f32>() else {
        errors.insert(
            spec.field_id(),
            "Font size must be a number from 8 to 48.".to_string(),
        );
        return None;
    };

    if !(8.0..=48.0).contains(&size) {
        errors.insert(
            spec.field_id(),
            "Font size must be a number from 8 to 48.".to_string(),
        );
        return None;
    }

    Some(ValidatedFieldValue::FontSize(size))
}

fn validate_font_weight(
    spec: AppearanceFieldSpec,
    value: &str,
    errors: &mut HashMap<SettingsFieldId, String>,
) -> Option<ValidatedFieldValue> {
    let Ok(weight) = value.trim().parse::<u16>() else {
        errors.insert(
            spec.field_id(),
            "Font weight must be an integer from 100 to 900.".to_string(),
        );
        return None;
    };

    if !(100..=900).contains(&weight) {
        errors.insert(
            spec.field_id(),
            "Font weight must be an integer from 100 to 900.".to_string(),
        );
        return None;
    }

    Some(ValidatedFieldValue::FontWeight(weight))
}

fn validate_color(
    spec: AppearanceFieldSpec,
    value: &str,
    errors: &mut HashMap<SettingsFieldId, String>,
) -> Option<ValidatedFieldValue> {
    if let Some(color) = RgbColor::parse(value) {
        return Some(ValidatedFieldValue::Text(color.to_hex()));
    }

    errors.insert(
        spec.field_id(),
        format!("{} must use #rrggbb hex color syntax.", spec.field.label()),
    );
    None
}

fn assign_field_value(
    settings: &mut AppearanceSettings,
    spec: AppearanceFieldSpec,
    value: ValidatedFieldValue,
) {
    if spec.section.is_typography() {
        assign_typography_field(role_settings_mut(settings, spec.section), spec.field, value);
        return;
    }
    if spec.section.is_transcript_foreground() {
        assign_foreground_field(
            foreground_settings_mut(settings, spec.section),
            spec.field,
            value,
        );
        return;
    }

    match spec.section {
        AppearanceSection::PrimaryButton => {
            assign_button_field(&mut settings.chrome.primary_button, spec.field, value)
        }
        AppearanceSection::SecondaryButton => {
            assign_button_field(&mut settings.chrome.secondary_button, spec.field, value)
        }
        _ => {
            let ValidatedFieldValue::Text(value) = value else {
                return;
            };
            match spec.section {
                AppearanceSection::Chrome => assign_chrome_field(settings, spec.field, value),
                AppearanceSection::Input => {
                    assign_input_field(&mut settings.chrome.input, spec.field, value)
                }
                AppearanceSection::TranscriptShell => assign_transcript_shell_field(
                    &mut settings.chrome.transcript_shell,
                    spec.field,
                    value,
                ),
                AppearanceSection::StatusLine => {
                    assign_status_line_field(&mut settings.chrome.status_line, spec.field, value)
                }
                AppearanceSection::Surfaces => {
                    assign_surface_field(&mut settings.chrome.surfaces, spec.field, value)
                }
                _ => {}
            }
        }
    }
}

fn assign_typography_field(
    settings: &mut AppearanceRoleSettings,
    field: AppearanceField,
    value: ValidatedFieldValue,
) {
    match (field, value) {
        (AppearanceField::FontFamily, ValidatedFieldValue::Text(value)) => {
            settings.font_family = value
        }
        (AppearanceField::FontSize, ValidatedFieldValue::FontSize(value)) => {
            settings.font_size = value
        }
        (AppearanceField::FontWeight, ValidatedFieldValue::FontWeight(value)) => {
            settings.font_weight = value
        }
        (AppearanceField::Foreground, ValidatedFieldValue::Text(value)) => {
            settings.foreground = value
        }
        (AppearanceField::Background, ValidatedFieldValue::Text(value)) => {
            settings.background = value
        }
        _ => {}
    }
}

fn assign_foreground_field(
    settings: &mut AppearanceForegroundSettings,
    field: AppearanceField,
    value: ValidatedFieldValue,
) {
    if let (AppearanceField::Foreground, ValidatedFieldValue::Text(value)) = (field, value) {
        settings.foreground = value;
    }
}

fn assign_chrome_field(settings: &mut AppearanceSettings, field: AppearanceField, value: String) {
    match field {
        AppearanceField::ToolbarBackground => settings.chrome.toolbar_background = value,
        AppearanceField::ConversationThreadStripBackground => {
            settings.chrome.conversation_thread_strip_background = value
        }
        AppearanceField::Separator => settings.chrome.separator = value,
        _ => {}
    }
}

fn assign_button_field(
    settings: &mut crate::AppearanceButtonSettings,
    field: AppearanceField,
    value: ValidatedFieldValue,
) {
    match field {
        AppearanceField::FontWeight => {
            if let ValidatedFieldValue::FontWeight(value) = value {
                settings.font_weight = value;
            }
        }
        AppearanceField::NormalBackground => assign_text(value, |value| {
            settings.normal.background = value;
        }),
        AppearanceField::NormalBorder => assign_text(value, |value| {
            settings.normal.border = value;
        }),
        AppearanceField::NormalForeground => assign_text(value, |value| {
            settings.normal.foreground = value;
        }),
        AppearanceField::HoverBackground => assign_text(value, |value| {
            settings.hover.background = value;
        }),
        AppearanceField::HoverBorder => assign_text(value, |value| {
            settings.hover.border = value;
        }),
        AppearanceField::HoverForeground => assign_text(value, |value| {
            settings.hover.foreground = value;
        }),
        AppearanceField::ActiveBackground => assign_text(value, |value| {
            settings.active.background = value;
        }),
        AppearanceField::ActiveBorder => assign_text(value, |value| {
            settings.active.border = value;
        }),
        AppearanceField::ActiveForeground => assign_text(value, |value| {
            settings.active.foreground = value;
        }),
        AppearanceField::DisabledBackground => assign_text(value, |value| {
            settings.disabled.background = value;
        }),
        AppearanceField::DisabledBorder => assign_text(value, |value| {
            settings.disabled.border = value;
        }),
        AppearanceField::DisabledForeground => assign_text(value, |value| {
            settings.disabled.foreground = value;
        }),
        _ => {}
    }
}

fn assign_text(value: ValidatedFieldValue, assign: impl FnOnce(String)) {
    if let ValidatedFieldValue::Text(value) = value {
        assign(value);
    }
}

fn assign_input_field(
    settings: &mut crate::AppearanceInputSettings,
    field: AppearanceField,
    value: String,
) {
    match field {
        AppearanceField::PanelBackground => settings.panel_background = value,
        AppearanceField::InputBackground => settings.input_background = value,
        AppearanceField::InputBorder => settings.input_border = value,
        AppearanceField::InputForeground => settings.input_foreground = value,
        _ => {}
    }
}

fn assign_transcript_shell_field(
    settings: &mut crate::AppearanceTranscriptShellSettings,
    field: AppearanceField,
    value: String,
) {
    match field {
        AppearanceField::Background => settings.background = value,
        AppearanceField::Foreground => settings.foreground = value,
        _ => {}
    }
}

fn assign_status_line_field(
    settings: &mut crate::AppearanceStatusLineSettings,
    field: AppearanceField,
    value: String,
) {
    match field {
        AppearanceField::Background => settings.background = value,
        AppearanceField::TitleForeground => settings.title_foreground = value,
        AppearanceField::ValueForeground => settings.value_foreground = value,
        _ => {}
    }
}

fn assign_surface_field(
    settings: &mut crate::AppearanceSurfaceSettings,
    field: AppearanceField,
    value: String,
) {
    match field {
        AppearanceField::PanelBackground => settings.panel_background = value,
        AppearanceField::RowBackground => settings.row_background = value,
        AppearanceField::PopupBackground => settings.popup_background = value,
        AppearanceField::Border => settings.border = value,
        AppearanceField::MutedForeground => settings.muted_foreground = value,
        _ => {}
    }
}

pub(crate) fn settings_color_values(settings: &AppearanceSettings) -> Vec<String> {
    field_specs()
        .filter(|spec| spec.field.kind() == SettingsFieldKind::Color)
        .map(|spec| field_value(settings, spec))
        .collect()
}
