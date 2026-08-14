use beryl_backend::DynamicToolCallRequest;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{InstalledThemeId, ThemeDocument, ThemeDocumentError};

use super::{
    ACTIVATE_THEME_TOOL, DEFAULT_THEME_AUTHORING_GUIDE_ROLE_LIMIT,
    DEFAULT_THEME_EXPLANATION_ROLE_LIMIT, DEFAULT_THEME_SCHEMA_ROLE_LIMIT, INSTALL_THEME_TOOL,
    MAX_THEME_AUTHORING_GUIDE_ROLE_LIMIT, MAX_THEME_EXPLANATION_ROLE_LIMIT,
    MAX_THEME_SCHEMA_ROLE_LIMIT, MAX_THEME_TOOL_DOCUMENT_BYTES, MAX_THEME_TOOL_NAME_BYTES,
    PREVIEW_THEME_TOOL, READ_THEME_AUTHORING_GUIDE_TOOL, READ_THEME_REPOSITORY_TOOL,
    READ_THEME_SCHEMA_TOOL, SAVE_THEME_AS_TOOL, STOP_THEME_PREVIEW_TOOL, ThemeDynamicToolError,
    ThemeDynamicToolRequest, ThemeSaveAsSource, UPDATE_THEME_TOOL, VALIDATE_THEME_DOCUMENT_TOOL,
    authoring::ThemeAuthoringGuideSection,
};

use crate::dynamic_tools::BERYL_DYNAMIC_TOOL_NAMESPACE;

pub fn parse_beryl_theme_dynamic_tool_request(
    request: &DynamicToolCallRequest,
) -> Result<ThemeDynamicToolRequest, ThemeDynamicToolError> {
    validate_namespace(request)?;
    match request.tool() {
        READ_THEME_REPOSITORY_TOOL => {
            let arguments = parse_arguments::<ReadThemeRepositoryArguments>(request.arguments())?;
            Ok(ThemeDynamicToolRequest::ReadRepository {
                include_active_document: arguments.include_active_document.unwrap_or(false),
            })
        }
        READ_THEME_SCHEMA_TOOL => {
            let arguments = parse_arguments::<ReadThemeSchemaArguments>(request.arguments())?;
            validate_role_prefix_argument(arguments.role_prefix.as_deref())?;
            Ok(ThemeDynamicToolRequest::ReadSchema {
                role_prefix: arguments.role_prefix,
                limit: arguments
                    .limit
                    .unwrap_or(DEFAULT_THEME_SCHEMA_ROLE_LIMIT)
                    .min(MAX_THEME_SCHEMA_ROLE_LIMIT),
            })
        }
        READ_THEME_AUTHORING_GUIDE_TOOL => {
            let arguments =
                parse_arguments::<ReadThemeAuthoringGuideArguments>(request.arguments())?;
            validate_role_prefix_argument(arguments.role_prefix.as_deref())?;
            let section = ThemeAuthoringGuideSection::parse(arguments.section.as_deref())
                .ok_or_else(|| {
                    ThemeDynamicToolError::invalid_arguments(
                        "section must be one of the documented theme guide sections",
                    )
                })?;
            Ok(ThemeDynamicToolRequest::ReadAuthoringGuide {
                section,
                role_prefix: arguments.role_prefix,
                limit: arguments
                    .limit
                    .unwrap_or(DEFAULT_THEME_AUTHORING_GUIDE_ROLE_LIMIT)
                    .min(MAX_THEME_AUTHORING_GUIDE_ROLE_LIMIT),
            })
        }
        VALIDATE_THEME_DOCUMENT_TOOL => {
            let arguments = parse_arguments::<ValidateThemeDocumentArguments>(request.arguments())?;
            let explain_roles = validate_explain_roles_argument(arguments.explain_roles)?;
            Ok(ThemeDynamicToolRequest::ValidateDocument {
                document: validate_theme_document_text_argument(arguments.document)?,
                include_summary: arguments.include_summary.unwrap_or(true),
                explain_roles,
                role_explanation_limit: arguments
                    .role_explanation_limit
                    .unwrap_or(DEFAULT_THEME_EXPLANATION_ROLE_LIMIT)
                    .min(MAX_THEME_EXPLANATION_ROLE_LIMIT),
            })
        }
        PREVIEW_THEME_TOOL => {
            let arguments = parse_arguments::<ThemeDocumentArguments>(request.arguments())?;
            Ok(ThemeDynamicToolRequest::Preview {
                document: parse_theme_document_argument(arguments.document)?,
            })
        }
        STOP_THEME_PREVIEW_TOOL => {
            parse_arguments::<EmptyArguments>(request.arguments())?;
            Ok(ThemeDynamicToolRequest::StopPreview)
        }
        INSTALL_THEME_TOOL => {
            let arguments = parse_arguments::<NamedThemeDocumentArguments>(request.arguments())?;
            Ok(ThemeDynamicToolRequest::Install {
                name: validate_name_argument(arguments.name)?,
                document: parse_theme_document_argument(arguments.document)?,
            })
        }
        UPDATE_THEME_TOOL => {
            let arguments = parse_arguments::<ThemeIdDocumentArguments>(request.arguments())?;
            Ok(ThemeDynamicToolRequest::Update {
                theme_id: parse_theme_id_argument(arguments.theme_id)?,
                document: parse_theme_document_argument(arguments.document)?,
            })
        }
        SAVE_THEME_AS_TOOL => parse_save_as_request(request.arguments()),
        ACTIVATE_THEME_TOOL => {
            let arguments = parse_arguments::<ThemeIdArguments>(request.arguments())?;
            Ok(ThemeDynamicToolRequest::Activate {
                theme_id: parse_theme_id_argument(arguments.theme_id)?,
            })
        }
        tool => Err(ThemeDynamicToolError::new(
            "unsupported_tool",
            format!("unsupported Beryl theme dynamic tool {tool:?}"),
        )),
    }
}

