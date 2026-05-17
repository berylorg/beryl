use std::collections::BTreeMap;

use thiserror::Error;
use toml::Value;

use super::types::{InstalledThemeId, validate_theme_name};
use crate::appearance::theme::{
    StylePropertyKind, StylePropertySource, StylePropertyValue, ThemeDefinition,
    ThemeRoleDefinition, ThemeValidationDiagnostics, built_in_theme_schema,
};

pub const THEME_DOCUMENT_SCHEMA_VERSION: i64 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeDocument {
    id: Option<InstalledThemeId>,
    name: Option<String>,
    definition: ThemeDefinition,
}

#[derive(Debug, Error)]
pub enum ThemeDocumentError {
    #[error("failed to parse theme document TOML")]
    ParseToml {
        #[source]
        source: toml::de::Error,
    },
    #[error("failed to serialize theme document TOML")]
    SerializeToml {
        #[source]
        source: toml::ser::Error,
    },
    #[error("theme document schema must be 1")]
    InvalidSchema,
    #[error("theme document id is invalid")]
    InvalidId,
    #[error("theme document name is invalid")]
    InvalidName,
    #[error("theme document role record is invalid")]
    InvalidRoleRecord,
    #[error("theme document property `{property}` on role `{role}` is invalid")]
    InvalidPropertySource { role: String, property: String },
    #[error("theme document is not valid for the Beryl theme schema")]
    Validation {
        #[source]
        source: ThemeValidationDiagnostics,
    },
}

impl ThemeDocument {
    pub fn new(
        id: Option<InstalledThemeId>,
        name: Option<String>,
        definition: ThemeDefinition,
    ) -> Result<Self, ThemeDocumentError> {
        if let Some(name) = name.as_deref()
            && validate_theme_name(name).is_none()
        {
            return Err(ThemeDocumentError::InvalidName);
        }

        validate_definition(&definition)?;
        Ok(Self {
            id,
            name: name.map(|name| name.trim().to_string()),
            definition,
        })
    }

    pub fn from_toml_str(text: &str) -> Result<Self, ThemeDocumentError> {
        Self::from_toml_str_with_policy(text, UnsupportedPropertyPolicy::Strict)
    }

    pub(crate) fn from_toml_str_ignoring_unsupported_properties(
        text: &str,
    ) -> Result<Self, ThemeDocumentError> {
        Self::from_toml_str_with_policy(text, UnsupportedPropertyPolicy::Ignore)
    }

    fn from_toml_str_with_policy(
        text: &str,
        unsupported_property_policy: UnsupportedPropertyPolicy,
    ) -> Result<Self, ThemeDocumentError> {
        let value = toml::from_str::<Value>(text)
            .map_err(|source| ThemeDocumentError::ParseToml { source })?;
        let table = value
            .as_table()
            .ok_or(ThemeDocumentError::InvalidRoleRecord)?;

        match table.get("schema").and_then(Value::as_integer) {
            Some(THEME_DOCUMENT_SCHEMA_VERSION) => {}
            _ => return Err(ThemeDocumentError::InvalidSchema),
        }

        let id = match table.get("id").and_then(Value::as_str) {
            Some(value) => Some(
                InstalledThemeId::new(value.to_string())
                    .map_err(|_| ThemeDocumentError::InvalidId)?,
            ),
            None => None,
        };
        let name = match table.get("name").and_then(Value::as_str) {
            Some(value) => Some(validate_theme_name(value).ok_or(ThemeDocumentError::InvalidName)?),
            None => None,
        };
        let definition = definition_from_toml_table(table, unsupported_property_policy)?;
        validate_definition(&definition)?;

        Ok(Self {
            id,
            name,
            definition,
        })
    }

    pub fn to_toml_string(&self) -> Result<String, ThemeDocumentError> {
        validate_definition(&self.definition)?;

        let mut text = String::new();
        text.push_str("schema = 1\n");
        if let Some(id) = &self.id {
            text.push_str("id = ");
            text.push_str(&toml_string_literal(id.as_str())?);
            text.push('\n');
        }
        if let Some(name) = &self.name {
            text.push_str("name = ");
            text.push_str(&toml_string_literal(name)?);
            text.push('\n');
        }

        for role in self.definition.roles() {
            text.push_str("\n[[role]]\n");
            text.push_str("id = ");
            text.push_str(&toml_string_literal(role.role_id().as_str())?);
            text.push('\n');
            if let Some(parent) = role.static_parent() {
                text.push_str("static_parent = ");
                text.push_str(&toml_string_literal(parent.as_str())?);
                text.push('\n');
            }
            for (property_id, source) in role.properties() {
                text.push_str(property_id.as_str());
                text.push_str(" = ");
                text.push_str(&source_toml_value(source)?);
                text.push('\n');
            }
        }

        Ok(text)
    }

