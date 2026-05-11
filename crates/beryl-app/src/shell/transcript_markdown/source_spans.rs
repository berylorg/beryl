use std::{collections::HashMap, ops::Range};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct MarkdownSourceSpan {
    start: usize,
    end: usize,
}

impl MarkdownSourceSpan {
    pub(crate) fn new(start: usize, end: usize) -> Option<Self> {
        (start <= end).then_some(Self { start, end })
    }

    pub(crate) fn start(self) -> usize {
        self.start
    }

    pub(crate) fn end(self) -> usize {
        self.end
    }

    pub(crate) fn range(self) -> Range<usize> {
        self.start..self.end
    }

    pub(crate) fn source_text<'a>(self, source: &'a str) -> Option<&'a str> {
        source.get(self.range())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MarkdownSourceMap {
    block_spans: HashMap<String, MarkdownSourceSpan>,
    list_item_spans: HashMap<String, MarkdownSourceSpan>,
    inline_spans: HashMap<String, MarkdownSourceSpan>,
}

impl MarkdownSourceMap {
    pub(crate) fn set_block_span(
        &mut self,
        path: impl Into<String>,
        span: Option<MarkdownSourceSpan>,
    ) {
        if let Some(span) = span {
            self.block_spans.insert(path.into(), span);
        }
    }

    pub(crate) fn block_span(&self, path: &str) -> Option<MarkdownSourceSpan> {
        self.block_spans.get(path).copied()
    }

    pub(crate) fn set_list_item_span(
        &mut self,
        path: impl Into<String>,
        span: Option<MarkdownSourceSpan>,
    ) {
        if let Some(span) = span {
            self.list_item_spans.insert(path.into(), span);
        }
    }

    pub(crate) fn list_item_span(&self, path: &str) -> Option<MarkdownSourceSpan> {
        self.list_item_spans.get(path).copied()
    }

    pub(crate) fn set_inline_span(
        &mut self,
        path: impl Into<String>,
        span: Option<MarkdownSourceSpan>,
    ) {
        if let Some(span) = span {
            self.inline_spans.insert(path.into(), span);
        }
    }

    pub(crate) fn inline_span(&self, path: &str) -> Option<MarkdownSourceSpan> {
        self.inline_spans.get(path).copied()
    }
}

pub(crate) fn markdown_block_source_path(parent_path: &str, index: usize) -> String {
    markdown_source_path(parent_path, format!("b{index}"))
}

pub(crate) fn markdown_block_quote_source_path(block_path: &str) -> String {
    markdown_source_path(block_path, "q")
}

pub(crate) fn markdown_list_item_source_path(list_path: &str, index: usize) -> String {
    markdown_source_path(list_path, format!("i{index}"))
}

pub(crate) fn markdown_inline_source_path(parent_path: &str, index: usize) -> String {
    markdown_source_path(parent_path, format!("i{index}"))
}

pub(crate) fn markdown_inline_child_source_path(parent_path: &str, index: usize) -> String {
    markdown_source_path(parent_path, format!("c{index}"))
}

fn markdown_source_path(parent_path: &str, segment: impl AsRef<str>) -> String {
    if parent_path.is_empty() {
        segment.as_ref().to_string()
    } else {
        format!("{parent_path}.{}", segment.as_ref())
    }
}
