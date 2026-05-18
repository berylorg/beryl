use serde_json::{Value, json};

use crate::{
    StylePropertyKind, StylePropertySource, StylePropertyValue, ThemeDocument, ThemeSchema,
    built_in_theme_schema,
};

use super::MAX_THEME_SCHEMA_ROLE_LIMIT;

pub fn theme_schema_value(
    role_prefix: Option<&str>,
    limit: usize,
) -> Result<Value, super::ThemeDynamicToolError> {
    let schema = built_in_theme_schema();
    Ok(theme_schema_value_from_schema(&schema, role_prefix, limit))
}

fn theme_schema_value_from_schema(
    schema: &ThemeSchema,
    role_prefix: Option<&str>,
    limit: usize,
) -> Value {
    let filtered = schema.roles().iter().filter(|role| {
        !role.properties().is_empty()
            && role_prefix.is_none_or(|prefix| role.role_id().as_str().starts_with(prefix))
    });
    let mut total_count = 0usize;
    let mut roles = Vec::new();
    for role in filtered {
        total_count = total_count.saturating_add(1);
        if roles.len() >= limit {
            continue;
        }
        roles.push(json!({
            "id": role.role_id().as_str(),
            "staticParent": role.static_parent().map(|parent| parent.as_str()),
            "properties": role.properties().iter().map(|(property_id, property)| {
                json!({
                    "id": property_id.as_str(),
                    "kind": property_kind_label(property.kind()),
                    "fallback": property_value_json(property.fallback()),
                })
            }).collect::<Vec<_>>(),
        }));
    }

    json!({
        "roles": roles,
        "roleCount": total_count,
        "rolesTruncated": total_count > limit,
        "supportedSources": ["static_parent", "ambient_parent", "fallback", "concrete_value"],
    })
}

fn property_kind_label(kind: StylePropertyKind) -> &'static str {
    match kind {
        StylePropertyKind::Color => "color",
        StylePropertyKind::FontFamily => "font_family",
        StylePropertyKind::LogicalPixels => "logical_pixels",
        StylePropertyKind::FontWeight => "font_weight",
    }
}

pub(super) fn property_source_json(source: &StylePropertySource) -> Value {
    match source {
        StylePropertySource::Concrete(value) => {
            json!({ "source": "concrete", "value": property_value_json(value) })
        }
        StylePropertySource::StaticParent => json!({ "source": "static_parent" }),
        StylePropertySource::AmbientParent => json!({ "source": "ambient_parent" }),
        StylePropertySource::Fallback => json!({ "source": "fallback" }),
    }
}

pub(super) fn property_value_json(value: &StylePropertyValue) -> Value {
    match value {
        StylePropertyValue::Color(value) => json!({ "kind": "color", "value": value }),
        StylePropertyValue::FontFamily(value) => {
            json!({ "kind": "font_family", "value": value })
        }
        StylePropertyValue::LogicalPixels(value) => {
            json!({ "kind": "logical_pixels", "value": value })
        }
        StylePropertyValue::FontWeight(value) => json!({ "kind": "font_weight", "value": value }),
    }
}

pub fn theme_document_summary_value(document: &ThemeDocument) -> Value {
    let schema = built_in_theme_schema();
    json!({
        "embeddedId": document.id().map(|id| id.as_str()),
        "name": document.name(),
        "roleCount": document.definition().roles().len(),
        "roles": document.definition().roles().iter().take(MAX_THEME_SCHEMA_ROLE_LIMIT).map(|role| {
            let supported_properties = schema
                .roles()
                .iter()
                .find(|schema_role| schema_role.role_id() == role.role_id())
                .map(|schema_role| {
                    schema_role.properties().iter().map(|(property_id, property)| {
                        json!({
                            "id": property_id.as_str(),
                            "kind": property_kind_label(property.kind()),
                        })
                    }).collect::<Vec<_>>()
                })
                .unwrap_or_default();
            json!({
                "id": role.role_id().as_str(),
                "staticParent": role.static_parent().map(|parent| parent.as_str()),
                "supportedProperties": supported_properties,
                "properties": role.properties().iter().map(|(property_id, source)| {
                    json!({
                        "id": property_id.as_str(),
                        "value": property_source_json(source),
                    })
                }).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
        "rolesTruncated": document.definition().roles().len() > MAX_THEME_SCHEMA_ROLE_LIMIT,
    })
}
