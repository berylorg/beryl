use super::{
    Block, CodeBlock, Document, Heading, InlineRenderLine, List, ListKind, MarkdownSourceMap,
    MarkdownSourceSpan, MathBlock, UnsupportedBlock, UnsupportedKind,
    inline_render_lines_with_copy_source, markdown_block_quote_source_path,
    markdown_block_source_path, markdown_list_item_source_path,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BlockRenderPlan {
    pub(crate) blocks: Vec<BlockRenderNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BlockRenderNode {
    Paragraph {
        lines: Vec<InlineRenderLine>,
        source_span: Option<MarkdownSourceSpan>,
    },
    Heading {
        level: u8,
        lines: Vec<InlineRenderLine>,
        source_span: Option<MarkdownSourceSpan>,
    },
    List(BlockRenderList),
    BlockQuote {
        blocks: Vec<BlockRenderNode>,
        source_span: Option<MarkdownSourceSpan>,
    },
    Code(BlockRenderCode),
    Math {
        source: String,
        fallback: String,
        source_span: Option<MarkdownSourceSpan>,
    },
    ThematicBreak,
    Unsupported {
        kind: UnsupportedKind,
        source: String,
        source_span: Option<MarkdownSourceSpan>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BlockRenderList {
    pub(crate) kind: BlockRenderListKind,
    pub(crate) tight: bool,
    pub(crate) items: Vec<BlockRenderListItem>,
    pub(crate) source_span: Option<MarkdownSourceSpan>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlockRenderListKind {
    Unordered,
    Ordered { start: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BlockRenderListItem {
    pub(crate) marker: String,
    pub(crate) blocks: Vec<BlockRenderNode>,
    pub(crate) source_span: Option<MarkdownSourceSpan>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BlockRenderCode {
    pub(crate) language: Option<String>,
    pub(crate) meta: Option<String>,
    pub(crate) source: String,
    pub(crate) source_span: Option<MarkdownSourceSpan>,
    pub(crate) content_source_span: Option<MarkdownSourceSpan>,
    pub(crate) copy_opening_fence: String,
    pub(crate) copy_closing_fence: String,
}

pub(crate) fn block_render_plan(document: &Document) -> BlockRenderPlan {
    block_render_plan_inner(document, None)
}

pub(crate) fn block_render_plan_with_copy_source(
    document: &Document,
    markdown_source: &str,
) -> BlockRenderPlan {
    block_render_plan_inner(document, Some(markdown_source))
}

fn block_render_plan_inner(document: &Document, markdown_source: Option<&str>) -> BlockRenderPlan {
    BlockRenderPlan {
        blocks: block_render_nodes(
            document.blocks(),
            document.source_map(),
            "",
            markdown_source,
        ),
    }
}

fn block_render_nodes(
    blocks: &[Block],
    source_map: &MarkdownSourceMap,
    parent_path: &str,
    markdown_source: Option<&str>,
) -> Vec<BlockRenderNode> {
    blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            let path = markdown_block_source_path(parent_path, index);
            block_render_node(block, source_map, path.as_str(), markdown_source)
        })
        .collect()
}

fn block_render_node(
    block: &Block,
    source_map: &MarkdownSourceMap,
    path: &str,
    markdown_source: Option<&str>,
) -> BlockRenderNode {
    let source_span = source_map.block_span(path);
    match block {
        Block::Paragraph(inlines) => BlockRenderNode::Paragraph {
            lines: inline_render_lines_with_copy_source(
                inlines,
                Some(source_map),
                path,
                markdown_source,
            ),
            source_span,
        },
        Block::Heading(heading) => {
            heading_render_node(heading, source_map, path, source_span, markdown_source)
        }
        Block::List(list) => BlockRenderNode::List(list_render_node(
            list,
            source_map,
            path,
            source_span,
            markdown_source,
        )),
        Block::BlockQuote(blocks) => BlockRenderNode::BlockQuote {
            blocks: block_render_nodes(
                blocks,
                source_map,
                markdown_block_quote_source_path(path).as_str(),
                markdown_source,
            ),
            source_span,
        },
        Block::Code(code) => {
            BlockRenderNode::Code(code_render_node(code, source_span, markdown_source))
        }
        Block::Math(math) => BlockRenderNode::Math {
            source: math.source().to_string(),
            fallback: math_block_fallback_text(math),
            source_span,
        },
        Block::ThematicBreak => BlockRenderNode::ThematicBreak,
        Block::Unsupported(unsupported) => unsupported_render_node(unsupported, source_span),
    }
}

fn heading_render_node(
    heading: &Heading,
    source_map: &MarkdownSourceMap,
    path: &str,
    source_span: Option<MarkdownSourceSpan>,
    markdown_source: Option<&str>,
) -> BlockRenderNode {
    BlockRenderNode::Heading {
        level: heading.level().get(),
        lines: inline_render_lines_with_copy_source(
            heading.children(),
            Some(source_map),
            path,
            markdown_source,
        ),
        source_span,
    }
}

fn list_render_node(
    list: &List,
    source_map: &MarkdownSourceMap,
    path: &str,
    source_span: Option<MarkdownSourceSpan>,
    markdown_source: Option<&str>,
) -> BlockRenderList {
    let kind = match list.kind() {
        ListKind::Unordered => BlockRenderListKind::Unordered,
        ListKind::Ordered { start } => BlockRenderListKind::Ordered { start },
    };
    let start = match kind {
        BlockRenderListKind::Unordered => 0,
        BlockRenderListKind::Ordered { start } => start,
    };

    BlockRenderList {
        kind,
        tight: list.tight(),
        items: list
            .items()
            .iter()
            .enumerate()
            .map(|(index, item)| BlockRenderListItem {
                marker: list_item_marker(kind, start, index),
                blocks: block_render_nodes(
                    item.blocks(),
                    source_map,
                    markdown_list_item_source_path(path, index).as_str(),
                    markdown_source,
                ),
                source_span: source_map
                    .list_item_span(markdown_list_item_source_path(path, index).as_str()),
            })
            .collect(),
        source_span,
    }
}

fn list_item_marker(kind: BlockRenderListKind, ordered_start: u64, index: usize) -> String {
    match kind {
        BlockRenderListKind::Unordered => "-".to_string(),
        BlockRenderListKind::Ordered { .. } => {
            format!("{}.", ordered_start.saturating_add(index as u64))
        }
    }
}

fn code_render_node(
    code: &CodeBlock,
    source_span: Option<MarkdownSourceSpan>,
    markdown_source: Option<&str>,
) -> BlockRenderCode {
    let (copy_opening_fence, copy_closing_fence) =
        code_block_copy_fences(code, source_span, markdown_source);
    let content_source_span = code_block_content_source_span(code, source_span, markdown_source);
    BlockRenderCode {
        language: code.language().map(str::to_string),
        meta: code.meta().map(str::to_string),
        source: code.source().to_string(),
        source_span,
        content_source_span,
        copy_opening_fence,
        copy_closing_fence,
    }
}

fn code_block_content_source_span(
    code: &CodeBlock,
    source_span: Option<MarkdownSourceSpan>,
    markdown_source: Option<&str>,
) -> Option<MarkdownSourceSpan> {
    let source_span = source_span?;
    let block_source = source_span.source_text(markdown_source?)?;
    let opening_end = block_source.find('\n')?.saturating_add(1);
    let content = code.source();
    let content_start = if content.is_empty() {
        opening_end
    } else {
        opening_end + block_source[opening_end..].find(content)?
    };
    MarkdownSourceSpan::new(
        source_span.start().saturating_add(content_start),
        source_span
            .start()
            .saturating_add(content_start)
            .saturating_add(content.len()),
    )
}

fn code_block_copy_fences(
    code: &CodeBlock,
    source_span: Option<MarkdownSourceSpan>,
    markdown_source: Option<&str>,
) -> (String, String) {
    if let Some((opening, closing)) = markdown_source
        .and_then(|source| source_span.and_then(|span| span.source_text(source)))
        .and_then(code_block_source_fences)
    {
        return (opening, closing);
    }

    let mut opening = "```".to_string();
    if let Some(language) = code.language().filter(|language| !language.is_empty()) {
        opening.push_str(language);
    }
    if let Some(meta) = code.meta().filter(|meta| !meta.is_empty()) {
        opening.push(' ');
        opening.push_str(meta);
    }
    (opening, "```".to_string())
}

fn code_block_source_fences(source: &str) -> Option<(String, String)> {
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    let opening_end = normalized.find('\n')?;
    let closing_start = normalized.rfind('\n')?;
    if closing_start < opening_end {
        return None;
    }
    Some((
        normalized[..opening_end].to_string(),
        normalized[closing_start + 1..].to_string(),
    ))
}

fn math_block_fallback_text(math: &MathBlock) -> String {
    format!("$$\n{}\n$$", math.source())
}

fn unsupported_render_node(
    unsupported: &UnsupportedBlock,
    source_span: Option<MarkdownSourceSpan>,
) -> BlockRenderNode {
    BlockRenderNode::Unsupported {
        kind: unsupported.kind(),
        source: unsupported.source().to_string(),
        source_span,
    }
}
