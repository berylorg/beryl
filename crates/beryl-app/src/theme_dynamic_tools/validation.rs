use serde_json::{Value, json};

use crate::{
    ActiveThemeProjection, StylePropertySource, StyleRoleId, ThemeDefinition, ThemeDiagnosticKind,
    ThemeDocument, ThemeDocumentError, ThemeRepositorySnapshot, ThemeResolver,
    ThemeValidationDiagnostics, built_in_theme_schema,
};

use super::{
    MAX_THEME_EXPLANATION_ROLE_LIMIT,
    response::bounded_tool_string,
    schema_output::{property_source_json, property_value_json},
    theme_document_summary_value,
};

pub fn validate_theme_document_value(
    document_text: &str,
    include_summary: bool,
    explain_roles: &[String],
    role_explanation_limit: usize,
    snapshot: &ThemeRepositorySnapshot,
) -> Value {
    let explanation_limit = role_explanation_limit.min(MAX_THEME_EXPLANATION_ROLE_LIMIT);
    match ThemeDocument::from_toml_str(document_text) {
        Ok(document) => valid_or_duplicate_document_value(
            &document,
            include_summary,
            explain_roles,
            explanation_limit,
            snapshot,
        ),
        Err(error) => invalid_document_value(error),
    }
}

fn valid_or_duplicate_document_value(
    document: &ThemeDocument,
    include_summary: bool,
    explain_roles: &[String],
    explanation_limit: usize,
    snapshot: &ThemeRepositorySnapshot,
) -> Value {
    let mut diagnostics = Vec::new();
    if let Some(id) = document.id()
        && snapshot.themes().iter().any(|theme| theme.id() == id)
    {
        diagnostics.push(json!({
            "kind": "duplicate_embedded_theme_id",
            "roleId": Value::Null,
            "propertyId": Value::Null,
            "message": format!("theme document embedded id {} is already installed", id.as_str()),
        }));
    }

    if let Err(diagnostic) = validate_projection(document.definition().clone()) {
        diagnostics.push(diagnostic);
    }

    let valid = diagnostics.is_empty();
    let diagnostic_count = diagnostics.len();
    json!({
        "valid": valid,
        "diagnostics": diagnostics,
        "diagnosticCount": diagnostic_count,
        "diagnosticsTruncated": false,
        "summary": include_summary.then(|| theme_document_summary_value(document)),
        "roleExplanations": role_explanations(document.definition(), explain_roles, explanation_limit),
        "roleExplanationsTruncated": explain_roles.len() > explanation_limit,
    })
}

fn invalid_document_value(error: ThemeDocumentError) -> Value {
    let (diagnostics, truncated_count) = diagnostics_from_document_error(error);
    let diagnostic_count = diagnostics.len() + truncated_count;
    json!({
        "valid": false,
        "diagnostics": diagnostics,
        "diagnosticCount": diagnostic_count,
        "diagnosticsTruncated": truncated_count > 0,
        "summary": Value::Null,
        "roleExplanations": [],
        "roleExplanationsTruncated": false,
    })
}

fn validate_projection(definition: ThemeDefinition) -> Result<(), Value> {
    let resolver = ThemeResolver::new(built_in_theme_schema(), definition)
        .map_err(|diagnostics| validation_diagnostics_value(&diagnostics))?;
    ActiveThemeProjection::from_built_in_resolver(resolver)
        .map(|_| ())
        .map_err(|source| {
            json!({
                "kind": "resolution_error",
                "roleId": Value::Null,
                "propertyId": Value::Null,
                "message": bounded_tool_string(source.to_string()),
            })
        })
}

fn diagnostics_from_document_error(error: ThemeDocumentError) -> (Vec<Value>, usize) {
    match error {
        ThemeDocumentError::ParseToml { source } => (
            vec![json!({
                "kind": "parse_toml",
                "roleId": Value::Null,
                "propertyId": Value::Null,
                "message": bounded_tool_string(source.to_string()),
            })],
            0,
        ),
        ThemeDocumentError::SerializeToml { source } => (
            vec![json!({
                "kind": "serialize_toml",
                "roleId": Value::Null,
                "propertyId": Value::Null,
                "message": source.to_string(),
            })],
            0,
        ),
        ThemeDocumentError::InvalidSchema => (
            vec![json!({
                "kind": "invalid_schema",
                "roleId": Value::Null,
                "propertyId": Value::Null,
                "message": "theme document schema must be 1",
            })],
            0,
        ),
        ThemeDocumentError::InvalidId => (
            vec![json!({
                "kind": "invalid_id",
                "roleId": Value::Null,
                "propertyId": Value::Null,
                "message": "theme document id is invalid",
            })],
            0,
        ),
        ThemeDocumentError::InvalidName => (
            vec![json!({
                "kind": "invalid_name",
                "roleId": Value::Null,
                "propertyId": Value::Null,
                "message": "theme document name is invalid",
            })],
            0,
        ),
        ThemeDocumentError::InvalidRoleRecord => (
            vec![json!({
                "kind": "invalid_role_record",
                "roleId": Value::Null,
                "propertyId": Value::Null,
                "message": "theme document role record is invalid",
            })],
            0,
        ),
        ThemeDocumentError::InvalidPropertySource { role, property } => (
            vec![json!({
                "kind": "invalid_property_source",
                "roleId": role,
                "propertyId": property,
                "message": "theme document property source is invalid",
            })],
            0,
        ),
        ThemeDocumentError::Validation { source } => {
            let truncated_count = source.truncated_count();
            let diagnostics = source
                .diagnostics()
                .iter()
                .map(|diagnostic| {
                    json!({
                        "kind": diagnostic_kind_label(diagnostic.kind()),
                        "roleId": diagnostic.role_id().map(|role| role.as_str()),
                        "propertyId": diagnostic.property_id().map(|property| property.as_str()),
                        "message": diagnostic.message(),
                    })
                })
                .collect();
            (diagnostics, truncated_count)
        }
    }
}

