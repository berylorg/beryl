use std::{error::Error, fmt};

use markdown::{
    Constructs, ParseOptions,
    mdast::{self, Node},
};

#[path = "parser_inline.rs"]
mod parser_inline;

use super::{
    Block, Document, HeadingLevel, List, ListItem, MarkdownSourceMap, MarkdownSourceSpan,
    UnsupportedKind, markdown_block_quote_source_path, markdown_block_source_path,
    markdown_list_item_source_path,
};
use parser_inline::{inline_from_node, inlines_from_nodes};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParseError {
    message: String,
}

impl ParseError {
    fn new(message: markdown::message::Message) -> Self {
        Self {
            message: message.to_string(),
        }
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for ParseError {}

pub(crate) fn parse(source: &str) -> Result<Document, ParseError> {
    let tree = markdown::to_mdast(source, &parse_options()).map_err(ParseError::new)?;
    Ok(document_from_node(tree, source))
}

fn parse_options() -> ParseOptions {
    ParseOptions {
        constructs: Constructs {
            math_flow: true,
            math_text: true,
            mdx_esm: false,
            mdx_expression_flow: false,
            mdx_expression_text: false,
            mdx_jsx_flow: false,
            mdx_jsx_text: false,
            ..Constructs::default()
        },
        math_text_single_dollar: false,
        ..ParseOptions::default()
    }
}

fn document_from_node(node: Node, source: &str) -> Document {
    let mut source_map = MarkdownSourceMap::default();
    let blocks = match node {
        Node::Root(root) => blocks_from_nodes(root.children, source, &mut source_map, ""),
        other => block_from_node(
            other,
            source,
            &mut source_map,
            markdown_block_source_path("", 0).as_str(),
        )
        .into_iter()
        .collect(),
    };
    Document::with_source_map(blocks, source_map)
}

fn blocks_from_nodes(
    nodes: Vec<Node>,
    source: &str,
    source_map: &mut MarkdownSourceMap,
    parent_path: &str,
) -> Vec<Block> {
    nodes
        .into_iter()
        .enumerate()
        .filter_map(|(index, node)| {
            let path = markdown_block_source_path(parent_path, index);
            block_from_node(node, source, source_map, path.as_str())
        })
        .collect()
}

fn block_from_node(
    node: Node,
    source: &str,
    source_map: &mut MarkdownSourceMap,
    path: &str,
) -> Option<Block> {
    source_map.set_block_span(path, node_source_span(&node, source));
    match node {
        Node::Root(root) => Some(Block::block_quote(blocks_from_nodes(
            root.children,
            source,
            source_map,
            markdown_block_quote_source_path(path).as_str(),
        ))),
        Node::Paragraph(paragraph) => Some(Block::paragraph(inlines_from_nodes(
            paragraph.children,
            source,
            source_map,
            path,
        ))),
        Node::Heading(heading) => {
            let level = HeadingLevel::new(heading.depth)?;
            Some(Block::heading(
                level,
                inlines_from_nodes(heading.children, source, source_map, path),
            ))
        }
        Node::Blockquote(blockquote) => Some(Block::block_quote(blocks_from_nodes(
            blockquote.children,
            source,
            source_map,
            markdown_block_quote_source_path(path).as_str(),
        ))),
        Node::List(list) => Some(block_from_list(list, source, source_map, path)),
        Node::Code(code) => Some(Block::code_block(code.lang, code.meta, code.value)),
        Node::Math(math) => Some(Block::math_block(math.value)),
        Node::ThematicBreak(_) => Some(Block::ThematicBreak),
        node @ Node::Html(_) => Some(unsupported_block(UnsupportedKind::Html, &node, source)),
        node @ Node::Definition(_) => Some(unsupported_block(
            UnsupportedKind::Definition,
            &node,
            source,
        )),
        node @ (Node::FootnoteDefinition(_) | Node::FootnoteReference(_)) => {
            Some(unsupported_block(UnsupportedKind::Footnote, &node, source))
        }
        node @ (Node::Table(_) | Node::TableRow(_) | Node::TableCell(_)) => {
            Some(unsupported_block(UnsupportedKind::Table, &node, source))
        }
        node @ (Node::MdxjsEsm(_)
        | Node::MdxFlowExpression(_)
        | Node::MdxTextExpression(_)
        | Node::MdxJsxFlowElement(_)
        | Node::MdxJsxTextElement(_)) => {
            Some(unsupported_block(UnsupportedKind::Mdx, &node, source))
        }
        node @ (Node::Toml(_) | Node::Yaml(_) | Node::Delete(_) | Node::ListItem(_)) => {
            Some(unsupported_block(UnsupportedKind::Other, &node, source))
        }
        inline_node => {
            let inline = inline_from_node(inline_node, source, source_map, path);
            Some(Block::paragraph(inline.into_iter().collect()))
        }
    }
}

fn block_from_list(
    list: mdast::List,
    source: &str,
    source_map: &mut MarkdownSourceMap,
    path: &str,
) -> Block {
    let items = list
        .children
        .into_iter()
        .enumerate()
        .map(|(index, node)| {
            let item_path = markdown_list_item_source_path(path, index);
            source_map.set_list_item_span(item_path.as_str(), node_source_span(&node, source));
            match node {
                Node::ListItem(item) => ListItem::new(blocks_from_nodes(
                    item.children,
                    source,
                    source_map,
                    item_path.as_str(),
                )),
                other => ListItem::new(vec![unsupported_block(
                    UnsupportedKind::Other,
                    &other,
                    source,
                )]),
            }
        })
        .collect::<Vec<_>>();
    let tight = !list.spread;
    let list = if list.ordered {
        List::ordered(list.start.unwrap_or(1).into(), tight, items)
    } else {
        List::unordered(tight, items)
    };

    Block::list(list)
}

fn unsupported_block(kind: UnsupportedKind, node: &Node, source: &str) -> Block {
    Block::unsupported(kind, node_source(node, source))
}

fn node_source(node: &Node, source: &str) -> String {
    node_source_span(node, source)
        .and_then(|span| span.source_text(source))
        .map(str::to_string)
        .unwrap_or_else(|| node.to_string())
}

fn node_source_span(node: &Node, source: &str) -> Option<MarkdownSourceSpan> {
    node.position()
        .and_then(|position| source_span_from_position(source, position))
}

fn source_span_from_position(
    source: &str,
    position: &markdown::unist::Position,
) -> Option<MarkdownSourceSpan> {
    source
        .get(position.start.offset..position.end.offset)
        .and_then(|_| MarkdownSourceSpan::new(position.start.offset, position.end.offset))
        .or_else(|| {
            source_span_from_char_offsets(source, position.start.offset, position.end.offset)
        })
}

fn source_span_from_char_offsets(
    source: &str,
    start: usize,
    end: usize,
) -> Option<MarkdownSourceSpan> {
    if start > end {
        return None;
    }

    let start = byte_index_for_char_offset(source, start)?;
    let end = byte_index_for_char_offset(source, end)?;
    source
        .get(start..end)
        .and_then(|_| MarkdownSourceSpan::new(start, end))
}

fn byte_index_for_char_offset(source: &str, target: usize) -> Option<usize> {
    let mut current = 0;
    for (byte_index, _) in source.char_indices() {
        if current == target {
            return Some(byte_index);
        }
        current += 1;
    }
    (current == target).then_some(source.len())
}
