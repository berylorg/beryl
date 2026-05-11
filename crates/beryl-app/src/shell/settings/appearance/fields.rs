use gpui_settings_window::{SettingsFieldId, SettingsFieldKind, SettingsSectionId};

use crate::{AppearanceForegroundSettings, AppearanceRoleSettings, AppearanceSettings};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AppearanceSection {
    GeneralUi,
    ConversationText,
    TranscriptReasoning,
    TranscriptCommentary,
    MarkdownHeader,
    Code,
    Emphasis,
    StrongEmphasis,
    Chrome,
    PrimaryButton,
    SecondaryButton,
    Input,
    TranscriptShell,
    StatusLine,
    Surfaces,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AppearanceField {
    FontFamily,
    FontSize,
    FontWeight,
    Foreground,
    Background,
    ToolbarBackground,
    ConversationThreadStripBackground,
    Separator,
    NormalBackground,
    NormalBorder,
    NormalForeground,
    HoverBackground,
    HoverBorder,
    HoverForeground,
    ActiveBackground,
    ActiveBorder,
    ActiveForeground,
    DisabledBackground,
    DisabledBorder,
    DisabledForeground,
    PanelBackground,
    InputBackground,
    InputBorder,
    InputForeground,
    TitleForeground,
    ValueForeground,
    RowBackground,
    PopupBackground,
    Border,
    MutedForeground,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AppearanceFieldSpec {
    pub(super) section: AppearanceSection,
    pub(super) field: AppearanceField,
}

pub(super) const SECTIONS: [AppearanceSection; 15] = [
    AppearanceSection::GeneralUi,
    AppearanceSection::ConversationText,
    AppearanceSection::TranscriptReasoning,
    AppearanceSection::TranscriptCommentary,
    AppearanceSection::MarkdownHeader,
    AppearanceSection::Code,
    AppearanceSection::Emphasis,
    AppearanceSection::StrongEmphasis,
    AppearanceSection::Chrome,
    AppearanceSection::PrimaryButton,
    AppearanceSection::SecondaryButton,
    AppearanceSection::Input,
    AppearanceSection::TranscriptShell,
    AppearanceSection::StatusLine,
    AppearanceSection::Surfaces,
];

const TYPOGRAPHY_FIELDS: [AppearanceField; 5] = [
    AppearanceField::FontFamily,
    AppearanceField::FontSize,
    AppearanceField::FontWeight,
    AppearanceField::Foreground,
    AppearanceField::Background,
];

const FOREGROUND_FIELDS: [AppearanceField; 1] = [AppearanceField::Foreground];

const BUTTON_FIELDS: [AppearanceField; 12] = [
    AppearanceField::NormalBackground,
    AppearanceField::NormalBorder,
    AppearanceField::NormalForeground,
    AppearanceField::HoverBackground,
    AppearanceField::HoverBorder,
    AppearanceField::HoverForeground,
    AppearanceField::ActiveBackground,
    AppearanceField::ActiveBorder,
    AppearanceField::ActiveForeground,
    AppearanceField::DisabledBackground,
    AppearanceField::DisabledBorder,
    AppearanceField::DisabledForeground,
];

const CHROME_FIELDS: [AppearanceField; 3] = [
    AppearanceField::ToolbarBackground,
    AppearanceField::ConversationThreadStripBackground,
    AppearanceField::Separator,
];

const INPUT_FIELDS: [AppearanceField; 4] = [
    AppearanceField::PanelBackground,
    AppearanceField::InputBackground,
    AppearanceField::InputBorder,
    AppearanceField::InputForeground,
];

const TRANSCRIPT_SHELL_FIELDS: [AppearanceField; 2] =
    [AppearanceField::Background, AppearanceField::Foreground];

const STATUS_LINE_FIELDS: [AppearanceField; 3] = [
    AppearanceField::Background,
    AppearanceField::TitleForeground,
    AppearanceField::ValueForeground,
];

const SURFACE_FIELDS: [AppearanceField; 5] = [
    AppearanceField::PanelBackground,
    AppearanceField::RowBackground,
    AppearanceField::PopupBackground,
    AppearanceField::Border,
    AppearanceField::MutedForeground,
];

impl AppearanceSection {
    pub(super) fn id(self) -> &'static str {
        match self {
            Self::GeneralUi => "general_ui",
            Self::ConversationText => "conversation_text",
            Self::TranscriptReasoning => "transcript_reasoning",
            Self::TranscriptCommentary => "transcript_commentary",
            Self::MarkdownHeader => "markdown_header",
            Self::Code => "code",
            Self::Emphasis => "emphasis",
            Self::StrongEmphasis => "strong_emphasis",
            Self::Chrome => "chrome",
            Self::PrimaryButton => "primary_button",
            Self::SecondaryButton => "secondary_button",
            Self::Input => "input",
            Self::TranscriptShell => "transcript_shell",
            Self::StatusLine => "status_line",
            Self::Surfaces => "surfaces",
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::GeneralUi => "General UI",
            Self::ConversationText => "Conversation Text",
            Self::TranscriptReasoning => "Transcript Reasoning",
            Self::TranscriptCommentary => "Transcript Commentary",
            Self::MarkdownHeader => "Markdown Header",
            Self::Code => "Code",
            Self::Emphasis => "Emphasis",
            Self::StrongEmphasis => "Strong Emphasis",
            Self::Chrome => "Chrome",
            Self::PrimaryButton => "Primary Button",
            Self::SecondaryButton => "Secondary Button",
            Self::Input => "Input",
            Self::TranscriptShell => "Transcript Shell",
            Self::StatusLine => "Status Line",
            Self::Surfaces => "Surfaces",
        }
    }

    pub(super) fn section_id(self) -> SettingsSectionId {
        SettingsSectionId::from(self.id())
    }

    pub(super) fn fields(self) -> &'static [AppearanceField] {
        match self {
            Self::GeneralUi
            | Self::ConversationText
            | Self::MarkdownHeader
            | Self::Code
            | Self::Emphasis
            | Self::StrongEmphasis => &TYPOGRAPHY_FIELDS,
            Self::TranscriptReasoning | Self::TranscriptCommentary => &FOREGROUND_FIELDS,
            Self::Chrome => &CHROME_FIELDS,
            Self::PrimaryButton | Self::SecondaryButton => &BUTTON_FIELDS,
            Self::Input => &INPUT_FIELDS,
            Self::TranscriptShell => &TRANSCRIPT_SHELL_FIELDS,
            Self::StatusLine => &STATUS_LINE_FIELDS,
            Self::Surfaces => &SURFACE_FIELDS,
        }
    }

    pub(super) fn is_typography(self) -> bool {
        matches!(
            self,
            Self::GeneralUi
                | Self::ConversationText
                | Self::MarkdownHeader
                | Self::Code
                | Self::Emphasis
                | Self::StrongEmphasis
        )
    }

    pub(super) fn is_transcript_foreground(self) -> bool {
        matches!(self, Self::TranscriptReasoning | Self::TranscriptCommentary)
    }
}

