use std::collections::HashMap;

use gpui::{Rgba, rgb};

#[derive(Clone, Debug)]
pub(super) struct ShellRoleStyle {
    pub(super) background: Option<Rgba>,
    pub(super) border: Option<Rgba>,
    pub(super) color: Option<Rgba>,
    pub(super) foreground: Option<Rgba>,
    pub(super) font_family: Option<String>,
    pub(super) font_weight: Option<gpui::FontWeight>,
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
        background: shell_style_color(&resolved, role, crate::BerylThemeProperty::Background),
        border: shell_style_color(&resolved, role, crate::BerylThemeProperty::Border),
        color: shell_style_color(&resolved, role, crate::BerylThemeProperty::Color),
        foreground: shell_style_color(&resolved, role, crate::BerylThemeProperty::Foreground),
        font_family: shell_style_font_family(&resolved, role),
        font_weight: shell_style_font_weight(&resolved, role),
    }
}

fn shell_style_color(
    style: &crate::ResolvedStyle,
    role: crate::BerylThemeRole,
    property: crate::BerylThemeProperty,
) -> Option<Rgba> {
    match shell_resolved_property(style, role, property) {
        Some(crate::StylePropertyValue::Color(value)) => chrome_color(value),
        Some(_) => panic!(
            "Beryl theme role {} property {} must resolve as a color",
            role.id(),
            property.id()
        ),
        None => None,
    }
}

fn shell_style_font_family(
    style: &crate::ResolvedStyle,
    role: crate::BerylThemeRole,
) -> Option<String> {
    match shell_resolved_property(style, role, crate::BerylThemeProperty::FontFamily) {
        Some(crate::StylePropertyValue::FontFamily(value)) => Some(value.clone()),
        Some(_) => panic!(
            "Beryl theme role {} property {} must resolve as a font family",
            role.id(),
            crate::BerylThemeProperty::FontFamily.id()
        ),
        None => None,
    }
}

fn shell_style_font_weight(
    style: &crate::ResolvedStyle,
    role: crate::BerylThemeRole,
) -> Option<gpui::FontWeight> {
    match shell_resolved_property(style, role, crate::BerylThemeProperty::FontWeight) {
        Some(crate::StylePropertyValue::FontWeight(value)) => Some(gpui::FontWeight(*value as f32)),
        Some(_) => panic!(
            "Beryl theme role {} property {} must resolve as a font weight",
            role.id(),
            crate::BerylThemeProperty::FontWeight.id()
        ),
        None => None,
    }
}

fn shell_resolved_property(
    style: &crate::ResolvedStyle,
    _role: crate::BerylThemeRole,
    property: crate::BerylThemeProperty,
) -> Option<&crate::StylePropertyValue> {
    style.property(&crate::StylePropertyId::from(property.id()))
}

pub(super) fn style_background(
    styles: &HashMap<crate::BerylThemeRole, ShellRoleStyle>,
    role: crate::BerylThemeRole,
    fallback: Rgba,
) -> Rgba {
    styles
        .get(&role)
        .and_then(|style| style.background)
        .unwrap_or(fallback)
}

pub(super) fn style_border(
    styles: &HashMap<crate::BerylThemeRole, ShellRoleStyle>,
    role: crate::BerylThemeRole,
    fallback: Rgba,
) -> Rgba {
    styles
        .get(&role)
        .and_then(|style| style.border)
        .unwrap_or(fallback)
}

pub(super) fn style_single_color(
    styles: &HashMap<crate::BerylThemeRole, ShellRoleStyle>,
    role: crate::BerylThemeRole,
    fallback: Rgba,
) -> Rgba {
    styles
        .get(&role)
        .and_then(|style| style.color)
        .unwrap_or(fallback)
}

pub(super) fn style_foreground(
    styles: &HashMap<crate::BerylThemeRole, ShellRoleStyle>,
    role: crate::BerylThemeRole,
    fallback: Rgba,
) -> Rgba {
    styles
        .get(&role)
        .and_then(|style| style.foreground)
        .unwrap_or(fallback)
}

pub(super) fn style_single_color_packed_rgb(
    styles: &HashMap<crate::BerylThemeRole, ShellRoleStyle>,
    role: crate::BerylThemeRole,
    fallback: u32,
) -> u32 {
    styles.get(&role).map_or(fallback, |style| {
        style.color.and_then(rgba_to_packed_rgb).unwrap_or(fallback)
    })
}

fn rgba_from_role_color(color: Option<crate::ParsedHexColor>) -> Option<Rgba> {
    color.map(|color| {
        rgb(((color.red() as u32) << 16) | ((color.green() as u32) << 8) | color.blue() as u32)
    })
}

fn rgba_to_packed_rgb(value: Rgba) -> Option<u32> {
    (value.r.is_finite() && value.g.is_finite() && value.b.is_finite()).then(|| {
        let red = (value.r.clamp(0.0, 1.0) * 255.0).round() as u32;
        let green = (value.g.clamp(0.0, 1.0) * 255.0).round() as u32;
        let blue = (value.b.clamp(0.0, 1.0) * 255.0).round() as u32;
        (red << 16) | (green << 8) | blue
    })
}

fn chrome_color(value: &str) -> Option<Rgba> {
    rgba_from_role_color(crate::ParsedHexColor::parse(value))
}
