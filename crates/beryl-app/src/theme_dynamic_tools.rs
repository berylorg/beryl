mod authoring;
mod parser;
mod response;
mod schema;
mod schema_output;
mod validation;

use beryl_backend::{DynamicToolCallRequest, DynamicToolSpec};
use serde_json::Value;

pub use authoring::{
    THEME_AUTHORING_GUIDE_SECTION_NAMES, ThemeAuthoringGuideSection, theme_authoring_guide_value,
};
pub use parser::parse_beryl_theme_dynamic_tool_request;
pub use response::{
    theme_mutation_value, theme_preview_value, theme_repository_value, theme_tool_failure_response,
    theme_tool_success_response,
};
pub use schema_output::{theme_document_summary_value, theme_schema_value};
pub use validation::validate_theme_document_value;

use crate::{InstalledThemeId, ThemeDocument};

use crate::dynamic_tools::BERYL_DYNAMIC_TOOL_NAMESPACE;

pub const READ_THEME_REPOSITORY_TOOL: &str = "read_theme_repository";
pub const READ_THEME_SCHEMA_TOOL: &str = "read_theme_schema";
pub const READ_THEME_AUTHORING_GUIDE_TOOL: &str = "read_theme_authoring_guide";
pub const VALIDATE_THEME_DOCUMENT_TOOL: &str = "validate_theme_document";
pub const PREVIEW_THEME_TOOL: &str = "preview_theme";
pub const STOP_THEME_PREVIEW_TOOL: &str = "stop_theme_preview";
pub const INSTALL_THEME_TOOL: &str = "install_theme";
pub const UPDATE_THEME_TOOL: &str = "update_theme";
pub const SAVE_THEME_AS_TOOL: &str = "save_theme_as";
pub const ACTIVATE_THEME_TOOL: &str = "activate_theme";

pub const MAX_THEME_TOOL_DOCUMENT_BYTES: usize = 64 * 1024;
pub const MAX_THEME_ACTIVE_DOCUMENT_RESPONSE_BYTES: usize = 16 * 1024;
pub const MAX_THEME_TOOL_NAME_BYTES: usize = 128;
pub const MAX_THEME_SCHEMA_ROLE_LIMIT: usize = 256;
pub const MAX_THEME_AUTHORING_GUIDE_ROLE_LIMIT: usize = 64;
pub const MAX_THEME_EXPLANATION_ROLE_LIMIT: usize = 32;
pub(super) const DEFAULT_THEME_SCHEMA_ROLE_LIMIT: usize = 128;
pub(super) const DEFAULT_THEME_AUTHORING_GUIDE_ROLE_LIMIT: usize = 24;
pub(super) const DEFAULT_THEME_EXPLANATION_ROLE_LIMIT: usize = 8;
pub(super) const MAX_THEME_TOOL_ERROR_BYTES: usize = 512;

#[derive(Clone, Debug)]
pub enum ThemeDynamicToolRequest {
    ReadRepository {
        include_active_document: bool,
    },
    ReadSchema {
        role_prefix: Option<String>,
        limit: usize,
    },
    ReadAuthoringGuide {
        section: ThemeAuthoringGuideSection,
        role_prefix: Option<String>,
        limit: usize,
    },
    ValidateDocument {
        document: String,
        include_summary: bool,
        explain_roles: Vec<String>,
        role_explanation_limit: usize,
    },
    Preview {
        document: ThemeDocument,
    },
    StopPreview,
    Install {
        name: String,
        document: ThemeDocument,
    },
    Update {
        theme_id: InstalledThemeId,
        document: ThemeDocument,
    },
    SaveAs {
        name: String,
        source: ThemeSaveAsSource,
    },
    Activate {
        theme_id: InstalledThemeId,
    },
}

#[derive(Clone, Debug)]
pub enum ThemeSaveAsSource {
    Document(ThemeDocument),
    ExistingTheme(InstalledThemeId),
}