impl AppearanceField {
    pub(super) fn id(self) -> &'static str {
        match self {
            Self::FontFamily => "font_family",
            Self::FontSize => "font_size",
            Self::FontWeight => "font_weight",
            Self::Foreground => "foreground",
            Self::Background => "background",
            Self::ToolbarBackground => "toolbar_background",
            Self::ConversationThreadStripBackground => "conversation_thread_strip_background",
            Self::Separator => "separator",
            Self::NormalBackground => "normal_background",
            Self::NormalBorder => "normal_border",
            Self::NormalForeground => "normal_foreground",
            Self::HoverBackground => "hover_background",
            Self::HoverBorder => "hover_border",
            Self::HoverForeground => "hover_foreground",
            Self::ActiveBackground => "active_background",
            Self::ActiveBorder => "active_border",
            Self::ActiveForeground => "active_foreground",
            Self::DisabledBackground => "disabled_background",
            Self::DisabledBorder => "disabled_border",
            Self::DisabledForeground => "disabled_foreground",
            Self::PanelBackground => "panel_background",
            Self::InputBackground => "input_background",
            Self::InputBorder => "input_border",
            Self::InputForeground => "input_foreground",
            Self::TitleForeground => "title_foreground",
            Self::ValueForeground => "value_foreground",
            Self::RowBackground => "row_background",
            Self::PopupBackground => "popup_background",
            Self::Border => "border",
            Self::MutedForeground => "muted_foreground",
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::FontFamily => "Font family",
            Self::FontSize => "Font size",
            Self::FontWeight => "Font weight",
            Self::Foreground => "Foreground",
            Self::Background => "Background",
            Self::ToolbarBackground => "Toolbar background",
            Self::ConversationThreadStripBackground => "Conversation thread strip background",
            Self::Separator => "Separator",
            Self::NormalBackground => "Normal background",
            Self::NormalBorder => "Normal border",
            Self::NormalForeground => "Normal foreground",
            Self::HoverBackground => "Hover background",
            Self::HoverBorder => "Hover border",
            Self::HoverForeground => "Hover foreground",
            Self::ActiveBackground => "Active background",
            Self::ActiveBorder => "Active border",
            Self::ActiveForeground => "Active foreground",
            Self::DisabledBackground => "Disabled background",
            Self::DisabledBorder => "Disabled border",
            Self::DisabledForeground => "Disabled foreground",
            Self::PanelBackground => "Panel background",
            Self::InputBackground => "Input background",
            Self::InputBorder => "Input border",
            Self::InputForeground => "Input foreground",
            Self::TitleForeground => "Title foreground",
            Self::ValueForeground => "Value foreground",
            Self::RowBackground => "Row background",
            Self::PopupBackground => "Popup background",
            Self::Border => "Border",
            Self::MutedForeground => "Muted foreground",
        }
    }

    pub(super) fn kind(self) -> SettingsFieldKind {
        match self {
            Self::FontFamily | Self::FontSize | Self::FontWeight => SettingsFieldKind::Text,
            _ => SettingsFieldKind::Color,
        }
    }
}

