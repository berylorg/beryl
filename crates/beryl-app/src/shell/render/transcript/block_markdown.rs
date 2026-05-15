use gpui::{AnyElement, App, FontWeight, Pixels, div, prelude::*, px, rgb};

use crate::AppearanceSettings;
use crate::shell::rgba_from_role_color;
use crate::shell::transcript_markdown::{
    BlockRenderCode, BlockRenderList, BlockRenderListItem, BlockRenderListKind, BlockRenderNode,
    BlockRenderPlan, InlineRenderLine, InlineRenderRole, MARKDOWN_LIST_LEADING_MARGIN_M,
    MARKDOWN_LIST_MARKER_BODY_GAP_M, MarkdownSourceSpan, markdown_code_panel_block_path,
    markdown_code_panel_block_quote_path, markdown_code_panel_list_item_path,
    markdown_list_marker_width_m,
};

use super::super::code_panel::{
    CodePanelDisplayProjectionInput, CodePanelSyntaxTheme, CodePanelWrapMode,
};
use super::TranscriptCodeLayout;
use super::block_fallback::{
    empty_line, fallback_inline_lines, fallback_inline_lines_with_source_span,
};
use super::code_panel_controls::TranscriptCodePanelControls;
use super::inline_markdown::{
    InlineMarkdownStyle, TranscriptInlineImageMarker, TranscriptInlineSelectionContext,
    render_heading_lines_with_style, render_heading_lines_with_style_and_selection,
    render_heading_lines_with_style_markers_and_selection, render_inline_lines_with_style,
    render_inline_lines_with_style_and_selection,
    render_inline_lines_with_style_markers_and_selection,
};
use super::text_blocks::labeled_code_block;

pub(super) fn render_markdown_plan_with_style_and_selection(
    plan: &BlockRenderPlan,
    appearance: &AppearanceSettings,
    code_layout: TranscriptCodeLayout,
    conversation_m_advance: Pixels,
    style: InlineMarkdownStyle,
    code_panel_controls: TranscriptCodePanelControls,
    selection_context: TranscriptInlineSelectionContext,
    cx: &mut App,
) -> AnyElement {
    render_markdown_plan(
        plan,
        appearance,
        code_layout,
        conversation_m_advance,
        style,
        Some(code_panel_controls),
        Some(selection_context),
        &[],
        cx,
    )
}

pub(super) fn markdown_prose_block_with_selection(
    label: &str,
    plan: &BlockRenderPlan,
    background: gpui::Rgba,
    appearance: &AppearanceSettings,
    code_layout: TranscriptCodeLayout,
    conversation_m_advance: Pixels,
    style: InlineMarkdownStyle,
    code_panel_controls: TranscriptCodePanelControls,
    selection_context: TranscriptInlineSelectionContext,
    cx: &mut App,
) -> AnyElement {
    markdown_prose_block_inner(
        label,
        plan,
        background,
        appearance,
        code_layout,
        conversation_m_advance,
        style,
        Some(code_panel_controls),
        Some(selection_context),
        &[],
        cx,
    )
}

pub(super) fn markdown_prose_block_with_image_markers_and_selection(
    label: &str,
    plan: &BlockRenderPlan,
    background: gpui::Rgba,
    appearance: &AppearanceSettings,
    code_layout: TranscriptCodeLayout,
    conversation_m_advance: Pixels,
    style: InlineMarkdownStyle,
    code_panel_controls: TranscriptCodePanelControls,
    selection_context: TranscriptInlineSelectionContext,
    image_markers: &[TranscriptInlineImageMarker],
    cx: &mut App,
) -> AnyElement {
    markdown_prose_block_inner(
        label,
        plan,
        background,
        appearance,
        code_layout,
        conversation_m_advance,
        style,
        Some(code_panel_controls),
        Some(selection_context),
        image_markers,
        cx,
    )
}

fn markdown_prose_block_inner(
    label: &str,
    plan: &BlockRenderPlan,
    background: gpui::Rgba,
    appearance: &AppearanceSettings,
    code_layout: TranscriptCodeLayout,
    conversation_m_advance: Pixels,
    style: InlineMarkdownStyle,
    code_panel_controls: Option<TranscriptCodePanelControls>,
    selection_context: Option<TranscriptInlineSelectionContext>,
    image_markers: &[TranscriptInlineImageMarker],
    cx: &mut App,
) -> AnyElement {
    let mut block = div()
        .rounded_md()
        .bg(background)
        .border_1()
        .border_color(rgb(0x1f2937))
        .p_3()
        .flex()
        .flex_col()
        .gap_2();

    if !label.is_empty() {
        block = block.child(
            div()
                .text_xs()
                .text_color(rgb(0x94a3b8))
                .child(label.to_string()),
        );
    }

    block
        .child(render_markdown_plan(
            plan,
            appearance,
            code_layout,
            conversation_m_advance,
            style,
            code_panel_controls,
            selection_context,
            image_markers,
            cx,
        ))
        .into_any_element()
}

