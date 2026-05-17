use gpui::{AnyElement, App, Pixels, div, prelude::*, px};

use std::time::Instant;

use crate::shell::transcript_markdown::{
    BlockRenderCode, BlockRenderList, BlockRenderListItem, BlockRenderListKind, BlockRenderNode,
    BlockRenderPlan, InlineRenderLine, InlineRenderRole, MARKDOWN_LIST_LEADING_MARGIN_M,
    MARKDOWN_LIST_MARKER_BODY_GAP_M, MarkdownSourceSpan, markdown_code_panel_block_path,
    markdown_code_panel_block_quote_path, markdown_code_panel_list_item_path,
    markdown_list_marker_width_m,
};

use super::super::code_panel::{CodePanelDisplayProjectionInput, CodePanelWrapMode};
use super::super::code_panel_projection_cache::CodePanelSourceRevision;
use super::TranscriptCodeLayout;
use super::TranscriptTheme;
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
    theme: &TranscriptTheme,
    code_layout: TranscriptCodeLayout,
    conversation_m_advance: Pixels,
    style: InlineMarkdownStyle,
    code_panel_controls: TranscriptCodePanelControls,
    selection_context: TranscriptInlineSelectionContext,
    cx: &mut App,
) -> AnyElement {
    render_markdown_plan(
        plan,
        theme,
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
    theme: &TranscriptTheme,
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
        theme,
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
    theme: &TranscriptTheme,
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
        theme,
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
    theme: &TranscriptTheme,
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
        .border_color(theme.user_input.border())
        .p_3()
        .flex()
        .flex_col()
        .gap_2();

    if !label.is_empty() {
        block = block.child(
            div()
                .text_xs()
                .text_color(theme.user_input.foreground())
                .child(label.to_string()),
        );
    }

    block
        .child(render_markdown_plan(
            plan,
            theme,
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
    theme: &TranscriptTheme,
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
        theme,
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
    theme: &TranscriptTheme,
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
                theme,
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
    theme: &TranscriptTheme,
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
        BlockRenderNode::Paragraph { lines, .. } => render_paragraph(
            lines,
            theme,
            style,
            selection_context,
            image_markers,
            code_panel_controls.as_ref(),
        ),
        BlockRenderNode::Heading { level, lines, .. } => render_heading(
            *level,
            lines,
            theme,
            style,
            selection_context,
            image_markers,
            code_panel_controls.as_ref(),
        ),
        BlockRenderNode::List(list) => render_list(
            list,
            theme,
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
            theme,
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
            theme,
            code_layout,
            style,
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
            theme,
            selection_context,
            image_markers,
            code_panel_controls.as_ref(),
        ),
        BlockRenderNode::ThematicBreak => render_thematic_break(theme),
        BlockRenderNode::Unsupported {
            source,
            source_span,
            ..
        } => render_code_fallback_text(
            source,
            *source_span,
            theme,
            selection_context,
            image_markers,
            code_panel_controls.as_ref(),
        ),
    }
}

fn render_paragraph(
    lines: &[InlineRenderLine],
    theme: &TranscriptTheme,
    style: InlineMarkdownStyle,
    selection_context: Option<TranscriptInlineSelectionContext>,
    image_markers: &[TranscriptInlineImageMarker],
    instrumentation: Option<&TranscriptCodePanelControls>,
) -> AnyElement {
    let started = Instant::now();
    let element = div()
        .w_full()
        .min_w(px(0.0))
        .child(match selection_context {
            Some(selection_context) => render_inline_lines_with_style_markers_and_selection(
                lines,
                theme,
                style,
                Some(selection_context),
                image_markers,
            ),
            None => render_inline_lines_with_style(lines, theme, style),
        })
        .into_any_element();
    if let Some(instrumentation) = instrumentation {
        instrumentation.observe_inline_text_construction(started.elapsed());
    }
    element
}

fn render_heading(
    level: u8,
    lines: &[InlineRenderLine],
    theme: &TranscriptTheme,
    style: InlineMarkdownStyle,
    selection_context: Option<TranscriptInlineSelectionContext>,
    image_markers: &[TranscriptInlineImageMarker],
    instrumentation: Option<&TranscriptCodePanelControls>,
) -> AnyElement {
    let started = Instant::now();
    let selection_context = selection_context
        .map(|context| context.with_pending_prefix(format!("{} ", "#".repeat(level as usize))));

    let element = div()
        .w_full()
        .min_w(px(0.0))
        .pb_1()
        .child(match selection_context {
            Some(selection_context) if image_markers.is_empty() => {
                render_heading_lines_with_style_and_selection(
                    lines,
                    theme,
                    level,
                    style,
                    Some(selection_context),
                )
            }
            Some(selection_context) => render_heading_lines_with_style_markers_and_selection(
                lines,
                theme,
                level,
                style,
                Some(selection_context),
                image_markers,
            ),
            None => render_heading_lines_with_style(lines, theme, level, style),
        })
        .into_any_element();
    if let Some(instrumentation) = instrumentation {
        instrumentation.observe_inline_text_construction(started.elapsed());
    }
    element
}

fn render_list(
    list: &BlockRenderList,
    theme: &TranscriptTheme,
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
                theme,
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
    theme: &TranscriptTheme,
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
        .text_size(px(theme.list_marker.font_size()))
        .font_family(theme.list_marker.font_family().to_string())
        .font_weight(theme.list_marker.font_weight())
        .text_color(theme.list_marker.foreground());
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
            theme,
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
    theme: &TranscriptTheme,
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
        .border_color(theme.block_quote.border())
        .pl_3()
        .py_1()
        .child(render_block_sequence(
            blocks,
            theme,
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
    theme: &TranscriptTheme,
    code_layout: TranscriptCodeLayout,
    style: InlineMarkdownStyle,
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
            theme,
            structural_path,
            style,
            selection_context,
            image_markers,
            code_panel_controls.as_ref(),
        );
    }

    let Some(code_panel_controls) = code_panel_controls else {
        let selection =
            selection_context.map(|context| context.code_panel_selection(structural_path, code));
        return labeled_code_block(
            "",
            None,
            code.language.as_deref(),
            code.source.as_str(),
            CodePanelWrapMode::Smart {
                columns: code_layout.transcript_bordered_panel_columns,
            },
            CodePanelDisplayProjectionInput::BuildInline,
            theme,
            None,
            None,
            None,
            None,
            selection,
        )
        .into_any_element();
    };

    let panel_id = code_panel_controls.panel_id_for(structural_path);
    let code_panel_started = Instant::now();
    let wrap_mode = code_panel_controls.wrap_mode(panel_id.as_str(), code_layout);
    let source_revision = code_panel_source_revision(code);
    let display_projection =
        code_panel_controls.display_projection(panel_id.as_str(), source_revision, wrap_mode, cx);
    let display_projection_input = display_projection.input;
    let display_source_revision = display_projection.source_revision;
    let display_revision = display_source_revision.as_ref();
    let header = code_panel_controls.header(panel_id.as_str(), display_revision);
    let scroll_chrome = code_panel_controls.scroll_chrome(panel_id.as_str());
    let resize = code_panel_controls.resize(panel_id.as_str(), code_layout);
    let syntax_highlight = display_revision.map(|revision| {
        code_panel_controls.syntax_highlight(
            panel_id.as_str(),
            revision.display_source(),
            revision.syntax_label(),
            cx,
        )
    });
    let display_language = display_revision
        .and_then(CodePanelSourceRevision::syntax_label)
        .or(code.language.as_deref());
    let display_source = display_revision
        .map(CodePanelSourceRevision::display_source)
        .unwrap_or_default();
    let selection = match (selection_context, display_revision) {
        (Some(context), Some(revision)) => {
            let display_code = code_for_source_revision(code, revision);
            Some(context.code_panel_selection(structural_path, &display_code))
        }
        _ => None,
    };

    let element = labeled_code_block(
        "",
        Some(panel_id),
        display_language,
        display_source,
        wrap_mode,
        display_projection_input,
        theme,
        syntax_highlight.as_deref(),
        Some(header),
        Some(scroll_chrome),
        Some(resize),
        selection,
    )
    .into_any_element();
    code_panel_controls.observe_code_panel_render(code_panel_started.elapsed());
    element
}

fn code_panel_source_revision(code: &BlockRenderCode) -> CodePanelSourceRevision {
    CodePanelSourceRevision::new(
        code.source.as_str(),
        code.header_copy_source(),
        code.language.as_deref(),
        code.copy_opening_fence.as_str(),
        code.copy_closing_fence.as_str(),
    )
}

fn code_for_source_revision(
    code: &BlockRenderCode,
    revision: &CodePanelSourceRevision,
) -> BlockRenderCode {
    let mut display_code = code.clone();
    display_code.language = revision.syntax_label().map(str::to_string);
    display_code.meta = None;
    display_code.source = revision.display_source().to_string();
    display_code.source_span = None;
    display_code.content_source_span = None;
    display_code.copy_opening_fence = revision.copy_opening_fence().to_string();
    display_code.copy_closing_fence = revision.copy_closing_fence().to_string();
    display_code
}

fn render_code_block_with_image_markers(
    code: &BlockRenderCode,
    theme: &TranscriptTheme,
    structural_path: &str,
    style: InlineMarkdownStyle,
    selection_context: Option<TranscriptInlineSelectionContext>,
    image_markers: &[TranscriptInlineImageMarker],
    instrumentation: Option<&TranscriptCodePanelControls>,
) -> AnyElement {
    let started = Instant::now();
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

    let element = div()
        .w_full()
        .min_w(px(0.0))
        .rounded_md()
        .border_1()
        .border_color(theme.code_panel_border.border())
        .bg(theme.code_panel_body.background())
        .p_3()
        .child(render_inline_lines_with_style_markers_and_selection(
            lines.as_slice(),
            theme,
            style,
            selection_context,
            image_markers,
        ))
        .into_any_element();
    if let Some(instrumentation) = instrumentation {
        instrumentation.observe_inline_text_construction(started.elapsed());
    }
    element
}

fn render_thematic_break(theme: &TranscriptTheme) -> AnyElement {
    div()
        .w_full()
        .h(px(1.0))
        .bg(theme.thematic_break.color())
        .my_1()
        .into_any_element()
}

fn render_code_fallback_text(
    source: &str,
    source_span: Option<MarkdownSourceSpan>,
    theme: &TranscriptTheme,
    selection_context: Option<TranscriptInlineSelectionContext>,
    image_markers: &[TranscriptInlineImageMarker],
    instrumentation: Option<&TranscriptCodePanelControls>,
) -> AnyElement {
    let started = Instant::now();
    let lines = source_span
        .map(|source_span| {
            fallback_inline_lines_with_source_span(
                source,
                InlineRenderRole::Code,
                source_span.start(),
            )
        })
        .unwrap_or_else(|| fallback_inline_lines(source, InlineRenderRole::Code));
    let element = div()
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
                    theme,
                    InlineMarkdownStyle::unsupported_fallback(),
                    Some(selection_context),
                    image_markers,
                )
            }
            Some(selection_context) => render_inline_lines_with_style_and_selection(
                lines.as_slice(),
                theme,
                InlineMarkdownStyle::unsupported_fallback(),
                Some(selection_context),
            ),
            None => render_inline_lines_with_style(
                lines.as_slice(),
                theme,
                InlineMarkdownStyle::unsupported_fallback(),
            ),
        })
        .into_any_element();
    if let Some(instrumentation) = instrumentation {
        instrumentation.observe_inline_text_construction(started.elapsed());
    }
    element
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
