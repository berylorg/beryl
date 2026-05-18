use gpui_settings_window::{RgbColor, SettingsFieldKind, SettingsPageSplitItemPreviewStyle};

use crate::{
    ActiveThemeProjection, BerylThemeRole, StylePropertyId, StylePropertyKind, StylePropertySource,
    StylePropertyValue, StyleRoleId, ThemeDefinition, ThemeResolver, ThemeRoleSchema,
    built_in_theme_schema,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PropertySourceChoice {
    Value,
    StaticParent,
    AmbientParent,
    Fallback,
}

impl PropertySourceChoice {
    pub(super) fn from_source(source: Option<&StylePropertySource>) -> Self {
        match source {
            Some(StylePropertySource::Concrete(_)) => Self::Value,
            Some(StylePropertySource::StaticParent) => Self::StaticParent,
            Some(StylePropertySource::AmbientParent) => Self::AmbientParent,
            Some(StylePropertySource::Fallback) | None => Self::Fallback,
        }
    }

    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "value" => Some(Self::Value),
            "static_parent" => Some(Self::StaticParent),
            "ambient_parent" => Some(Self::AmbientParent),
            "fallback" => Some(Self::Fallback),
            _ => None,
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Value => "value",
            Self::StaticParent => "static_parent",
            Self::AmbientParent => "ambient_parent",
            Self::Fallback => "fallback",
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Value => "Value",
            Self::StaticParent => "Static parent",
            Self::AmbientParent => "Ambient parent",
            Self::Fallback => "Fallback",
        }
    }
}

pub(super) fn source_choices(
    static_parent: Option<&StyleRoleId>,
) -> Vec<(PropertySourceChoice, String)> {
    let mut choices = vec![(
        PropertySourceChoice::Value,
        PropertySourceChoice::Value.label().to_string(),
    )];
    if let Some(static_parent) = static_parent {
        choices.push((
            PropertySourceChoice::StaticParent,
            static_parent.as_str().to_string(),
        ));
    }
    choices.extend([
        (
            PropertySourceChoice::AmbientParent,
            PropertySourceChoice::AmbientParent.label().to_string(),
        ),
        (
            PropertySourceChoice::Fallback,
            PropertySourceChoice::Fallback.label().to_string(),
        ),
    ]);
    choices
}

pub(super) fn projection_from_definition(
    definition: &ThemeDefinition,
) -> Option<ActiveThemeProjection> {
    let resolver = ThemeResolver::new(built_in_theme_schema(), definition.clone()).ok()?;
    ActiveThemeProjection::from_built_in_resolver(resolver).ok()
}

pub(super) fn validate_property_value(
    property_id: &StylePropertyId,
    kind: StylePropertyKind,
    value: &str,
) -> Result<StylePropertyValue, String> {
    match kind {
        StylePropertyKind::Color => RgbColor::parse(value)
            .map(|color| StylePropertyValue::color(color.to_hex()))
            .ok_or_else(|| {
                format!(
                    "{} must use #rrggbb hex color syntax.",
                    property_label(property_id)
                )
            }),
        StylePropertyKind::FontFamily => {
            let value = value.trim();
            if value.is_empty() {
                Err(format!(
                    "{} must not be empty.",
                    property_label(property_id)
                ))
            } else {
                Ok(StylePropertyValue::font_family(value))
            }
        }
        StylePropertyKind::LogicalPixels => {
            let parsed = value.trim().parse::<f32>().map_err(|_| {
                format!(
                    "{} must be a non-negative number.",
                    property_label(property_id)
                )
            })?;
            if parsed.is_finite() && parsed >= 0.0 {
                Ok(StylePropertyValue::logical_pixels(parsed))
            } else {
                Err(format!(
                    "{} must be a non-negative number.",
                    property_label(property_id)
                ))
            }
        }
        StylePropertyKind::FontWeight => {
            let parsed = value.trim().parse::<u16>().map_err(|_| {
                format!(
                    "{} must be an integer from 100 to 900.",
                    property_label(property_id)
                )
            })?;
            if (100..=900).contains(&parsed) {
                Ok(StylePropertyValue::font_weight(parsed))
            } else {
                Err(format!(
                    "{} must be an integer from 100 to 900.",
                    property_label(property_id)
                ))
            }
        }
    }
}