fn render_markdown_plan(
    plan: &BlockRenderPlan,
    appearance: &AppearanceSettings,
    code_layout: TranscriptCodeLayout,
    conversation_m_advance: Pixels,
    style: InlineMarkdownStyle,
    code_panel_controls: Option<TranscriptCodePanelControls>,
    selection_context: Option<TranscriptInlineSelectionContext>,
    image_markers: &[TranscriptInlineImageMarker],
    cx: &mut App,
) -> AnyElement {
    render_block_sequence(
        plan.blocks.as_slice(),
        appearance,
        code_layout,
        conversation_m_advance,
        BlockSpacing::Normal,
        style,
        code_panel_controls,
        "",
        selection_context,
        image_markers,
        cx,
    )
}

#[derive(Clone, Copy)]
enum BlockSpacing {
    Normal,
    Tight,
}

impl BlockSpacing {
    fn raw_break_before(self) -> usize {
        match self {
            BlockSpacing::Normal => 2,
            BlockSpacing::Tight => 1,
        }
    }
}

fn render_block_sequence(
    blocks: &[BlockRenderNode],
    appearance: &AppearanceSettings,
    code_layout: TranscriptCodeLayout,
    conversation_m_advance: Pixels,
    spacing: BlockSpacing,
    style: InlineMarkdownStyle,
    code_panel_controls: Option<TranscriptCodePanelControls>,
    structural_parent_path: &str,
    selection_context: Option<TranscriptInlineSelectionContext>,
    image_markers: &[TranscriptInlineImageMarker],
    cx: &mut App,
) -> AnyElement {
    let mut container = div().w_full().min_w(px(0.0)).flex().flex_col();
    container = match spacing {
        BlockSpacing::Normal => container.gap_2(),
        BlockSpacing::Tight => container.gap_1(),
    };

    if blocks.is_empty() {
        return container.child(empty_line()).into_any_element();
    }

    container
        .children(blocks.iter().enumerate().map(|(index, block)| {
            let structural_path = markdown_code_panel_block_path(structural_parent_path, index);
            if index > 0
                && let Some(selection_context) = selection_context.as_ref()
            {
                selection_context.set_next_break_before(spacing.raw_break_before());
            }
            render_block(
                block,
                appearance,
                code_layout,
                conversation_m_advance,
                style,
                code_panel_controls.clone(),
                structural_path,
                selection_context.clone(),
                image_markers,
                cx,
            )
            .into_any_element()
        }))
        .into_any_element()
}

fn render_block(
    block: &BlockRenderNode,
    appearance: &AppearanceSettings,
    code_layout: TranscriptCodeLayout,
    conversation_m_advance: Pixels,
    style: InlineMarkdownStyle,
    code_panel_controls: Option<TranscriptCodePanelControls>,
    structural_path: String,
    selection_context: Option<TranscriptInlineSelectionContext>,
    image_markers: &[TranscriptInlineImageMarker],
    cx: &mut App,
) -> AnyElement {
    match block {
        BlockRenderNode::Paragraph { lines, .. } => {
            render_paragraph(lines, appearance, style, selection_context, image_markers)
        }
        BlockRenderNode::Heading { level, lines, .. } => render_heading(
            *level,
            lines,
            appearance,
            style,
            selection_context,
            image_markers,
        ),
        BlockRenderNode::List(list) => render_list(
            list,
            appearance,
            code_layout,
            conversation_m_advance,
            style,
            code_panel_controls,
            structural_path,
            selection_context,
            image_markers,
            cx,
        ),
        BlockRenderNode::BlockQuote { blocks, .. } => render_block_quote(
            blocks,
            appearance,
            code_layout,
            conversation_m_advance,
            style,
            code_panel_controls,
            structural_path,
            selection_context,
            image_markers,
            cx,
        ),
        BlockRenderNode::Code(code) => render_code_block(
            code,
            appearance,
            code_layout,
            code_panel_controls,
            structural_path.as_str(),
            selection_context,
            image_markers,
            cx,
        ),
        BlockRenderNode::Math {
            fallback,
            source_span,
            ..
        } => render_code_fallback_text(
            fallback,
            *source_span,
            appearance,
            selection_context,
            image_markers,
        ),
        BlockRenderNode::ThematicBreak => render_thematic_break(),
        BlockRenderNode::Unsupported {
            source,
            source_span,
            ..
        } => render_code_fallback_text(
            source,
            *source_span,
            appearance,
            selection_context,
            image_markers,
        ),
    }
}

