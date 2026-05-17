use super::{
    capabilities::{built_in_theme_supported_properties, built_in_theme_supports_property},
    defaults::*,
    roles::{BerylThemeProperty, BerylThemeRole},
};
use crate::{
    StylePropertyKind, StylePropertySource, StylePropertyValue, ThemeDefinition,
    ThemePropertySchema, ThemeResolver, ThemeRoleDefinition, ThemeRoleSchema, ThemeSchema,
};

pub fn built_in_theme_schema() -> ThemeSchema {
    ThemeSchema::new(
        BerylThemeRole::ALL
            .iter()
            .copied()
            .map(schema_role)
            .collect(),
    )
}

pub fn built_in_theme_definition() -> ThemeDefinition {
    ThemeDefinition::new(
        BerylThemeRole::ALL
            .iter()
            .copied()
            .map(theme_role)
            .collect(),
    )
}

pub fn built_in_theme_resolver() -> ThemeResolver {
    ThemeResolver::new(built_in_theme_schema(), built_in_theme_definition())
        .expect("built-in Beryl theme must validate")
}

fn schema_role(role: BerylThemeRole) -> ThemeRoleSchema {
    let mut role_schema = ThemeRoleSchema::new(role.id());
    if let Some(parent) = role.static_parent() {
        role_schema = role_schema.with_static_parent(parent.id());
    }

    for property in built_in_theme_supported_properties(role) {
        role_schema = role_schema.with_property(
            property.id(),
            ThemePropertySchema::new(property_kind(*property), fallback_value(role, *property)),
        );
    }

    role_schema
}

fn theme_role(role: BerylThemeRole) -> ThemeRoleDefinition {
    let mut role_definition = ThemeRoleDefinition::new(role.id());

    for property in built_in_theme_supported_properties(role) {
        role_definition =
            role_definition.with_property(property.id(), property_source(role, *property));
    }

    role_definition
}

fn property_source(role: BerylThemeRole, property: BerylThemeProperty) -> StylePropertySource {
    if matches!(role, BerylThemeRole::MarkdownInlineCode)
        && matches!(
            property,
            BerylThemeProperty::Background | BerylThemeProperty::TextBackground
        )
    {
        return StylePropertySource::AmbientParent;
    }

    if let Some(parent) = role.static_parent()
        && built_in_theme_supports_property(parent, property)
    {
        if fallback_value(role, property) == fallback_value(parent, property) {
            return StylePropertySource::StaticParent;
        }
    }

    StylePropertySource::Concrete(fallback_value(role, property))
}

fn property_kind(property: BerylThemeProperty) -> StylePropertyKind {
    match property {
        BerylThemeProperty::Background
        | BerylThemeProperty::Border
        | BerylThemeProperty::Color
        | BerylThemeProperty::Foreground
        | BerylThemeProperty::TextBackground => StylePropertyKind::Color,
        BerylThemeProperty::FontFamily => StylePropertyKind::FontFamily,
        BerylThemeProperty::FontSize => StylePropertyKind::LogicalPixels,
        BerylThemeProperty::FontWeight => StylePropertyKind::FontWeight,
    }
}

fn fallback_value(role: BerylThemeRole, property: BerylThemeProperty) -> StylePropertyValue {
    let defaults = role_defaults(role);
    match property {
        BerylThemeProperty::Background => StylePropertyValue::color(defaults.background),
        BerylThemeProperty::Border => StylePropertyValue::color(defaults.border),
        BerylThemeProperty::Color => StylePropertyValue::color(defaults.border),
        BerylThemeProperty::Foreground => StylePropertyValue::color(defaults.foreground),
        BerylThemeProperty::TextBackground => StylePropertyValue::color(defaults.text_background),
        BerylThemeProperty::FontFamily => StylePropertyValue::font_family(defaults.font_family),
        BerylThemeProperty::FontSize => StylePropertyValue::logical_pixels(defaults.font_size),
        BerylThemeProperty::FontWeight => StylePropertyValue::font_weight(defaults.font_weight),
    }
}

