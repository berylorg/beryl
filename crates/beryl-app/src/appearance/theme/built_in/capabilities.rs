use super::roles::{BerylThemeProperty, BerylThemeRole};

const NO_PROPERTIES: &[BerylThemeProperty] = &[];

const BACKGROUND_PROPERTIES: &[BerylThemeProperty] = &[BerylThemeProperty::Background];

const BACKGROUND_FONT_WEIGHT_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Background,
    BerylThemeProperty::FontWeight,
];

const BORDER_PROPERTIES: &[BerylThemeProperty] = &[BerylThemeProperty::Border];

const COLOR_PROPERTIES: &[BerylThemeProperty] = &[BerylThemeProperty::Color];

const FOREGROUND_PROPERTIES: &[BerylThemeProperty] = &[BerylThemeProperty::Foreground];

const FONT_WEIGHT_PROPERTIES: &[BerylThemeProperty] = &[BerylThemeProperty::FontWeight];

const FONT_FAMILY_FONT_WEIGHT_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::FontFamily,
    BerylThemeProperty::FontWeight,
];

const FOREGROUND_FONT_WEIGHT_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Foreground,
    BerylThemeProperty::FontWeight,
];

const BORDER_FOREGROUND_PROPERTIES: &[BerylThemeProperty] =
    &[BerylThemeProperty::Border, BerylThemeProperty::Foreground];

const BORDER_FOREGROUND_FONT_WEIGHT_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Border,
    BerylThemeProperty::Foreground,
    BerylThemeProperty::FontWeight,
];

const BACKGROUND_FOREGROUND_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Background,
    BerylThemeProperty::Foreground,
];

const BACKGROUND_BORDER_PROPERTIES: &[BerylThemeProperty] =
    &[BerylThemeProperty::Background, BerylThemeProperty::Border];

const APP_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Background,
    BerylThemeProperty::Foreground,
    BerylThemeProperty::FontWeight,
];

const FOREGROUND_TEXT_BACKGROUND_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Foreground,
    BerylThemeProperty::TextBackground,
];

const FOREGROUND_TYPOGRAPHY_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Foreground,
    BerylThemeProperty::FontFamily,
    BerylThemeProperty::FontSize,
    BerylThemeProperty::FontWeight,
];

const SURFACE_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Background,
    BerylThemeProperty::Border,
    BerylThemeProperty::Foreground,
];

const WEIGHTED_SURFACE_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Background,
    BerylThemeProperty::Border,
    BerylThemeProperty::Foreground,
    BerylThemeProperty::FontWeight,
];

const BUTTON_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Background,
    BerylThemeProperty::Border,
    BerylThemeProperty::Foreground,
    BerylThemeProperty::FontWeight,
];

const TEXT_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Foreground,
    BerylThemeProperty::TextBackground,
    BerylThemeProperty::FontFamily,
    BerylThemeProperty::FontSize,
    BerylThemeProperty::FontWeight,
];

const BACKGROUND_TEXT_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Background,
    BerylThemeProperty::Foreground,
    BerylThemeProperty::FontFamily,
    BerylThemeProperty::FontSize,
    BerylThemeProperty::FontWeight,
];

const TEXT_SURFACE_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Background,
    BerylThemeProperty::Border,
    BerylThemeProperty::Foreground,
    BerylThemeProperty::TextBackground,
    BerylThemeProperty::FontFamily,
    BerylThemeProperty::FontSize,
    BerylThemeProperty::FontWeight,
];

const STATUS_VALUE_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Background,
    BerylThemeProperty::Border,
    BerylThemeProperty::Foreground,
];

