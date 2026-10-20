use beryl_state::{ThemePropertyId as Property, ThemeValue};
use gpui::{FontWeight, Rgba, SharedString};

use crate::theme_runtime::{AdapterFailureClass, AppearanceGeneration};

#[derive(Clone)]
pub(super) struct NavigatorStyle {
    pub(super) foreground: Rgba,
    pub(super) navigator_background: Rgba,
    pub(super) navigator_border: Rgba,
    pub(super) column_background: Rgba,
    pub(super) column_border: Rgba,
    pub(super) header_foreground: Rgba,
    pub(super) row_foreground: Rgba,
    pub(super) hover_background: Rgba,
    pub(super) hover_foreground: Rgba,
    pub(super) pressed_background: Rgba,
    pub(super) pressed_foreground: Rgba,
    pub(super) selected_background: Rgba,
    pub(super) selected_foreground: Rgba,
    pub(super) focus_ring: Rgba,
    pub(super) tooltip_background: Rgba,
    pub(super) tooltip_foreground: Rgba,
    pub(super) tooltip_border: Rgba,
    pub(super) scrollbar_thumb: u32,
    pub(super) font_family: SharedString,
    pub(super) font_size: f32,
    pub(super) font_weight: FontWeight,
}

impl NavigatorStyle {
    pub(super) fn prepare(generation: &AppearanceGeneration) -> Result<Self, AdapterFailureClass> {
        let value = |role: &str, property| {
            generation
                .prepared()
                .appearance()
                .roles()
                .iter()
                .find(|(id, _)| id.as_str() == role)
                .and_then(|(_, style)| style.property(property))
                .ok_or(AdapterFailureClass::Rejected)
        };
        let rgb = |role, property| match value(role, property)? {
            ThemeValue::Color(color) => {
                let [r, g, b] = color.rgb();
                Ok(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
            }
            _ => Err(AdapterFailureClass::Rejected),
        };
        let color = |role, property| rgb(role, property).map(gpui::rgb);
        Ok(Self {
            foreground: color("theme-editor", Property::Foreground)?,
            navigator_background: color("theme-editor.navigator", Property::Background)?,
            navigator_border: color("theme-editor.navigator", Property::BorderColor)?,
            column_background: color("theme-editor.column", Property::Background)?,
            column_border: color("theme-editor.column", Property::BorderColor)?,
            header_foreground: color("theme-editor.column-header", Property::Foreground)?,
            row_foreground: color("theme-editor.role-row", Property::Foreground)?,
            hover_background: color("theme-editor.role-row.hover", Property::Background)?,
            hover_foreground: color("theme-editor.role-row.hover", Property::Foreground)?,
            pressed_background: color("theme-editor.role-row.pressed", Property::Background)?,
            pressed_foreground: color("theme-editor.role-row.pressed", Property::Foreground)?,
            selected_background: color("theme-editor.role-row.selected", Property::Background)?,
            selected_foreground: color("theme-editor.role-row.selected", Property::Foreground)?,
            focus_ring: color("theme-editor.role-row.focused", Property::Color)?,
            tooltip_background: color("tooltip", Property::Background)?,
            tooltip_foreground: color("tooltip.text", Property::Foreground)?,
            tooltip_border: color("tooltip", Property::Border)?,
            scrollbar_thumb: rgb("scrollbar.thumb", Property::Color)?,
            font_family: match value("settings.window.text", Property::FontFamily)? {
                ThemeValue::FontFamily(value) => value.as_str().to_owned().into(),
                _ => return Err(AdapterFailureClass::Rejected),
            },
            font_size: match value("settings.window.text", Property::FontSize)? {
                ThemeValue::LogicalPixels(value) => value.get(),
                _ => return Err(AdapterFailureClass::Rejected),
            },
            font_weight: match value("settings.window.text", Property::FontWeight)? {
                ThemeValue::FontWeight(value) => FontWeight(value.get() as f32),
                _ => return Err(AdapterFailureClass::Rejected),
            },
        })
    }
}