pub(super) fn role_is_editable(role_id: &StyleRoleId) -> bool {
    role_schema(role_id).is_some_and(|schema| !schema.properties().is_empty())
}

pub(super) fn editable_theme_roles() -> impl Iterator<Item = BerylThemeRole> {
    BerylThemeRole::ALL.iter().copied().filter(|role| {
        role_schema(&StyleRoleId::from(role.id()))
            .is_some_and(|schema| !schema.properties().is_empty())
    })
}

pub(super) fn role_schema(role_id: &StyleRoleId) -> Option<ThemeRoleSchema> {
    built_in_theme_schema()
        .roles()
        .iter()
        .find(|role| role.role_id() == role_id)
        .cloned()
}

pub(super) fn property_kind(
    role_id: &StyleRoleId,
    property_id: &StylePropertyId,
) -> Option<StylePropertyKind> {
    role_schema(role_id)?
        .properties()
        .get(property_id)
        .map(|property| property.kind())
}

pub(super) fn preview_style(
    projection: &ActiveThemeProjection,
    role_id: &StyleRoleId,
) -> SettingsPageSplitItemPreviewStyle {
    let Some(role_schema) = role_schema(role_id) else {
        return SettingsPageSplitItemPreviewStyle::default();
    };
    let Ok(style) = projection.default_style(role_id.clone()) else {
        return SettingsPageSplitItemPreviewStyle::default();
    };
    let mut preview = SettingsPageSplitItemPreviewStyle::default();
    for (property_id, _) in role_schema.properties() {
        let Some(value) = style.property(property_id) else {
            continue;
        };
        preview = apply_preview_value(preview, property_id, value);
    }
    preview
}

pub(super) fn apply_preview_value(
    preview: SettingsPageSplitItemPreviewStyle,
    property_id: &StylePropertyId,
    value: &StylePropertyValue,
) -> SettingsPageSplitItemPreviewStyle {
    match (property_id.as_str(), value) {
        ("foreground", StylePropertyValue::Color(value)) => {
            if let Some(color) = RgbColor::parse(value) {
                preview.with_foreground(color)
            } else {
                preview
            }
        }
        ("background", StylePropertyValue::Color(value)) => {
            if let Some(color) = RgbColor::parse(value) {
                preview.with_background(color)
            } else {
                preview
            }
        }
        ("border" | "color", StylePropertyValue::Color(value)) => {
            if let Some(color) = RgbColor::parse(value) {
                preview.with_border(color)
            } else {
                preview
            }
        }
        ("font_family", StylePropertyValue::FontFamily(value)) => {
            preview.with_font_family(value.clone())
        }
        ("font_size", StylePropertyValue::LogicalPixels(value)) => {
            preview.with_font_size((*value).round().clamp(1.0, 96.0) as u16)
        }
        ("font_weight", StylePropertyValue::FontWeight(value)) => preview.with_font_weight(*value),
        _ => preview,
    }
}

pub(super) fn property_value_text(
    projection: &ActiveThemeProjection,
    role_id: &StyleRoleId,
    property_id: &StylePropertyId,
    kind: StylePropertyKind,
) -> String {
    projection
        .resolve_property(
            role_id.clone(),
            property_id.clone(),
            &crate::ThemeResolutionContext::new(),
        )
        .ok()
        .filter(|value| value.kind() == kind)
        .map(|value| style_value_text(&value))
        .unwrap_or_default()
}

pub(super) fn style_value_text(value: &StylePropertyValue) -> String {
    match value {
        StylePropertyValue::Color(value) | StylePropertyValue::FontFamily(value) => value.clone(),
        StylePropertyValue::LogicalPixels(value) => format!("{value:.1}"),
        StylePropertyValue::FontWeight(value) => value.to_string(),
    }
}

pub(super) fn field_kind(kind: StylePropertyKind) -> SettingsFieldKind {
    match kind {
        StylePropertyKind::Color => SettingsFieldKind::Color,
        StylePropertyKind::FontFamily => SettingsFieldKind::Text,
        StylePropertyKind::LogicalPixels | StylePropertyKind::FontWeight => {
            SettingsFieldKind::Number
        }
    }
}

pub(super) fn property_label(property_id: &StylePropertyId) -> String {
    property_id
        .as_str()
        .split('_')
        .enumerate()
        .map(|(index, word)| {
            if index == 0 {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                    None => String::new(),
                }
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
