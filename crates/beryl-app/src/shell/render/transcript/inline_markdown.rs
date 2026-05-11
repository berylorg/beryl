#![allow(dead_code)]

use std::ops::Range;

use gpui::{
    AnyElement, CursorStyle, Font, FontStyle, FontWeight, SharedString, StyledText, TextRun,
    UnderlineStyle, div, hsla, prelude::*, px, rgb,
};

use crate::shell::execution_detail::{TranscriptImageMarker, TranscriptImagePreviewState};
use crate::shell::rgba_from_role_color;
use crate::shell::transcript_markdown::{
    Inline, InlineRenderFragment, InlineRenderLine, InlineRenderRole, inline_render_lines,
};
use crate::shell::transcript_selection::TranscriptLineCopyText;
use crate::{AppearanceRoleSettings, AppearanceSettings};

use super::markdown_copy::inline_line_copy_text;
pub(super) use super::selection_context::{
    TranscriptInlineSelectionContext, TranscriptSelectableImageMarker, TranscriptSelectableTextLine,
};

pub(super) fn render_inline_markdown(
    inlines: &[Inline],
    appearance: &AppearanceSettings,
) -> AnyElement {
    render_inline_lines(&inline_render_lines(inlines), appearance)
}

pub(super) fn render_inline_lines(
    lines: &[InlineRenderLine],
    appearance: &AppearanceSettings,
) -> AnyElement {
    render_inline_lines_with_style(lines, appearance, InlineMarkdownStyle::default())
}

pub(super) fn render_inline_lines_with_style(
    lines: &[InlineRenderLine],
    appearance: &AppearanceSettings,
    style: InlineMarkdownStyle,
) -> AnyElement {
    render_inline_lines_with_style_and_selection(lines, appearance, style, None)
}

pub(super) fn render_inline_lines_with_style_and_selection(
    lines: &[InlineRenderLine],
    appearance: &AppearanceSettings,
    style: InlineMarkdownStyle,
    selection_context: Option<TranscriptInlineSelectionContext>,
) -> AnyElement {
    render_inline_lines_with_base(
        lines,
        appearance,
        InlineBlockRole::Conversation,
        style,
        selection_context,
        &[],
    )
}

pub(super) fn render_inline_lines_with_style_markers_and_selection(
    lines: &[InlineRenderLine],
    appearance: &AppearanceSettings,
    style: InlineMarkdownStyle,
    selection_context: Option<TranscriptInlineSelectionContext>,
    markers: &[TranscriptInlineImageMarker],
) -> AnyElement {
    render_inline_lines_with_base(
        lines,
        appearance,
        InlineBlockRole::Conversation,
        style,
        selection_context,
        markers,
    )
}

pub(super) fn render_heading_lines(
    lines: &[InlineRenderLine],
    appearance: &AppearanceSettings,
    level: u8,
) -> AnyElement {
    render_heading_lines_with_style(lines, appearance, level, InlineMarkdownStyle::default())
}

pub(super) fn render_heading_lines_with_style(
    lines: &[InlineRenderLine],
    appearance: &AppearanceSettings,
    level: u8,
    style: InlineMarkdownStyle,
) -> AnyElement {
    render_heading_lines_with_style_and_selection(lines, appearance, level, style, None)
}

pub(super) fn render_heading_lines_with_style_and_selection(
    lines: &[InlineRenderLine],
    appearance: &AppearanceSettings,
    level: u8,
    style: InlineMarkdownStyle,
    selection_context: Option<TranscriptInlineSelectionContext>,
) -> AnyElement {
    render_inline_lines_with_base(
        lines,
        appearance,
        InlineBlockRole::Heading { level },
        style,
        selection_context,
        &[],
    )
}