fn role_defaults(role: BerylThemeRole) -> RoleDefaults {
    match role {
        BerylThemeRole::AppWindow => APP,
        BerylThemeRole::MainToolbar | BerylThemeRole::MainThreadStrip => APP,
        BerylThemeRole::MainSeparator | BerylThemeRole::StructuralSeparator => SEPARATOR,
        BerylThemeRole::Panel => PANEL,
        BerylThemeRole::SurfaceRow => ROW,
        BerylThemeRole::SurfaceRowHover => ROW_HOVER,
        BerylThemeRole::SurfaceRowSelected => SELECTED,
        BerylThemeRole::SurfaceRowDisabled => DISABLED,
        BerylThemeRole::SurfaceRowPending => PENDING,
        BerylThemeRole::SurfaceRowUnavailable => UNAVAILABLE,
        BerylThemeRole::SurfaceRowError => ERROR,
        BerylThemeRole::SurfaceRowWarning => WARNING,
        BerylThemeRole::SurfaceRowInfo => INFO,
        BerylThemeRole::SurfaceRowSuccess => SUCCESS,
        BerylThemeRole::ButtonPrimaryNormal => PRIMARY_BUTTON,
        BerylThemeRole::ButtonPrimaryHover => PRIMARY_BUTTON_HOVER,
        BerylThemeRole::ButtonPrimaryPressed => PRIMARY_BUTTON_PRESSED,
        BerylThemeRole::ButtonPrimaryActive => PRIMARY_BUTTON_ACTIVE,
        BerylThemeRole::ButtonPrimaryDisabled => BUTTON_DISABLED,
        BerylThemeRole::ButtonSecondaryNormal => SECONDARY_BUTTON,
        BerylThemeRole::ButtonSecondaryHover => SECONDARY_BUTTON_HOVER,
        BerylThemeRole::ButtonSecondaryPressed => SECONDARY_BUTTON_PRESSED,
        BerylThemeRole::ButtonSecondaryActive => SECONDARY_BUTTON_ACTIVE,
        BerylThemeRole::ButtonSecondaryDisabled => BUTTON_DISABLED,
        BerylThemeRole::InputPanel => APP,
        BerylThemeRole::InputField => INPUT,
        BerylThemeRole::InputFieldFocused => INPUT_FOCUSED,
        BerylThemeRole::InputSelection => SELECTED,
        BerylThemeRole::InputCaret => ACCENT,
        BerylThemeRole::InputError => ERROR,
        BerylThemeRole::SettingsWindow | BerylThemeRole::SettingsPage => APP,
        BerylThemeRole::SettingsSidebar => PANEL,
        BerylThemeRole::SettingsSidebarRowNormal => ROW,
        BerylThemeRole::SettingsSidebarRowHover => ROW_HOVER,
        BerylThemeRole::SettingsSidebarRowSelected => SELECTED,
        BerylThemeRole::SettingsGroup => PANEL,
        BerylThemeRole::SettingsRowNormal => ROW,
        BerylThemeRole::SettingsRowHover => ROW_HOVER,
        BerylThemeRole::SettingsRowModified => INFO,
        BerylThemeRole::SettingsRowDisabled => DISABLED,
        BerylThemeRole::SettingsInputNormal => INPUT,
        BerylThemeRole::SettingsInputFocused => INPUT_FOCUSED,
        BerylThemeRole::SettingsInputError => ERROR,
        BerylThemeRole::SettingsInputSelection => SELECTED,
        BerylThemeRole::SettingsPopup => POPUP,
        BerylThemeRole::SettingsButtonPrimary => PRIMARY_BUTTON,
        BerylThemeRole::SettingsButtonSecondary => SECONDARY_BUTTON,
        BerylThemeRole::TranscriptShell => TRANSCRIPT,
        BerylThemeRole::TranscriptAssistantFinal => TRANSCRIPT_TEXT,
        BerylThemeRole::TranscriptAssistantCommentary => COMMENTARY,
        BerylThemeRole::TranscriptAssistantReasoning => REASONING,
        BerylThemeRole::TranscriptUserInput => USER_INPUT,
        BerylThemeRole::TranscriptActivityCaret => ACCENT,
        BerylThemeRole::TranscriptSelection => SELECTED,
        BerylThemeRole::TranscriptQuotePopup | BerylThemeRole::TranscriptContextMenu => POPUP,
        BerylThemeRole::TranscriptPending => PENDING,
        BerylThemeRole::TranscriptUnavailable => UNAVAILABLE,
        BerylThemeRole::MarkdownParagraph => TRANSCRIPT_TEXT,
        BerylThemeRole::MarkdownHeading => HEADING,
        BerylThemeRole::MarkdownEmphasis => EMPHASIS,
        BerylThemeRole::MarkdownStrongEmphasis => STRONG_EMPHASIS,
        BerylThemeRole::MarkdownInlineCode => INLINE_CODE,
        BerylThemeRole::MarkdownLink => LINK,
        BerylThemeRole::MarkdownBlockQuote => BLOCK_QUOTE,
        BerylThemeRole::MarkdownListMarker => LIST_MARKER,
        BerylThemeRole::MarkdownThematicBreak => SEPARATOR,
        BerylThemeRole::MarkdownUnsupportedFallback => CODE,
        BerylThemeRole::CodePanelContainer => CODE_PANEL,
        BerylThemeRole::CodePanelHeader => CODE_PANEL_HEADER,
        BerylThemeRole::CodePanelBody => CODE,
        BerylThemeRole::CodePanelBorder => CODE_PANEL_BORDER,
        BerylThemeRole::CodePanelSelection => SELECTED,
        BerylThemeRole::CodePanelResizeHandle => ACCENT,
        BerylThemeRole::CodePanelButtonNormal => SECONDARY_BUTTON,
        BerylThemeRole::CodePanelButtonHover => SECONDARY_BUTTON_HOVER,
        BerylThemeRole::CodePanelButtonActive => SECONDARY_BUTTON_ACTIVE,
        BerylThemeRole::CodePanelButtonDisabled => BUTTON_DISABLED,
        BerylThemeRole::SyntaxMarkupHeadingMarker => SYNTAX_HEADING,
        BerylThemeRole::SyntaxMarkupQuoteMarker => SYNTAX_QUOTE,
        BerylThemeRole::SyntaxMarkupListMarker => SYNTAX_LIST,
        BerylThemeRole::SyntaxMarkupThematicBreak => SYNTAX_PUNCTUATION,
        BerylThemeRole::SyntaxMarkupFenceDelimiter => SYNTAX_PUNCTUATION,
        BerylThemeRole::SyntaxMarkupFenceInfo => SYNTAX_KEY,
        BerylThemeRole::SyntaxMarkupCodeBlock => CODE,
        BerylThemeRole::SyntaxMarkupCodeSpanDelimiter => SYNTAX_PUNCTUATION,
        BerylThemeRole::SyntaxMarkupCodeSpan => CODE,
        BerylThemeRole::SyntaxMarkupEmphasisDelimiter => SYNTAX_PUNCTUATION,
        BerylThemeRole::SyntaxMarkupStrongDelimiter => SYNTAX_PUNCTUATION,
        BerylThemeRole::SyntaxMarkupLinkText => LINK,
        BerylThemeRole::SyntaxMarkupLinkDestination => SYNTAX_STRING,
        BerylThemeRole::SyntaxMarkupImageMarker => SYNTAX_IMAGE,
        BerylThemeRole::SyntaxMarkupPunctuation => SYNTAX_PUNCTUATION,
        BerylThemeRole::SyntaxMarkupHtml => SYNTAX_COMMENT,
        BerylThemeRole::SyntaxEscape => SYNTAX_ESCAPE,
        BerylThemeRole::SyntaxStructuralPunctuation => SYNTAX_PUNCTUATION,
        BerylThemeRole::SyntaxKey => SYNTAX_KEY,
        BerylThemeRole::SyntaxString => SYNTAX_STRING,
        BerylThemeRole::SyntaxNumber => SYNTAX_NUMBER,
        BerylThemeRole::SyntaxBoolean => SYNTAX_BOOLEAN,
        BerylThemeRole::SyntaxNull => SYNTAX_NULL,
        BerylThemeRole::SyntaxDateTime => SYNTAX_DATE,
        BerylThemeRole::SyntaxComment => SYNTAX_COMMENT,
        BerylThemeRole::SyntaxSectionHeader => SYNTAX_HEADING,
        BerylThemeRole::SyntaxAssignment => SYNTAX_ASSIGNMENT,
        BerylThemeRole::SyntaxTokenEscape => SYNTAX_ESCAPE,
        BerylThemeRole::SyntaxError => SYNTAX_ERROR,
        BerylThemeRole::GraphOverlay => PANEL,
        BerylThemeRole::GraphColumn => PANEL,
        BerylThemeRole::GraphColumnHeader => ROW,
        BerylThemeRole::GraphRowTopic => ROW,
        BerylThemeRole::GraphRowChecklist => ROW,
        BerylThemeRole::GraphRowChecklistItem => ROW,
        BerylThemeRole::GraphRowThreadRef => LINK,
        BerylThemeRole::GraphRowSoftLink => EMPHASIS,
        BerylThemeRole::GraphRowHover => ROW_HOVER,
        BerylThemeRole::GraphRowSelected => SELECTED,
        BerylThemeRole::GraphRowPending => PENDING,
        BerylThemeRole::GraphRowDisabled => DISABLED,
        BerylThemeRole::GraphRowInvalid => UNAVAILABLE,
        BerylThemeRole::GraphRowError => ERROR,
        BerylThemeRole::ChecklistSidebar => PANEL,
        BerylThemeRole::ChecklistHeader => ROW,
        BerylThemeRole::ChecklistRow => ROW,
        BerylThemeRole::ChecklistStatusTodo => MUTED,
        BerylThemeRole::ChecklistStatusInProgress => WARNING,
        BerylThemeRole::ChecklistStatusDone => SUCCESS,
        BerylThemeRole::ThreadSelectorSurface => POPUP,
        BerylThemeRole::ThreadSelectorRow => ROW,
        BerylThemeRole::ThreadSelectorRowSelected => SELECTED,
        BerylThemeRole::ThreadSelectorRowUnavailable => UNAVAILABLE,
        BerylThemeRole::WorkspacePickerSurface => POPUP,
        BerylThemeRole::WorkspacePickerWorkspaceRow => ROW,
        BerylThemeRole::WorkspacePickerMemberRow => ROW,
        BerylThemeRole::WorkspacePickerRowActive => SELECTED,
        BerylThemeRole::ColumnSelectorColumn => PANEL,
        BerylThemeRole::ColumnSelectorHeader => ROW,
        BerylThemeRole::ColumnSelectorRow => ROW,
        BerylThemeRole::ColumnSelectorRowSelected => SELECTED,
        BerylThemeRole::ColumnSelectorAccent => ACCENT,
        BerylThemeRole::PopupSurface => POPUP,
        BerylThemeRole::PopupRowNormal => ROW,
        BerylThemeRole::PopupRowHover => ROW_HOVER,
        BerylThemeRole::PopupRowSelected => SELECTED,
        BerylThemeRole::PopupRowDisabled => DISABLED,
        BerylThemeRole::OverlayBackdrop => OVERLAY,
        BerylThemeRole::NoticeInfo => INFO,
        BerylThemeRole::NoticeWarning => WARNING,
        BerylThemeRole::NoticeError => ERROR,
        BerylThemeRole::NoticeSuccess => SUCCESS,
        BerylThemeRole::DiagnosticSurface => PANEL,
        BerylThemeRole::DiagnosticRow => ROW,
        BerylThemeRole::DiagnosticError => ERROR,
        BerylThemeRole::DiagnosticWarning => WARNING,
        BerylThemeRole::StatusLine => STATUS_LINE,
        BerylThemeRole::StatusValueWorking => WORKING,
        BerylThemeRole::StatusValueCompacting => COMPACTING,
        BerylThemeRole::StatusValueOk => SUCCESS,
        BerylThemeRole::StatusValueError => ERROR,
        BerylThemeRole::StatusValuePending => PENDING,
        BerylThemeRole::StatusValueUnavailable => UNAVAILABLE,
        BerylThemeRole::StatusValueStreaming => STREAMING,
        BerylThemeRole::ScrollbarThumbNormal => SCROLLBAR,
        BerylThemeRole::ScrollbarThumbHover => SCROLLBAR_HOVER,
        BerylThemeRole::ScrollbarThumbDragging => ACCENT,
        BerylThemeRole::MediaPlaceholder => MEDIA,
        BerylThemeRole::MediaPlaceholderLoading => PENDING,
        BerylThemeRole::MediaPlaceholderUnavailable => UNAVAILABLE,
        BerylThemeRole::MediaBorder => MEDIA_BORDER,
        BerylThemeRole::MediaCaption => MUTED,
        BerylThemeRole::ComposerImageMarker => INLINE_CODE,
        BerylThemeRole::TranscriptImageMarker => INLINE_CODE,
        BerylThemeRole::FocusRing => ACCENT,
    }
}
