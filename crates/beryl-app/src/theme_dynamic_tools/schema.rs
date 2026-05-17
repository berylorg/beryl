use serde_json::{Value, json};

use super::{
    MAX_THEME_AUTHORING_GUIDE_ROLE_LIMIT, MAX_THEME_EXPLANATION_ROLE_LIMIT,
    MAX_THEME_SCHEMA_ROLE_LIMIT, MAX_THEME_TOOL_DOCUMENT_BYTES, MAX_THEME_TOOL_NAME_BYTES,
    THEME_AUTHORING_GUIDE_SECTION_NAMES,
};

pub(super) fn empty_object_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

pub(super) fn read_theme_repository_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "includeActiveDocument": {
                "type": "boolean",
                "default": false
            }
        },
        "additionalProperties": false
    })
}

pub(super) fn read_theme_schema_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "rolePrefix": {
                "type": "string",
                "maxLength": MAX_THEME_TOOL_NAME_BYTES
            },
            "limit": {
                "type": "integer",
                "minimum": 0,
                "maximum": MAX_THEME_SCHEMA_ROLE_LIMIT,
                "default": 128
            }
        },
        "additionalProperties": false
    })
}

pub(super) fn read_theme_authoring_guide_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "section": {
                "type": "string",
                "enum": THEME_AUTHORING_GUIDE_SECTION_NAMES,
                "default": "all"
            },
            "rolePrefix": {
                "type": "string",
                "maxLength": MAX_THEME_TOOL_NAME_BYTES
            },
            "limit": {
                "type": "integer",
                "minimum": 0,
                "maximum": MAX_THEME_AUTHORING_GUIDE_ROLE_LIMIT,
                "default": 24
            }
        },
        "additionalProperties": false
    })
}

pub(super) fn theme_document_schema() -> Value {
    json!({
        "type": "object",
        "required": ["document"],
        "properties": {
            "document": theme_document_property()
        },
        "additionalProperties": false
    })
}

pub(super) fn validate_theme_document_schema() -> Value {
    json!({
        "type": "object",
        "required": ["document"],
        "properties": {
            "document": theme_document_property(),
            "includeSummary": {
                "type": "boolean",
                "default": true
            },
            "explainRoles": {
                "type": "array",
                "items": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": MAX_THEME_TOOL_NAME_BYTES
                },
                "maxItems": MAX_THEME_EXPLANATION_ROLE_LIMIT,
                "default": []
            },
            "roleExplanationLimit": {
                "type": "integer",
                "minimum": 0,
                "maximum": MAX_THEME_EXPLANATION_ROLE_LIMIT,
                "default": 8
            }
        },
        "additionalProperties": false
    })
}

pub(super) fn named_theme_document_schema() -> Value {
    json!({
        "type": "object",
        "required": ["name", "document"],
        "properties": {
            "name": theme_name_property(),
            "document": theme_document_property()
        },
        "additionalProperties": false
    })
}

pub(super) fn theme_id_document_schema() -> Value {
    json!({
        "type": "object",
        "required": ["themeId", "document"],
        "properties": {
            "themeId": theme_id_property(),
            "document": theme_document_property()
        },
        "additionalProperties": false
    })
}

pub(super) fn theme_id_schema() -> Value {
    json!({
        "type": "object",
        "required": ["themeId"],
        "properties": {
            "themeId": theme_id_property()
        },
        "additionalProperties": false
    })
}

pub(super) fn save_theme_as_schema() -> Value {
    json!({
        "type": "object",
        "required": ["name"],
        "properties": {
            "name": theme_name_property(),
            "document": theme_document_property(),
            "sourceThemeId": theme_id_property()
        },
        "additionalProperties": false
    })
}

fn theme_id_property() -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "maxLength": 64,
        "pattern": "^[a-z0-9][a-z0-9-]*[a-z0-9]$|^[a-z0-9]$"
    })
}

fn theme_name_property() -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "maxLength": MAX_THEME_TOOL_NAME_BYTES
    })
}

fn theme_document_property() -> Value {
    json!({
        "type": "string",
        "maxLength": MAX_THEME_TOOL_DOCUMENT_BYTES,
        "description": "Compact TOML Beryl theme document."
    })
}
