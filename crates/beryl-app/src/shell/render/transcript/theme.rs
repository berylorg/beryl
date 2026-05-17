use gpui::{FontWeight, Rgba, rgb};

use crate::shell::syntax_highlighting::SyntaxTokenRole;
use crate::shell::transcript_anchor::{TranscriptAnchorRole, TranscriptAnchorTheme};
use crate::{
    ActiveThemeProjection, BerylThemeProperty, BerylThemeRole, ResolvedStyle, StylePropertyId,
    StylePropertyValue, ThemeResolutionContext,
};

use super::super::code_panel::{
    CodePanelHeaderButtonState, CodePanelHeaderButtonTheme, CodePanelSyntaxTheme,
};

#[derive(Clone, Debug)]
pub(crate) struct TranscriptTheme {
    revision: u64,
    pub(crate) assistant_final: TranscriptRoleStyle,
    pub(crate) assistant_commentary: TranscriptRoleStyle,
    pub(crate) assistant_reasoning: TranscriptRoleStyle,
    pub(crate) user_input: TranscriptRoleStyle,
    pub(crate) paragraph: TranscriptRoleStyle,
    pub(crate) heading: TranscriptRoleStyle,
    pub(crate) emphasis: TranscriptRoleStyle,
    pub(crate) strong_emphasis: TranscriptRoleStyle,
    pub(crate) link: TranscriptRoleStyle,
    pub(crate) block_quote: TranscriptRoleStyle,
    pub(crate) list_marker: TranscriptRoleStyle,
    pub(crate) thematic_break: TranscriptRoleStyle,
    pub(crate) unsupported_fallback: TranscriptRoleStyle,
    pub(crate) activity_caret: TranscriptRoleStyle,
    pub(crate) selection: TranscriptRoleStyle,
    pub(crate) pending: TranscriptRoleStyle,
    pub(crate) unavailable: TranscriptRoleStyle,
    pub(crate) quote_popup: TranscriptRoleStyle,
    pub(crate) code_panel_container: TranscriptRoleStyle,
    pub(crate) code_panel_header: TranscriptRoleStyle,
    pub(crate) code_panel_body: TranscriptRoleStyle,
    pub(crate) code_panel_border: TranscriptRoleStyle,
    pub(crate) code_panel_resize_handle: TranscriptRoleStyle,
    pub(crate) code_panel_button: CodePanelHeaderButtonTheme,
    pub(crate) media_placeholder: TranscriptRoleStyle,
    pub(crate) media_loading: TranscriptRoleStyle,
    pub(crate) media_unavailable: TranscriptRoleStyle,
    pub(crate) media_border: TranscriptRoleStyle,
    pub(crate) media_caption: TranscriptRoleStyle,
    pub(crate) image_marker: TranscriptRoleStyle,
    pub(crate) syntax: CodePanelSyntaxTheme,
    inline_code: TranscriptInlineCodeStyles,
}

