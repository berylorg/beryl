use gpui_settings_window::{
    RgbColor, SettingsButtonStateTheme, SettingsButtonTheme, SettingsInputTheme,
    SettingsSurfaceTheme, SettingsWindowTheme,
};

use crate::{
    ActiveThemeProjection, BerylThemeProperty, BerylThemeRole, StylePropertyValue,
    ThemeResolutionContext,
};

pub(super) fn settings_window_theme(theme: &ActiveThemeProjection) -> SettingsWindowTheme {
    let defaults = SettingsWindowTheme::default();
    SettingsWindowTheme {
        window_background: color_or(
            theme,
            BerylThemeRole::SettingsWindow,
            BerylThemeProperty::Background,
            defaults.window_background,
        ),
        panel: surface_theme(
            theme,
            BerylThemeRole::SettingsGroup,
            BerylThemeRole::SettingsRowDisabledText,
            &defaults.panel,
        ),
        row: surface_theme(
            theme,
            BerylThemeRole::SettingsRowNormal,
            BerylThemeRole::SettingsRowDisabledText,
            &defaults.row,
        ),
        popup: surface_theme(
            theme,
            BerylThemeRole::SettingsPopup,
            BerylThemeRole::SettingsRowDisabledText,
            &defaults.popup,
        ),
        input: SettingsInputTheme {
            background: color_or(
                theme,
                BerylThemeRole::SettingsInputNormal,
                BerylThemeProperty::Background,
                defaults.input.background,
            ),
            border: color_or(
                theme,
                BerylThemeRole::SettingsInputNormal,
                BerylThemeProperty::Border,
                defaults.input.border,
            ),
            active_border: color_or(
                theme,
                BerylThemeRole::SettingsInputFocused,
                BerylThemeProperty::Border,
                defaults.input.active_border,
            ),
            error_border: color_or(
                theme,
                BerylThemeRole::SettingsInputError,
                BerylThemeProperty::Border,
                defaults.input.error_border,
            ),
            foreground: color_or(
                theme,
                BerylThemeRole::SettingsInputText,
                BerylThemeProperty::Foreground,
                defaults.input.foreground,
            ),
            caret: color_or(
                theme,
                BerylThemeRole::SettingsInputCaret,
                BerylThemeProperty::Color,
                defaults.input.caret,
            ),
            selection_background: color_or(
                theme,
                BerylThemeRole::SettingsInputSelection,
                BerylThemeProperty::TextBackground,
                defaults.input.selection_background,
            ),
        },
        navigation_button: button_theme(
            theme,
            BerylThemeRole::SettingsButtonSecondary,
            BerylThemeRole::SettingsButtonSecondaryLabel,
            BerylThemeRole::ButtonSecondaryHover,
            BerylThemeRole::ButtonSecondaryActive,
            BerylThemeRole::ButtonSecondaryDisabled,
            &defaults.navigation_button,
        ),
        primary_button: button_theme(
            theme,
            BerylThemeRole::SettingsButtonPrimary,
            BerylThemeRole::SettingsButtonPrimaryLabel,
            BerylThemeRole::ButtonPrimaryHover,
            BerylThemeRole::ButtonPrimaryActive,
            BerylThemeRole::ButtonPrimaryDisabled,
            &defaults.primary_button,
        ),
        secondary_button: button_theme(
            theme,
            BerylThemeRole::SettingsButtonSecondary,
            BerylThemeRole::SettingsButtonSecondaryLabel,
            BerylThemeRole::ButtonSecondaryHover,
            BerylThemeRole::ButtonSecondaryActive,
            BerylThemeRole::ButtonSecondaryDisabled,
            &defaults.secondary_button,
        ),
    }
}

fn surface_theme(
    theme: &ActiveThemeProjection,
    role: BerylThemeRole,
    muted_role: BerylThemeRole,
    fallback: &SettingsSurfaceTheme,
) -> SettingsSurfaceTheme {
    SettingsSurfaceTheme {
        background: color_or(
            theme,
            role,
            BerylThemeProperty::Background,
            fallback.background,
        ),
        border: color_or(theme, role, BerylThemeProperty::Border, fallback.border),
        foreground: color_or(
            theme,
            role,
            BerylThemeProperty::Foreground,
            fallback.foreground,
        ),
        muted_foreground: color_or(
            theme,
            muted_role,
            BerylThemeProperty::Foreground,
            fallback.muted_foreground,
        ),
    }
}

fn button_theme(
    theme: &ActiveThemeProjection,
    normal: BerylThemeRole,
    label: BerylThemeRole,
    hover: BerylThemeRole,
    active: BerylThemeRole,
    disabled: BerylThemeRole,
    fallback: &SettingsButtonTheme,
) -> SettingsButtonTheme {
    SettingsButtonTheme {
        font_weight: font_weight_or(theme, label, fallback.font_weight),
        normal: button_state_theme(theme, normal, &fallback.normal),
        hover: button_state_theme(theme, hover, &fallback.hover),
        active: button_state_theme(theme, active, &fallback.active),
        disabled: button_state_theme(theme, disabled, &fallback.disabled),
    }
}

fn button_state_theme(
    theme: &ActiveThemeProjection,
    role: BerylThemeRole,
    fallback: &SettingsButtonStateTheme,
) -> SettingsButtonStateTheme {
    SettingsButtonStateTheme {
        background: color_or(
            theme,
            role,
            BerylThemeProperty::Background,
            fallback.background,
        ),
        border: color_or(theme, role, BerylThemeProperty::Border, fallback.border),
        foreground: color_or(
            theme,
            role,
            BerylThemeProperty::Foreground,
            fallback.foreground,
        ),
    }
}

fn color_or(
    theme: &ActiveThemeProjection,
    role: BerylThemeRole,
    property: BerylThemeProperty,
    fallback: RgbColor,
) -> RgbColor {
    match theme.resolve_property(role.id(), property.id(), &ThemeResolutionContext::new()) {
        Ok(StylePropertyValue::Color(value)) => RgbColor::parse(&value).unwrap_or(fallback),
        _ => fallback,
    }
}

fn font_weight_or(theme: &ActiveThemeProjection, role: BerylThemeRole, fallback: u16) -> u16 {
    match theme.resolve_property(
        role.id(),
        BerylThemeProperty::FontWeight.id(),
        &ThemeResolutionContext::new(),
    ) {
        Ok(StylePropertyValue::FontWeight(value)) => value,
        _ => fallback,
    }
}
