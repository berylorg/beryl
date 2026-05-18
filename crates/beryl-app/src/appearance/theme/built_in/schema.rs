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
        BerylThemeRole::Root => APP,
        BerylThemeRole::Text => APP,
        BerylThemeRole::TextMuted | BerylThemeRole::TextSubtle => MUTED,
        BerylThemeRole::TextValue => STATUS_LINE,
        BerylThemeRole::TextLink => LINK,
        BerylThemeRole::TextCode => CODE,
        BerylThemeRole::TextSemanticInfo => INFO,
        BerylThemeRole::TextSemanticWarning => WARNING,
        BerylThemeRole::TextSemanticError => ERROR,
        BerylThemeRole::TextSemanticSuccess => SUCCESS,
        BerylThemeRole::Surface | BerylThemeRole::SurfaceWindow => APP,
        BerylThemeRole::SurfacePanel => PANEL,
        BerylThemeRole::SurfaceElevated | BerylThemeRole::SurfaceOverlay => POPUP,
        BerylThemeRole::SurfaceInset => INPUT,
        BerylThemeRole::Primitive => APP,
        BerylThemeRole::PrimitiveSeparator => SEPARATOR,
        BerylThemeRole::PrimitiveFocusRing
        | BerylThemeRole::PrimitiveCaret
        | BerylThemeRole::PrimitiveAccentMarker
        | BerylThemeRole::PrimitiveResizeHandle => ACCENT,
        BerylThemeRole::PrimitiveScrollbarThumb => SCROLLBAR,
        BerylThemeRole::Control => APP,
        BerylThemeRole::ControlButton => SECONDARY_BUTTON,
        BerylThemeRole::ControlButtonLabel => SECONDARY_BUTTON,
        BerylThemeRole::ControlInput => INPUT,
        BerylThemeRole::ControlInputText => INPUT,
        BerylThemeRole::ControlSelection => SELECTED,
        BerylThemeRole::ControlList => PANEL,
        BerylThemeRole::ControlListHeader => ROW,
        BerylThemeRole::ControlMenu => POPUP,
        BerylThemeRole::ControlMenuItem => ROW,
        BerylThemeRole::ControlMenuItemLabel => ROW,
        BerylThemeRole::ControlPopup => POPUP,
        BerylThemeRole::ControlPopupHeader => POPUP,
        BerylThemeRole::ControlNotice => INFO,
        BerylThemeRole::ControlNoticeTitle => INFO,
        BerylThemeRole::ControlNoticeDetail => MUTED,
        BerylThemeRole::ControlStatus => STATUS_LINE,
        BerylThemeRole::ControlStatusLabel => MUTED,
        BerylThemeRole::ControlStatusValue => STATUS_LINE,
        BerylThemeRole::ControlDropdown => INPUT,
        BerylThemeRole::ControlDropdownLabel => INPUT,
        BerylThemeRole::ControlColorInput => INPUT,
        BerylThemeRole::ControlColorInputLabel => INPUT,
        BerylThemeRole::ControlColorInputValue => CODE,
        BerylThemeRole::ControlFilePicker => INPUT,
        BerylThemeRole::ControlFilePickerLabel => INPUT,
        BerylThemeRole::ControlTooltip => POPUP,
        BerylThemeRole::ControlTooltipText => MUTED,
        BerylThemeRole::ControlScrollbar => APP,
        BerylThemeRole::InteractionHover => ROW_HOVER,
        BerylThemeRole::InteractionPressed => SECONDARY_BUTTON_PRESSED,
        BerylThemeRole::InteractionActive | BerylThemeRole::InteractionSelected => SELECTED,
        BerylThemeRole::InteractionFocused => INPUT_FOCUSED,
        BerylThemeRole::InteractionDisabled => DISABLED,
        BerylThemeRole::SemanticInfo => INFO,
        BerylThemeRole::SemanticWarning => WARNING,
        BerylThemeRole::SemanticError => ERROR,
        BerylThemeRole::SemanticSuccess => SUCCESS,
        BerylThemeRole::AppWindow => APP,
        BerylThemeRole::AppWindowTitle => STATUS_LINE,
        BerylThemeRole::MainToolbar | BerylThemeRole::MainThreadStrip => APP,
        BerylThemeRole::MainToolbarTitle => STATUS_LINE,
        BerylThemeRole::MainThreadStripActiveThread => SECONDARY_BUTTON,
        BerylThemeRole::MainThreadStripActiveThreadLabel => SECONDARY_BUTTON,
        BerylThemeRole::MainSeparator | BerylThemeRole::StructuralSeparator => SEPARATOR,
        BerylThemeRole::Panel => PANEL,
        BerylThemeRole::SurfaceRow => ROW,
        BerylThemeRole::ControlRowLabel => ROW,
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
        BerylThemeRole::ButtonPrimaryLabel => PRIMARY_BUTTON,
        BerylThemeRole::ButtonSecondaryNormal => SECONDARY_BUTTON,
        BerylThemeRole::ButtonSecondaryHover => SECONDARY_BUTTON_HOVER,
        BerylThemeRole::ButtonSecondaryPressed => SECONDARY_BUTTON_PRESSED,
        BerylThemeRole::ButtonSecondaryActive => SECONDARY_BUTTON_ACTIVE,
        BerylThemeRole::ButtonSecondaryDisabled => BUTTON_DISABLED,
        BerylThemeRole::ButtonSecondaryLabel => SECONDARY_BUTTON,
        BerylThemeRole::InputPanel => APP,
        BerylThemeRole::InputField => INPUT,
        BerylThemeRole::InputFieldText => INPUT,
        BerylThemeRole::InputFieldFocused => INPUT_FOCUSED,
        BerylThemeRole::InputSelection => SELECTED,
        BerylThemeRole::InputCaret => ACCENT,
        BerylThemeRole::InputError => ERROR,
        BerylThemeRole::SettingsWindow | BerylThemeRole::SettingsPage => APP,
        BerylThemeRole::SettingsSidebar => PANEL,
        BerylThemeRole::SettingsSidebarRowNormal => ROW,
        BerylThemeRole::SettingsSidebarRowText => ROW,
        BerylThemeRole::SettingsSidebarRowHover => ROW_HOVER,
        BerylThemeRole::SettingsSidebarRowSelected => SELECTED,
        BerylThemeRole::SettingsGroup => PANEL,
        BerylThemeRole::SettingsGroupHeaderText => ROW,
        BerylThemeRole::SettingsRowNormal => ROW,
        BerylThemeRole::SettingsRowLabel => ROW,
        BerylThemeRole::SettingsRowValue => STATUS_LINE,
        BerylThemeRole::SettingsRowHover => ROW_HOVER,
        BerylThemeRole::SettingsRowModified => INFO,
        BerylThemeRole::SettingsRowDisabled => DISABLED,
        BerylThemeRole::SettingsRowDisabledText => DISABLED,
        BerylThemeRole::SettingsInputNormal => INPUT,
        BerylThemeRole::SettingsInputText => INPUT,
        BerylThemeRole::SettingsInputFocused => INPUT_FOCUSED,
        BerylThemeRole::SettingsInputError => ERROR,
        BerylThemeRole::SettingsInputSelection => SELECTED,
        BerylThemeRole::SettingsInputCaret => ACCENT,
        BerylThemeRole::SettingsPopup => POPUP,
        BerylThemeRole::SettingsButtonPrimary => PRIMARY_BUTTON,
        BerylThemeRole::SettingsButtonSecondary => SECONDARY_BUTTON,
        BerylThemeRole::SettingsButtonPrimaryLabel => PRIMARY_BUTTON,
        BerylThemeRole::SettingsButtonSecondaryLabel => SECONDARY_BUTTON,
        BerylThemeRole::TranscriptShell => TRANSCRIPT,
        BerylThemeRole::TranscriptAssistantFinal => TRANSCRIPT_TEXT,
        BerylThemeRole::TranscriptAssistantCommentary => COMMENTARY,
        BerylThemeRole::TranscriptAssistantReasoning => REASONING,
        BerylThemeRole::TranscriptUserInput => USER_INPUT,
        BerylThemeRole::TranscriptUserInputText => USER_INPUT,
        BerylThemeRole::TranscriptActivityCaret => ACCENT,
        BerylThemeRole::TranscriptSelection => SELECTED,
        BerylThemeRole::TranscriptQuotePopup | BerylThemeRole::TranscriptContextMenu => POPUP,
        BerylThemeRole::TranscriptQuotePopupText
        | BerylThemeRole::TranscriptContextMenuHeaderText => POPUP,
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
        BerylThemeRole::CodePanelHeaderText => CODE_PANEL_HEADER,
        BerylThemeRole::CodePanelBody => CODE,
        BerylThemeRole::CodePanelBodyText => CODE,
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
        BerylThemeRole::GraphColumnHeaderText => ROW,
        BerylThemeRole::GraphRowTopic => ROW,
        BerylThemeRole::GraphRowTopicText => ROW,
        BerylThemeRole::GraphRowChecklist => ROW,
        BerylThemeRole::GraphRowChecklistText => ROW,
        BerylThemeRole::GraphRowChecklistItem => ROW,
        BerylThemeRole::GraphRowChecklistItemText => ROW,
        BerylThemeRole::GraphRowThreadRef => LINK,
        BerylThemeRole::GraphRowThreadRefText => LINK,
        BerylThemeRole::GraphRowThreadRefMeta => MUTED,
        BerylThemeRole::GraphRowSoftLink => EMPHASIS,
        BerylThemeRole::GraphRowSoftLinkText => EMPHASIS,
        BerylThemeRole::GraphRowHover => ROW_HOVER,
        BerylThemeRole::GraphRowSelected => SELECTED,
        BerylThemeRole::GraphRowSelectedText => SELECTED,
        BerylThemeRole::GraphRowPending => PENDING,
        BerylThemeRole::GraphRowPendingText => PENDING,
        BerylThemeRole::GraphRowDisabled => DISABLED,
        BerylThemeRole::GraphRowDisabledText => DISABLED,
        BerylThemeRole::GraphRowInvalid => UNAVAILABLE,
        BerylThemeRole::GraphRowInvalidText => UNAVAILABLE,
        BerylThemeRole::GraphRowError => ERROR,
        BerylThemeRole::GraphRowErrorText => ERROR,
        BerylThemeRole::ChecklistSidebar => PANEL,
        BerylThemeRole::ChecklistHeader => ROW,
        BerylThemeRole::ChecklistRow => ROW,
        BerylThemeRole::ChecklistRowNumberText => MUTED,
        BerylThemeRole::ChecklistRowText => ROW,
        BerylThemeRole::ChecklistStatusTodo => MUTED,
        BerylThemeRole::ChecklistStatusTodoText => MUTED,
        BerylThemeRole::ChecklistStatusInProgress => WARNING,
        BerylThemeRole::ChecklistStatusInProgressText => WARNING,
        BerylThemeRole::ChecklistStatusDone => SUCCESS,
        BerylThemeRole::ChecklistStatusDoneText => SUCCESS,
        BerylThemeRole::ThreadSelectorSurface => POPUP,
        BerylThemeRole::ThreadSelectorHeaderText => POPUP,
        BerylThemeRole::ThreadSelectorColumn => PANEL,
        BerylThemeRole::ThreadSelectorColumnHeader => POPUP,
        BerylThemeRole::ThreadSelectorColumnHeaderText => ROW,
        BerylThemeRole::ThreadSelectorRow => ROW,
        BerylThemeRole::ThreadSelectorRowLabel => ROW,
        BerylThemeRole::ThreadSelectorRowMeta => MUTED,
        BerylThemeRole::ThreadSelectorRowSelected => SELECTED,
        BerylThemeRole::ThreadSelectorRowSelectedText => SELECTED,
        BerylThemeRole::ThreadSelectorRowActive => SUCCESS,
        BerylThemeRole::ThreadSelectorRowActiveText => SUCCESS,
        BerylThemeRole::ThreadSelectorRowUnavailable => UNAVAILABLE,
        BerylThemeRole::ThreadSelectorRowUnavailableText => MUTED,
        BerylThemeRole::WorkspacePickerSurface => POPUP,
        BerylThemeRole::WorkspacePickerHeaderText => POPUP,
        BerylThemeRole::WorkspacePickerHeaderDetail => MUTED,
        BerylThemeRole::WorkspacePickerWorkspaceRow => ROW,
        BerylThemeRole::WorkspacePickerWorkspaceRowTitle => ROW,
        BerylThemeRole::WorkspacePickerWorkspaceRowPath => CODE,
        BerylThemeRole::WorkspacePickerMemberRow => ROW,
        BerylThemeRole::WorkspacePickerMemberRowTitle => ROW,
        BerylThemeRole::WorkspacePickerMemberRowPath => CODE,
        BerylThemeRole::WorkspacePickerRuntimeRow => ROW,
        BerylThemeRole::WorkspacePickerRuntimeRowText => ROW,
        BerylThemeRole::WorkspacePickerUnavailableText => MUTED,
        BerylThemeRole::WorkspacePickerRowActive => SELECTED,
        BerylThemeRole::ColumnSelectorColumn => PANEL,
        BerylThemeRole::ColumnSelectorHeader => ROW,
        BerylThemeRole::ColumnSelectorHeaderText => ROW,
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
        BerylThemeRole::StatusLineCell => STATUS_LINE,
        BerylThemeRole::StatusLineLabel => MUTED,
        BerylThemeRole::StatusLineValue => STATUS_LINE,
        BerylThemeRole::StatusValueWorking => WORKING,
        BerylThemeRole::StatusValueCompacting => COMPACTING,
        BerylThemeRole::StatusValueOk => SUCCESS,
        BerylThemeRole::StatusValueError => ERROR,
        BerylThemeRole::StatusValuePending => PENDING,
        BerylThemeRole::StatusValueUnavailable => UNAVAILABLE,
        BerylThemeRole::StatusValueStreaming => STREAMING,
        BerylThemeRole::ActivityPanel => STATUS_LINE,
        BerylThemeRole::ActivityRow => STATUS_LINE,
        BerylThemeRole::ActivityLabel => MUTED,
        BerylThemeRole::ActivityValue => STATUS_LINE,
        BerylThemeRole::ActivityIndicatorRunning => WORKING,
        BerylThemeRole::ActivityIndicatorOk => SUCCESS,
        BerylThemeRole::ActivityIndicatorError => ERROR,
        BerylThemeRole::ActivityResizeHandle => ACCENT,
        BerylThemeRole::ScrollbarThumbNormal => SCROLLBAR,
        BerylThemeRole::ScrollbarThumbHover => SCROLLBAR_HOVER,
        BerylThemeRole::ScrollbarThumbDragging => ACCENT,
        BerylThemeRole::MediaPlaceholder => MEDIA,
        BerylThemeRole::MediaPlaceholderText => MUTED,
        BerylThemeRole::MediaPlaceholderLoading => PENDING,
        BerylThemeRole::MediaPlaceholderLoadingText => PENDING,
        BerylThemeRole::MediaPlaceholderUnavailable => UNAVAILABLE,
        BerylThemeRole::MediaPlaceholderUnavailableText => WARNING,
        BerylThemeRole::MediaBorder => MEDIA_BORDER,
        BerylThemeRole::MediaCaption => MUTED,
        BerylThemeRole::ComposerImageMarker => INLINE_CODE,
        BerylThemeRole::TranscriptImageMarker => INLINE_CODE,
        BerylThemeRole::FocusRing => ACCENT,
    }
}
