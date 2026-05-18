use super::roles::{BerylThemeProperty, BerylThemeRole};

const NO_PROPERTIES: &[BerylThemeProperty] = &[];

const BACKGROUND_PROPERTIES: &[BerylThemeProperty] = &[BerylThemeProperty::Background];

const BORDER_PROPERTIES: &[BerylThemeProperty] = &[BerylThemeProperty::Border];

const COLOR_PROPERTIES: &[BerylThemeProperty] = &[BerylThemeProperty::Color];

const FOREGROUND_PROPERTIES: &[BerylThemeProperty] = &[BerylThemeProperty::Foreground];

const TEXT_BACKGROUND_PROPERTIES: &[BerylThemeProperty] = &[BerylThemeProperty::TextBackground];

const BACKGROUND_FOREGROUND_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Background,
    BerylThemeProperty::Foreground,
];

const FOREGROUND_TEXT_BACKGROUND_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Foreground,
    BerylThemeProperty::TextBackground,
];

const SURFACE_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Background,
    BerylThemeProperty::Border,
    BerylThemeProperty::Foreground,
];

const TEXT_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Foreground,
    BerylThemeProperty::TextBackground,
    BerylThemeProperty::FontFamily,
    BerylThemeProperty::FontSize,
    BerylThemeProperty::FontWeight,
];

const ROOT_PROPERTIES: &[BerylThemeProperty] = &[
    BerylThemeProperty::Background,
    BerylThemeProperty::Border,
    BerylThemeProperty::Color,
    BerylThemeProperty::Foreground,
    BerylThemeProperty::TextBackground,
    BerylThemeProperty::FontFamily,
    BerylThemeProperty::FontSize,
    BerylThemeProperty::FontWeight,
];

