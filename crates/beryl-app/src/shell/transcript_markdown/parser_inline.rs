use markdown::mdast::Node;

use super::super::{
    Inline, MarkdownSourceMap, MarkdownSourceSpan, UnsupportedKind,
    markdown_inline_child_source_path, markdown_inline_source_path, normalize_line_endings,
};
use super::{node_source, node_source_span};

#[derive(Clone, Copy)]
enum InlinePathKind {
    BlockChild,
    InlineChild,
}

pub(super) fn inlines_from_nodes(
    nodes: Vec<Node>,
    source: &str,
    source_map: &mut MarkdownSourceMap,
    parent_path: &str,
) -> Vec<Inline> {
    let mut inlines = Vec::new();
    push_inline_nodes(
        nodes,
        source,
        source_map,
        parent_path,
        InlinePathKind::BlockChild,
        &mut inlines,
    );
    inlines
}

pub(super) fn inline_from_node(
    node: Node,
    source: &str,
    source_map: &mut MarkdownSourceMap,
    parent_path: &str,
) -> Vec<Inline> {
    let mut inlines = Vec::new();
    push_inline_node(
        node,
        source,
        source_map,
        parent_path,
        InlinePathKind::BlockChild,
        &mut inlines,
    );
    inlines
}

fn push_inline_nodes(
    nodes: Vec<Node>,
    source: &str,
    source_map: &mut MarkdownSourceMap,
    parent_path: &str,
    path_kind: InlinePathKind,
    inlines: &mut Vec<Inline>,
) {
    for node in nodes {
        push_inline_node(node, source, source_map, parent_path, path_kind, inlines);
    }
}

fn push_inline_node(
    node: Node,
    source: &str,
    source_map: &mut MarkdownSourceMap,
    parent_path: &str,
    path_kind: InlinePathKind,
    inlines: &mut Vec<Inline>,
) {
    let span = node_source_span(&node, source);
    match node {
        Node::Text(text) => push_text_inlines(
            text.value,
            span,
            source_map,
            parent_path,
            path_kind,
            inlines,
        ),
        Node::Emphasis(emphasis) => {
            let path = next_inline_path(parent_path, inlines.len(), path_kind);
            source_map.set_inline_span(path.as_str(), span);
            inlines.push(Inline::emphasis(child_inlines_from_nodes(
                emphasis.children,
                source,
                source_map,
                path.as_str(),
            )));
        }
        Node::Strong(strong) => {
            let path = next_inline_path(parent_path, inlines.len(), path_kind);
            source_map.set_inline_span(path.as_str(), span);
            inlines.push(Inline::strong(child_inlines_from_nodes(
                strong.children,
                source,
                source_map,
                path.as_str(),
            )));
        }
        Node::InlineCode(code) => {
            let path = next_inline_path(parent_path, inlines.len(), path_kind);
            source_map.set_inline_span(path, span);
            inlines.push(Inline::code(code.value));
        }
        Node::InlineMath(math) => {
            let path = next_inline_path(parent_path, inlines.len(), path_kind);
            source_map.set_inline_span(path, span);
            inlines.push(Inline::math(math.value));
        }
        Node::Break(_) => {
            let path = next_inline_path(parent_path, inlines.len(), path_kind);
            source_map.set_inline_span(path, span);
            inlines.push(Inline::hard_break());
        }
        Node::Link(link) => {
            let path = next_inline_path(parent_path, inlines.len(), path_kind);
            source_map.set_inline_span(path.as_str(), span);
            inlines.push(Inline::link(
                link.url,
                link.title,
                child_inlines_from_nodes(link.children, source, source_map, path.as_str()),
            ));
        }
        Node::Image(image) => {
            let path = next_inline_path(parent_path, inlines.len(), path_kind);
            source_map.set_inline_span(path, span);
            inlines.push(Inline::image(image.alt, image.url, image.title));
        }
        node @ Node::Html(_) => push_unsupported_inline(
            UnsupportedKind::Html,
            node,
            span,
            source,
            source_map,
            parent_path,
            path_kind,
            inlines,
        ),
        node @ (Node::LinkReference(_) | Node::ImageReference(_)) => push_unsupported_inline(
            UnsupportedKind::Reference,
            node,
            span,
            source,
            source_map,
            parent_path,
            path_kind,
            inlines,
        ),
        node @ Node::FootnoteReference(_) => push_unsupported_inline(
            UnsupportedKind::Footnote,
            node,
            span,
            source,
            source_map,
            parent_path,
            path_kind,
            inlines,
        ),
        node @ (Node::MdxTextExpression(_)
        | Node::MdxJsxTextElement(_)
        | Node::MdxFlowExpression(_)
        | Node::MdxJsxFlowElement(_)
        | Node::MdxjsEsm(_)) => push_unsupported_inline(
            UnsupportedKind::Mdx,
            node,
            span,
            source,
            source_map,
            parent_path,
            path_kind,
            inlines,
        ),
        Node::Root(root) => push_inline_nodes(
            root.children,
            source,
            source_map,
            parent_path,
            path_kind,
            inlines,
        ),
        Node::Paragraph(paragraph) => push_inline_nodes(
            paragraph.children,
            source,
            source_map,
            parent_path,
            path_kind,
            inlines,
        ),
        Node::Heading(heading) => push_inline_nodes(
            heading.children,
            source,
            source_map,
            parent_path,
            path_kind,
            inlines,
        ),
        Node::Blockquote(blockquote) => fallback_inline_block_children(
            blockquote.children,
            source,
            source_map,
            parent_path,
            path_kind,
            inlines,
        ),
        Node::List(list) => fallback_inline_block_children(
            list.children,
            source,
            source_map,
            parent_path,
            path_kind,
            inlines,
        ),
        Node::ListItem(item) => fallback_inline_block_children(
            item.children,
            source,
            source_map,
            parent_path,
            path_kind,
            inlines,
        ),
        Node::Delete(delete) => fallback_inline_block_children(
            delete.children,
            source,
            source_map,
            parent_path,
            path_kind,
            inlines,
        ),
        Node::FootnoteDefinition(definition) => fallback_inline_block_children(
            definition.children,
            source,
            source_map,
            parent_path,
            path_kind,
            inlines,
        ),
        Node::Table(table) => fallback_inline_block_children(
            table.children,
            source,
            source_map,
            parent_path,
            path_kind,
            inlines,
        ),
        Node::TableRow(row) => fallback_inline_block_children(
            row.children,
            source,
            source_map,
            parent_path,
            path_kind,
            inlines,
        ),
        Node::TableCell(cell) => fallback_inline_block_children(
            cell.children,
            source,
            source_map,
            parent_path,
            path_kind,
            inlines,
        ),
        node => push_unsupported_inline(
            UnsupportedKind::Other,
            node,
            span,
            source,
            source_map,
            parent_path,
            path_kind,
            inlines,
        ),
    }
}

