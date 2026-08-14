#[path = "../src/shell/transcript_markdown.rs"]
mod transcript_markdown;

use transcript_markdown::{
    BlockRenderListKind, BlockRenderNode, InlineRenderLine, InlineRenderRole, MarkdownSourceSpan,
    block_render_plan, block_render_plan_with_copy_source, markdown_code_panel_id,
    markdown_code_panel_id_belongs_to_row, markdown_code_panel_ids, parse,
};

#[test]
fn block_render_plan_selects_native_transcript_blocks() {
    let document = parse(
        "# Title\n\
         \n\
         Paragraph with **strong** text.\n\
         \n\
         > quoted\n\
         \n\
         ```rust\n\
         fn main() {}\n\
         ```\n\
         \n\
         $$\n\
         x^2\n\
         $$\n\
         \n\
         ---\n\
         \n\
         <div>\n\
         raw\n\
         </div>",
    )
    .expect("markdown should parse");

    let plan = block_render_plan(&document);
    assert_eq!(plan.blocks.len(), 7);

    let BlockRenderNode::Heading { level, lines, .. } = &plan.blocks[0] else {
        panic!("expected heading");
    };
    assert_eq!(*level, 1);
    assert_eq!(line_text(&lines[0]), "Title");

    let BlockRenderNode::Paragraph { lines, .. } = &plan.blocks[1] else {
        panic!("expected paragraph");
    };
    assert_eq!(line_text(&lines[0]), "Paragraph with strong text.");
    assert_eq!(
        lines[0].fragments[1].style.role,
        InlineRenderRole::StrongEmphasis
    );

    let BlockRenderNode::BlockQuote { blocks, .. } = &plan.blocks[2] else {
        panic!("expected block quote");
    };
    let [BlockRenderNode::Paragraph { lines, .. }] = blocks.as_slice() else {
        panic!("expected quoted paragraph");
    };
    assert_eq!(line_text(&lines[0]), "quoted");

    let BlockRenderNode::Code(code) = &plan.blocks[3] else {
        panic!("expected code block");
    };
    assert_eq!(code.language.as_deref(), Some("rust"));
    assert_eq!(code.source, "fn main() {}");

    let BlockRenderNode::Math {
        source, fallback, ..
    } = &plan.blocks[4]
    else {
        panic!("expected math block");
    };
    assert_eq!(source, "x^2");
    assert_eq!(fallback, "$$\nx^2\n$$");

    assert_eq!(plan.blocks[5], BlockRenderNode::ThematicBreak);

    let BlockRenderNode::Unsupported { source, .. } = &plan.blocks[6] else {
        panic!("expected unsupported raw html fallback");
    };
    assert_eq!(source, "<div>\nraw\n</div>");
}

#[test]
fn block_render_plan_preserves_ordered_list_numbering() {
    let document = parse(
        "4. first\n\
         5. second\n\
         \n\
         - alpha\n\
         - beta",
    )
    .expect("markdown should parse");
    let plan = block_render_plan(&document);
    assert_eq!(plan.blocks.len(), 2);

    let BlockRenderNode::List(ordered) = &plan.blocks[0] else {
        panic!("expected ordered list");
    };
    assert_eq!(ordered.kind, BlockRenderListKind::Ordered { start: 4 });
    assert_eq!(ordered.items[0].marker, "4.");
    assert_eq!(ordered.items[1].marker, "5.");
    assert_eq!(list_item_text(&ordered.items[0]), "first");
    assert_eq!(list_item_text(&ordered.items[1]), "second");

    let BlockRenderNode::List(unordered) = &plan.blocks[1] else {
        panic!("expected unordered list");
    };
    assert_eq!(unordered.kind, BlockRenderListKind::Unordered);
    assert_eq!(unordered.items[0].marker, "-");
    assert_eq!(unordered.items[1].marker, "-");
}