pub fn built_in_theme_supported_properties(role: BerylThemeRole) -> &'static [BerylThemeProperty] {
    match role {
        BerylThemeRole::Root => ROOT_PROPERTIES,
        BerylThemeRole::Text
        | BerylThemeRole::TextMuted
        | BerylThemeRole::TextSubtle
        | BerylThemeRole::TextValue
        | BerylThemeRole::TextLink
        | BerylThemeRole::TextCode
        | BerylThemeRole::TextSemanticInfo
        | BerylThemeRole::TextSemanticWarning
        | BerylThemeRole::TextSemanticError
        | BerylThemeRole::TextSemanticSuccess
        | BerylThemeRole::ControlButtonLabel
        | BerylThemeRole::ButtonPrimaryLabel
        | BerylThemeRole::ButtonSecondaryLabel
        | BerylThemeRole::ControlInputText
        | BerylThemeRole::ControlRowLabel
        | BerylThemeRole::ControlListHeader
        | BerylThemeRole::ControlMenuItemLabel
        | BerylThemeRole::ControlPopupHeader
        | BerylThemeRole::ControlNoticeTitle
        | BerylThemeRole::ControlNoticeDetail
        | BerylThemeRole::ControlStatusLabel
        | BerylThemeRole::ControlStatusValue
        | BerylThemeRole::ControlDropdownLabel
        | BerylThemeRole::ControlColorInputLabel
        | BerylThemeRole::ControlColorInputValue
        | BerylThemeRole::ControlFilePickerLabel
        | BerylThemeRole::ControlTooltipText
        | BerylThemeRole::InputFieldText
        | BerylThemeRole::SettingsSidebarRowText
        | BerylThemeRole::SettingsGroupHeaderText
        | BerylThemeRole::SettingsRowLabel
        | BerylThemeRole::SettingsRowValue
        | BerylThemeRole::SettingsRowDisabledText
        | BerylThemeRole::SettingsInputText
        | BerylThemeRole::SettingsButtonPrimaryLabel
        | BerylThemeRole::SettingsButtonSecondaryLabel
        | BerylThemeRole::GraphColumnHeaderText
        | BerylThemeRole::GraphRowTopicText
        | BerylThemeRole::GraphRowChecklistText
        | BerylThemeRole::GraphRowChecklistItemText
        | BerylThemeRole::GraphRowThreadRefText
        | BerylThemeRole::GraphRowThreadRefMeta
        | BerylThemeRole::GraphRowSoftLinkText
        | BerylThemeRole::GraphRowSelectedText
        | BerylThemeRole::GraphRowPendingText
        | BerylThemeRole::GraphRowInvalidText
        | BerylThemeRole::GraphRowErrorText
        | BerylThemeRole::ChecklistHeader
        | BerylThemeRole::ChecklistRowNumberText
        | BerylThemeRole::ChecklistRowText
        | BerylThemeRole::ChecklistStatusTodoText
        | BerylThemeRole::ChecklistStatusInProgressText
        | BerylThemeRole::ChecklistStatusDoneText => TEXT_PROPERTIES,
        BerylThemeRole::Surface
        | BerylThemeRole::SurfaceWindow
        | BerylThemeRole::SurfacePanel
        | BerylThemeRole::SurfaceElevated
        | BerylThemeRole::SurfaceInset
        | BerylThemeRole::SurfaceOverlay
        | BerylThemeRole::Control
        | BerylThemeRole::ControlButton
        | BerylThemeRole::ControlInput
        | BerylThemeRole::ControlSelection
        | BerylThemeRole::ControlList
        | BerylThemeRole::ControlMenu
        | BerylThemeRole::ControlMenuItem
        | BerylThemeRole::ControlPopup
        | BerylThemeRole::ControlNotice
        | BerylThemeRole::ControlStatus
        | BerylThemeRole::ControlDropdown
        | BerylThemeRole::ControlColorInput
        | BerylThemeRole::ControlFilePicker
        | BerylThemeRole::ControlTooltip
        | BerylThemeRole::ControlScrollbar
        | BerylThemeRole::InteractionHover
        | BerylThemeRole::InteractionPressed
        | BerylThemeRole::InteractionActive
        | BerylThemeRole::InteractionSelected
        | BerylThemeRole::InteractionFocused
        | BerylThemeRole::InteractionDisabled
        | BerylThemeRole::SemanticInfo
        | BerylThemeRole::SemanticWarning
        | BerylThemeRole::SemanticError
        | BerylThemeRole::SemanticSuccess => SURFACE_PROPERTIES,
        BerylThemeRole::Primitive
        | BerylThemeRole::PrimitiveSeparator
        | BerylThemeRole::PrimitiveFocusRing
        | BerylThemeRole::PrimitiveCaret
        | BerylThemeRole::PrimitiveAccentMarker
        | BerylThemeRole::PrimitiveResizeHandle
        | BerylThemeRole::PrimitiveScrollbarThumb => COLOR_PROPERTIES,
        BerylThemeRole::AppWindow => BACKGROUND_FOREGROUND_PROPERTIES,
        BerylThemeRole::AppWindowTitle => TEXT_PROPERTIES,
        BerylThemeRole::MainToolbar => BACKGROUND_PROPERTIES,
        BerylThemeRole::MainToolbarTitle | BerylThemeRole::MainThreadStripActiveThreadLabel => {
            TEXT_PROPERTIES
        }
        BerylThemeRole::MainThreadStripActiveThread => SURFACE_PROPERTIES,
        BerylThemeRole::MainThreadStrip | BerylThemeRole::InputPanel => BACKGROUND_PROPERTIES,
        BerylThemeRole::MainSeparator | BerylThemeRole::StructuralSeparator => COLOR_PROPERTIES,
        BerylThemeRole::Panel => SURFACE_PROPERTIES,
        BerylThemeRole::SurfaceRow => SURFACE_PROPERTIES,
        BerylThemeRole::SurfaceRowHover => BACKGROUND_PROPERTIES,
        BerylThemeRole::SurfaceRowDisabled => SURFACE_PROPERTIES,
        BerylThemeRole::SurfaceRowInfo => SURFACE_PROPERTIES,
        BerylThemeRole::SurfaceRowSelected
        | BerylThemeRole::SurfaceRowPending
        | BerylThemeRole::SurfaceRowUnavailable
        | BerylThemeRole::SurfaceRowError
        | BerylThemeRole::SurfaceRowWarning
        | BerylThemeRole::SurfaceRowSuccess => NO_PROPERTIES,

        BerylThemeRole::ButtonPrimaryNormal
        | BerylThemeRole::ButtonSecondaryNormal
        | BerylThemeRole::SettingsButtonPrimary
        | BerylThemeRole::SettingsButtonSecondary => SURFACE_PROPERTIES,
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
        BerylThemeRole::InputSelection => TEXT_BACKGROUND_PROPERTIES,
        BerylThemeRole::InputCaret | BerylThemeRole::FocusRing => COLOR_PROPERTIES,
        BerylThemeRole::InputFieldFocused | BerylThemeRole::InputError => NO_PROPERTIES,

        BerylThemeRole::SettingsWindow => BACKGROUND_PROPERTIES,
        BerylThemeRole::SettingsSidebar
        | BerylThemeRole::SettingsSidebarRowNormal
        | BerylThemeRole::SettingsSidebarRowSelected
        | BerylThemeRole::SettingsPage
        | BerylThemeRole::SettingsGroup
        | BerylThemeRole::SettingsRowNormal
        | BerylThemeRole::SettingsPopup
        | BerylThemeRole::SettingsInputNormal => SURFACE_PROPERTIES,
        BerylThemeRole::SettingsSidebarRowHover
        | BerylThemeRole::SettingsRowHover
        | BerylThemeRole::SettingsRowModified => BACKGROUND_PROPERTIES,
        BerylThemeRole::SettingsRowDisabled => NO_PROPERTIES,
        BerylThemeRole::SettingsInputFocused => BORDER_PROPERTIES,
        BerylThemeRole::SettingsInputError => BORDER_PROPERTIES,
        BerylThemeRole::SettingsInputSelection => TEXT_BACKGROUND_PROPERTIES,
        BerylThemeRole::SettingsInputCaret => COLOR_PROPERTIES,

        BerylThemeRole::TranscriptShell => BACKGROUND_FOREGROUND_PROPERTIES,
        BerylThemeRole::MediaPlaceholder
        | BerylThemeRole::MediaPlaceholderLoading
        | BerylThemeRole::MediaPlaceholderUnavailable => BACKGROUND_PROPERTIES,
        BerylThemeRole::TranscriptAssistantFinal
        | BerylThemeRole::TranscriptAssistantCommentary
        | BerylThemeRole::TranscriptAssistantReasoning
        | BerylThemeRole::TranscriptUserInputText
        | BerylThemeRole::TranscriptQuotePopupText
        | BerylThemeRole::TranscriptContextMenuHeaderText
        | BerylThemeRole::MarkdownParagraph
        | BerylThemeRole::MarkdownHeading
        | BerylThemeRole::MarkdownEmphasis
        | BerylThemeRole::MarkdownStrongEmphasis
        | BerylThemeRole::MarkdownInlineCode
        | BerylThemeRole::MarkdownLink
        | BerylThemeRole::MarkdownUnsupportedFallback => TEXT_PROPERTIES,
        BerylThemeRole::TranscriptUserInput => SURFACE_PROPERTIES,
        BerylThemeRole::TranscriptActivityCaret
        | BerylThemeRole::MarkdownThematicBreak
        | BerylThemeRole::CodePanelResizeHandle
        | BerylThemeRole::ScrollbarThumbNormal => COLOR_PROPERTIES,
        BerylThemeRole::TranscriptSelection => TEXT_BACKGROUND_PROPERTIES,
        BerylThemeRole::TranscriptQuotePopup
        | BerylThemeRole::TranscriptPending
        | BerylThemeRole::TranscriptUnavailable => SURFACE_PROPERTIES,
        BerylThemeRole::TranscriptContextMenu => SURFACE_PROPERTIES,
        BerylThemeRole::MarkdownBlockQuote | BerylThemeRole::CodePanelBorder => COLOR_PROPERTIES,
        BerylThemeRole::MediaBorder => BORDER_PROPERTIES,
        BerylThemeRole::MarkdownListMarker
        | BerylThemeRole::CodePanelHeaderText
        | BerylThemeRole::CodePanelBodyText => TEXT_PROPERTIES,
        BerylThemeRole::CodePanelHeader | BerylThemeRole::CodePanelBody => BACKGROUND_PROPERTIES,
        BerylThemeRole::CodePanelContainer => BACKGROUND_PROPERTIES,
        BerylThemeRole::CodePanelSelection => NO_PROPERTIES,
        BerylThemeRole::TranscriptImageMarker => FOREGROUND_TEXT_BACKGROUND_PROPERTIES,
        BerylThemeRole::ComposerImageMarker => NO_PROPERTIES,
        BerylThemeRole::MediaPlaceholderText
        | BerylThemeRole::MediaPlaceholderLoadingText
        | BerylThemeRole::MediaPlaceholderUnavailableText
        | BerylThemeRole::MediaCaption => TEXT_PROPERTIES,

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

        BerylThemeRole::GraphOverlay
        | BerylThemeRole::GraphColumn
        | BerylThemeRole::GraphColumnHeader
        | BerylThemeRole::GraphRowTopic
        | BerylThemeRole::GraphRowChecklist
        | BerylThemeRole::GraphRowChecklistItem
        | BerylThemeRole::GraphRowThreadRef
        | BerylThemeRole::GraphRowSoftLink
        | BerylThemeRole::GraphRowSelected
        | BerylThemeRole::GraphRowInvalid
        | BerylThemeRole::GraphRowError
        | BerylThemeRole::ChecklistSidebar
        | BerylThemeRole::ChecklistRow => SURFACE_PROPERTIES,
        BerylThemeRole::PopupSurface => SURFACE_PROPERTIES,
        BerylThemeRole::GraphRowHover => BACKGROUND_PROPERTIES,
        BerylThemeRole::GraphRowPending
        | BerylThemeRole::GraphRowDisabled
        | BerylThemeRole::GraphRowDisabledText => NO_PROPERTIES,
        BerylThemeRole::ChecklistStatusTodo
        | BerylThemeRole::ChecklistStatusInProgress
        | BerylThemeRole::ChecklistStatusDone => COLOR_PROPERTIES,

        BerylThemeRole::ThreadSelectorSurface
        | BerylThemeRole::ThreadSelectorColumn
        | BerylThemeRole::ThreadSelectorColumnHeader
        | BerylThemeRole::ThreadSelectorRow
        | BerylThemeRole::ThreadSelectorRowActive
        | BerylThemeRole::WorkspacePickerSurface
        | BerylThemeRole::WorkspacePickerWorkspaceRow
        | BerylThemeRole::WorkspacePickerMemberRow
        | BerylThemeRole::WorkspacePickerRuntimeRow
        | BerylThemeRole::ColumnSelectorColumn
        | BerylThemeRole::ColumnSelectorHeader
        | BerylThemeRole::ColumnSelectorRow
        | BerylThemeRole::ColumnSelectorRowSelected => SURFACE_PROPERTIES,
        BerylThemeRole::ThreadSelectorHeaderText
        | BerylThemeRole::ThreadSelectorColumnHeaderText
        | BerylThemeRole::ThreadSelectorRowLabel
        | BerylThemeRole::ThreadSelectorRowMeta
        | BerylThemeRole::ThreadSelectorRowSelectedText
        | BerylThemeRole::ThreadSelectorRowActiveText
        | BerylThemeRole::ThreadSelectorRowUnavailableText
        | BerylThemeRole::WorkspacePickerHeaderText
        | BerylThemeRole::WorkspacePickerHeaderDetail
        | BerylThemeRole::WorkspacePickerWorkspaceRowTitle
        | BerylThemeRole::WorkspacePickerWorkspaceRowPath
        | BerylThemeRole::WorkspacePickerMemberRowTitle
        | BerylThemeRole::WorkspacePickerMemberRowPath
        | BerylThemeRole::WorkspacePickerRuntimeRowText
        | BerylThemeRole::WorkspacePickerUnavailableText
        | BerylThemeRole::ColumnSelectorHeaderText => TEXT_PROPERTIES,
        BerylThemeRole::ThreadSelectorRowSelected
        | BerylThemeRole::ThreadSelectorRowUnavailable => SURFACE_PROPERTIES,
        BerylThemeRole::WorkspacePickerRowActive | BerylThemeRole::ColumnSelectorAccent => {
            COLOR_PROPERTIES
        }

        BerylThemeRole::PopupRowHover | BerylThemeRole::PopupRowSelected => BACKGROUND_PROPERTIES,
        BerylThemeRole::PopupRowNormal | BerylThemeRole::PopupRowDisabled => NO_PROPERTIES,
        BerylThemeRole::OverlayBackdrop => NO_PROPERTIES,
        BerylThemeRole::NoticeInfo
        | BerylThemeRole::NoticeWarning
        | BerylThemeRole::NoticeError
        | BerylThemeRole::NoticeSuccess => SURFACE_PROPERTIES,
        BerylThemeRole::DiagnosticSurface
        | BerylThemeRole::DiagnosticRow
        | BerylThemeRole::DiagnosticError
        | BerylThemeRole::DiagnosticWarning => NO_PROPERTIES,
        BerylThemeRole::StatusLine | BerylThemeRole::ActivityPanel => {
            BACKGROUND_FOREGROUND_PROPERTIES
        }
        BerylThemeRole::StatusLineCell | BerylThemeRole::ActivityRow => SURFACE_PROPERTIES,
        BerylThemeRole::StatusLineLabel
        | BerylThemeRole::StatusLineValue
        | BerylThemeRole::ActivityLabel
        | BerylThemeRole::ActivityValue => TEXT_PROPERTIES,
        BerylThemeRole::StatusValueWorking
        | BerylThemeRole::StatusValueOk
        | BerylThemeRole::StatusValueError
        | BerylThemeRole::StatusValueCompacting
        | BerylThemeRole::StatusValuePending
        | BerylThemeRole::StatusValueUnavailable
        | BerylThemeRole::StatusValueStreaming => FOREGROUND_PROPERTIES,
        BerylThemeRole::ActivityIndicatorRunning
        | BerylThemeRole::ActivityIndicatorOk
        | BerylThemeRole::ActivityIndicatorError
        | BerylThemeRole::ActivityResizeHandle => COLOR_PROPERTIES,
        BerylThemeRole::ScrollbarThumbHover | BerylThemeRole::ScrollbarThumbDragging => {
            COLOR_PROPERTIES
        }
    }
}

pub fn built_in_theme_supports_property(
    role: BerylThemeRole,
    property: BerylThemeProperty,
) -> bool {
    built_in_theme_supported_properties(role).contains(&property)
}

pub fn built_in_theme_role_is_editable(role: BerylThemeRole) -> bool {
    !built_in_theme_supported_properties(role).is_empty()
}