#[derive(Debug)]
pub struct ThemeDynamicToolError {
    kind: &'static str,
    message: String,
}

pub fn beryl_theme_dynamic_tool_specs() -> Vec<DynamicToolSpec> {
    vec![
        theme_tool_spec(
            READ_THEME_REPOSITORY_TOOL,
            "Read bounded metadata for Beryl's installed theme repository and optionally the active compact theme document.",
            schema::read_theme_repository_schema(),
        ),
        theme_tool_spec(
            READ_THEME_SCHEMA_TOOL,
            "Read a bounded slice of Beryl's theme role and property schema.",
            schema::read_theme_schema_schema(),
        ),
        theme_tool_spec(
            READ_THEME_AUTHORING_GUIDE_TOOL,
            "Read bounded model-facing guidance for authoring and troubleshooting Beryl compact TOML themes.",
            schema::read_theme_authoring_guide_schema(),
        ),
        theme_tool_spec(
            VALIDATE_THEME_DOCUMENT_TOOL,
            "Validate one compact Beryl theme document and optionally explain role property sources without previewing, installing, or mutating GUI state.",
            schema::validate_theme_document_schema(),
        ),
        theme_tool_spec(
            PREVIEW_THEME_TOOL,
            "Preview one compact Beryl theme document transiently in the running GUI without installing it or creating transcript content.",
            schema::theme_document_schema(),
        ),
        theme_tool_spec(
            STOP_THEME_PREVIEW_TOOL,
            "Stop an active CAS dynamic-tool theme preview and restore the durable active theme.",
            schema::empty_object_schema(),
        ),
        theme_tool_spec(
            INSTALL_THEME_TOOL,
            "Install one compact Beryl theme document as a new durable theme without activating it or creating transcript content.",
            schema::named_theme_document_schema(),
        ),
        theme_tool_spec(
            UPDATE_THEME_TOOL,
            "Replace one existing installed Beryl theme with a compact theme document while preserving its installed theme id.",
            schema::theme_id_document_schema(),
        ),
        theme_tool_spec(
            SAVE_THEME_AS_TOOL,
            "Save a compact Beryl theme document or an existing installed theme as a new durable active installed theme.",
            schema::save_theme_as_schema(),
        ),
        theme_tool_spec(
            ACTIVATE_THEME_TOOL,
            "Activate one installed Beryl theme by stable installed theme id.",
            schema::theme_id_schema(),
        ),
    ]
}

pub fn is_beryl_theme_dynamic_tool(request: &DynamicToolCallRequest) -> bool {
    request
        .namespace()
        .is_none_or(|namespace| namespace == BERYL_DYNAMIC_TOOL_NAMESPACE)
        && matches!(
            request.tool(),
            READ_THEME_REPOSITORY_TOOL
                | READ_THEME_SCHEMA_TOOL
                | READ_THEME_AUTHORING_GUIDE_TOOL
                | VALIDATE_THEME_DOCUMENT_TOOL
                | PREVIEW_THEME_TOOL
                | STOP_THEME_PREVIEW_TOOL
                | INSTALL_THEME_TOOL
                | UPDATE_THEME_TOOL
                | SAVE_THEME_AS_TOOL
                | ACTIVATE_THEME_TOOL
        )
}

impl ThemeDynamicToolError {
    pub fn kind(&self) -> &'static str {
        self.kind
    }

    pub(super) fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: response::bounded_tool_string(message),
        }
    }

    pub(super) fn invalid_arguments(message: impl Into<String>) -> Self {
        Self::new("invalid_arguments", message)
    }
}

impl std::fmt::Display for ThemeDynamicToolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ThemeDynamicToolError {}

fn theme_tool_spec(name: &str, description: &str, input_schema: Value) -> DynamicToolSpec {
    DynamicToolSpec::new(name, description, input_schema)
        .with_namespace(BERYL_DYNAMIC_TOOL_NAMESPACE)
        .with_defer_loading(false)
}