fn render_paragraph(
    lines: &[InlineRenderLine],
    appearance: &AppearanceSettings,
    style: InlineMarkdownStyle,
    selection_context: Option<TranscriptInlineSelectionContext>,
    image_markers: &[TranscriptInlineImageMarker],
) -> AnyElement {
    div()
        .w_full()
        .min_w(px(0.0))
        .child(match selection_context {
            Some(selection_context) => render_inline_lines_with_style_markers_and_selection(
                lines,
                appearance,
                style,
                Some(selection_context),
                image_markers,
            ),
            None => render_inline_lines_with_style(lines, appearance, style),
        })
        .into_any_element()
}

fn render_heading(
    level: u8,
    lines: &[InlineRenderLine],
    appearance: &AppearanceSettings,
    style: InlineMarkdownStyle,
    selection_context: Option<TranscriptInlineSelectionContext>,
    image_markers: &[TranscriptInlineImageMarker],
) -> AnyElement {
    let selection_context = selection_context
        .map(|context| context.with_pending_prefix(format!("{} ", "#".repeat(level as usize))));

    div()
        .w_full()
        .min_w(px(0.0))
        .pb_1()
        .child(match selection_context {
            Some(selection_context) if image_markers.is_empty() => {
                render_heading_lines_with_style_and_selection(
                    lines,
                    appearance,
                    level,
                    style,
                    Some(selection_context),
                )
            }
            Some(selection_context) => render_heading_lines_with_style_markers_and_selection(
                lines,
                appearance,
                level,
                style,
                Some(selection_context),
                image_markers,
            ),
            None => render_heading_lines_with_style(lines, appearance, level, style),
        })
        .into_any_element()
}

fn render_list(
    list: &BlockRenderList,
    appearance: &AppearanceSettings,
    code_layout: TranscriptCodeLayout,
    conversation_m_advance: Pixels,
    style: InlineMarkdownStyle,
    code_panel_controls: Option<TranscriptCodePanelControls>,
    structural_path: String,
    selection_context: Option<TranscriptInlineSelectionContext>,
    image_markers: &[TranscriptInlineImageMarker],
    cx: &mut App,
) -> AnyElement {
    let spacing = if list.tight {
        BlockSpacing::Tight
    } else {
        BlockSpacing::Normal
    };
    let marker_width = list_marker_width(list, conversation_m_advance);
    let marker_align_end = matches!(list.kind, BlockRenderListKind::Ordered { .. })
        && list_marker_char_counts_vary(list);
    let mut container = div()
        .w_full()
        .min_w(px(0.0))
        .pl(conversation_m_advance * MARKDOWN_LIST_LEADING_MARGIN_M)
        .flex()
        .flex_col();
    container = match spacing {
        BlockSpacing::Normal => container.gap_2(),
        BlockSpacing::Tight => container.gap_1(),
    };

    container
        .children(list.items.iter().enumerate().map(|(index, item)| {
            if index > 0
                && let Some(selection_context) = selection_context.as_ref()
            {
                selection_context.set_next_break_before(spacing.raw_break_before());
            }
            render_list_item(
                item,
                appearance,
                code_layout,
                spacing,
                marker_width,
                marker_align_end,
                conversation_m_advance,
                style,
                code_panel_controls.clone(),
                markdown_code_panel_list_item_path(structural_path.as_str(), index),
                selection_context.clone(),
                image_markers,
                cx,
            )
        }))
        .into_any_element()
}

fn render_list_item(
    item: &BlockRenderListItem,
    appearance: &AppearanceSettings,
    code_layout: TranscriptCodeLayout,
    spacing: BlockSpacing,
    marker_width: Pixels,
    marker_align_end: bool,
    conversation_m_advance: Pixels,
    style: InlineMarkdownStyle,
    code_panel_controls: Option<TranscriptCodePanelControls>,
    structural_path: String,
    selection_context: Option<TranscriptInlineSelectionContext>,
    image_markers: &[TranscriptInlineImageMarker],
    cx: &mut App,
) -> AnyElement {
    let item_selection_context = selection_context
        .as_ref()
        .map(|context| context.with_pending_prefix(format!("{} ", item.marker)));
    let mut marker = div()
        .flex_none()
        .w(marker_width)
        .flex()
        .text_size(px(appearance.conversation_text.font_size))
        .font_family(appearance.conversation_text.font_family.clone())
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(0x94a3b8));
    if marker_align_end {
        marker = marker.justify_end();
    }

    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .items_start()
        .child(marker.child(item.marker.clone()))
        .child(
            div()
                .flex_none()
                .w(conversation_m_advance * MARKDOWN_LIST_MARKER_BODY_GAP_M),
        )
        .child(div().flex_1().min_w(px(0.0)).child(render_block_sequence(
            item.blocks.as_slice(),
            appearance,
            code_layout,
            conversation_m_advance,
            spacing,
            style,
            code_panel_controls,
            structural_path.as_str(),
            item_selection_context,
            image_markers,
            cx,
        )))
        .into_any_element()
}