impl AppearanceFieldSpec {
    pub(super) fn field_id(self) -> SettingsFieldId {
        SettingsFieldId::from(format!("{}.{}", self.section.id(), self.field.id()))
    }
}

pub(crate) fn default_section_id() -> SettingsSectionId {
    AppearanceSection::GeneralUi.section_id()
}

pub(crate) fn has_section_id(section_id: &SettingsSectionId) -> bool {
    SECTIONS
        .into_iter()
        .any(|section| section.section_id() == *section_id)
}

pub(super) fn field_specs() -> impl Iterator<Item = AppearanceFieldSpec> {
    SECTIONS.into_iter().flat_map(|section| {
        section
            .fields()
            .iter()
            .copied()
            .map(move |field| AppearanceFieldSpec { section, field })
    })
}

pub(super) fn field_value(settings: &AppearanceSettings, spec: AppearanceFieldSpec) -> String {
    if spec.section.is_typography() {
        return typography_field_value(role_settings(settings, spec.section), spec.field);
    }
    if spec.section.is_transcript_foreground() {
        return foreground_field_value(foreground_settings(settings, spec.section), spec.field);
    }

    let chrome = &settings.chrome;
    match spec.section {
        AppearanceSection::Chrome => match spec.field {
            AppearanceField::ToolbarBackground => chrome.toolbar_background.clone(),
            AppearanceField::ConversationThreadStripBackground => {
                chrome.conversation_thread_strip_background.clone()
            }
            AppearanceField::Separator => chrome.separator.clone(),
            _ => String::new(),
        },
        AppearanceSection::PrimaryButton => button_field_value(&chrome.primary_button, spec.field),
        AppearanceSection::SecondaryButton => {
            button_field_value(&chrome.secondary_button, spec.field)
        }
        AppearanceSection::Input => input_field_value(&chrome.input, spec.field),
        AppearanceSection::TranscriptShell => {
            transcript_shell_field_value(&chrome.transcript_shell, spec.field)
        }
        AppearanceSection::StatusLine => status_line_field_value(&chrome.status_line, spec.field),
        AppearanceSection::Surfaces => surface_field_value(&chrome.surfaces, spec.field),
        _ => String::new(),
    }
}

pub(super) fn role_settings_mut(
    settings: &mut AppearanceSettings,
    section: AppearanceSection,
) -> &mut AppearanceRoleSettings {
    match section {
        AppearanceSection::GeneralUi => &mut settings.general_ui,
        AppearanceSection::ConversationText => &mut settings.conversation_text,
        AppearanceSection::MarkdownHeader => &mut settings.markdown_header,
        AppearanceSection::Code => &mut settings.code,
        AppearanceSection::Emphasis => &mut settings.emphasis,
        AppearanceSection::StrongEmphasis => &mut settings.strong_emphasis,
        _ => &mut settings.general_ui,
    }
}

pub(super) fn foreground_settings_mut(
    settings: &mut AppearanceSettings,
    section: AppearanceSection,
) -> &mut AppearanceForegroundSettings {
    match section {
        AppearanceSection::TranscriptReasoning => &mut settings.transcript_reasoning,
        AppearanceSection::TranscriptCommentary => &mut settings.transcript_commentary,
        _ => &mut settings.transcript_reasoning,
    }
}