#[derive(Clone, Debug)]
pub(crate) struct TranscriptRoleStyle {
    pub(crate) background: Rgba,
    pub(crate) border: Rgba,
    pub(crate) foreground: Rgba,
    pub(crate) text_background: Rgba,
    pub(crate) font_family: String,
    pub(crate) font_size: f32,
    pub(crate) font_weight: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptTextRole {
    AssistantFinal,
    AssistantCommentary,
    AssistantReasoning,
    UserInput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptInlineCodeHost {
    AssistantFinal,
    AssistantCommentary,
    AssistantReasoning,
    UserInput,
    UnsupportedFallback,
    Heading,
    Emphasis,
    StrongEmphasis,
    Link,
}

#[derive(Clone, Debug)]
struct TranscriptInlineCodeStyles {
    assistant_final: TranscriptRoleStyle,
    assistant_commentary: TranscriptRoleStyle,
    assistant_reasoning: TranscriptRoleStyle,
    user_input: TranscriptRoleStyle,
    unsupported_fallback: TranscriptRoleStyle,
    heading: TranscriptRoleStyle,
    emphasis: TranscriptRoleStyle,
    strong_emphasis: TranscriptRoleStyle,
    link: TranscriptRoleStyle,
}

impl TranscriptTheme {
    pub(crate) fn from_active_theme(theme: &ActiveThemeProjection) -> Self {
        let assistant_final = resolved_role_style(theme, BerylThemeRole::TranscriptAssistantFinal);
        let assistant_commentary =
            resolved_role_style(theme, BerylThemeRole::TranscriptAssistantCommentary);
        let assistant_reasoning =
            resolved_role_style(theme, BerylThemeRole::TranscriptAssistantReasoning);
        let user_input = resolved_role_style(theme, BerylThemeRole::TranscriptUserInput);
        let paragraph = resolved_role_style(theme, BerylThemeRole::MarkdownParagraph);
        let heading = resolved_role_style(theme, BerylThemeRole::MarkdownHeading);
        let emphasis = resolved_role_style(theme, BerylThemeRole::MarkdownEmphasis);
        let strong_emphasis = resolved_role_style(theme, BerylThemeRole::MarkdownStrongEmphasis);
        let link = resolved_role_style(theme, BerylThemeRole::MarkdownLink);
        let unsupported_fallback =
            resolved_role_style(theme, BerylThemeRole::MarkdownUnsupportedFallback);
        let code_panel_body = resolved_role_style(theme, BerylThemeRole::CodePanelBody);
        let inline_code = TranscriptInlineCodeStyles {
            assistant_final: inline_code_style(theme, &paragraph.resolved),
            assistant_commentary: inline_code_style(theme, &assistant_commentary.resolved),
            assistant_reasoning: inline_code_style(theme, &assistant_reasoning.resolved),
            user_input: inline_code_style(theme, &user_input.resolved),
            unsupported_fallback: inline_code_style(theme, &unsupported_fallback.resolved),
            heading: inline_code_style(theme, &heading.resolved),
            emphasis: inline_code_style(theme, &emphasis.resolved),
            strong_emphasis: inline_code_style(theme, &strong_emphasis.resolved),
            link: inline_code_style(theme, &link.resolved),
        };

        Self {
            revision: theme.style_revision(),
            syntax: syntax_theme(theme, &code_panel_body.style),
            assistant_final: assistant_final.style,
            assistant_commentary: assistant_commentary.style,
            assistant_reasoning: assistant_reasoning.style,
            user_input: user_input.style,
            paragraph: paragraph.style,
            heading: heading.style,
            emphasis: emphasis.style,
            strong_emphasis: strong_emphasis.style,
            link: link.style,
            block_quote: role_style(theme, BerylThemeRole::MarkdownBlockQuote),
            list_marker: role_style(theme, BerylThemeRole::MarkdownListMarker),
            thematic_break: role_style(theme, BerylThemeRole::MarkdownThematicBreak),
            unsupported_fallback: unsupported_fallback.style,
            activity_caret: role_style(theme, BerylThemeRole::TranscriptActivityCaret),
            selection: role_style(theme, BerylThemeRole::TranscriptSelection),
            pending: role_style(theme, BerylThemeRole::TranscriptPending),
            unavailable: role_style(theme, BerylThemeRole::TranscriptUnavailable),
            quote_popup: role_style(theme, BerylThemeRole::TranscriptQuotePopup),
            code_panel_container: role_style(theme, BerylThemeRole::CodePanelContainer),
            code_panel_header: role_style(theme, BerylThemeRole::CodePanelHeader),
            code_panel_body: code_panel_body.style,
            code_panel_border: role_style(theme, BerylThemeRole::CodePanelBorder),
            code_panel_resize_handle: role_style(theme, BerylThemeRole::CodePanelResizeHandle),
            code_panel_button: code_panel_button_theme(theme),
            media_placeholder: role_style(theme, BerylThemeRole::MediaPlaceholder),
            media_loading: role_style(theme, BerylThemeRole::MediaPlaceholderLoading),
            media_unavailable: role_style(theme, BerylThemeRole::MediaPlaceholderUnavailable),
            media_border: role_style(theme, BerylThemeRole::MediaBorder),
            media_caption: role_style(theme, BerylThemeRole::MediaCaption),
            image_marker: role_style(theme, BerylThemeRole::TranscriptImageMarker),
            inline_code,
        }
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn text_role(&self, role: TranscriptTextRole) -> &TranscriptRoleStyle {
        match role {
            TranscriptTextRole::AssistantFinal => &self.assistant_final,
            TranscriptTextRole::AssistantCommentary => &self.assistant_commentary,
            TranscriptTextRole::AssistantReasoning => &self.assistant_reasoning,
            TranscriptTextRole::UserInput => &self.user_input,
        }
    }

    pub(crate) fn inline_code_style(&self, host: TranscriptInlineCodeHost) -> &TranscriptRoleStyle {
        match host {
            TranscriptInlineCodeHost::AssistantFinal => &self.inline_code.assistant_final,
            TranscriptInlineCodeHost::AssistantCommentary => &self.inline_code.assistant_commentary,
            TranscriptInlineCodeHost::AssistantReasoning => &self.inline_code.assistant_reasoning,
            TranscriptInlineCodeHost::UserInput => &self.inline_code.user_input,
            TranscriptInlineCodeHost::UnsupportedFallback => &self.inline_code.unsupported_fallback,
            TranscriptInlineCodeHost::Heading => &self.inline_code.heading,
            TranscriptInlineCodeHost::Emphasis => &self.inline_code.emphasis,
            TranscriptInlineCodeHost::StrongEmphasis => &self.inline_code.strong_emphasis,
            TranscriptInlineCodeHost::Link => &self.inline_code.link,
        }
    }

    pub(crate) fn anchor_theme(&self) -> TranscriptAnchorTheme {
        TranscriptAnchorTheme {
            conversation: self.user_input.anchor_role(),
            heading: self.heading.anchor_role(),
            emphasis: self.emphasis.anchor_role(),
            strong_emphasis: self.strong_emphasis.anchor_role(),
            code: self
                .inline_code_style(TranscriptInlineCodeHost::UserInput)
                .anchor_role(),
            code_panel: self.code_panel_body.anchor_role(),
            code_panel_header: self.code_panel_header.anchor_role(),
        }
    }
}

impl TranscriptRoleStyle {
    pub(crate) fn font_weight(&self) -> FontWeight {
        FontWeight(self.font_weight as f32)
    }

    fn anchor_role(&self) -> TranscriptAnchorRole {
        TranscriptAnchorRole {
            font_family: self.font_family.clone(),
            font_size: self.font_size,
            font_weight: self.font_weight,
        }
    }
}

fn role_style(theme: &ActiveThemeProjection, role: BerylThemeRole) -> TranscriptRoleStyle {
    resolved_role_style(theme, role).style
}

struct ResolvedTranscriptRoleStyle {
    style: TranscriptRoleStyle,
    resolved: ResolvedStyle,
}

fn resolved_role_style(
    theme: &ActiveThemeProjection,
    role: BerylThemeRole,
) -> ResolvedTranscriptRoleStyle {
    let resolved = theme
        .resolve_style(role.id(), &ThemeResolutionContext::new())
        .or_else(|_| theme.default_style(role.id()).cloned())
        .unwrap_or_else(|_| panic!("Beryl theme role {} must resolve", role.id()));
    let style = role_style_from_resolved(role, &resolved);
    ResolvedTranscriptRoleStyle { style, resolved }
}

fn inline_code_style(
    theme: &ActiveThemeProjection,
    ambient_parent: &ResolvedStyle,
) -> TranscriptRoleStyle {
    let context = ThemeResolutionContext::new().with_ambient_parent(ambient_parent.clone());
    let role = BerylThemeRole::MarkdownInlineCode;
    let resolved = theme
        .resolve_style(role.id(), &context)
        .or_else(|_| theme.default_style(role.id()).cloned())
        .unwrap_or_else(|_| panic!("Beryl theme role {} must resolve", role.id()));
    role_style_from_resolved(role, &resolved)
}

fn role_style_from_resolved(role: BerylThemeRole, resolved: &ResolvedStyle) -> TranscriptRoleStyle {
    TranscriptRoleStyle {
        background: style_color(resolved, role, BerylThemeProperty::Background),
        border: style_color(resolved, role, BerylThemeProperty::Border),
        foreground: style_color(resolved, role, BerylThemeProperty::Foreground),
        text_background: style_color(resolved, role, BerylThemeProperty::TextBackground),
        font_family: style_font_family(resolved, role),
        font_size: style_font_size(resolved, role),
        font_weight: style_font_weight(resolved, role),
    }
}

fn style_color(style: &ResolvedStyle, role: BerylThemeRole, property: BerylThemeProperty) -> Rgba {
    match resolved_property(style, role, property) {
        StylePropertyValue::Color(value) => crate::ParsedHexColor::parse(value)
            .map(|color| {
                rgb(((color.red() as u32) << 16)
                    | ((color.green() as u32) << 8)
                    | color.blue() as u32)
            })
            .unwrap_or_else(|| {
                panic!(
                    "Beryl theme role {} property {} must be a valid #RRGGBB color",
                    role.id(),
                    property.id()
                )
            }),
        _ => panic!(
            "Beryl theme role {} property {} must resolve as a color",
            role.id(),
            property.id()
        ),
    }
}

fn style_font_family(style: &ResolvedStyle, role: BerylThemeRole) -> String {
    match resolved_property(style, role, BerylThemeProperty::FontFamily) {
        StylePropertyValue::FontFamily(value) => value.clone(),
        _ => panic!(
            "Beryl theme role {} property {} must resolve as a font family",
            role.id(),
            BerylThemeProperty::FontFamily.id()
        ),
    }
}

fn style_font_size(style: &ResolvedStyle, role: BerylThemeRole) -> f32 {
    match resolved_property(style, role, BerylThemeProperty::FontSize) {
        StylePropertyValue::LogicalPixels(value) => *value,
        _ => panic!(
            "Beryl theme role {} property {} must resolve as logical pixels",
            role.id(),
            BerylThemeProperty::FontSize.id()
        ),
    }
}

fn style_font_weight(style: &ResolvedStyle, role: BerylThemeRole) -> u16 {
    match resolved_property(style, role, BerylThemeProperty::FontWeight) {
        StylePropertyValue::FontWeight(value) => *value,
        _ => panic!(
            "Beryl theme role {} property {} must resolve as a font weight",
            role.id(),
            BerylThemeProperty::FontWeight.id()
        ),
    }
}

fn resolved_property(
    style: &ResolvedStyle,
    role: BerylThemeRole,
    property: BerylThemeProperty,
) -> &StylePropertyValue {
    style
        .property(&StylePropertyId::from(property.id()))
        .unwrap_or_else(|| {
            panic!(
                "Beryl theme role {} missing resolved property {}",
                role.id(),
                property.id()
            )
        })
}

fn code_panel_button_theme(theme: &ActiveThemeProjection) -> CodePanelHeaderButtonTheme {
    CodePanelHeaderButtonTheme {
        normal: button_state(theme, BerylThemeRole::CodePanelButtonNormal),
        hover: button_state(theme, BerylThemeRole::CodePanelButtonHover),
        active: button_state(theme, BerylThemeRole::CodePanelButtonActive),
    }
}

fn button_state(theme: &ActiveThemeProjection, role: BerylThemeRole) -> CodePanelHeaderButtonState {
    let style = role_style(theme, role);
    CodePanelHeaderButtonState {
        background: style.background,
        border: style.border,
        foreground: style.foreground,
    }
}

fn syntax_theme(theme: &ActiveThemeProjection, body: &TranscriptRoleStyle) -> CodePanelSyntaxTheme {
    CodePanelSyntaxTheme::from_role_foregrounds(
        body.foreground,
        body.font_family.clone(),
        body.font_size,
        body.font_weight(),
        |role| role_style(theme, syntax_role(role)).foreground,
    )
}

fn syntax_role(role: SyntaxTokenRole) -> BerylThemeRole {
    match role {
        SyntaxTokenRole::MarkupHeadingMarker => BerylThemeRole::SyntaxMarkupHeadingMarker,
        SyntaxTokenRole::MarkupQuoteMarker => BerylThemeRole::SyntaxMarkupQuoteMarker,
        SyntaxTokenRole::MarkupListMarker => BerylThemeRole::SyntaxMarkupListMarker,
        SyntaxTokenRole::MarkupThematicBreak => BerylThemeRole::SyntaxMarkupThematicBreak,
        SyntaxTokenRole::MarkupFenceDelimiter => BerylThemeRole::SyntaxMarkupFenceDelimiter,
        SyntaxTokenRole::MarkupFenceInfo => BerylThemeRole::SyntaxMarkupFenceInfo,
        SyntaxTokenRole::MarkupCodeBlock => BerylThemeRole::SyntaxMarkupCodeBlock,
        SyntaxTokenRole::MarkupCodeSpanDelimiter => BerylThemeRole::SyntaxMarkupCodeSpanDelimiter,
        SyntaxTokenRole::MarkupCodeSpan => BerylThemeRole::SyntaxMarkupCodeSpan,
        SyntaxTokenRole::MarkupEmphasisDelimiter => BerylThemeRole::SyntaxMarkupEmphasisDelimiter,
        SyntaxTokenRole::MarkupStrongDelimiter => BerylThemeRole::SyntaxMarkupStrongDelimiter,
        SyntaxTokenRole::MarkupLinkText => BerylThemeRole::SyntaxMarkupLinkText,
        SyntaxTokenRole::MarkupLinkDestination => BerylThemeRole::SyntaxMarkupLinkDestination,
        SyntaxTokenRole::MarkupImageMarker => BerylThemeRole::SyntaxMarkupImageMarker,
        SyntaxTokenRole::MarkupPunctuation => BerylThemeRole::SyntaxMarkupPunctuation,
        SyntaxTokenRole::MarkupHtml => BerylThemeRole::SyntaxMarkupHtml,
        SyntaxTokenRole::Escape => BerylThemeRole::SyntaxEscape,
        SyntaxTokenRole::SyntaxStructuralPunctuation => BerylThemeRole::SyntaxStructuralPunctuation,
        SyntaxTokenRole::SyntaxKey => BerylThemeRole::SyntaxKey,
        SyntaxTokenRole::SyntaxString => BerylThemeRole::SyntaxString,
        SyntaxTokenRole::SyntaxNumber => BerylThemeRole::SyntaxNumber,
        SyntaxTokenRole::SyntaxBoolean => BerylThemeRole::SyntaxBoolean,
        SyntaxTokenRole::SyntaxNull => BerylThemeRole::SyntaxNull,
        SyntaxTokenRole::SyntaxDateTime => BerylThemeRole::SyntaxDateTime,
        SyntaxTokenRole::SyntaxComment => BerylThemeRole::SyntaxComment,
        SyntaxTokenRole::SyntaxSectionHeader => BerylThemeRole::SyntaxSectionHeader,
        SyntaxTokenRole::SyntaxAssignment => BerylThemeRole::SyntaxAssignment,
        SyntaxTokenRole::SyntaxEscape => BerylThemeRole::SyntaxTokenEscape,
        SyntaxTokenRole::SyntaxError => BerylThemeRole::SyntaxError,
    }
}
