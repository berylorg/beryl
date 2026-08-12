use std::{collections::BTreeMap, sync::OnceLock};

use super::{
    ThemeColor, ThemeFontFamily, ThemeFontWeight, ThemeLogicalPixels, ThemePropertyId, ThemeRoleId,
    ThemeValue, ThemeValueKind,
};

pub const THEME_SOURCE_KEYWORDS: [&str; 3] = ["static_parent", "ambient_parent", "fallback"];
pub const CANONICAL_THEME_ROLE_COUNT: usize = ROLE_INVENTORY.len();
pub const CANONICAL_THEME_PROPERTY_ENTRY_COUNT_MAX: usize = CANONICAL_THEME_ROLE_COUNT * 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemePropertySchema {
    id: ThemePropertyId,
    kind: ThemeValueKind,
    fallback: ThemeValue,
    static_parent_eligible: bool,
    ambient_parent_eligible: bool,
}

impl ThemePropertySchema {
    #[must_use]
    pub const fn id(&self) -> ThemePropertyId {
        self.id
    }
    #[must_use]
    pub const fn kind(&self) -> ThemeValueKind {
        self.kind
    }
    #[must_use]
    pub const fn fallback(&self) -> &ThemeValue {
        &self.fallback
    }
    #[must_use]
    pub const fn static_parent_eligible(&self) -> bool {
        self.static_parent_eligible
    }
    #[must_use]
    pub const fn ambient_parent_eligible(&self) -> bool {
        self.ambient_parent_eligible
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeRoleSchema {
    id: ThemeRoleId,
    static_parent: Option<ThemeRoleId>,
    properties: BTreeMap<ThemePropertyId, ThemePropertySchema>,
}

impl ThemeRoleSchema {
    #[must_use]
    pub const fn id(&self) -> &ThemeRoleId {
        &self.id
    }
    #[must_use]
    pub const fn static_parent(&self) -> Option<&ThemeRoleId> {
        self.static_parent.as_ref()
    }
    #[must_use]
    pub const fn properties(&self) -> &BTreeMap<ThemePropertyId, ThemePropertySchema> {
        &self.properties
    }
    #[must_use]
    pub fn property(&self, id: ThemePropertyId) -> Option<&ThemePropertySchema> {
        self.properties.get(&id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeSchema {
    roles: BTreeMap<ThemeRoleId, ThemeRoleSchema>,
}

impl ThemeSchema {
    #[must_use]
    pub const fn roles(&self) -> &BTreeMap<ThemeRoleId, ThemeRoleSchema> {
        &self.roles
    }

    #[must_use]
    pub fn role(&self, id: &str) -> Option<&ThemeRoleSchema> {
        self.roles.values().find(|role| role.id.as_str() == id)
    }
}

#[must_use]
pub fn canonical_theme_schema() -> &'static ThemeSchema {
    static SCHEMA: OnceLock<ThemeSchema> = OnceLock::new();
    SCHEMA.get_or_init(build_schema)
}

fn build_schema() -> ThemeSchema {
    let known = ROLE_INVENTORY.iter().copied().collect::<BTreeMap<_, _>>();
    let mut roles = BTreeMap::new();
    for &(id, parent) in ROLE_INVENTORY {
        let role_id = ThemeRoleId::canonical(id);
        let static_parent = parent.map(ThemeRoleId::canonical);
        let mut properties = BTreeMap::new();
        for &property in supported_properties(id) {
            let fallback = fallback_value(id, property);
            let static_parent_eligible = parent.is_some_and(|parent| {
                known.contains_key(parent) && supported_properties(parent).contains(&property)
            });
            let ambient_parent_eligible = id == "markdown.inline_code"
                && matches!(
                    property,
                    ThemePropertyId::Background | ThemePropertyId::TextBackground
                );
            properties.insert(
                property,
                ThemePropertySchema {
                    id: property,
                    kind: fallback.kind(),
                    fallback,
                    static_parent_eligible,
                    ambient_parent_eligible,
                },
            );
        }
        roles.insert(
            role_id.clone(),
            ThemeRoleSchema {
                id: role_id,
                static_parent,
                properties,
            },
        );
    }
    ThemeSchema { roles }
}

const NONE: &[ThemePropertyId] = &[];
const BACKGROUND: &[ThemePropertyId] = &[ThemePropertyId::Background];
const BORDER: &[ThemePropertyId] = &[ThemePropertyId::Border];
const COLOR: &[ThemePropertyId] = &[ThemePropertyId::Color];
const FOREGROUND: &[ThemePropertyId] = &[ThemePropertyId::Foreground];
const TEXT_BACKGROUND: &[ThemePropertyId] = &[ThemePropertyId::TextBackground];
const BACKGROUND_FOREGROUND: &[ThemePropertyId] =
    &[ThemePropertyId::Background, ThemePropertyId::Foreground];
const FOREGROUND_TEXT_BACKGROUND: &[ThemePropertyId] =
    &[ThemePropertyId::Foreground, ThemePropertyId::TextBackground];
const SURFACE: &[ThemePropertyId] = &[
    ThemePropertyId::Background,
    ThemePropertyId::Border,
    ThemePropertyId::Foreground,
];
const TEXT: &[ThemePropertyId] = &[
    ThemePropertyId::Foreground,
    ThemePropertyId::TextBackground,
    ThemePropertyId::FontFamily,
    ThemePropertyId::FontSize,
    ThemePropertyId::FontWeight,
];
const ROOT: &[ThemePropertyId] = &ThemePropertyId::ALL;

fn supported_properties(id: &str) -> &'static [ThemePropertyId] {
    if id == "root" {
        return ROOT;
    }
    if is_text_role(id) {
        return TEXT;
    }
    if id.starts_with("syntax.") {
        return FOREGROUND;
    }
    match id {
        "separator"
        | "focus_ring"
        | "caret"
        | "accent_marker"
        | "resize_handle"
        | "scrollbar.thumb"
        | "main.separator"
        | "structural.separator"
        | "input.caret"
        | "focus.ring"
        | "transcript.activity_caret"
        | "markdown.thematic_break"
        | "markdown.block_quote"
        | "code_panel.border"
        | "code_panel.resize_handle"
        | "scrollbar.thumb.normal"
        | "scrollbar.thumb.hover"
        | "scrollbar.thumb.dragging"
        | "activity.indicator.running"
        | "activity.indicator.ok"
        | "activity.indicator.error"
        | "activity.resize_handle" => COLOR,

        "app.window" | "transcript.shell" | "status.line" | "activity.panel" => {
            BACKGROUND_FOREGROUND
        }
        "main.toolbar"
        | "input.panel"
        | "settings.window"
        | "settings.sidebar.row.hover"
        | "settings.row.hover"
        | "settings.row.modified"
        | "code_panel.container"
        | "code_panel.header"
        | "code_panel.body"
        | "popup.row.hover"
        | "popup.row.selected"
        | "media.placeholder"
        | "media.placeholder.loading"
        | "media.placeholder.unavailable" => BACKGROUND,
        "settings.input.focused" | "settings.input.error" | "media.border" => BORDER,
        "input.selection" | "settings.input.selection" => TEXT_BACKGROUND,
        "transcript.selection" | "transcript.image_marker" => FOREGROUND_TEXT_BACKGROUND,

        "row.selected"
        | "row.pending"
        | "row.unavailable"
        | "row.error"
        | "row.warning"
        | "row.success"
        | "button.primary.pressed"
        | "button.secondary.pressed"
        | "input.field.focused"
        | "input.error"
        | "settings.row.disabled"
        | "code_panel.button.disabled"
        | "code_panel.selection"
        | "composer.image_marker"
        | "popup.row.normal"
        | "popup.row.disabled"
        | "overlay.backdrop"
        | "diagnostic.surface"
        | "diagnostic.row"
        | "diagnostic.error"
        | "diagnostic.warning" => NONE,

        "row.hover" => BACKGROUND,
        "status.value.working"
        | "status.value.compacting"
        | "status.value.ok"
        | "status.value.error"
        | "status.value.pending"
        | "status.value.unavailable"
        | "status.value.streaming" => FOREGROUND,
        _ => SURFACE,
    }
}

fn is_text_role(id: &str) -> bool {
    id == "text"
        || id.starts_with("text.")
        || id.ends_with(".text")
        || id.ends_with(".label")
        || id.ends_with(".title")
        || id.ends_with(".detail")
        || id.ends_with(".value")
        || matches!(
            id,
            "list.header"
                | "popup.header"
                | "markdown.paragraph"
                | "markdown.heading"
                | "markdown.emphasis"
                | "markdown.strong_emphasis"
                | "markdown.inline_code"
                | "markdown.link"
                | "markdown.list_marker"
                | "markdown.unsupported_fallback"
                | "transcript.turn.assistant.final"
                | "transcript.turn.assistant.commentary"
                | "transcript.turn.assistant.reasoning"
                | "media.caption"
        )
}

#[derive(Clone, Copy)]
struct Palette {
    background: [u8; 3],
    border: [u8; 3],
    foreground: [u8; 3],
    font_family: &'static str,
    font_size: f32,
    font_weight: u16,
}

fn fallback_value(id: &str, property: ThemePropertyId) -> ThemeValue {
    let palette = palette(id);
    match property {
        ThemePropertyId::Background => ThemeValue::Color(rgb(palette.background)),
        ThemePropertyId::Border | ThemePropertyId::Color => ThemeValue::Color(rgb(palette.border)),
        ThemePropertyId::Foreground => ThemeValue::Color(rgb(palette.foreground)),
        ThemePropertyId::TextBackground => ThemeValue::Color(rgb(palette.background)),
        ThemePropertyId::FontFamily => ThemeValue::FontFamily(
            ThemeFontFamily::new(palette.font_family).expect("built-in font family is valid"),
        ),
        ThemePropertyId::FontSize => ThemeValue::LogicalPixels(
            ThemeLogicalPixels::new(palette.font_size).expect("built-in font size is valid"),
        ),
        ThemePropertyId::FontWeight => ThemeValue::FontWeight(
            ThemeFontWeight::new(palette.font_weight).expect("built-in font weight is valid"),
        ),
    }
}

const fn rgb(value: [u8; 3]) -> ThemeColor {
    ThemeColor::from_rgb(value[0], value[1], value[2])
}

fn palette(id: &str) -> Palette {
    let code = id.starts_with("syntax.")
        || id.starts_with("code_panel.")
        || id == "text.code"
        || id == "markdown.inline_code"
        || id == "markdown.unsupported_fallback";
    let mut value = Palette {
        background: [0x0b, 0x10, 0x20],
        border: [0x24, 0x30, 0x47],
        foreground: [0xe7, 0xee, 0xf7],
        font_family: if code { "Consolas" } else { "Inter" },
        font_size: if code { 13.0 } else { 14.0 },
        font_weight: 400,
    };
    if id.contains("disabled") || id.contains("unavailable") {
        value.background = [0x14, 0x1b, 0x2a];
        value.border = [0x2b, 0x35, 0x47];
        value.foreground = [0x7f, 0x8e, 0xa3];
    } else if id.contains("error") {
        value.background = [0x3a, 0x17, 0x1c];
        value.border = [0xef, 0x44, 0x44];
        value.foreground = [0xfe, 0xca, 0xca];
        value.font_weight = 500;
    } else if id.contains("warning") {
        value.background = [0x34, 0x26, 0x0f];
        value.border = [0xf5, 0x9e, 0x0b];
        value.foreground = [0xfd, 0xe6, 0x8a];
        value.font_weight = 500;
    } else if id.contains("success") || id.ends_with(".ok") {
        value.background = [0x10, 0x30, 0x22];
        value.border = [0x22, 0xc5, 0x5e];
        value.foreground = [0xbb, 0xf7, 0xd0];
        value.font_weight = 500;
    } else if id.contains("info") || id.contains("pending") || id.contains("working") {
        value.background = [0x12, 0x28, 0x3b];
        value.border = [0x38, 0xbd, 0xf8];
        value.foreground = [0xba, 0xe6, 0xfd];
        value.font_weight = 500;
    } else if id.contains("selected") || id.contains("focused") || id.contains("active") {
        value.background = [0x17, 0x3a, 0x5e];
        value.border = [0x38, 0xbd, 0xf8];
        value.foreground = [0xff, 0xff, 0xff];
        value.font_weight = 500;
    } else if id.contains("hover") {
        value.background = [0x1f, 0x2b, 0x42];
        value.border = [0x40, 0x51, 0x6b];
        value.foreground = [0xf3, 0xf7, 0xfb];
    } else if id.starts_with("surface.panel") || id == "panel" || id.contains("group") {
        value.background = [0x11, 0x18, 0x27];
        value.border = [0x2f, 0x3b, 0x52];
    } else if id.starts_with("popup") || id.starts_with("notice") || id.contains("tooltip") {
        value.background = [0x10, 0x18, 0x27];
        value.border = [0x3a, 0x48, 0x60];
        value.foreground = [0xf3, 0xf7, 0xfb];
    } else if id.starts_with("row") || id.contains(".row") {
        value.background = [0x17, 0x20, 0x33];
        value.border = [0x2d, 0x3a, 0x52];
    }
    if id.contains("heading") {
        value.foreground = [0x93, 0xc5, 0xfd];
        value.font_size = 18.0;
        value.font_weight = 600;
    } else if id.contains("strong") {
        value.font_weight = 700;
    } else if id.starts_with("status.") || id.starts_with("activity.") {
        value.font_size = 12.0;
    }
    value
}

const ROLE_INVENTORY: &[(&str, Option<&str>)] = &[
    ("root", None),
    ("text", Some("root")),
    ("text.muted", Some("text")),
    ("text.subtle", Some("text")),
    ("text.value", Some("text")),
    ("text.link", Some("text")),
    ("text.code", Some("text")),
    ("text.semantic.info", Some("text")),
    ("text.semantic.warning", Some("text")),
    ("text.semantic.error", Some("text")),
    ("text.semantic.success", Some("text")),
    ("surface", Some("root")),
    ("surface.window", Some("surface")),
    ("surface.panel", Some("surface")),
    ("surface.elevated", Some("surface")),
    ("surface.inset", Some("surface")),
    ("surface.overlay", Some("surface")),
    ("primitive", Some("root")),
    ("separator", Some("primitive")),
    ("focus_ring", Some("primitive")),
    ("caret", Some("primitive")),
    ("accent_marker", Some("primitive")),
    ("resize_handle", Some("primitive")),
    ("scrollbar.thumb", Some("primitive")),
    ("control", Some("root")),
    ("button", Some("control")),
    ("button.label", Some("text")),
    ("input", Some("control")),
    ("input.text", Some("text")),
    ("selection", Some("interaction.selected")),
    ("list", Some("control")),
    ("list.header", Some("text")),
    ("menu", Some("control")),
    ("menu.item", Some("row")),
    ("menu.item.label", Some("text")),
    ("popup", Some("control")),
    ("popup.header", Some("text")),
    ("notice", Some("control")),
    ("notice.title", Some("text")),
    ("notice.detail", Some("text.subtle")),
    ("status", Some("control")),
    ("status.label", Some("text.muted")),
    ("status.value", Some("text.value")),
    ("dropdown", Some("input")),
    ("dropdown.label", Some("text")),
    ("color-input", Some("input")),
    ("color-input.label", Some("text")),
    ("color-input.value", Some("text.code")),
    ("file-picker", Some("input")),
    ("file-picker.label", Some("text")),
    ("tooltip", Some("popup")),
    ("tooltip.text", Some("text.subtle")),
    ("scrollbar", Some("control")),
    ("interaction.hover", Some("row")),
    ("interaction.pressed", Some("button")),
    ("interaction.active", Some("control")),
    ("interaction.selected", Some("row")),
    ("interaction.focused", Some("input")),
    ("interaction.disabled", Some("control")),
    ("semantic.info", Some("notice")),
    ("semantic.warning", Some("notice")),
    ("semantic.error", Some("notice")),
    ("semantic.success", Some("notice")),
    ("app.window", Some("surface.window")),
    ("app.window.title", Some("text.value")),
    ("main.toolbar", Some("surface.window")),
    ("main.toolbar.title", Some("text.value")),
    ("main.separator", Some("separator")),
    ("panel", Some("surface.panel")),
    ("row", Some("control")),
    ("row.label", Some("text")),
    ("row.hover", Some("interaction.hover")),
    ("row.selected", Some("interaction.selected")),
    ("row.disabled", Some("interaction.disabled")),
    ("row.pending", Some("row")),
    ("row.unavailable", Some("interaction.disabled")),
    ("row.error", Some("semantic.error")),
    ("row.warning", Some("semantic.warning")),
    ("row.info", Some("semantic.info")),
    ("row.success", Some("semantic.success")),
    ("structural.separator", Some("separator")),
    ("button.primary.normal", Some("button")),
    ("button.primary.hover", Some("button.primary.normal")),
    ("button.primary.pressed", Some("button.primary.normal")),
    ("button.primary.active", Some("button.primary.normal")),
    ("button.primary.disabled", Some("button.primary.normal")),
    ("button.primary.label", Some("button.label")),
    ("button.secondary.normal", Some("button")),
    ("button.secondary.hover", Some("button.secondary.normal")),
    ("button.secondary.pressed", Some("button.secondary.normal")),
    ("button.secondary.active", Some("button.secondary.normal")),
    ("button.secondary.disabled", Some("button.secondary.normal")),
    ("button.secondary.label", Some("button.label")),
    ("input.panel", Some("panel")),
    ("input.field", Some("input")),
    ("input.field.text", Some("input.text")),
    ("input.field.focused", Some("interaction.focused")),
    ("input.selection", Some("selection")),
    ("input.caret", Some("caret")),
    ("input.error", Some("semantic.error")),
    ("settings.window", Some("app.window")),
    ("settings.sidebar", Some("list")),
    ("settings.sidebar.row.normal", Some("row")),
    ("settings.sidebar.row.text", Some("row.label")),
    ("settings.sidebar.row.hover", Some("row.hover")),
    ("settings.sidebar.row.selected", Some("row.selected")),
    ("settings.page", Some("surface.window")),
    ("settings.group", Some("surface.panel")),
    ("settings.group.header.text", Some("list.header")),
    ("settings.row.normal", Some("row")),
    ("settings.row.label", Some("row.label")),
    ("settings.row.value", Some("text.value")),
    ("settings.row.hover", Some("row.hover")),
    ("settings.row.modified", Some("row.info")),
    ("settings.row.disabled", Some("row.disabled")),
    ("settings.row.disabled.text", Some("text.muted")),
    ("settings.input.normal", Some("input")),
    ("settings.input.text", Some("input.text")),
    ("settings.input.focused", Some("settings.input.normal")),
    ("settings.input.error", Some("settings.input.normal")),
    ("settings.input.selection", Some("selection")),
    ("settings.input.caret", Some("input.caret")),
    ("settings.popup", Some("popup.surface")),
    ("settings.button.primary", Some("button.primary.normal")),
    ("settings.button.secondary", Some("button.secondary.normal")),
    (
        "settings.button.primary.label",
        Some("button.primary.label"),
    ),
    (
        "settings.button.secondary.label",
        Some("button.secondary.label"),
    ),
    ("transcript.shell", Some("surface.panel")),
    ("transcript.turn.assistant.final", Some("text")),
    (
        "transcript.turn.assistant.commentary",
        Some("transcript.turn.assistant.final"),
    ),
    ("transcript.turn.assistant.reasoning", Some("text.subtle")),
    ("transcript.turn.user", Some("surface.panel")),
    ("transcript.turn.user.text", Some("text")),
    ("transcript.activity_caret", Some("caret")),
    ("transcript.selection", Some("selection")),
    ("transcript.quote_popup", Some("popup.surface")),
    ("transcript.quote_popup.text", Some("button.label")),
    ("transcript.context_menu", Some("popup.surface")),
    ("transcript.context_menu.header.text", Some("popup.header")),
    ("transcript.pending", Some("transcript.shell")),
    ("transcript.unavailable", Some("transcript.shell")),
    ("markdown.paragraph", Some("text")),
    ("markdown.heading", Some("text.value")),
    ("markdown.emphasis", Some("markdown.paragraph")),
    ("markdown.strong_emphasis", Some("markdown.paragraph")),
    ("markdown.inline_code", Some("text.code")),
    ("markdown.link", Some("text.link")),
    ("markdown.block_quote", Some("separator")),
    ("markdown.list_marker", Some("text.muted")),
    ("markdown.thematic_break", Some("separator")),
    ("markdown.unsupported_fallback", Some("text.code")),
    ("code_panel.container", Some("surface.inset")),
    ("code_panel.header", Some("surface.inset")),
    ("code_panel.header.text", Some("text.code")),
    ("code_panel.body", Some("surface.inset")),
    ("code_panel.body.text", Some("text.code")),
    ("code_panel.border", Some("separator")),
    ("code_panel.selection", Some("selection")),
    ("code_panel.resize_handle", Some("resize_handle")),
    ("code_panel.button.normal", Some("button.secondary.normal")),
    ("code_panel.button.hover", Some("code_panel.button.normal")),
    ("code_panel.button.active", Some("code_panel.button.normal")),
    (
        "code_panel.button.disabled",
        Some("code_panel.button.normal"),
    ),
    ("syntax.markup.heading_marker", Some("code_panel.body.text")),
    ("syntax.markup.quote_marker", Some("code_panel.body.text")),
    ("syntax.markup.list_marker", Some("code_panel.body.text")),
    ("syntax.markup.thematic_break", Some("code_panel.body.text")),
    (
        "syntax.markup.fence_delimiter",
        Some("code_panel.body.text"),
    ),
    ("syntax.markup.fence_info", Some("code_panel.body.text")),
    ("syntax.markup.code_block", Some("code_panel.body.text")),
    (
        "syntax.markup.code_span_delimiter",
        Some("code_panel.body.text"),
    ),
    ("syntax.markup.code_span", Some("code_panel.body.text")),
    (
        "syntax.markup.emphasis_delimiter",
        Some("code_panel.body.text"),
    ),
    (
        "syntax.markup.strong_delimiter",
        Some("code_panel.body.text"),
    ),
    ("syntax.markup.link_text", Some("code_panel.body.text")),
    (
        "syntax.markup.link_destination",
        Some("code_panel.body.text"),
    ),
    ("syntax.markup.image_marker", Some("code_panel.body.text")),
    ("syntax.markup.punctuation", Some("code_panel.body.text")),
    ("syntax.markup.html", Some("code_panel.body.text")),
    ("syntax.escape", Some("code_panel.body.text")),
    (
        "syntax.structural_punctuation",
        Some("code_panel.body.text"),
    ),
    ("syntax.key", Some("code_panel.body.text")),
    ("syntax.string", Some("code_panel.body.text")),
    ("syntax.number", Some("code_panel.body.text")),
    ("syntax.boolean", Some("code_panel.body.text")),
    ("syntax.null", Some("code_panel.body.text")),
    ("syntax.date_time", Some("code_panel.body.text")),
    ("syntax.comment", Some("code_panel.body.text")),
    ("syntax.section_header", Some("code_panel.body.text")),
    ("syntax.assignment", Some("code_panel.body.text")),
    ("syntax.token_escape", Some("code_panel.body.text")),
    ("syntax.error", Some("code_panel.body.text")),
    ("popup.surface", Some("popup")),
    ("popup.row.normal", Some("popup.surface")),
    ("popup.row.hover", Some("popup.row.normal")),
    ("popup.row.selected", Some("popup.row.normal")),
    ("popup.row.disabled", Some("popup.row.normal")),
    ("overlay.backdrop", Some("app.window")),
    ("notice.info", Some("popup.surface")),
    ("notice.warning", Some("popup.surface")),
    ("notice.error", Some("popup.surface")),
    ("notice.success", Some("popup.surface")),
    ("diagnostic.surface", Some("panel")),
    ("diagnostic.row", Some("diagnostic.surface")),
    ("diagnostic.error", Some("diagnostic.row")),
    ("diagnostic.warning", Some("diagnostic.row")),
    ("status.line", Some("app.window")),
    ("status.line.cell", Some("status")),
    ("status.line.label", Some("status.label")),
    ("status.line.value", Some("status.value")),
    ("status.value.working", Some("status.line.value")),
    ("status.value.compacting", Some("status.line.value")),
    ("status.value.ok", Some("status.line.value")),
    ("status.value.error", Some("status.line.value")),
    ("status.value.pending", Some("status.line.value")),
    ("status.value.unavailable", Some("status.line.value")),
    ("status.value.streaming", Some("status.line.value")),
    ("activity.panel", Some("status.line")),
    ("activity.row", Some("status.line.cell")),
    ("activity.label", Some("status.line.label")),
    ("activity.value", Some("status.line.value")),
    ("activity.indicator.running", Some("accent_marker")),
    ("activity.indicator.ok", Some("accent_marker")),
    ("activity.indicator.error", Some("accent_marker")),
    ("activity.resize_handle", Some("resize_handle")),
    ("scrollbar.thumb.normal", Some("scrollbar.thumb")),
    ("scrollbar.thumb.hover", Some("scrollbar.thumb")),
    ("scrollbar.thumb.dragging", Some("scrollbar.thumb")),
    ("media.placeholder", Some("transcript.shell")),
    ("media.placeholder.text", Some("text.muted")),
    ("media.placeholder.loading", Some("media.placeholder")),
    (
        "media.placeholder.loading.text",
        Some("media.placeholder.text"),
    ),
    ("media.placeholder.unavailable", Some("media.placeholder")),
    (
        "media.placeholder.unavailable.text",
        Some("text.semantic.warning"),
    ),
    ("media.border", Some("media.placeholder")),
    ("media.caption", Some("media.placeholder.text")),
    ("composer.image_marker", Some("input.field")),
    ("transcript.image_marker", Some("markdown.paragraph")),
    ("focus.ring", Some("focus_ring")),
];
