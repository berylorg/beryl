use std::collections::HashMap;

use gpui::{Rgba, rgb};

#[derive(Clone, Debug)]
pub(super) struct ShellRoleStyle {
    pub(super) background: Rgba,
    pub(super) border: Rgba,
    pub(super) foreground: Rgba,
    pub(super) font_family: String,
    pub(super) font_weight: gpui::FontWeight,
}

pub(super) fn shell_role_styles(
    projection: &crate::ActiveThemeProjection,
) -> HashMap<crate::BerylThemeRole, ShellRoleStyle> {
    crate::BerylThemeRole::ALL
        .iter()
        .copied()
        .map(|role| (role, shell_role_style(projection, role)))
        .collect()
}

fn shell_role_style(
    projection: &crate::ActiveThemeProjection,
    role: crate::BerylThemeRole,
) -> ShellRoleStyle {
    let resolved = projection
        .resolve_style(role.id(), &crate::ThemeResolutionContext::new())
        .or_else(|_| projection.default_style(role.id()).cloned())
        .unwrap_or_else(|_| panic!("Beryl theme role {} must resolve", role.id()));
    ShellRoleStyle {
        background: shell_style_color(
            &resolved,
            role,
            crate::BerylThemeProperty::Background,
            rgb(0x000000),
        ),
        border: shell_style_color(
            &resolved,
            role,
            crate::BerylThemeProperty::Border,
            rgb(0x000000),
        ),
        foreground: shell_style_color(
            &resolved,
            role,
            crate::BerylThemeProperty::Foreground,
            rgb(0xffffff),
        ),
        font_family: shell_style_font_family(&resolved, role),
        font_weight: shell_style_font_weight(&resolved, role),
    }
}

fn shell_style_color(
    style: &crate::ResolvedStyle,
    role: crate::BerylThemeRole,
    property: crate::BerylThemeProperty,
    fallback: Rgba,
) -> Rgba {
    match shell_resolved_property(style, role, property) {
        crate::StylePropertyValue::Color(value) => chrome_color(value, fallback),
        _ => fallback,
    }
}

fn shell_style_font_family(style: &crate::ResolvedStyle, role: crate::BerylThemeRole) -> String {
    match shell_resolved_property(style, role, crate::BerylThemeProperty::FontFamily) {
        crate::StylePropertyValue::FontFamily(value) => value.clone(),
        _ => "Inter".to_string(),
    }
}

fn shell_style_font_weight(
    style: &crate::ResolvedStyle,
    role: crate::BerylThemeRole,
) -> gpui::FontWeight {
    match shell_resolved_property(style, role, crate::BerylThemeProperty::FontWeight) {
        crate::StylePropertyValue::FontWeight(value) => gpui::FontWeight(*value as f32),
        _ => gpui::FontWeight::NORMAL,
    }
}

fn shell_resolved_property(
    style: &crate::ResolvedStyle,
    role: crate::BerylThemeRole,
    property: crate::BerylThemeProperty,
) -> &crate::StylePropertyValue {
    style
        .property(&crate::StylePropertyId::from(property.id()))
        .unwrap_or_else(|| {
            panic!(
                "Beryl theme role {} missing resolved property {}",
                role.id(),
                property.id()
            )
        })
}

pub(super) fn style_background(
    styles: &HashMap<crate::BerylThemeRole, ShellRoleStyle>,
    role: crate::BerylThemeRole,
    fallback: Rgba,
) -> Rgba {
    styles
        .get(&role)
        .map(|style| style.background)
        .unwrap_or(fallback)
}

pub(super) fn style_border(
    styles: &HashMap<crate::BerylThemeRole, ShellRoleStyle>,
    role: crate::BerylThemeRole,
    fallback: Rgba,
) -> Rgba {
    styles
        .get(&role)
        .map(|style| style.border)
        .unwrap_or(fallback)
}

pub(super) fn style_foreground(
    styles: &HashMap<crate::BerylThemeRole, ShellRoleStyle>,
    role: crate::BerylThemeRole,
    fallback: Rgba,
) -> Rgba {
    styles
        .get(&role)
        .map(|style| style.foreground)
        .unwrap_or(fallback)
}

pub(super) fn style_background_packed_rgb(
    styles: &HashMap<crate::BerylThemeRole, ShellRoleStyle>,
    role: crate::BerylThemeRole,
    fallback: u32,
) -> u32 {
    styles.get(&role).map_or(fallback, |style| {
        rgba_to_packed_rgb(style.background).unwrap_or(fallback)
    })
}

fn rgba_from_role_color(color: Option<crate::ParsedHexColor>, fallback: Rgba) -> Rgba {
    color
        .map(|color| {
            rgb(((color.red() as u32) << 16) | ((color.green() as u32) << 8) | color.blue() as u32)
        })
        .unwrap_or(fallback)
}

fn rgba_to_packed_rgb(value: Rgba) -> Option<u32> {
    (value.r.is_finite() && value.g.is_finite() && value.b.is_finite()).then(|| {
        let red = (value.r.clamp(0.0, 1.0) * 255.0).round() as u32;
        let green = (value.g.clamp(0.0, 1.0) * 255.0).round() as u32;
        let blue = (value.b.clamp(0.0, 1.0) * 255.0).round() as u32;
        (red << 16) | (green << 8) | blue
    })
}

fn chrome_color(value: &str, fallback: Rgba) -> Rgba {
    rgba_from_role_color(crate::ParsedHexColor::parse(value), fallback)
}
