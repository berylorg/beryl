use gpui_settings_window::{
    RgbColor, SettingsButtonStateTheme, SettingsButtonTheme, SettingsInputTheme,
    SettingsSurfaceTheme, SettingsWindowTheme,
};

use crate::AppearanceSettings;

pub(super) fn settings_window_theme(settings: &AppearanceSettings) -> SettingsWindowTheme {
    let defaults = SettingsWindowTheme::default();
    let chrome = &settings.chrome;
    SettingsWindowTheme {
        window_background: color_or(&settings.general_ui.background, defaults.window_background),
        panel: surface_theme(
            &chrome.surfaces.panel_background,
            &chrome.surfaces.border,
            &settings.general_ui.foreground,
            &chrome.surfaces.muted_foreground,
            &defaults.panel,
        ),
        row: surface_theme(
            &chrome.surfaces.row_background,
            &chrome.surfaces.border,
            &settings.general_ui.foreground,
            &chrome.surfaces.muted_foreground,
            &defaults.row,
        ),
        popup: surface_theme(
            &chrome.surfaces.popup_background,
            &chrome.surfaces.border,
            &settings.general_ui.foreground,
            &chrome.surfaces.muted_foreground,
            &defaults.popup,
        ),
        input: SettingsInputTheme {
            background: color_or(&chrome.input.input_background, defaults.input.background),
            border: color_or(&chrome.input.input_border, defaults.input.border),
            active_border: color_or(
                &chrome.primary_button.normal.border,
                defaults.input.active_border,
            ),
            error_border: defaults.input.error_border,
            foreground: color_or(&chrome.input.input_foreground, defaults.input.foreground),
            caret: color_or(&chrome.input.input_foreground, defaults.input.caret),
            selection_background: color_or(
                &chrome.primary_button.normal.border,
                defaults.input.selection_background,
            ),
        },
        navigation_button: button_theme(&chrome.secondary_button, &defaults.navigation_button),
        primary_button: button_theme(&chrome.primary_button, &defaults.primary_button),
        secondary_button: button_theme(&chrome.secondary_button, &defaults.secondary_button),
    }
}

fn surface_theme(
    background: &str,
    border: &str,
    foreground: &str,
    muted_foreground: &str,
    fallback: &SettingsSurfaceTheme,
) -> SettingsSurfaceTheme {
    SettingsSurfaceTheme {
        background: color_or(background, fallback.background),
        border: color_or(border, fallback.border),
        foreground: color_or(foreground, fallback.foreground),
        muted_foreground: color_or(muted_foreground, fallback.muted_foreground),
    }
}

fn button_theme(
    settings: &crate::AppearanceButtonSettings,
    fallback: &SettingsButtonTheme,
) -> SettingsButtonTheme {
    SettingsButtonTheme {
        font_weight: font_weight_or(settings.font_weight, fallback.font_weight),
        normal: button_state_theme(&settings.normal, &fallback.normal),
        hover: button_state_theme(&settings.hover, &fallback.hover),
        active: button_state_theme(&settings.active, &fallback.active),
        disabled: button_state_theme(&settings.disabled, &fallback.disabled),
    }
}

fn button_state_theme(
    settings: &crate::AppearanceButtonStateSettings,
    fallback: &SettingsButtonStateTheme,
) -> SettingsButtonStateTheme {
    SettingsButtonStateTheme {
        background: color_or(&settings.background, fallback.background),
        border: color_or(&settings.border, fallback.border),
        foreground: color_or(&settings.foreground, fallback.foreground),
    }
}

fn color_or(value: &str, fallback: RgbColor) -> RgbColor {
    RgbColor::parse(value).unwrap_or(fallback)
}

fn font_weight_or(value: u16, fallback: u16) -> u16 {
    if (100..=900).contains(&value) {
        value
    } else {
        fallback
    }
}