fn role_settings(
    settings: &AppearanceSettings,
    section: AppearanceSection,
) -> &AppearanceRoleSettings {
    match section {
        AppearanceSection::GeneralUi => &settings.general_ui,
        AppearanceSection::ConversationText => &settings.conversation_text,
        AppearanceSection::MarkdownHeader => &settings.markdown_header,
        AppearanceSection::Code => &settings.code,
        AppearanceSection::Emphasis => &settings.emphasis,
        AppearanceSection::StrongEmphasis => &settings.strong_emphasis,
        _ => &settings.general_ui,
    }
}

fn foreground_settings(
    settings: &AppearanceSettings,
    section: AppearanceSection,
) -> &AppearanceForegroundSettings {
    match section {
        AppearanceSection::TranscriptReasoning => &settings.transcript_reasoning,
        AppearanceSection::TranscriptCommentary => &settings.transcript_commentary,
        _ => &settings.transcript_reasoning,
    }
}

fn typography_field_value(settings: &AppearanceRoleSettings, field: AppearanceField) -> String {
    match field {
        AppearanceField::FontFamily => settings.font_family.clone(),
        AppearanceField::FontSize => format!("{:.1}", settings.font_size),
        AppearanceField::FontWeight => settings.font_weight.to_string(),
        AppearanceField::Foreground => settings.foreground.clone(),
        AppearanceField::Background => settings.background.clone(),
        _ => String::new(),
    }
}

fn foreground_field_value(
    settings: &AppearanceForegroundSettings,
    field: AppearanceField,
) -> String {
    match field {
        AppearanceField::Foreground => settings.foreground.clone(),
        _ => String::new(),
    }
}

fn button_field_value(
    settings: &crate::AppearanceButtonSettings,
    field: AppearanceField,
) -> String {
    match field {
        AppearanceField::NormalBackground => settings.normal.background.clone(),
        AppearanceField::NormalBorder => settings.normal.border.clone(),
        AppearanceField::NormalForeground => settings.normal.foreground.clone(),
        AppearanceField::HoverBackground => settings.hover.background.clone(),
        AppearanceField::HoverBorder => settings.hover.border.clone(),
        AppearanceField::HoverForeground => settings.hover.foreground.clone(),
        AppearanceField::ActiveBackground => settings.active.background.clone(),
        AppearanceField::ActiveBorder => settings.active.border.clone(),
        AppearanceField::ActiveForeground => settings.active.foreground.clone(),
        AppearanceField::DisabledBackground => settings.disabled.background.clone(),
        AppearanceField::DisabledBorder => settings.disabled.border.clone(),
        AppearanceField::DisabledForeground => settings.disabled.foreground.clone(),
        _ => String::new(),
    }
}

fn input_field_value(settings: &crate::AppearanceInputSettings, field: AppearanceField) -> String {
    match field {
        AppearanceField::PanelBackground => settings.panel_background.clone(),
        AppearanceField::InputBackground => settings.input_background.clone(),
        AppearanceField::InputBorder => settings.input_border.clone(),
        AppearanceField::InputForeground => settings.input_foreground.clone(),
        _ => String::new(),
    }
}

fn transcript_shell_field_value(
    settings: &crate::AppearanceTranscriptShellSettings,
    field: AppearanceField,
) -> String {
    match field {
        AppearanceField::Background => settings.background.clone(),
        AppearanceField::Foreground => settings.foreground.clone(),
        _ => String::new(),
    }
}

fn status_line_field_value(
    settings: &crate::AppearanceStatusLineSettings,
    field: AppearanceField,
) -> String {
    match field {
        AppearanceField::Background => settings.background.clone(),
        AppearanceField::TitleForeground => settings.title_foreground.clone(),
        AppearanceField::ValueForeground => settings.value_foreground.clone(),
        _ => String::new(),
    }
}

fn surface_field_value(
    settings: &crate::AppearanceSurfaceSettings,
    field: AppearanceField,
) -> String {
    match field {
        AppearanceField::PanelBackground => settings.panel_background.clone(),
        AppearanceField::RowBackground => settings.row_background.clone(),
        AppearanceField::PopupBackground => settings.popup_background.clone(),
        AppearanceField::Border => settings.border.clone(),
        AppearanceField::MutedForeground => settings.muted_foreground.clone(),
        _ => String::new(),
    }
}
