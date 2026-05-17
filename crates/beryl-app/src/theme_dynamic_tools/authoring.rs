use serde_json::{Value, json};

use crate::{StylePropertyKind, ThemeSchema, built_in_theme_schema};

use super::MAX_THEME_AUTHORING_GUIDE_ROLE_LIMIT;

pub const THEME_AUTHORING_GUIDE_SECTION_NAMES: &[&str] = &[
    "all",
    "overview",
    "syntax",
    "inheritance",
    "role_groups",
    "transcript_roles",
    "code_roles",
    "settings_roles",
    "examples",
    "troubleshooting",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeAuthoringGuideSection {
    All,
    Overview,
    Syntax,
    Inheritance,
    RoleGroups,
    TranscriptRoles,
    CodeRoles,
    SettingsRoles,
    Examples,
    Troubleshooting,
}

pub fn theme_authoring_guide_value(
    section: ThemeAuthoringGuideSection,
    role_prefix: Option<&str>,
    limit: usize,
) -> Value {
    let limit = limit.min(MAX_THEME_AUTHORING_GUIDE_ROLE_LIMIT);
    let schema = built_in_theme_schema();
    json!({
        "section": section.id(),
        "availableSections": THEME_AUTHORING_GUIDE_SECTION_NAMES,
        "guidance": selected_sections(section),
        "roleHints": role_hints(&schema, role_prefix, limit),
        "roleHintLimit": limit,
    })
}

impl ThemeAuthoringGuideSection {
    pub fn parse(value: Option<&str>) -> Option<Self> {
        match value.unwrap_or("all") {
            "all" => Some(Self::All),
            "overview" => Some(Self::Overview),
            "syntax" => Some(Self::Syntax),
            "inheritance" => Some(Self::Inheritance),
            "role_groups" => Some(Self::RoleGroups),
            "transcript_roles" => Some(Self::TranscriptRoles),
            "code_roles" => Some(Self::CodeRoles),
            "settings_roles" => Some(Self::SettingsRoles),
            "examples" => Some(Self::Examples),
            "troubleshooting" => Some(Self::Troubleshooting),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Overview => "overview",
            Self::Syntax => "syntax",
            Self::Inheritance => "inheritance",
            Self::RoleGroups => "role_groups",
            Self::TranscriptRoles => "transcript_roles",
            Self::CodeRoles => "code_roles",
            Self::SettingsRoles => "settings_roles",
            Self::Examples => "examples",
            Self::Troubleshooting => "troubleshooting",
        }
    }
}

fn selected_sections(section: ThemeAuthoringGuideSection) -> Vec<Value> {
    match section {
        ThemeAuthoringGuideSection::All => vec![
            overview_section(),
            syntax_section(),
            inheritance_section(),
            role_groups_section(),
            transcript_roles_section(),
            code_roles_section(),
            settings_roles_section(),
            examples_section(),
            troubleshooting_section(),
        ],
        ThemeAuthoringGuideSection::Overview => vec![overview_section()],
        ThemeAuthoringGuideSection::Syntax => vec![syntax_section()],
        ThemeAuthoringGuideSection::Inheritance => vec![inheritance_section()],
        ThemeAuthoringGuideSection::RoleGroups => vec![role_groups_section()],
        ThemeAuthoringGuideSection::TranscriptRoles => vec![transcript_roles_section()],
        ThemeAuthoringGuideSection::CodeRoles => vec![code_roles_section()],
        ThemeAuthoringGuideSection::SettingsRoles => vec![settings_roles_section()],
        ThemeAuthoringGuideSection::Examples => vec![examples_section()],
        ThemeAuthoringGuideSection::Troubleshooting => vec![troubleshooting_section()],
    }
}

fn overview_section() -> Value {
    json!({
        "id": "overview",
        "title": "Theme document overview",
        "points": [
            "Author one compact TOML document in a fenced code block with language beryl-theme when proposing a theme to the operator.",
            "Use read_theme_schema for the exact role and property inventory; this guide explains how to use that schema.",
            "Each role accepts only the properties listed for that role. Roles with an empty property list are not theme-editable until a render site consumes one of their properties.",
            "A theme document can include only roles and properties that it changes. Omitted properties resolve from built-in fallback values unless the document requests another source.",
            "Use validate_theme_document before previewing or installing a candidate so parser and resolver diagnostics can be fixed without mutating GUI state."
        ],
    })
}

fn syntax_section() -> Value {
    json!({
        "id": "syntax",
        "title": "Compact TOML syntax",
        "points": [
            "The top level requires schema = 1 and may include name = \"Theme Name\".",
            "Each role is a [[role]] record with id = \"role.id\" and optional static_parent = \"other.role\".",
            "Only use properties that read_theme_schema lists for that role. Concrete values use inline tables such as foreground = { value = \"#aabbcc\" }, color = { value = \"#aabbcc\" }, font_family = { value = \"Inter\" }, font_size = { value = 14.0 }, and font_weight = { value = 500 }.",
            "Source keywords are string values: static_parent, ambient_parent, and fallback.",
            "Color values must be six-digit hex colors in #rrggbb form after normalization."
        ],
    })
}

fn inheritance_section() -> Value {
    json!({
        "id": "inheritance",
        "title": "Static and ambient inheritance",
        "points": [
            "Static inheritance is a theme-document relationship between roles. A property set to static_parent resolves from the same property on the role's effective static parent chain.",
            "Runtime ambient inheritance is render-site context. A property set to ambient_parent resolves from the surrounding style at the place where the role is embedded.",
            "Inline code usually keeps concrete foreground and typography while text_background uses ambient_parent so it fits final answers, user input, settings rows, and popups.",
            "fallback means use the built-in value for that role and property. Omitting a property also resolves as fallback.",
            "Missing static parents and static-parent cycles are invalid theme documents."
        ],
    })
}

fn role_groups_section() -> Value {
    json!({
        "id": "role_groups",
        "title": "Role groups",
        "groups": [
            { "prefix": "app.", "use": "overall window defaults and app-level foreground/background" },
            { "prefix": "main.", "use": "main toolbar, thread strip, and separators" },
            { "prefix": "button.", "use": "shared primary and secondary button states" },
            { "prefix": "input.", "use": "composer and text input surfaces" },
            { "prefix": "settings.", "use": "settings window, sidebar, pages, rows, inputs, popups, and settings buttons" },
            { "prefix": "transcript.", "use": "conversation shell and assistant/user turn text" },
            { "prefix": "markdown.", "use": "rendered Markdown paragraph, heading, emphasis, links, inline code, lists, and fallbacks" },
            { "prefix": "code_panel.", "use": "fenced code block container, header, body, buttons, selection, and resize handle" },
            { "prefix": "syntax.", "use": "parser-backed code token colors" },
            { "prefix": "graph.", "use": "graph overlay columns and rows" },
            { "prefix": "checklist.", "use": "checklist sidebar rows and status markers" },
            { "prefix": "status.", "use": "bottom status line and dynamic turn-state values" },
            { "prefix": "notice.", "use": "info, warning, error, and success notices" },
            { "prefix": "media.", "use": "transcript media placeholders, borders, and captions" }
        ],
    })
}

fn transcript_roles_section() -> Value {
    json!({
        "id": "transcript_roles",
        "title": "Transcript roles",
        "points": [
            "Use transcript.shell for the transcript region surface.",
            "Use transcript.turn.assistant.final for final answer narrative text.",
            "Use transcript.turn.assistant.commentary for commentary text and transcript.turn.assistant.reasoning for reasoning text.",
            "Use transcript.turn.user for user input fragments.",
            "Markdown roles inherit from transcript narrative contexts unless a document overrides them."
        ],
    })
}

fn code_roles_section() -> Value {
    json!({
        "id": "code_roles",
        "title": "Code and syntax roles",
        "points": [
            "Use markdown.inline_code for inline code embedded in narrative text.",
            "Use code_panel.container, code_panel.header, and code_panel.body for fenced code blocks.",
            "Use code_panel.button.* for Preview, Install Theme, Copy, and Soft Wrap button states inside code panels.",
            "Use syntax.* roles for parser token colors inside code panels. Syntax roles expose foreground only."
        ],
    })
}

fn settings_roles_section() -> Value {
    json!({
        "id": "settings_roles",
        "title": "Settings roles",
        "points": [
            "Use settings.window, settings.sidebar, settings.page, and settings.group for settings-window structure.",
            "Use settings.row.* for setting rows and modified or disabled row states.",
            "Use settings.input.* for text and color inputs inside settings.",
            "Use settings.button.primary and settings.button.secondary for settings-window actions."
        ],
    })
}

fn examples_section() -> Value {
    json!({
        "id": "examples",
        "title": "Authoring examples",
        "examples": [
            "schema = 1\nname = \"Example\"\n\n[[role]]\nid = \"app.window\"\nbackground = { value = \"#000000\" }\nforeground = { value = \"#aaaaaa\" }\n\n[[role]]\nid = \"markdown.inline_code\"\ntext_background = \"ambient_parent\"\nforeground = { value = \"#ffd766\" }",
            "schema = 1\nname = \"Transcript Commentary\"\n\n[[role]]\nid = \"transcript.turn.assistant.commentary\"\nforeground = { value = \"#66cc88\" }"
        ],
    })
}

fn troubleshooting_section() -> Value {
    json!({
        "id": "troubleshooting",
        "title": "Troubleshooting",
        "points": [
            "If a color does not change, validate the document and inspect whether the property was omitted, set to fallback, or set on a different role than the visible surface uses.",
            "If inline code backgrounds look wrong, check whether markdown.inline_code text_background uses ambient_parent or an intentional concrete color.",
            "If a role fails validation, read_theme_schema with a rolePrefix near that role to confirm the exact role id and property ids.",
            "If a static_parent value fails, confirm the parent role exists and the role graph has no cycle.",
            "If a preview works but the operator wants a durable record, emit a beryl-theme code block or call an explicit install operation."
        ],
    })
}

fn role_hints(schema: &ThemeSchema, role_prefix: Option<&str>, limit: usize) -> Value {
    let mut total_count = 0usize;
    let mut roles = Vec::new();
    for role in schema
        .roles()
        .iter()
        .filter(|role| role_prefix.is_none_or(|prefix| role.role_id().as_str().starts_with(prefix)))
    {
        total_count = total_count.saturating_add(1);
        if roles.len() >= limit {
            continue;
        }
        roles.push(json!({
            "id": role.role_id().as_str(),
            "staticParent": role.static_parent().map(|parent| parent.as_str()),
            "group": role_group(role.role_id().as_str()),
            "supportedProperties": role.properties().iter().map(|(property_id, property)| {
                json!({
                    "id": property_id.as_str(),
                    "kind": property_kind_label(property.kind()),
                })
            }).collect::<Vec<_>>(),
            "supportedPropertyCount": role.properties().len(),
        }));
    }

    json!({
        "roles": roles,
        "roleCount": total_count,
        "rolesTruncated": total_count > limit,
    })
}

fn role_group(role_id: &str) -> &str {
    role_id
        .split_once('.')
        .map(|(prefix, _)| prefix)
        .unwrap_or(role_id)
}

fn property_kind_label(kind: StylePropertyKind) -> &'static str {
    match kind {
        StylePropertyKind::Color => "color",
        StylePropertyKind::FontFamily => "font_family",
        StylePropertyKind::LogicalPixels => "logical_pixels",
        StylePropertyKind::FontWeight => "font_weight",
    }
}