fn child_inlines_from_nodes(
    nodes: Vec<Node>,
    source: &str,
    source_map: &mut MarkdownSourceMap,
    parent_path: &str,
) -> Vec<Inline> {
    let mut inlines = Vec::new();
    push_inline_nodes(
        nodes,
        source,
        source_map,
        parent_path,
        InlinePathKind::InlineChild,
        &mut inlines,
    );
    inlines
}

fn push_text_inlines(
    value: String,
    span: Option<MarkdownSourceSpan>,
    source_map: &mut MarkdownSourceMap,
    parent_path: &str,
    path_kind: InlinePathKind,
    inlines: &mut Vec<Inline>,
) {
    let normalized = normalize_line_endings(&value);

    for (index, segment) in normalized.split('\n').enumerate() {
        if index > 0 {
            let path = next_inline_path(parent_path, inlines.len(), path_kind);
            source_map.set_inline_span(path, span);
            inlines.push(Inline::soft_break());
        }
        if !segment.is_empty() {
            let path = next_inline_path(parent_path, inlines.len(), path_kind);
            source_map.set_inline_span(path, span);
            inlines.push(Inline::text(segment));
        }
    }
}

fn push_unsupported_inline(
    kind: UnsupportedKind,
    node: Node,
    span: Option<MarkdownSourceSpan>,
    source: &str,
    source_map: &mut MarkdownSourceMap,
    parent_path: &str,
    path_kind: InlinePathKind,
    inlines: &mut Vec<Inline>,
) {
    let path = next_inline_path(parent_path, inlines.len(), path_kind);
    source_map.set_inline_span(path, span);
    inlines.push(unsupported_inline(kind, &node, source));
}

fn fallback_inline_block_children(
    nodes: Vec<Node>,
    source: &str,
    source_map: &mut MarkdownSourceMap,
    parent_path: &str,
    path_kind: InlinePathKind,
    inlines: &mut Vec<Inline>,
) {
    push_inline_nodes(nodes, source, source_map, parent_path, path_kind, inlines);
}

fn next_inline_path(parent_path: &str, index: usize, path_kind: InlinePathKind) -> String {
    match path_kind {
        InlinePathKind::BlockChild => markdown_inline_source_path(parent_path, index),
        InlinePathKind::InlineChild => markdown_inline_child_source_path(parent_path, index),
    }
}

fn unsupported_inline(kind: UnsupportedKind, node: &Node, source: &str) -> Inline {
    Inline::unsupported(kind, node_source(node, source))
}
