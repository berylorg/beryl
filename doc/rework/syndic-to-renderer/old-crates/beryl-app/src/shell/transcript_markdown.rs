#![allow(dead_code)]

#[path = "transcript_markdown/block_render.rs"]
mod block_render;
#[path = "transcript_markdown/cache.rs"]
mod cache;
#[path = "transcript_markdown/code_panels.rs"]
mod code_panels;
#[path = "transcript_markdown/inline_render.rs"]
mod inline_render;
#[path = "transcript_markdown/ir.rs"]
mod ir;
#[path = "transcript_markdown/list_layout.rs"]
mod list_layout;
#[path = "transcript_markdown/parser.rs"]
mod parser;
#[path = "transcript_markdown/source_spans.rs"]
mod source_spans;

#[allow(unused_imports)]
pub(crate) use block_render::*;
#[allow(unused_imports)]
pub(crate) use cache::*;
#[allow(unused_imports)]
pub(crate) use code_panels::*;
#[allow(unused_imports)]
pub(crate) use inline_render::*;
pub(crate) use ir::*;
#[allow(unused_imports)]
pub(crate) use list_layout::*;
#[allow(unused_imports)]
pub(crate) use source_spans::*;

pub(crate) type ParseError = parser::ParseError;

pub(crate) fn parse(source: &str) -> Result<Document, ParseError> {
    parser::parse(source)
}