fn validation_diagnostics_value(source: &ThemeValidationDiagnostics) -> Value {
    let diagnostics = source
        .diagnostics()
        .iter()
        .map(|diagnostic| {
            json!({
                "kind": diagnostic_kind_label(diagnostic.kind()),
                "roleId": diagnostic.role_id().map(|role| role.as_str()),
                "propertyId": diagnostic.property_id().map(|property| property.as_str()),
                "message": diagnostic.message(),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "kind": "validation_error",
        "roleId": Value::Null,
        "propertyId": Value::Null,
        "message": bounded_tool_string(source.to_string()),
        "diagnostics": diagnostics,
        "diagnosticsTruncated": source.truncated_count() > 0,
    })
}

fn diagnostic_kind_label(kind: ThemeDiagnosticKind) -> &'static str {
    match kind {
        ThemeDiagnosticKind::DuplicateRole => "duplicate_role",
        ThemeDiagnosticKind::UnknownRole => "unknown_role",
        ThemeDiagnosticKind::UnknownProperty => "unknown_property",
        ThemeDiagnosticKind::InvalidPropertyType => "invalid_property_type",
        ThemeDiagnosticKind::InvalidPropertyValue => "invalid_property_value",
        ThemeDiagnosticKind::MissingStaticParent => "missing_static_parent",
        ThemeDiagnosticKind::StaticParentCycle => "static_parent_cycle",
        ThemeDiagnosticKind::InvalidFallback => "invalid_fallback",
    }
}

fn role_explanations(
    definition: &ThemeDefinition,
    explain_roles: &[String],
    limit: usize,
) -> Vec<Value> {
    let schema = built_in_theme_schema();
    let resolver = ThemeResolver::new(schema.clone(), definition.clone()).ok();
    explain_roles
        .iter()
        .take(limit)
        .map(|role_id| {
            let role_id = StyleRoleId::from(role_id.as_str());
            let schema_role = schema
                .roles()
                .iter()
                .find(|role| role.role_id() == &role_id);
            let document_role = definition
                .roles()
                .iter()
                .find(|role| role.role_id() == &role_id);
            let Some(schema_role) = schema_role else {
                return json!({
                    "roleId": role_id.as_str(),
                    "known": false,
                    "properties": [],
                });
            };
            let effective_static_parent = document_role
                .and_then(|role| role.static_parent())
                .or_else(|| schema_role.static_parent());
            let properties = schema_role
                .properties()
                .keys()
                .map(|property_id| {
                    let declared_source = document_role
                        .and_then(|role| role.properties().get(property_id));
                    let source_label = source_label(declared_source);
                    let resolved = resolver.as_ref().and_then(|resolver| {
                        resolver
                            .resolve_property(
                                role_id.clone(),
                                property_id.clone(),
                                &crate::ThemeResolutionContext::new(),
                            )
                            .ok()
                    });
                    json!({
                        "id": property_id.as_str(),
                        "source": source_label,
                        "declared": declared_source.map(property_source_json),
                        "parentRole": matches!(declared_source, Some(StylePropertySource::StaticParent)).then(|| effective_static_parent.map(|parent| parent.as_str())).flatten(),
                        "resolvedWithoutAmbient": resolved.as_ref().map(property_value_json),
                        "ambientDependent": matches!(declared_source, Some(StylePropertySource::AmbientParent)),
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "roleId": role_id.as_str(),
                "known": true,
                "definedInDocument": document_role.is_some(),
                "staticParent": effective_static_parent.map(|parent| parent.as_str()),
                "properties": properties,
            })
        })
        .collect()
}

fn source_label(source: Option<&StylePropertySource>) -> &'static str {
    match source {
        Some(StylePropertySource::Concrete(_)) => "concrete_value",
        Some(StylePropertySource::StaticParent) => "static_parent",
        Some(StylePropertySource::AmbientParent) => "ambient_parent",
        Some(StylePropertySource::Fallback) | None => "fallback",
    }
}
