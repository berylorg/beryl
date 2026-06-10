use gpui::{Pixels, px};

use super::super::transcript_markdown::{
    BlockRenderCode, BlockRenderList, BlockRenderNode, BlockRenderPlan, InlineRenderFragment,
    InlineRenderLine, InlineRenderRole, InlineRenderStyle, MARKDOWN_LIST_LEADING_MARGIN_M,
    MARKDOWN_LIST_MARKER_BODY_GAP_M, markdown_list_marker_width_m,
};
use super::{
    CODE_PANEL_BORDER, CODE_PANEL_CONTENT_PADDING, CODE_PANEL_DEFAULT_MAX_HEIGHT,
    CODE_PANEL_HEADER_CONTENT_BORDER, CODE_PANEL_HEADER_MIN_CONTENT_HEIGHT,
    CODE_PANEL_HEADER_VERTICAL_PADDING, CODE_PANEL_MIN_HEIGHT,
    CODE_PANEL_RESIZABLE_CONTENT_VERTICAL_PADDING, CODE_PANEL_RESIZE_HANDLE_HEIGHT,
    CODE_PANEL_VISIBLE_LINE_CAP, MARKDOWN_HEADING_BOTTOM_PADDING, MARKDOWN_NORMAL_BLOCK_GAP,
    MARKDOWN_QUOTE_BORDER, MARKDOWN_QUOTE_PADDING_LEFT, MARKDOWN_QUOTE_PADDING_VERTICAL,
    MARKDOWN_THEMATIC_BREAK_HEIGHT, MARKDOWN_THEMATIC_BREAK_MARGIN_VERTICAL,
    MARKDOWN_TIGHT_BLOCK_GAP, prompt_lines,
};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PromptBlockLayout {
    pub(super) height: Pixels,
    pub(super) last_line_top: Pixels,
    pub(super) visual_line_tops: Vec<Pixels>,
}