    pub fn id(&self) -> Option<&InstalledThemeId> {
        self.id.as_ref()
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn definition(&self) -> &ThemeDefinition {
        &self.definition
    }

    pub fn into_definition(self) -> ThemeDefinition {
        self.definition
    }
}

#[derive(Clone, Copy)]
enum UnsupportedPropertyPolicy {
    Strict,
    Ignore,
}

fn definition_from_toml_table(
    table: &toml::Table,
    unsupported_property_policy: UnsupportedPropertyPolicy,
) -> Result<ThemeDefinition, ThemeDocumentError> {
    let schema = built_in_theme_schema();
    let roles = table
        .get("role")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let mut definitions = Vec::new();

    for role_value in roles {
        let role_table = role_value
            .as_table()
            .ok_or(ThemeDocumentError::InvalidRoleRecord)?;
        let role_id = role_table
            .get("id")
            .and_then(Value::as_str)
            .ok_or(ThemeDocumentError::InvalidRoleRecord)?
            .to_string();
        let mut role = ThemeRoleDefinition::new(role_id.clone());
        if let Some(parent) = role_table.get("static_parent").and_then(Value::as_str) {
            role = role.with_static_parent(parent.to_string());
        }

        let properties = role_table
            .iter()
            .filter(|(key, _)| *key != "id" && *key != "static_parent")
            .collect::<BTreeMap<_, _>>();
        for (property_id, value) in properties {
            let kind = schema_property_kind(&schema, &role_id, property_id);
            if matches!(
                unsupported_property_policy,
                UnsupportedPropertyPolicy::Ignore
            ) && kind.is_none()
            {
                continue;
            }
            let source = property_source_from_toml_value(&role_id, property_id, value, kind)?;
            role = role.with_property(property_id.clone(), source);
        }
        definitions.push(role);
    }

    Ok(ThemeDefinition::new(definitions))
}

fn property_source_from_toml_value(
    role_id: &str,
    property_id: &str,
    value: &Value,
    kind: Option<StylePropertyKind>,
) -> Result<StylePropertySource, ThemeDocumentError> {
    if let Some(keyword) = value.as_str() {
        return match keyword {
            "static_parent" => Ok(StylePropertySource::StaticParent),
            "ambient_parent" => Ok(StylePropertySource::AmbientParent),
            "fallback" => Ok(StylePropertySource::Fallback),
            _ => Err(ThemeDocumentError::InvalidPropertySource {
                role: role_id.to_string(),
                property: property_id.to_string(),
            }),
        };
    }

    let Some(table) = value.as_table() else {
        return Err(ThemeDocumentError::InvalidPropertySource {
            role: role_id.to_string(),
            property: property_id.to_string(),
        });
    };
    if table.len() != 1 || !table.contains_key("value") {
        return Err(ThemeDocumentError::InvalidPropertySource {
            role: role_id.to_string(),
            property: property_id.to_string(),
        });
    }
    let value = table
        .get("value")
        .expect("table contains value key after contains_key check");
    let kind = kind.or_else(|| infer_property_kind(value)).ok_or_else(|| {
        ThemeDocumentError::InvalidPropertySource {
            role: role_id.to_string(),
            property: property_id.to_string(),
        }
    })?;
    let value = concrete_value_from_toml_value(value, kind).ok_or_else(|| {
        ThemeDocumentError::InvalidPropertySource {
            role: role_id.to_string(),
            property: property_id.to_string(),
        }
    })?;
    Ok(StylePropertySource::Concrete(value))
}

fn concrete_value_from_toml_value(
    value: &Value,
    kind: StylePropertyKind,
) -> Option<StylePropertyValue> {
    match kind {
        StylePropertyKind::Color => value.as_str().map(StylePropertyValue::color),
        StylePropertyKind::FontFamily => value.as_str().map(StylePropertyValue::font_family),
        StylePropertyKind::LogicalPixels => value
            .as_float()
            .map(|value| value as f32)
            .or_else(|| value.as_integer().map(|value| value as f32))
            .map(StylePropertyValue::logical_pixels),
        StylePropertyKind::FontWeight => value
            .as_integer()
            .and_then(|value| u16::try_from(value).ok())
            .map(StylePropertyValue::font_weight),
    }
}

fn infer_property_kind(value: &Value) -> Option<StylePropertyKind> {
    match value {
        Value::String(value) if value.trim_start().starts_with('#') => {
            Some(StylePropertyKind::Color)
        }
        Value::String(_) => Some(StylePropertyKind::FontFamily),
        Value::Integer(_) => Some(StylePropertyKind::FontWeight),
        Value::Float(_) => Some(StylePropertyKind::LogicalPixels),
        _ => None,
    }
}

fn schema_property_kind(
    schema: &crate::appearance::theme::ThemeSchema,
    role_id: &str,
    property_id: &str,
) -> Option<StylePropertyKind> {
    schema
        .roles()
        .iter()
        .find(|role| role.role_id().as_str() == role_id)?
        .properties()
        .iter()
        .find(|(id, _)| id.as_str() == property_id)
        .map(|(_, property)| property.kind())
}

fn validate_definition(definition: &ThemeDefinition) -> Result<(), ThemeDocumentError> {
    crate::appearance::theme::ThemeResolver::new(built_in_theme_schema(), definition.clone())
        .map(|_| ())
        .map_err(|source| ThemeDocumentError::Validation { source })
}

fn source_toml_value(source: &StylePropertySource) -> Result<String, ThemeDocumentError> {
    match source {
        StylePropertySource::Concrete(value) => {
            Ok(format!("{{ value = {} }}", concrete_toml_value(value)?))
        }
        StylePropertySource::StaticParent => toml_string_literal("static_parent"),
        StylePropertySource::AmbientParent => toml_string_literal("ambient_parent"),
        StylePropertySource::Fallback => toml_string_literal("fallback"),
    }
}

fn concrete_toml_value(value: &StylePropertyValue) -> Result<String, ThemeDocumentError> {
    match value {
        StylePropertyValue::Color(value) | StylePropertyValue::FontFamily(value) => {
            toml_string_literal(value)
        }
        StylePropertyValue::LogicalPixels(value) => Ok(format!("{value}")),
        StylePropertyValue::FontWeight(value) => Ok(value.to_string()),
    }
}

fn toml_string_literal(value: &str) -> Result<String, ThemeDocumentError> {
    let mut table = toml::Table::new();
    table.insert("value".to_string(), Value::String(value.to_string()));
    let text = toml::to_string(&Value::Table(table))
        .map_err(|source| ThemeDocumentError::SerializeToml { source })?;
    Ok(text
        .trim()
        .strip_prefix("value = ")
        .expect("single value table serializes with value key")
        .to_string())
}
