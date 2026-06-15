#![allow(dead_code, unused_imports)]

#[path = "../src/branch_bootstrap_core.rs"]
mod branch_bootstrap_core;
#[path = "../src/shell/transcript_markdown.rs"]
mod transcript_markdown;
#[path = "../src/shell/transcript_thread_links.rs"]
mod transcript_thread_links;

use transcript_markdown::{
    Block, Inline, InlineRenderLine, inline_render_lines, inline_render_lines_with_copy_source,
    markdown_block_source_path, parse,
};
use transcript_thread_links::inline_thread_link_ranges;

#[test]
fn thread_links_create_hit_ranges_for_visible_link_text_only() {
    let line = first_paragraph_line("[Open parent](beryl_threadid://parent%20thread)");
    let links = inline_thread_link_ranges(&line);

    assert_eq!(line_text(&line), "Open parent");
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].thread_id().as_str(), "parent thread");
    assert_eq!(links[0].display_range(), 0.."Open parent".len());
}

#[test]
fn split_inline_styles_merge_into_one_thread_link_range() {
    let line = first_paragraph_line("[Open **parent** and `source`](beryl_threadid://parent)");
    let links = inline_thread_link_ranges(&line);

    assert_eq!(line_text(&line), "Open parent and source");
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].thread_id().as_str(), "parent");
    assert_eq!(links[0].display_range(), 0.."Open parent and source".len());
}

#[test]
fn adjacent_links_to_same_thread_remain_separate_when_visible_text_is_separated() {
    let line = first_paragraph_line(
        "[Parent](beryl_threadid://parent) then [Parent](beryl_threadid://parent)",
    );
    let links = inline_thread_link_ranges(&line);

    assert_eq!(line_text(&line), "Parent then Parent");
    assert_eq!(links.len(), 2);
    assert_eq!(links[0].display_range(), 0.."Parent".len());
    assert_eq!(
        links[1].display_range(),
        "Parent then ".len().."Parent then Parent".len()
    );
}

#[test]
fn malformed_and_non_beryl_links_do_not_create_thread_ranges() {
    for destination in [
        "https://example.invalid",
        "beryl_threadid://",
        "beryl_threadid://bad%XX",
        "beryl_threadid://bad%",
        "beryl_threadid://%FF",
    ] {
        let lines =
            inline_render_lines(&[Inline::link(destination, None, vec![Inline::text("label")])]);

        assert_eq!(
            inline_thread_link_ranges(&lines[0]),
            Vec::new(),
            "destination {destination:?} must not be clickable as a thread link"
        );
    }
}

#[test]
fn markdown_image_destinations_do_not_create_thread_ranges() {
    let line = first_paragraph_line("![parent](beryl_threadid://parent)");

    assert_eq!(line_text(&line), "![parent](beryl_threadid://parent)");
    assert_eq!(inline_thread_link_ranges(&line), Vec::new());
}

#[test]
fn ordinary_markdown_copy_source_is_preserved_for_thread_links() {
    let source = "[Open parent](beryl_threadid://parent)";
    let line = first_paragraph_line_with_source(source);

    assert_eq!(line.fragments.len(), 1);
    assert_eq!(line.fragments[0].copy_prefix, "[");
    assert_eq!(line.fragments[0].copy_suffix, "](beryl_threadid://parent)");
    assert_eq!(line.fragments[0].text, "Open parent");
}

fn first_paragraph_line(source: &str) -> InlineRenderLine {
    let document = parse(source).unwrap();
    let Block::Paragraph(inlines) = &document.blocks()[0] else {
        panic!("expected first block to be a paragraph");
    };
    inline_render_lines(inlines).into_iter().next().unwrap()
}

fn first_paragraph_line_with_source(source: &str) -> InlineRenderLine {
    let document = parse(source).unwrap();
    let Block::Paragraph(inlines) = &document.blocks()[0] else {
        panic!("expected first block to be a paragraph");
    };
    inline_render_lines_with_copy_source(
        inlines,
        Some(document.source_map()),
        markdown_block_source_path("", 0).as_str(),
        Some(source),
    )
    .into_iter()
    .next()
    .unwrap()
}

fn line_text(line: &InlineRenderLine) -> String {
    line.fragments
        .iter()
        .map(|fragment| fragment.text.as_str())
        .collect()
}