impl PromptBlockLayout {
    fn empty_line(line_height: Pixels) -> Self {
        Self {
            height: line_height,
            last_line_top: px(0.0),
            visual_line_tops: vec![px(0.0)],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum AnchorBlockRole {
    Conversation,
    Heading { level: u8 },
}

#[derive(Clone, Copy, Debug)]
enum BlockSpacing {
    Normal,
    Tight,
}

impl BlockSpacing {
    fn gap(self) -> Pixels {
        match self {
            Self::Normal => px(MARKDOWN_NORMAL_BLOCK_GAP),
            Self::Tight => px(MARKDOWN_TIGHT_BLOCK_GAP),
        }
    }
}

pub(super) trait PromptTextMeasurer {
    fn inline_visual_line_count(
        &mut self,
        line: &InlineRenderLine,
        role: AnchorBlockRole,
        wrap_width: Pixels,
    ) -> usize;

    fn conversation_m_advance(&mut self) -> Pixels;

    fn block_line_height(&self, role: AnchorBlockRole) -> Pixels;

    fn code_line_height(&self) -> Pixels;

    fn code_header_line_height(&self) -> Pixels;
}

#[cfg(test)]
pub(super) fn prompt_markdown_layout(
    source: &str,
    prompt_width: Pixels,
    transcript_code_columns: usize,
    measurer: &mut impl PromptTextMeasurer,
) -> PromptBlockLayout {
    use super::super::transcript_markdown::{block_render_plan, parse};

    let blocks = parse(source)
        .map(|document| block_render_plan(&document).blocks)
        .unwrap_or_else(|_| plain_fallback_blocks(source));

    measure_block_sequence(
        blocks.as_slice(),
        prompt_width,
        transcript_code_columns,
        BlockSpacing::Normal,
        measurer,
    )
}

pub(super) fn prompt_markdown_layout_from_plan(
    plan: &BlockRenderPlan,
    prompt_width: Pixels,
    transcript_code_columns: usize,
    measurer: &mut impl PromptTextMeasurer,
) -> PromptBlockLayout {
    measure_block_sequence(
        plan.blocks.as_slice(),
        prompt_width,
        transcript_code_columns,
        BlockSpacing::Normal,
        measurer,
    )
}

#[cfg(test)]
fn plain_fallback_blocks(source: &str) -> Vec<BlockRenderNode> {
    let style = InlineRenderStyle {
        role: InlineRenderRole::Conversation,
        link: false,
        emphasis: false,
        strong: false,
        fallback: true,
        atom: false,
    };

    vec![BlockRenderNode::Paragraph {
        lines: prompt_lines(source)
            .into_iter()
            .map(|line| {
                if line.is_empty() {
                    InlineRenderLine {
                        fragments: Vec::new(),
                    }
                } else {
                    InlineRenderLine {
                        fragments: vec![InlineRenderFragment {
                            text: line,
                            style,
                            link_destination: None,
                            source_span: None,
                            display_source_span: None,
                            copy_prefix: String::new(),
                            copy_suffix: String::new(),
                            copy_replacement: None,
                        }],
                    }
                }
            })
            .collect(),
        source_span: None,
    }]
}

fn measure_block_sequence(
    blocks: &[BlockRenderNode],
    width: Pixels,
    transcript_code_columns: usize,
    spacing: BlockSpacing,
    measurer: &mut impl PromptTextMeasurer,
) -> PromptBlockLayout {
    if blocks.is_empty() {
        return PromptBlockLayout::empty_line(
            measurer.block_line_height(AnchorBlockRole::Conversation),
        );
    }

    let mut cursor = px(0.0);
    let mut last_line_top = px(0.0);
    let mut visual_line_tops = Vec::new();

    for (index, block) in blocks.iter().enumerate() {
        if index > 0 {
            cursor += spacing.gap();
        }

        let block_layout = measure_block(block, width, transcript_code_columns, measurer);
        last_line_top = cursor + block_layout.last_line_top;
        visual_line_tops.extend(
            block_layout
                .visual_line_tops
                .into_iter()
                .map(|line_top| cursor + line_top),
        );
        cursor += block_layout.height;
    }

    PromptBlockLayout {
        height: cursor,
        last_line_top,
        visual_line_tops,
    }
}

fn measure_block(
    block: &BlockRenderNode,
    width: Pixels,
    transcript_code_columns: usize,
    measurer: &mut impl PromptTextMeasurer,
) -> PromptBlockLayout {
    match block {
        BlockRenderNode::Paragraph { lines, .. } => {
            measure_inline_lines(lines, width, AnchorBlockRole::Conversation, measurer)
        }
        BlockRenderNode::Heading { level, lines, .. } => {
            let text_layout = measure_inline_lines(
                lines,
                width,
                AnchorBlockRole::Heading { level: *level },
                measurer,
            );
            PromptBlockLayout {
                height: text_layout.height + px(MARKDOWN_HEADING_BOTTOM_PADDING),
                last_line_top: text_layout.last_line_top,
                visual_line_tops: text_layout.visual_line_tops,
            }
        }
        BlockRenderNode::List(list) => measure_list(list, width, transcript_code_columns, measurer),
        BlockRenderNode::BlockQuote { blocks, .. } => {
            let child_width =
                (width - px(MARKDOWN_QUOTE_BORDER + MARKDOWN_QUOTE_PADDING_LEFT)).max(px(1.0));
            let child_layout = measure_block_sequence(
                blocks,
                child_width,
                transcript_code_columns,
                BlockSpacing::Normal,
                measurer,
            );
            PromptBlockLayout {
                height: px(MARKDOWN_QUOTE_PADDING_VERTICAL)
                    + child_layout.height
                    + px(MARKDOWN_QUOTE_PADDING_VERTICAL),
                last_line_top: px(MARKDOWN_QUOTE_PADDING_VERTICAL) + child_layout.last_line_top,
                visual_line_tops: child_layout
                    .visual_line_tops
                    .into_iter()
                    .map(|line_top| px(MARKDOWN_QUOTE_PADDING_VERTICAL) + line_top)
                    .collect(),
            }
        }
        BlockRenderNode::Code(code) => {
            measure_code_block(code, width, transcript_code_columns, measurer)
        }
        BlockRenderNode::Math { fallback, .. }
        | BlockRenderNode::Unsupported {
            source: fallback, ..
        } => {
            let lines = fallback_lines(fallback, InlineRenderRole::Code);
            measure_inline_lines(
                lines.as_slice(),
                width,
                AnchorBlockRole::Conversation,
                measurer,
            )
        }
        BlockRenderNode::ThematicBreak => {
            let line_top = px(MARKDOWN_THEMATIC_BREAK_MARGIN_VERTICAL);
            PromptBlockLayout {
                height: line_top
                    + px(MARKDOWN_THEMATIC_BREAK_HEIGHT)
                    + px(MARKDOWN_THEMATIC_BREAK_MARGIN_VERTICAL),
                last_line_top: line_top,
                visual_line_tops: vec![line_top],
            }
        }
    }
}

fn measure_inline_lines(
    lines: &[InlineRenderLine],
    width: Pixels,
    role: AnchorBlockRole,
    measurer: &mut impl PromptTextMeasurer,
) -> PromptBlockLayout {
    let line_height = measurer.block_line_height(role);
    if lines.is_empty() {
        return PromptBlockLayout::empty_line(line_height);
    }

    let mut cursor = px(0.0);
    let mut last_line_top = px(0.0);
    let mut visual_line_tops = Vec::new();

    for line in lines {
        let visual_line_count = measurer.inline_visual_line_count(line, role, width).max(1);
        visual_line_tops.extend(
            (0..visual_line_count).map(|visual_index| cursor + (line_height * visual_index as f32)),
        );
        last_line_top = cursor + (line_height * visual_line_count.saturating_sub(1) as f32);
        cursor += line_height * visual_line_count as f32;
    }

    PromptBlockLayout {
        height: cursor,
        last_line_top,
        visual_line_tops,
    }
}

fn measure_list(
    list: &BlockRenderList,
    width: Pixels,
    transcript_code_columns: usize,
    measurer: &mut impl PromptTextMeasurer,
) -> PromptBlockLayout {
    if list.items.is_empty() {
        return PromptBlockLayout::empty_line(
            measurer.block_line_height(AnchorBlockRole::Conversation),
        );
    }

    let spacing = if list.tight {
        BlockSpacing::Tight
    } else {
        BlockSpacing::Normal
    };
    let conversation_m_advance = measurer.conversation_m_advance();
    let child_width = (width - list_item_body_offset(list, conversation_m_advance)).max(px(1.0));
    let marker_height = measurer.block_line_height(AnchorBlockRole::Conversation);
    let mut cursor = px(0.0);
    let mut last_line_top = px(0.0);
    let mut visual_line_tops = Vec::new();

    for (index, item) in list.items.iter().enumerate() {
        if index > 0 {
            cursor += spacing.gap();
        }

        let child_layout = measure_block_sequence(
            item.blocks.as_slice(),
            child_width,
            transcript_code_columns,
            spacing,
            measurer,
        );
        last_line_top = cursor + child_layout.last_line_top;
        visual_line_tops.extend(
            child_layout
                .visual_line_tops
                .into_iter()
                .map(|line_top| cursor + line_top),
        );
        cursor += child_layout.height.max(marker_height);
    }

    PromptBlockLayout {
        height: cursor,
        last_line_top,
        visual_line_tops,
    }
}

fn list_item_body_offset(list: &BlockRenderList, conversation_m_advance: Pixels) -> Pixels {
    conversation_m_advance * MARKDOWN_LIST_LEADING_MARGIN_M
        + list_marker_width(list, conversation_m_advance)
        + conversation_m_advance * MARKDOWN_LIST_MARKER_BODY_GAP_M
}

fn list_marker_width(list: &BlockRenderList, conversation_m_advance: Pixels) -> Pixels {
    conversation_m_advance
        * markdown_list_marker_width_m(
            list.kind,
            list.items.iter().map(|item| item.marker.chars().count()),
        )
}

fn measure_code_block(
    code: &BlockRenderCode,
    _width: Pixels,
    _transcript_code_columns: usize,
    measurer: &mut impl PromptTextMeasurer,
) -> PromptBlockLayout {
    let line_count = no_wrap_code_line_count(code.source.as_str()).max(1);
    let visible_line_count = line_count.min(CODE_PANEL_VISIBLE_LINE_CAP);
    let line_height = measurer.code_line_height();
    let header_height = px(CODE_PANEL_HEADER_VERTICAL_PADDING * 2.0)
        + measurer
            .code_header_line_height()
            .max(px(CODE_PANEL_HEADER_MIN_CONTENT_HEIGHT))
        + px(CODE_PANEL_HEADER_CONTENT_BORDER);
    let content_top = px(CODE_PANEL_BORDER + CODE_PANEL_CONTENT_PADDING) + header_height;
    let resizable_content_height = (px(CODE_PANEL_RESIZABLE_CONTENT_VERTICAL_PADDING)
        + (line_height * visible_line_count as f32))
        .max(px(CODE_PANEL_MIN_HEIGHT))
        .min(px(CODE_PANEL_DEFAULT_MAX_HEIGHT));

    PromptBlockLayout {
        height: content_top
            + resizable_content_height
            + px(CODE_PANEL_CONTENT_PADDING + CODE_PANEL_RESIZE_HANDLE_HEIGHT + CODE_PANEL_BORDER),
        last_line_top: content_top + (line_height * visible_line_count.saturating_sub(1) as f32),
        visual_line_tops: (0..visible_line_count)
            .map(|line_index| content_top + (line_height * line_index as f32))
            .collect(),
    }
}

fn no_wrap_code_line_count(source: &str) -> usize {
    if source.is_empty() {
        return 1;
    }

    let mut count = 0usize;
    let mut line_start = 0usize;
    loop {
        let Some(newline_offset) = source[line_start..].find('\n') else {
            count = count.saturating_add(1);
            break;
        };

        count = count.saturating_add(1);
        line_start += newline_offset + 1;

        if line_start == source.len() {
            count = count.saturating_add(1);
            break;
        }
    }
    count
}

fn fallback_lines(source: &str, role: InlineRenderRole) -> Vec<InlineRenderLine> {
    let style = InlineRenderStyle {
        role,
        link: false,
        emphasis: false,
        strong: false,
        fallback: true,
        atom: false,
    };

    prompt_lines(source)
        .into_iter()
        .map(|line| {
            if line.is_empty() {
                InlineRenderLine {
                    fragments: Vec::new(),
                }
            } else {
                InlineRenderLine {
                    fragments: vec![InlineRenderFragment {
                        text: line,
                        style,
                        link_destination: None,
                        source_span: None,
                        display_source_span: None,
                        copy_prefix: String::new(),
                        copy_suffix: String::new(),
                        copy_replacement: None,
                    }],
                }
            }
        })
        .collect()
}