#[test]
fn markdown_code_panel_ids_follow_structural_code_block_paths() {
    let document = parse(concat!(
        "```rust\n",
        "top();\n",
        "```\n\n",
        "- item\n",
        "  ```text\n",
        "  nested\n",
        "  ```\n\n",
        "> quote\n",
        "> ```sh\n",
        "> echo quoted\n",
        "> ```",
    ))
    .expect("markdown should parse");
    let plan = block_render_plan(&document);

    assert_eq!(
        markdown_code_panel_ids("row-a", "item:answer", &plan),
        vec![
            markdown_code_panel_id("row-a", "item:answer", "b0"),
            markdown_code_panel_id("row-a", "item:answer", "b1.i0.b1"),
            markdown_code_panel_id("row-a", "item:answer", "b2.q.b1"),
        ]
    );
}

#[test]
fn markdown_code_panel_ids_are_scoped_by_row_and_transcript_block() {
    assert_ne!(
        markdown_code_panel_id("row-a", "item:answer", "b0"),
        markdown_code_panel_id("row-b", "item:answer", "b0")
    );
    assert_ne!(
        markdown_code_panel_id("row-a", "item:answer", "b0"),
        markdown_code_panel_id("row-a", "user-prompt", "b0")
    );
}

#[test]
fn markdown_code_panel_ids_do_not_depend_on_fence_language_or_metadata() {
    let markdown_document =
        parse("```markdown linenos\n# Heading\n```").expect("markdown should parse");
    let rust_document = parse("```rust\n# Heading\n```").expect("markdown should parse");
    let markdown_plan = block_render_plan(&markdown_document);
    let rust_plan = block_render_plan(&rust_document);

    assert_eq!(
        markdown_code_panel_ids("row-a", "item:answer", &markdown_plan),
        markdown_code_panel_ids("row-a", "item:answer", &rust_plan)
    );
}

#[test]
fn markdown_code_panel_row_match_uses_encoded_row_identity_length() {
    let panel_id = markdown_code_panel_id("row", "item:answer", "b0");

    assert!(markdown_code_panel_id_belongs_to_row(&panel_id, "row"));
    assert!(!markdown_code_panel_id_belongs_to_row(&panel_id, "ro"));
    assert!(!markdown_code_panel_id_belongs_to_row(
        &panel_id,
        "row:extra"
    ));
}

#[test]
fn block_render_plan_carries_markdown_source_spans() {
    let source = concat!(
        "Hello, `world` and [docs](https://example.invalid)\n\n",
        "- *first*\n\n",
        "> quote\n\n",
        "```rust\n",
        "fn main() {}\n",
        "```",
    );
    let document = parse(source).expect("markdown should parse");
    let plan = block_render_plan_with_copy_source(&document, source);

    let BlockRenderNode::Paragraph { lines, source_span } = &plan.blocks[0] else {
        panic!("expected paragraph");
    };
    assert_eq!(
        span_text(*source_span, source),
        "Hello, `world` and [docs](https://example.invalid)"
    );
    assert_eq!(line_text(&lines[0]), "Hello, world and docs");
    assert_eq!(
        span_text(lines[0].fragments[1].source_span, source),
        "`world`"
    );
    assert_eq!(lines[0].fragments[1].copy_prefix, "`");
    assert_eq!(lines[0].fragments[1].copy_suffix, "`");
    assert_eq!(
        span_text(lines[0].fragments[3].source_span, source),
        "[docs](https://example.invalid)"
    );
    assert_eq!(lines[0].fragments[3].copy_prefix, "[");
    assert_eq!(
        lines[0].fragments[3].copy_suffix,
        "](https://example.invalid)"
    );

    let BlockRenderNode::List(list) = &plan.blocks[1] else {
        panic!("expected list");
    };
    assert_eq!(span_text(list.source_span, source), "- *first*\n");
    assert_eq!(span_text(list.items[0].source_span, source), "- *first*\n");
    let [BlockRenderNode::Paragraph { lines, .. }] = list.items[0].blocks.as_slice() else {
        panic!("expected list item paragraph");
    };
    assert_eq!(
        span_text(lines[0].fragments[0].source_span, source),
        "*first*"
    );
    assert_eq!(lines[0].fragments[0].copy_prefix, "*");
    assert_eq!(lines[0].fragments[0].copy_suffix, "*");

    let BlockRenderNode::BlockQuote { source_span, .. } = &plan.blocks[2] else {
        panic!("expected block quote");
    };
    assert_eq!(span_text(*source_span, source), "> quote");

    let BlockRenderNode::Code(code) = &plan.blocks[3] else {
        panic!("expected code block");
    };
    assert_eq!(
        span_text(code.source_span, source),
        "```rust\nfn main() {}\n```"
    );
    assert_eq!(code.source, "fn main() {}");
    assert_eq!(code.copy_opening_fence, "```rust");
    assert_eq!(code.copy_closing_fence, "```");
}

