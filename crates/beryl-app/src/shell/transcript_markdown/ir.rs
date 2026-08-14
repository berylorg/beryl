#![allow(dead_code)]

use super::source_spans::{
    MarkdownSourceMap, MarkdownSourceSpan, markdown_block_quote_source_path,
    markdown_block_source_path, markdown_inline_source_path, markdown_list_item_source_path,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Document {
    blocks: Vec<Block>,
    source_map: MarkdownSourceMap,
}

impl Document {
    pub(crate) fn new(blocks: Vec<Block>) -> Self {
        Self {
            blocks,
            source_map: MarkdownSourceMap::default(),
        }
    }

    pub(crate) fn with_source_map(blocks: Vec<Block>, source_map: MarkdownSourceMap) -> Self {
        Self { blocks, source_map }
    }

    pub(crate) fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub(crate) fn source_map(&self) -> &MarkdownSourceMap {
        &self.source_map
    }

    pub(crate) fn into_blocks(self) -> Vec<Block> {
        self.blocks
    }

    pub(crate) fn image_requests(&self) -> Vec<MarkdownImageRequest> {
        let mut requests = Vec::new();
        collect_block_image_requests(self.blocks(), self.source_map(), "", &mut requests);
        requests
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MarkdownImageRequest {
    alt: String,
    destination: String,
    title: Option<String>,
    source_span: Option<MarkdownSourceSpan>,
}

impl MarkdownImageRequest {
    pub(crate) fn new(
        alt: impl Into<String>,
        destination: impl Into<String>,
        title: Option<String>,
        source_span: Option<MarkdownSourceSpan>,
    ) -> Self {
        Self {
            alt: normalize_owned(alt),
            destination: normalize_owned(destination),
            title: title.map(normalize_owned),
            source_span,
        }
    }

    pub(crate) fn alt(&self) -> &str {
        &self.alt
    }

    pub(crate) fn destination(&self) -> &str {
        &self.destination
    }

    pub(crate) fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub(crate) fn source_span(&self) -> Option<MarkdownSourceSpan> {
        self.source_span
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Block {
    Paragraph(Vec<Inline>),
    Heading(Heading),
    List(List),
    BlockQuote(Vec<Block>),
    Code(CodeBlock),
    Math(MathBlock),
    ThematicBreak,
    Unsupported(UnsupportedBlock),
}

impl Block {
    pub(crate) fn paragraph(children: Vec<Inline>) -> Self {
        Self::Paragraph(children)
    }

    pub(crate) fn heading(level: HeadingLevel, children: Vec<Inline>) -> Self {
        Self::Heading(Heading::new(level, children))
    }

    pub(crate) fn list(list: List) -> Self {
        Self::List(list)
    }

    pub(crate) fn block_quote(blocks: Vec<Block>) -> Self {
        Self::BlockQuote(blocks)
    }

    pub(crate) fn code_block(
        language: Option<String>,
        meta: Option<String>,
        source: impl Into<String>,
    ) -> Self {
        Self::Code(CodeBlock::new(language, meta, source))
    }

    pub(crate) fn math_block(source: impl Into<String>) -> Self {
        Self::Math(MathBlock::new(source))
    }

    pub(crate) fn unsupported(kind: UnsupportedKind, source: impl Into<String>) -> Self {
        Self::Unsupported(UnsupportedBlock::new(kind, source))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Heading {
    level: HeadingLevel,
    children: Vec<Inline>,
}

impl Heading {
    pub(crate) fn new(level: HeadingLevel, children: Vec<Inline>) -> Self {
        Self { level, children }
    }

    pub(crate) fn level(&self) -> HeadingLevel {
        self.level
    }

    pub(crate) fn children(&self) -> &[Inline] {
        &self.children
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HeadingLevel(u8);

impl HeadingLevel {
    pub(crate) fn new(level: u8) -> Option<Self> {
        (1..=6).contains(&level).then_some(Self(level))
    }

    pub(crate) fn get(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct List {
    kind: ListKind,
    tight: bool,
    items: Vec<ListItem>,
}

impl List {
    pub(crate) fn new(kind: ListKind, tight: bool, items: Vec<ListItem>) -> Self {
        Self { kind, tight, items }
    }

    pub(crate) fn unordered(tight: bool, items: Vec<ListItem>) -> Self {
        Self::new(ListKind::Unordered, tight, items)
    }

    pub(crate) fn ordered(start: u64, tight: bool, items: Vec<ListItem>) -> Self {
        Self::new(ListKind::Ordered { start }, tight, items)
    }

    pub(crate) fn kind(&self) -> ListKind {
        self.kind
    }

    pub(crate) fn tight(&self) -> bool {
        self.tight
    }

    pub(crate) fn items(&self) -> &[ListItem] {
        &self.items
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ListKind {
    Unordered,
    Ordered { start: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ListItem {
    blocks: Vec<Block>,
}

impl ListItem {
    pub(crate) fn new(blocks: Vec<Block>) -> Self {
        Self { blocks }
    }

    pub(crate) fn blocks(&self) -> &[Block] {
        &self.blocks
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodeBlock {
    language: Option<String>,
    meta: Option<String>,
    source: String,
}

impl CodeBlock {
    pub(crate) fn new(
        language: Option<String>,
        meta: Option<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            language: language.map(normalize_owned),
            meta: meta.map(normalize_owned),
            source: normalize_owned(source),
        }
    }

    pub(crate) fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    pub(crate) fn meta(&self) -> Option<&str> {
        self.meta.as_deref()
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Link {
    destination: String,
    title: Option<String>,
    children: Vec<Inline>,
}

impl Link {
    pub(crate) fn new(
        destination: impl Into<String>,
        title: Option<String>,
        children: Vec<Inline>,
    ) -> Self {
        Self {
            destination: normalize_owned(destination),
            title: title.map(normalize_owned),
            children,
        }
    }

    pub(crate) fn destination(&self) -> &str {
        &self.destination
    }

    pub(crate) fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub(crate) fn children(&self) -> &[Inline] {
        &self.children
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Image {
    destination: String,
    title: Option<String>,
    alt: String,
}

impl Image {
    pub(crate) fn new(
        alt: impl Into<String>,
        destination: impl Into<String>,
        title: Option<String>,
    ) -> Self {
        Self {
            alt: normalize_owned(alt),
            destination: normalize_owned(destination),
            title: title.map(normalize_owned),
        }
    }

    pub(crate) fn alt(&self) -> &str {
        &self.alt
    }

    pub(crate) fn destination(&self) -> &str {
        &self.destination
    }

    pub(crate) fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MathSpan {
    source: String,
}

impl MathSpan {
    pub(crate) fn new(source: impl Into<String>) -> Self {
        Self {
            source: normalize_owned(source),
        }
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MathBlock {
    source: String,
}

impl MathBlock {
    pub(crate) fn new(source: impl Into<String>) -> Self {
        Self {
            source: normalize_owned(source),
        }
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Inline {
    Text(String),
    Emphasis(Vec<Inline>),
    Strong(Vec<Inline>),
    Code(String),
    Link(Link),
    Image(Image),
    Math(MathSpan),
    SoftBreak,
    HardBreak,
    Unsupported(UnsupportedInline),
}

impl Inline {
    pub(crate) fn text(text: impl Into<String>) -> Self {
        Self::Text(normalize_owned(text))
    }

    pub(crate) fn emphasis(children: Vec<Inline>) -> Self {
        Self::Emphasis(children)
    }

    pub(crate) fn strong(children: Vec<Inline>) -> Self {
        Self::Strong(children)
    }

    pub(crate) fn code(source: impl Into<String>) -> Self {
        Self::Code(normalize_owned(source))
    }

    pub(crate) fn link(
        destination: impl Into<String>,
        title: Option<String>,
        children: Vec<Inline>,
    ) -> Self {
        Self::Link(Link::new(destination, title, children))
    }

    pub(crate) fn image(
        alt: impl Into<String>,
        destination: impl Into<String>,
        title: Option<String>,
    ) -> Self {
        Self::Image(Image::new(alt, destination, title))
    }

    pub(crate) fn math(source: impl Into<String>) -> Self {
        Self::Math(MathSpan::new(source))
    }

    pub(crate) fn soft_break() -> Self {
        Self::SoftBreak
    }

    pub(crate) fn hard_break() -> Self {
        Self::HardBreak
    }

    pub(crate) fn unsupported(kind: UnsupportedKind, source: impl Into<String>) -> Self {
        Self::Unsupported(UnsupportedInline::new(kind, source))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnsupportedKind {
    Html,
    Mdx,
    Definition,
    Reference,
    Footnote,
    Table,
    TaskList,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnsupportedBlock {
    kind: UnsupportedKind,
    source: String,
}

impl UnsupportedBlock {
    pub(crate) fn new(kind: UnsupportedKind, source: impl Into<String>) -> Self {
        Self {
            kind,
            source: normalize_owned(source),
        }
    }

    pub(crate) fn kind(&self) -> UnsupportedKind {
        self.kind
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnsupportedInline {
    kind: UnsupportedKind,
    source: String,
}

impl UnsupportedInline {
    pub(crate) fn new(kind: UnsupportedKind, source: impl Into<String>) -> Self {
        Self {
            kind,
            source: normalize_owned(source),
        }
    }

    pub(crate) fn kind(&self) -> UnsupportedKind {
        self.kind
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }
}

pub(crate) fn normalize_line_endings(source: &str) -> String {
    if !source.contains('\r') {
        return source.to_string();
    }

    let mut normalized = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(ch);
        }
    }
    normalized
}

fn collect_block_image_requests(
    blocks: &[Block],
    source_map: &MarkdownSourceMap,
    parent_path: &str,
    requests: &mut Vec<MarkdownImageRequest>,
) {
    for (index, block) in blocks.iter().enumerate() {
        let path = markdown_block_source_path(parent_path, index);
        collect_block_image_request(block, source_map, path.as_str(), requests);
    }
}

fn collect_block_image_request(
    block: &Block,
    source_map: &MarkdownSourceMap,
    path: &str,
    requests: &mut Vec<MarkdownImageRequest>,
) {
    match block {
        Block::Paragraph(inlines) => {
            collect_inline_image_requests(inlines, source_map, path, requests)
        }
        Block::Heading(heading) => {
            collect_inline_image_requests(heading.children(), source_map, path, requests)
        }
        Block::List(list) => {
            for (index, item) in list.items().iter().enumerate() {
                collect_block_image_requests(
                    item.blocks(),
                    source_map,
                    markdown_list_item_source_path(path, index).as_str(),
                    requests,
                );
            }
        }
        Block::BlockQuote(blocks) => collect_block_image_requests(
            blocks,
            source_map,
            markdown_block_quote_source_path(path).as_str(),
            requests,
        ),
        Block::Code(_) | Block::Math(_) | Block::ThematicBreak | Block::Unsupported(_) => {}
    }
}

fn collect_inline_image_requests(
    inlines: &[Inline],
    source_map: &MarkdownSourceMap,
    parent_path: &str,
    requests: &mut Vec<MarkdownImageRequest>,
) {
    for (index, inline) in inlines.iter().enumerate() {
        collect_inline_image_request(
            inline,
            source_map,
            markdown_inline_source_path(parent_path, index).as_str(),
            requests,
        );
    }
}

fn collect_inline_image_request(
    inline: &Inline,
    source_map: &MarkdownSourceMap,
    path: &str,
    requests: &mut Vec<MarkdownImageRequest>,
) {
    match inline {
        Inline::Image(image) => requests.push(MarkdownImageRequest::new(
            image.alt(),
            image.destination(),
            image.title().map(str::to_string),
            source_map.inline_span(path),
        )),
        Inline::Emphasis(children) | Inline::Strong(children) => {
            if let Some(image) = standalone_wrapped_image(children) {
                requests.push(MarkdownImageRequest::new(
                    image.alt(),
                    image.destination(),
                    image.title().map(str::to_string),
                    source_map.inline_span(path),
                ));
            }
        }
        Inline::Text(_)
        | Inline::Code(_)
        | Inline::Link(_)
        | Inline::Math(_)
        | Inline::SoftBreak
        | Inline::HardBreak
        | Inline::Unsupported(_) => {}
    }
}

fn standalone_wrapped_image(inlines: &[Inline]) -> Option<&Image> {
    let [inline] = inlines else {
        return None;
    };
    match inline {
        Inline::Image(image) => Some(image),
        Inline::Emphasis(children) | Inline::Strong(children) => standalone_wrapped_image(children),
        _ => None,
    }
}

fn normalize_owned(source: impl Into<String>) -> String {
    let source = source.into();
    if source.contains('\r') {
        normalize_line_endings(&source)
    } else {
        source
    }
}
