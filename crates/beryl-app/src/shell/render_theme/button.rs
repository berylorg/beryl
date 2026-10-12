use std::collections::HashMap;

use gpui::rgb;

use super::role_style::ShellRoleStyle;

pub(super) use crate::widgets::command_style::{ChromeButtonStateTheme, ChromeButtonTheme};

impl ChromeButtonTheme {
    pub(super) fn primary() -> Self {
        Self {
            font_weight: gpui::FontWeight(500.0),
            normal: ChromeButtonStateTheme::new(rgb(0x1d4ed8), rgb(0x3b82f6), rgb(0xeff6ff)),
            hover: ChromeButtonStateTheme::new(rgb(0x2563eb), rgb(0x60a5fa), rgb(0xffffff)),
            active: ChromeButtonStateTheme::new(rgb(0x1e40af), rgb(0x3b82f6), rgb(0xffffff)),
            disabled: ChromeButtonStateTheme::new(rgb(0x334155), rgb(0x475569), rgb(0x94a3b8)),
        }
    }
}

pub(super) fn button_theme_from_styles(
    styles: &HashMap<crate::BerylThemeRole, ShellRoleStyle>,
    normal: crate::BerylThemeRole,
    label: crate::BerylThemeRole,
    hover: crate::BerylThemeRole,
    active: crate::BerylThemeRole,
    disabled: crate::BerylThemeRole,
    fallback: ChromeButtonTheme,
) -> ChromeButtonTheme {
    ChromeButtonTheme {
        font_weight: styles
            .get(&label)
            .and_then(|style| style.font_weight)
            .unwrap_or(fallback.font_weight),
        normal: button_state_from_styles(styles, normal, fallback.normal),
        hover: button_state_from_styles(styles, hover, fallback.hover),
        active: button_state_from_styles(styles, active, fallback.active),
        disabled: button_state_from_styles(styles, disabled, fallback.disabled),
    }
}

fn button_state_from_styles(
    styles: &HashMap<crate::BerylThemeRole, ShellRoleStyle>,
    role: crate::BerylThemeRole,
    fallback: ChromeButtonStateTheme,
) -> ChromeButtonStateTheme {
    styles
        .get(&role)
        .map_or(fallback, |style| ChromeButtonStateTheme {
            background: style.background.unwrap_or(fallback.background),
            border: style.border.unwrap_or(fallback.border),
            foreground: style.foreground.unwrap_or(fallback.foreground),
        })
}