fn parse_save_as_request(
    arguments: &Value,
) -> Result<ThemeDynamicToolRequest, ThemeDynamicToolError> {
    let arguments = parse_arguments::<SaveThemeAsArguments>(arguments)?;
    let source = match (arguments.document, arguments.source_theme_id) {
        (Some(document), None) => {
            ThemeSaveAsSource::Document(parse_theme_document_argument(document)?)
        }
        (None, Some(theme_id)) => {
            ThemeSaveAsSource::ExistingTheme(parse_theme_id_argument(theme_id)?)
        }
        _ => {
            return Err(ThemeDynamicToolError::invalid_arguments(
                "exactly one of document or sourceThemeId is required",
            ));
        }
    };
    Ok(ThemeDynamicToolRequest::SaveAs {
        name: validate_name_argument(arguments.name)?,
        source,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReadThemeRepositoryArguments {
    include_active_document: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReadThemeSchemaArguments {
    role_prefix: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReadThemeAuthoringGuideArguments {
    section: Option<String>,
    role_prefix: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ValidateThemeDocumentArguments {
    document: String,
    include_summary: Option<bool>,
    explain_roles: Option<Vec<String>>,
    role_explanation_limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArguments {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThemeDocumentArguments {
    document: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NamedThemeDocumentArguments {
    name: String,
    document: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThemeIdDocumentArguments {
    theme_id: String,
    document: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThemeIdArguments {
    theme_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SaveThemeAsArguments {
    name: String,
    document: Option<String>,
    source_theme_id: Option<String>,
}

fn validate_namespace(request: &DynamicToolCallRequest) -> Result<(), ThemeDynamicToolError> {
    if let Some(namespace) = request.namespace()
        && namespace != BERYL_DYNAMIC_TOOL_NAMESPACE
    {
        return Err(ThemeDynamicToolError::new(
            "unsupported_namespace",
            format!("unsupported Beryl dynamic tool namespace {namespace:?}"),
        ));
    }
    Ok(())
}

fn parse_arguments<T>(arguments: &Value) -> Result<T, ThemeDynamicToolError>
where
    T: for<'de> Deserialize<'de>,
{
    let arguments = if arguments.is_null() {
        json!({})
    } else {
        arguments.clone()
    };
    serde_json::from_value(arguments).map_err(|source| {
        ThemeDynamicToolError::invalid_arguments(format!("invalid theme tool arguments: {source}"))
    })
}

fn parse_theme_document_argument(text: String) -> Result<ThemeDocument, ThemeDynamicToolError> {
    let text = validate_theme_document_text_argument(text)?;
    ThemeDocument::from_toml_str(&text).map_err(theme_document_error)
}

fn validate_theme_document_text_argument(text: String) -> Result<String, ThemeDynamicToolError> {
    if text.len() > MAX_THEME_TOOL_DOCUMENT_BYTES {
        return Err(ThemeDynamicToolError::invalid_arguments(format!(
            "document exceeds {MAX_THEME_TOOL_DOCUMENT_BYTES} bytes"
        )));
    }
    Ok(text)
}

fn parse_theme_id_argument(value: String) -> Result<InstalledThemeId, ThemeDynamicToolError> {
    InstalledThemeId::new(value).map_err(|source| {
        ThemeDynamicToolError::invalid_arguments(format!("invalid themeId: {source}"))
    })
}

fn validate_name_argument(value: String) -> Result<String, ThemeDynamicToolError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_THEME_TOOL_NAME_BYTES {
        return Err(ThemeDynamicToolError::invalid_arguments(format!(
            "name must be non-empty and at most {MAX_THEME_TOOL_NAME_BYTES} bytes"
        )));
    }
    Ok(trimmed.to_string())
}

fn validate_role_prefix_argument(prefix: Option<&str>) -> Result<(), ThemeDynamicToolError> {
    if let Some(prefix) = prefix
        && prefix.len() > MAX_THEME_TOOL_NAME_BYTES
    {
        return Err(ThemeDynamicToolError::invalid_arguments(format!(
            "rolePrefix exceeds {MAX_THEME_TOOL_NAME_BYTES} bytes"
        )));
    }
    Ok(())
}

fn validate_explain_roles_argument(
    roles: Option<Vec<String>>,
) -> Result<Vec<String>, ThemeDynamicToolError> {
    let roles = roles.unwrap_or_default();
    if roles.len() > MAX_THEME_EXPLANATION_ROLE_LIMIT {
        return Err(ThemeDynamicToolError::invalid_arguments(format!(
            "explainRoles may contain at most {MAX_THEME_EXPLANATION_ROLE_LIMIT} roles"
        )));
    }
    for role in &roles {
        if role.is_empty() || role.len() > MAX_THEME_TOOL_NAME_BYTES {
            return Err(ThemeDynamicToolError::invalid_arguments(format!(
                "explainRoles entries must be non-empty and at most {MAX_THEME_TOOL_NAME_BYTES} bytes"
            )));
        }
    }
    Ok(roles)
}

fn theme_document_error(error: ThemeDocumentError) -> ThemeDynamicToolError {
    ThemeDynamicToolError::new("invalid_theme_document", error.to_string())
}