fn list_marker_width(list: &BlockRenderList, conversation_m_advance: Pixels) -> Pixels {
    conversation_m_advance
        * markdown_list_marker_width_m(
            list.kind,
            list.items.iter().map(|item| item.marker.chars().count()),
        )
}

fn list_marker_char_counts_vary(list: &BlockRenderList) -> bool {
    let mut counts = list.items.iter().map(|item| item.marker.chars().count());
    let Some(first) = counts.next() else {
        return false;
    };
    counts.any(|count| count != first)
}

fn render_block_quote(
    blocks: &[BlockRenderNode],
    appearance: &AppearanceSettings,
    code_layout: TranscriptCodeLayout,
    conversation_m_advance: Pixels,
    style: InlineMarkdownStyle,
    code_panel_controls: Option<TranscriptCodePanelControls>,
    structural_path: String,
    selection_context: Option<TranscriptInlineSelectionContext>,
    image_markers: &[TranscriptInlineImageMarker],
    cx: &mut App,
) -> AnyElement {
    let selection_context = selection_context.map(|context| context.with_line_prefix("> "));

    div()
        .w_full()
        .min_w(px(0.0))
        .border_l_2()
        .border_color(rgb(0x334155))
        .pl_3()
        .py_1()
        .child(render_block_sequence(
            blocks,
            appearance,
            code_layout,
            conversation_m_advance,
            BlockSpacing::Normal,
            style,
            code_panel_controls,
            markdown_code_panel_block_quote_path(structural_path.as_str()).as_str(),
            selection_context,
            image_markers,
            cx,
        ))
        .into_any_element()
}

fn render_code_block(
    code: &BlockRenderCode,
    appearance: &AppearanceSettings,
    code_layout: TranscriptCodeLayout,
    code_panel_controls: Option<TranscriptCodePanelControls>,
    structural_path: &str,
    selection_context: Option<TranscriptInlineSelectionContext>,
    image_markers: &[TranscriptInlineImageMarker],
    cx: &mut App,
) -> AnyElement {
    if code
        .content_source_span
        .is_some_and(|source_span| markers_intersect_source_span(image_markers, source_span))
    {
        return render_code_block_with_image_markers(
            code,
            appearance,
            structural_path,
            selection_context,
            image_markers,
        );
    }

    let code_foreground = code_panel_foreground(appearance);
    let code_background = code_panel_background(appearance);
    let syntax_theme = code_panel_syntax_theme(appearance);
    let selection =
        selection_context.map(|context| context.code_panel_selection(structural_path, code));
    let Some(code_panel_controls) = code_panel_controls else {
        return labeled_code_block(
            "",
            None,
            code.language.as_deref(),
            code.source.as_str(),
            CodePanelWrapMode::Smart {
                columns: code_layout.transcript_bordered_panel_columns,
            },
            CodePanelDisplayProjectionInput::BuildInline,
            code_background,
            code_foreground,
            syntax_theme,
            None,
            None,
            None,
            None,
            selection,
        )
        .into_any_element();
    };

    let panel_id = code_panel_controls.panel_id_for(structural_path);
    let wrap_mode = code_panel_controls.wrap_mode(panel_id.as_str(), code_layout);
    let header = code_panel_controls.header(panel_id.as_str(), code.header_copy_source());
    let scroll_chrome = code_panel_controls.scroll_chrome(panel_id.as_str());
    let resize = code_panel_controls.resize(panel_id.as_str(), code_layout);
    let syntax_highlight = code_panel_controls.syntax_highlight(
        panel_id.as_str(),
        code.source.as_str(),
        code.language.as_deref(),
        cx,
    );
    let display_projection = code_panel_controls.display_projection(
        panel_id.as_str(),
        code.source.as_str(),
        wrap_mode,
        cx,
    );

    labeled_code_block(
        "",
        Some(panel_id),
        code.language.as_deref(),
        code.source.as_str(),
        wrap_mode,
        display_projection,
        code_background,
        code_foreground,
        syntax_theme,
        Some(syntax_highlight.as_ref()),
        Some(header),
        Some(scroll_chrome),
        Some(resize),
        selection,
    )
    .into_any_element()
}