pub fn built_in_theme_supported_properties(role: BerylThemeRole) -> &'static [BerylThemeProperty] {
    match role {
        BerylThemeRole::AppWindow => APP_PROPERTIES,
        BerylThemeRole::MainToolbar => BACKGROUND_FONT_WEIGHT_PROPERTIES,
        BerylThemeRole::MainThreadStrip | BerylThemeRole::InputPanel => BACKGROUND_PROPERTIES,
        BerylThemeRole::MainSeparator | BerylThemeRole::StructuralSeparator => COLOR_PROPERTIES,
        BerylThemeRole::Panel => SURFACE_PROPERTIES,
        BerylThemeRole::SurfaceRow | BerylThemeRole::SurfaceRowHover => BACKGROUND_PROPERTIES,
        BerylThemeRole::SurfaceRowDisabled => SURFACE_PROPERTIES,
        BerylThemeRole::SurfaceRowInfo => FOREGROUND_FONT_WEIGHT_PROPERTIES,
        BerylThemeRole::SurfaceRowSelected
        | BerylThemeRole::SurfaceRowPending
        | BerylThemeRole::SurfaceRowUnavailable
        | BerylThemeRole::SurfaceRowError
        | BerylThemeRole::SurfaceRowWarning
        | BerylThemeRole::SurfaceRowSuccess => NO_PROPERTIES,

        BerylThemeRole::ButtonPrimaryNormal
        | BerylThemeRole::ButtonSecondaryNormal
        | BerylThemeRole::SettingsButtonPrimary
        | BerylThemeRole::SettingsButtonSecondary => BUTTON_PROPERTIES,
        BerylThemeRole::ButtonPrimaryHover
        | BerylThemeRole::ButtonPrimaryActive
        | BerylThemeRole::ButtonPrimaryDisabled
        | BerylThemeRole::ButtonSecondaryHover
        | BerylThemeRole::ButtonSecondaryActive
        | BerylThemeRole::ButtonSecondaryDisabled
        | BerylThemeRole::CodePanelButtonNormal
        | BerylThemeRole::CodePanelButtonHover
        | BerylThemeRole::CodePanelButtonActive => SURFACE_PROPERTIES,
        BerylThemeRole::ButtonPrimaryPressed
        | BerylThemeRole::ButtonSecondaryPressed
        | BerylThemeRole::CodePanelButtonDisabled => NO_PROPERTIES,

        BerylThemeRole::InputField => SURFACE_PROPERTIES,
        BerylThemeRole::InputFieldFocused
        | BerylThemeRole::InputSelection
        | BerylThemeRole::InputCaret
        | BerylThemeRole::InputError => NO_PROPERTIES,

        BerylThemeRole::SettingsWindow => BACKGROUND_PROPERTIES,
        BerylThemeRole::SettingsGroup
        | BerylThemeRole::SettingsRowNormal
        | BerylThemeRole::SettingsPopup
        | BerylThemeRole::SettingsInputNormal => SURFACE_PROPERTIES,
        BerylThemeRole::SettingsRowDisabled => FOREGROUND_PROPERTIES,
        BerylThemeRole::SettingsInputFocused => BORDER_FOREGROUND_PROPERTIES,
        BerylThemeRole::SettingsInputError => BORDER_PROPERTIES,
        BerylThemeRole::SettingsInputSelection => BACKGROUND_PROPERTIES,
        BerylThemeRole::SettingsSidebar
        | BerylThemeRole::SettingsSidebarRowNormal
        | BerylThemeRole::SettingsSidebarRowHover
        | BerylThemeRole::SettingsSidebarRowSelected
        | BerylThemeRole::SettingsPage
        | BerylThemeRole::SettingsRowHover
        | BerylThemeRole::SettingsRowModified => NO_PROPERTIES,

        BerylThemeRole::TranscriptShell
        | BerylThemeRole::MediaPlaceholder
        | BerylThemeRole::MediaPlaceholderLoading
        | BerylThemeRole::MediaPlaceholderUnavailable => BACKGROUND_FOREGROUND_PROPERTIES,
        BerylThemeRole::TranscriptAssistantFinal
        | BerylThemeRole::TranscriptAssistantCommentary
        | BerylThemeRole::TranscriptAssistantReasoning
        | BerylThemeRole::MarkdownParagraph
        | BerylThemeRole::MarkdownHeading
        | BerylThemeRole::MarkdownEmphasis
        | BerylThemeRole::MarkdownStrongEmphasis
        | BerylThemeRole::MarkdownInlineCode
        | BerylThemeRole::MarkdownLink
        | BerylThemeRole::MarkdownUnsupportedFallback => TEXT_PROPERTIES,
        BerylThemeRole::TranscriptUserInput => TEXT_SURFACE_PROPERTIES,
        BerylThemeRole::TranscriptActivityCaret
        | BerylThemeRole::MarkdownThematicBreak
        | BerylThemeRole::CodePanelResizeHandle
        | BerylThemeRole::ScrollbarThumbNormal => COLOR_PROPERTIES,
        BerylThemeRole::TranscriptSelection => BACKGROUND_PROPERTIES,
        BerylThemeRole::TranscriptQuotePopup
        | BerylThemeRole::TranscriptPending
        | BerylThemeRole::TranscriptUnavailable => SURFACE_PROPERTIES,
        BerylThemeRole::TranscriptContextMenu => NO_PROPERTIES,
        BerylThemeRole::MarkdownBlockQuote
        | BerylThemeRole::CodePanelBorder
        | BerylThemeRole::MediaBorder => BORDER_PROPERTIES,
        BerylThemeRole::MarkdownListMarker | BerylThemeRole::CodePanelHeader => {
            FOREGROUND_TYPOGRAPHY_PROPERTIES
        }
        BerylThemeRole::CodePanelBody => BACKGROUND_TEXT_PROPERTIES,
        BerylThemeRole::CodePanelContainer => BACKGROUND_PROPERTIES,
        BerylThemeRole::CodePanelSelection => NO_PROPERTIES,
        BerylThemeRole::TranscriptImageMarker => FOREGROUND_TEXT_BACKGROUND_PROPERTIES,
        BerylThemeRole::ComposerImageMarker => NO_PROPERTIES,
        BerylThemeRole::MediaCaption => FOREGROUND_PROPERTIES,

        BerylThemeRole::SyntaxMarkupHeadingMarker
        | BerylThemeRole::SyntaxMarkupQuoteMarker
        | BerylThemeRole::SyntaxMarkupListMarker
        | BerylThemeRole::SyntaxMarkupThematicBreak
        | BerylThemeRole::SyntaxMarkupFenceDelimiter
        | BerylThemeRole::SyntaxMarkupFenceInfo
        | BerylThemeRole::SyntaxMarkupCodeBlock
        | BerylThemeRole::SyntaxMarkupCodeSpanDelimiter
        | BerylThemeRole::SyntaxMarkupCodeSpan
        | BerylThemeRole::SyntaxMarkupEmphasisDelimiter
        | BerylThemeRole::SyntaxMarkupStrongDelimiter
        | BerylThemeRole::SyntaxMarkupLinkText
        | BerylThemeRole::SyntaxMarkupLinkDestination
        | BerylThemeRole::SyntaxMarkupImageMarker
        | BerylThemeRole::SyntaxMarkupPunctuation
        | BerylThemeRole::SyntaxMarkupHtml
        | BerylThemeRole::SyntaxEscape
        | BerylThemeRole::SyntaxStructuralPunctuation
        | BerylThemeRole::SyntaxKey
        | BerylThemeRole::SyntaxString
        | BerylThemeRole::SyntaxNumber
        | BerylThemeRole::SyntaxBoolean
        | BerylThemeRole::SyntaxNull
        | BerylThemeRole::SyntaxDateTime
        | BerylThemeRole::SyntaxComment
        | BerylThemeRole::SyntaxSectionHeader
        | BerylThemeRole::SyntaxAssignment
        | BerylThemeRole::SyntaxTokenEscape
        | BerylThemeRole::SyntaxError => FOREGROUND_PROPERTIES,

        BerylThemeRole::GraphOverlay | BerylThemeRole::GraphColumn => BACKGROUND_BORDER_PROPERTIES,
        BerylThemeRole::GraphColumnHeader
        | BerylThemeRole::GraphRowTopic
        | BerylThemeRole::GraphRowChecklist
        | BerylThemeRole::GraphRowChecklistItem
        | BerylThemeRole::GraphRowThreadRef
        | BerylThemeRole::GraphRowSoftLink
        | BerylThemeRole::GraphRowSelected
        | BerylThemeRole::GraphRowInvalid
        | BerylThemeRole::ChecklistRow
        | BerylThemeRole::PopupSurface => WEIGHTED_SURFACE_PROPERTIES,
        BerylThemeRole::GraphRowHover => BACKGROUND_PROPERTIES,
        BerylThemeRole::GraphRowPending | BerylThemeRole::GraphRowDisabled => FOREGROUND_PROPERTIES,
        BerylThemeRole::GraphRowError => BACKGROUND_FOREGROUND_PROPERTIES,
        BerylThemeRole::ChecklistSidebar => SURFACE_PROPERTIES,
        BerylThemeRole::ChecklistHeader
        | BerylThemeRole::ChecklistStatusTodo
        | BerylThemeRole::ChecklistStatusInProgress
        | BerylThemeRole::ChecklistStatusDone => FOREGROUND_FONT_WEIGHT_PROPERTIES,

        BerylThemeRole::ThreadSelectorSurface | BerylThemeRole::WorkspacePickerSurface => {
            FONT_WEIGHT_PROPERTIES
        }
        BerylThemeRole::ThreadSelectorRow => NO_PROPERTIES,
        BerylThemeRole::ThreadSelectorRowSelected
        | BerylThemeRole::ThreadSelectorRowUnavailable => SURFACE_PROPERTIES,
        BerylThemeRole::WorkspacePickerWorkspaceRow | BerylThemeRole::WorkspacePickerMemberRow => {
            FONT_FAMILY_FONT_WEIGHT_PROPERTIES
        }
        BerylThemeRole::WorkspacePickerRowActive => BORDER_FOREGROUND_FONT_WEIGHT_PROPERTIES,
        BerylThemeRole::ColumnSelectorColumn
        | BerylThemeRole::ColumnSelectorHeader
        | BerylThemeRole::ColumnSelectorRow
        | BerylThemeRole::ColumnSelectorRowSelected
        | BerylThemeRole::ColumnSelectorAccent => NO_PROPERTIES,

        BerylThemeRole::PopupRowHover | BerylThemeRole::PopupRowSelected => BACKGROUND_PROPERTIES,
        BerylThemeRole::PopupRowNormal | BerylThemeRole::PopupRowDisabled => NO_PROPERTIES,
        BerylThemeRole::OverlayBackdrop => NO_PROPERTIES,
        BerylThemeRole::NoticeInfo | BerylThemeRole::NoticeError => WEIGHTED_SURFACE_PROPERTIES,
        BerylThemeRole::NoticeWarning => SURFACE_PROPERTIES,
        BerylThemeRole::NoticeSuccess => FOREGROUND_PROPERTIES,
        BerylThemeRole::DiagnosticSurface
        | BerylThemeRole::DiagnosticRow
        | BerylThemeRole::DiagnosticError
        | BerylThemeRole::DiagnosticWarning => NO_PROPERTIES,
        BerylThemeRole::StatusLine => BACKGROUND_FOREGROUND_PROPERTIES,
        BerylThemeRole::StatusValueWorking => BORDER_FOREGROUND_PROPERTIES,
        BerylThemeRole::StatusValueOk | BerylThemeRole::StatusValueError => STATUS_VALUE_PROPERTIES,
        BerylThemeRole::StatusValueCompacting
        | BerylThemeRole::StatusValuePending
        | BerylThemeRole::StatusValueUnavailable
        | BerylThemeRole::StatusValueStreaming => FOREGROUND_PROPERTIES,
        BerylThemeRole::ScrollbarThumbHover
        | BerylThemeRole::ScrollbarThumbDragging
        | BerylThemeRole::FocusRing => NO_PROPERTIES,
    }
}

pub fn built_in_theme_supports_property(
    role: BerylThemeRole,
    property: BerylThemeProperty,
) -> bool {
    built_in_theme_supported_properties(role).contains(&property)
}