pub(super) fn render_heading_lines_with_style_markers_and_selection(
    lines: &[InlineRenderLine],
    appearance: &AppearanceSettings,
    level: u8,
    style: InlineMarkdownStyle,
    selection_context: Option<TranscriptInlineSelectionContext>,
    markers: &[TranscriptInlineImageMarker],
) -> AnyElement {
    render_inline_lines_with_base(
        lines,
        appearance,
        InlineBlockRole::Heading { level },
        style,
        selection_context,
        markers,
    )
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct InlineMarkdownStyle {
    conversation_foreground: Option<gpui::Rgba>,
    heading_foreground: Option<gpui::Rgba>,
    emphasis_foreground: Option<gpui::Rgba>,
    strong_emphasis_foreground: Option<gpui::Rgba>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TranscriptInlineImageMarker {
    occurrence_id: String,
    label: String,
    display_text: String,
    source_range: Range<usize>,
    copy_text: String,
    asset_id: Option<String>,
    preview_state: TranscriptImagePreviewState,
}

impl TranscriptInlineImageMarker {
    pub(super) fn from_transcript_marker(marker: &TranscriptImageMarker) -> Self {
        Self {
            occurrence_id: marker.occurrence_id().to_string(),
            label: marker.label().to_string(),
            display_text: marker_display_text(marker.label()),
            source_range: marker.display_range(),
            copy_text: marker.copy_text().to_string(),
            asset_id: marker.source().asset_id().map(str::to_string),
            preview_state: marker.source().preview_state(),
        }
    }

    pub(super) fn source_range(&self) -> Range<usize> {
        self.source_range.clone()
    }
}

impl InlineMarkdownStyle {
    pub(super) fn conversation_foreground(foreground: gpui::Rgba) -> Self {
        Self {
            conversation_foreground: Some(foreground),
            ..Self::default()
        }
    }

    fn foreground_for_role(self, role: InlinePresentationRole) -> Option<gpui::Rgba> {
        match role {
            InlinePresentationRole::Conversation => self.conversation_foreground,
            InlinePresentationRole::Heading => self.heading_foreground,
            InlinePresentationRole::Emphasis => self.emphasis_foreground,
            InlinePresentationRole::StrongEmphasis => self.strong_emphasis_foreground,
            InlinePresentationRole::Code => None,
        }
    }
}

fn render_inline_lines_with_base(
    lines: &[InlineRenderLine],
    appearance: &AppearanceSettings,
    block_role: InlineBlockRole,
    style: InlineMarkdownStyle,
    selection_context: Option<TranscriptInlineSelectionContext>,
    markers: &[TranscriptInlineImageMarker],
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_0()
        .children(lines.iter().map(|line| {
            render_inline_line(
                line,
                appearance,
                block_role,
                style,
                selection_context.clone(),
                markers,
            )
        }))
        .into_any_element()
}

#[derive(Clone, Copy)]
enum InlineBlockRole {
    Conversation,
    Heading { level: u8 },
}

fn render_inline_line(
    line: &InlineRenderLine,
    appearance: &AppearanceSettings,
    block_role: InlineBlockRole,
    style: InlineMarkdownStyle,
    selection_context: Option<TranscriptInlineSelectionContext>,
    markers: &[TranscriptInlineImageMarker],
) -> AnyElement {
    let base_role = block_role_settings(appearance, block_role);
    let base_presentation_role = block_presentation_role(block_role);
    let line_markers = line_image_markers(line, markers);
    let display_text = inline_line_display_text(line, line_markers.as_slice());
    let display_text_len = display_text.len();
    let (layout_text, runs) =
        styled_text_parts(line, line_markers.as_slice(), appearance, block_role, style);
    let styled_text = StyledText::new(layout_text).with_runs(runs);
    let text_layout = styled_text.layout().clone();
    let selectable_line = selection_context.as_ref().map(|context| {
        context
            .selectable_line(
                display_text.clone(),
                display_text_len,
                inline_line_copy_text_with_markers(line, line_markers.as_slice()),
            )
            .with_image_markers(line_markers.clone())
    });

    let line = div()
        .w_full()
        .min_w(px(0.0))
        .whitespace_normal()
        .text_size(px(block_font_size(base_role.font_size, block_role)))
        .font_family(base_role.font_family.clone())
        .font_weight(FontWeight(base_role.font_weight as f32))
        .text_color(role_foreground(base_presentation_role, base_role, style))
        .child(styled_text);

    line.when_some(selectable_line, |line, selectable_line| {
        line.cursor(CursorStyle::IBeam)
            .on_children_prepainted(move |bounds, _, cx| {
                let Some(bounds) = bounds.first().copied() else {
                    return;
                };
                selectable_line.entity.update(cx, |view, _| {
                    view.register_selectable_text_line(
                        selectable_line.clone(),
                        bounds,
                        text_layout.clone(),
                    );
                });
            })
    })
    .into_any_element()
}

fn inline_line_display_text(
    line: &InlineRenderLine,
    image_markers: &[TranscriptSelectableImageMarker],
) -> String {
    let mut display_text = String::new();
    let mut display_cursor = 0usize;
    for fragment in &line.fragments {
        let display_range = display_cursor..display_cursor + fragment.text.len();
        push_fragment_display_text(&mut display_text, fragment, display_range, image_markers);
        display_cursor += fragment.text.len();
    }
    display_text
}

fn styled_text_parts(
    line: &InlineRenderLine,
    image_markers: &[TranscriptSelectableImageMarker],
    appearance: &AppearanceSettings,
    block_role: InlineBlockRole,
    style: InlineMarkdownStyle,
) -> (String, Vec<TextRun>) {
    if line.fragments.is_empty() {
        return (
            " ".to_string(),
            vec![text_run(
                " ".len(),
                block_presentation_role(block_role),
                block_role_settings(appearance, block_role),
                false,
                false,
                style,
            )],
        );
    }

    let mut text = String::new();
    let mut runs = Vec::with_capacity(line.fragments.len());
    let mut display_cursor = 0usize;

    for fragment in &line.fragments {
        let display_range = display_cursor..display_cursor + fragment.text.len();
        push_fragment_display_text(&mut text, fragment, display_range, image_markers);
        runs.extend(fragment_text_runs(
            fragment,
            display_cursor,
            image_markers,
            appearance,
            block_role,
            style,
        ));
        display_cursor += fragment.text.len();
    }

    (text, runs)
}

fn fragment_text_runs(
    fragment: &InlineRenderFragment,
    display_start: usize,
    image_markers: &[TranscriptSelectableImageMarker],
    appearance: &AppearanceSettings,
    block_role: InlineBlockRole,
    style: InlineMarkdownStyle,
) -> Vec<TextRun> {
    let display_range = display_start..display_start + fragment.text.len();
    let mut boundaries = vec![0usize, fragment.text.len()];
    for marker in image_markers {
        push_local_overlap_boundaries(&mut boundaries, &display_range, &marker.display_range);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    boundaries
        .windows(2)
        .filter_map(|boundary| {
            let start = boundary[0];
            let end = boundary[1];
            (start < end).then(|| {
                let global = display_start + start..display_start + end;
                let atom = image_markers
                    .iter()
                    .any(|marker| ranges_intersect(&global, &marker.display_range));
                let mut fragment = fragment.clone();
                fragment.text = image_markers
                    .iter()
                    .find(|marker| marker.display_range == global)
                    .map(|marker| marker.display_text.clone())
                    .unwrap_or_else(|| fragment.text[start..end].to_string());
                fragment.style.atom = atom;
                fragment_text_run(&fragment, appearance, block_role, style)
            })
        })
        .collect()
}

fn fragment_text_run(
    fragment: &InlineRenderFragment,
    appearance: &AppearanceSettings,
    block_role: InlineBlockRole,
    style: InlineMarkdownStyle,
) -> TextRun {
    let role = fragment_presentation_role(fragment.style.role, block_role);
    let settings = role_settings(appearance, role);
    text_run(
        fragment.text.len(),
        role,
        settings,
        fragment.style.link,
        fragment.style.atom,
        style,
    )
}

fn line_image_markers(
    line: &InlineRenderLine,
    markers: &[TranscriptInlineImageMarker],
) -> Vec<TranscriptSelectableImageMarker> {
    let mut display_cursor = 0usize;
    let mut line_markers = Vec::new();

    for fragment in &line.fragments {
        let fragment_display_start = display_cursor;
        display_cursor = display_cursor.saturating_add(fragment.text.len());
        let Some(source_span) = fragment.display_source_span else {
            continue;
        };

        for marker in markers {
            if marker.source_range.start < source_span.start()
                || marker.source_range.end > source_span.end()
            {
                continue;
            }
            let local_start = marker.source_range.start - source_span.start();
            let local_end = marker.source_range.end - source_span.start();
            if local_start >= local_end
                || local_end > fragment.text.len()
                || !fragment.text.is_char_boundary(local_start)
                || !fragment.text.is_char_boundary(local_end)
            {
                continue;
            }
            line_markers.push(TranscriptSelectableImageMarker {
                occurrence_id: marker.occurrence_id.clone(),
                label: marker.label.clone(),
                display_text: marker.display_text.clone(),
                display_range: fragment_display_start + local_start
                    ..fragment_display_start + local_end,
                copy_text: marker.copy_text.clone(),
                asset_id: marker.asset_id.clone(),
                preview_state: marker.preview_state,
            });
        }
    }

    line_markers.sort_by_key(|marker| marker.display_range.start);
    line_markers.dedup_by(|left, right| left.occurrence_id == right.occurrence_id);
    line_markers
}

fn push_fragment_display_text(
    target: &mut String,
    fragment: &InlineRenderFragment,
    display_range: Range<usize>,
    image_markers: &[TranscriptSelectableImageMarker],
) {
    let mut cursor = display_range.start;
    for marker in image_markers {
        if !range_contains(&display_range, &marker.display_range) {
            continue;
        }
        if cursor < marker.display_range.start {
            let local_start = cursor - display_range.start;
            let local_end = marker.display_range.start - display_range.start;
            target.push_str(&fragment.text[local_start..local_end]);
        }
        target.push_str(&marker.display_text);
        cursor = marker.display_range.end;
    }
    if cursor < display_range.end {
        let local_start = cursor - display_range.start;
        let local_end = display_range.end - display_range.start;
        target.push_str(&fragment.text[local_start..local_end]);
    }
}

fn marker_display_text(label: &str) -> String {
    format!("[{label}]")
}

fn inline_line_copy_text_with_markers(
    line: &InlineRenderLine,
    image_markers: &[TranscriptSelectableImageMarker],
) -> TranscriptLineCopyText {
    if image_markers.is_empty() {
        return inline_line_copy_text(line);
    }

    let mut copy_text = TranscriptLineCopyText::default();
    let mut display_cursor = 0usize;
    for fragment in &line.fragments {
        let display_range = display_cursor..display_cursor + fragment.text.len();
        display_cursor = display_range.end;
        let replacements = image_markers
            .iter()
            .filter_map(|marker| {
                if !range_contains(&display_range, &marker.display_range) {
                    return None;
                }
                Some((
                    marker.display_range.start - display_range.start
                        ..marker.display_range.end - display_range.start,
                    marker.copy_text.clone(),
                ))
            })
            .collect::<Vec<_>>();
        if let Some(copy_replacement) = &fragment.copy_replacement
            && replacements.is_empty()
        {
            copy_text.push_atomic_run(fragment.text.clone(), copy_replacement.clone());
        } else {
            copy_text.push_wrapped_run_with_atomic_replacements(
                fragment.text.clone(),
                fragment.copy_prefix.clone(),
                fragment.copy_suffix.clone(),
                replacements,
            );
        }
    }
    copy_text
}

fn push_local_overlap_boundaries(
    boundaries: &mut Vec<usize>,
    local_parent: &Range<usize>,
    child: &Range<usize>,
) {
    let start = child.start.max(local_parent.start);
    let end = child.end.min(local_parent.end);
    if start < end {
        boundaries.push(start - local_parent.start);
        boundaries.push(end - local_parent.start);
    }
}

fn range_contains(parent: &Range<usize>, child: &Range<usize>) -> bool {
    child.start >= parent.start && child.end <= parent.end
}

fn ranges_intersect(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn text_run(
    len: usize,
    role: InlinePresentationRole,
    settings: &AppearanceRoleSettings,
    link: bool,
    atom: bool,
    style: InlineMarkdownStyle,
) -> TextRun {
    TextRun {
        len,
        font: Font {
            family: SharedString::from(settings.font_family.clone()),
            features: Default::default(),
            fallbacks: None,
            weight: FontWeight(settings.font_weight as f32),
            style: FontStyle::Normal,
        },
        color: if atom {
            hsla(0.58, 0.78, 0.72, 1.0)
        } else {
            role_foreground(role, settings, style).into()
        },
        background_color: if atom {
            Some(hsla(0.58, 0.55, 0.28, 0.85))
        } else {
            (role == InlinePresentationRole::Code).then(|| role_background(role, settings).into())
        },
        underline: link.then_some(UnderlineStyle {
            thickness: px(1.0),
            ..Default::default()
        }),
        strikethrough: None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InlinePresentationRole {
    Conversation,
    Heading,
    Emphasis,
    StrongEmphasis,
    Code,
}

fn block_presentation_role(block_role: InlineBlockRole) -> InlinePresentationRole {
    match block_role {
        InlineBlockRole::Conversation => InlinePresentationRole::Conversation,
        InlineBlockRole::Heading { .. } => InlinePresentationRole::Heading,
    }
}

fn fragment_presentation_role(
    role: InlineRenderRole,
    block_role: InlineBlockRole,
) -> InlinePresentationRole {
    match role {
        InlineRenderRole::Conversation => block_presentation_role(block_role),
        InlineRenderRole::Emphasis => InlinePresentationRole::Emphasis,
        InlineRenderRole::StrongEmphasis => InlinePresentationRole::StrongEmphasis,
        InlineRenderRole::Code => InlinePresentationRole::Code,
    }
}

fn block_role_settings(
    appearance: &AppearanceSettings,
    block_role: InlineBlockRole,
) -> &AppearanceRoleSettings {
    match block_role {
        InlineBlockRole::Conversation => &appearance.conversation_text,
        InlineBlockRole::Heading { .. } => &appearance.markdown_header,
    }
}

fn block_font_size(base_size: f32, block_role: InlineBlockRole) -> f32 {
    match block_role {
        InlineBlockRole::Conversation => base_size,
        InlineBlockRole::Heading { level } => match level {
            1 => base_size + 4.0,
            2 => base_size + 2.0,
            3 => base_size + 1.0,
            _ => base_size,
        },
    }
}

fn role_settings<'a>(
    appearance: &'a AppearanceSettings,
    role: InlinePresentationRole,
) -> &'a AppearanceRoleSettings {
    match role {
        InlinePresentationRole::Conversation => &appearance.conversation_text,
        InlinePresentationRole::Heading => &appearance.markdown_header,
        InlinePresentationRole::Emphasis => &appearance.emphasis,
        InlinePresentationRole::StrongEmphasis => &appearance.strong_emphasis,
        InlinePresentationRole::Code => &appearance.code,
    }
}

fn role_foreground(
    role: InlinePresentationRole,
    settings: &AppearanceRoleSettings,
    style: InlineMarkdownStyle,
) -> gpui::Rgba {
    if let Some(foreground) = style.foreground_for_role(role) {
        return foreground;
    }

    rgba_from_role_color(settings.parsed_foreground(), default_role_foreground(role))
}

fn role_background(role: InlinePresentationRole, settings: &AppearanceRoleSettings) -> gpui::Rgba {
    rgba_from_role_color(settings.parsed_background(), default_role_background(role))
}

fn default_role_foreground(role: InlinePresentationRole) -> gpui::Rgba {
    match role {
        InlinePresentationRole::Conversation => rgb(0xe2e8f0),
        InlinePresentationRole::Heading => rgb(0x93c5fd),
        InlinePresentationRole::Emphasis => rgb(0xbfdbfe),
        InlinePresentationRole::StrongEmphasis => rgb(0xf8fafc),
        InlinePresentationRole::Code => rgb(0xe2e8f0),
    }
}

fn default_role_background(role: InlinePresentationRole) -> gpui::Rgba {
    match role {
        InlinePresentationRole::Conversation
        | InlinePresentationRole::Heading
        | InlinePresentationRole::Emphasis
        | InlinePresentationRole::StrongEmphasis => rgb(0x091220),
        InlinePresentationRole::Code => rgb(0x0f172a),
    }
}