fn code_panel_syntax_theme(appearance: &AppearanceSettings) -> CodePanelSyntaxTheme {
    let code_foreground = code_panel_foreground(appearance);
    let conversation_foreground = rgba_from_role_color(
        appearance.conversation_text.parsed_foreground(),
        rgb(0xe2e8f0),
    );
    let heading_foreground = rgba_from_role_color(
        appearance.markdown_header.parsed_foreground(),
        rgb(0x93c5fd),
    );
    let emphasis_foreground =
        rgba_from_role_color(appearance.emphasis.parsed_foreground(), rgb(0xbfdbfe));
    let strong_emphasis_foreground = rgba_from_role_color(
        appearance.strong_emphasis.parsed_foreground(),
        rgb(0xf8fafc),
    );

    CodePanelSyntaxTheme {
        plain_foreground: code_foreground,
        structural_foreground: conversation_foreground,
        heading_foreground,
        emphasis_foreground,
        strong_emphasis_foreground,
        code_foreground,
        link_foreground: emphasis_foreground,
        escape_foreground: emphasis_foreground,
    }
}

fn code_panel_foreground(appearance: &AppearanceSettings) -> gpui::Rgba {
    rgba_from_role_color(appearance.code.parsed_foreground(), rgb(0xe2e8f0))
}

fn code_panel_background(appearance: &AppearanceSettings) -> gpui::Rgba {
    rgba_from_role_color(appearance.code.parsed_background(), rgb(0x0b1220))
}

fn render_code_block_with_image_markers(
    code: &BlockRenderCode,
    appearance: &AppearanceSettings,
    structural_path: &str,
    selection_context: Option<TranscriptInlineSelectionContext>,
    image_markers: &[TranscriptInlineImageMarker],
) -> AnyElement {
    let lines = code
        .content_source_span
        .map(|source_span| {
            fallback_inline_lines_with_source_span(
                code.source.as_str(),
                InlineRenderRole::Code,
                source_span.start(),
            )
        })
        .unwrap_or_else(|| fallback_inline_lines(code.source.as_str(), InlineRenderRole::Code));
    let selection_context =
        selection_context.map(|context| context.with_code_copy_group(structural_path, code));

    div()
        .w_full()
        .min_w(px(0.0))
        .rounded_md()
        .border_1()
        .border_color(rgb(0x1f2937))
        .bg(code_panel_background(appearance))
        .p_3()
        .child(render_inline_lines_with_style_markers_and_selection(
            lines.as_slice(),
            appearance,
            InlineMarkdownStyle::default(),
            selection_context,
            image_markers,
        ))
        .into_any_element()
}

fn render_thematic_break() -> AnyElement {
    div()
        .w_full()
        .h(px(1.0))
        .bg(rgb(0x334155))
        .my_1()
        .into_any_element()
}

fn render_code_fallback_text(
    source: &str,
    source_span: Option<MarkdownSourceSpan>,
    appearance: &AppearanceSettings,
    selection_context: Option<TranscriptInlineSelectionContext>,
    image_markers: &[TranscriptInlineImageMarker],
) -> AnyElement {
    let lines = source_span
        .map(|source_span| {
            fallback_inline_lines_with_source_span(
                source,
                InlineRenderRole::Code,
                source_span.start(),
            )
        })
        .unwrap_or_else(|| fallback_inline_lines(source, InlineRenderRole::Code));
    div()
        .w_full()
        .min_w(px(0.0))
        .child(match selection_context {
            Some(selection_context)
                if source_span.is_some_and(|source_span| {
                    markers_intersect_source_span(image_markers, source_span)
                }) =>
            {
                render_inline_lines_with_style_markers_and_selection(
                    lines.as_slice(),
                    appearance,
                    InlineMarkdownStyle::default(),
                    Some(selection_context),
                    image_markers,
                )
            }
            Some(selection_context) => render_inline_lines_with_style_and_selection(
                lines.as_slice(),
                appearance,
                InlineMarkdownStyle::default(),
                Some(selection_context),
            ),
            None => render_inline_lines_with_style(
                lines.as_slice(),
                appearance,
                InlineMarkdownStyle::default(),
            ),
        })
        .into_any_element()
}

fn markers_intersect_source_span(
    image_markers: &[TranscriptInlineImageMarker],
    source_span: MarkdownSourceSpan,
) -> bool {
    image_markers.iter().any(|marker| {
        let marker_range = marker.source_range();
        marker_range.start < source_span.end() && source_span.start() < marker_range.end
    })
}