#[test]
fn inline_copy_wrappers_cover_strong_links_and_fallback_source() {
    let source = concat!(
        "**strong** and [docs](https://example.invalid) ",
        "![diagram](artifact://diagram.png \"Diagram\") ",
        "$$x + y$$ <span>x</span>",
    );
    let document = parse(source).expect("markdown should parse");
    let plan = block_render_plan_with_copy_source(&document, source);

    let BlockRenderNode::Paragraph { lines, .. } = &plan.blocks[0] else {
        panic!("expected paragraph");
    };
    let fragments = &lines[0].fragments;
    assert_eq!(
        line_text(&lines[0]),
        concat!(
            "strong and docs ",
            "![diagram](artifact://diagram.png \"Diagram\") ",
            "$$x + y$$ <span>x</span>",
        )
    );
    assert_eq!(fragments[0].text, "strong");
    assert_eq!(fragments[0].copy_prefix, "**");
    assert_eq!(fragments[0].copy_suffix, "**");
    assert_eq!(fragments[2].text, "docs");
    assert_eq!(fragments[2].copy_prefix, "[");
    assert_eq!(fragments[2].copy_suffix, "](https://example.invalid)");

    let fallback_fragments = &fragments[4..];
    assert!(
        fallback_fragments
            .iter()
            .all(|fragment| fragment.copy_prefix.is_empty() && fragment.copy_suffix.is_empty())
    );
    assert_eq!(
        fallback_fragments[0].text,
        "![diagram](artifact://diagram.png \"Diagram\")"
    );
    assert_eq!(fallback_fragments[2].text, "$$x + y$$");
    assert_eq!(fallback_fragments[4].text, "<span>");
    assert_eq!(fallback_fragments[6].text, "</span>");
}

#[test]
fn code_block_copy_source_keeps_header_copy_bare_and_selection_copy_fenced() {
    let source = concat!("~~~python linenos\n", "print('x')\n", "~~~",);
    let document = parse(source).expect("markdown should parse");
    let plan = block_render_plan_with_copy_source(&document, source);

    let BlockRenderNode::Code(code) = &plan.blocks[0] else {
        panic!("expected code block");
    };
    assert_eq!(code.source, "print('x')");
    assert_eq!(code.header_copy_source(), "print('x')");
    assert_eq!(code.copy_opening_fence, "~~~python linenos");
    assert_eq!(code.copy_closing_fence, "~~~");
}

fn list_item_text(item: &transcript_markdown::BlockRenderListItem) -> String {
    let [BlockRenderNode::Paragraph { lines, .. }] = item.blocks.as_slice() else {
        panic!("expected paragraph list item");
    };
    line_text(&lines[0])
}

fn line_text(line: &InlineRenderLine) -> String {
    line.fragments
        .iter()
        .map(|fragment| fragment.text.as_str())
        .collect()
}

fn span_text(span: Option<MarkdownSourceSpan>, source: &str) -> &str {
    span.and_then(|span| span.source_text(source))
        .expect("source span should slice source")
}
