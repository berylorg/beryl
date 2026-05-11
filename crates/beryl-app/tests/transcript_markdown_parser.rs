#[path = "../src/shell/transcript_markdown.rs"]
mod transcript_markdown;

use transcript_markdown::{Block, Inline, ListKind, MarkdownSourceSpan, UnsupportedKind, parse};

#[test]
fn parses_common_assistant_markdown_into_transcript_ir() {
    let document = parse(
        "# Plan\n\
         \n\
         - *first* item\n\
         - **second** with `code`\n\
         \n\
         3. [read more](https://example.invalid/docs \"Docs\")\n\
         \n\
         > quoted text\n\
         \n\
         ```rust\n\
         fn main() {}\n\
         ```\n\
         \n\
         ---",
    )
    .expect("markdown should parse");

    let blocks = document.blocks();
    assert_eq!(blocks.len(), 6);

    let Block::Heading(heading) = &blocks[0] else {
        panic!("expected heading");
    };
    assert_eq!(heading.level().get(), 1);
    assert_eq!(heading.children(), &[Inline::Text("Plan".to_string())]);

    let Block::List(unordered) = &blocks[1] else {
        panic!("expected unordered list");
    };
    assert_eq!(unordered.kind(), ListKind::Unordered);
    assert!(unordered.tight());
    assert_eq!(unordered.items().len(), 2);

    let [Block::Paragraph(first_item)] = unordered.items()[0].blocks() else {
        panic!("expected first paragraph item");
    };
    assert_eq!(
        first_item.as_slice(),
        &[
            Inline::Emphasis(vec![Inline::Text("first".to_string())]),
            Inline::Text(" item".to_string()),
        ]
    );

    let [Block::Paragraph(second_item)] = unordered.items()[1].blocks() else {
        panic!("expected second paragraph item");
    };
    assert_eq!(
        second_item.as_slice(),
        &[
            Inline::Strong(vec![Inline::Text("second".to_string())]),
            Inline::Text(" with ".to_string()),
            Inline::Code("code".to_string()),
        ]
    );

    let Block::List(ordered) = &blocks[2] else {
        panic!("expected ordered list");
    };
    assert_eq!(ordered.kind(), ListKind::Ordered { start: 3 });

    let [Block::Paragraph(ordered_item)] = ordered.items()[0].blocks() else {
        panic!("expected ordered paragraph item");
    };
    let [Inline::Link(link)] = ordered_item.as_slice() else {
        panic!("expected link inline");
    };
    assert_eq!(link.destination(), "https://example.invalid/docs");
    assert_eq!(link.title(), Some("Docs"));
    assert_eq!(link.children(), &[Inline::Text("read more".to_string())]);

    let Block::BlockQuote(quote_blocks) = &blocks[3] else {
        panic!("expected block quote");
    };
    assert_eq!(
        quote_blocks.as_slice(),
        &[Block::Paragraph(vec![Inline::Text(
            "quoted text".to_string()
        )])]
    );

    let Block::Code(code) = &blocks[4] else {
        panic!("expected fenced code block");
    };
    assert_eq!(code.language(), Some("rust"));
    assert_eq!(code.source(), "fn main() {}");
    assert_eq!(blocks[5], Block::ThematicBreak);
}

#[test]
fn parses_breaks_math_images_and_raw_html_fallbacks() {
    let document = parse(
        "alpha\n\
         beta\\\n\
         gamma\n\
         \n\
         inline $$x + y$$ math and ![diagram](artifact://diagram.png \"Diagram\")\n\
         \n\
         $$\n\
         x^2\n\
         $$\n\
         \n\
         <div>\n\
         raw\n\
         </div>",
    )
    .expect("markdown should parse");

    let [
        Block::Paragraph(lines),
        Block::Paragraph(rich),
        Block::Math(math),
        Block::Unsupported(html),
    ] = document.blocks()
    else {
        panic!("expected paragraphs, math block, and html fallback");
    };

    assert_eq!(
        lines.as_slice(),
        &[
            Inline::Text("alpha".to_string()),
            Inline::SoftBreak,
            Inline::Text("beta".to_string()),
            Inline::HardBreak,
            Inline::Text("gamma".to_string()),
        ]
    );

    assert_eq!(rich.len(), 4);
    assert_eq!(rich[0], Inline::Text("inline ".to_string()));
    let Inline::Math(math_span) = &rich[1] else {
        panic!("expected inline math span");
    };
    assert_eq!(math_span.source(), "x + y");
    assert_eq!(rich[2], Inline::Text(" math and ".to_string()));
    let Inline::Image(image) = &rich[3] else {
        panic!("expected image reference");
    };
    assert_eq!(image.alt(), "diagram");
    assert_eq!(image.destination(), "artifact://diagram.png");
    assert_eq!(image.title(), Some("Diagram"));

    assert_eq!(math.source(), "x^2");
    assert_eq!(html.kind(), UnsupportedKind::Html);
    assert_eq!(html.source(), "<div>\nraw\n</div>");
}

#[test]
fn single_dollar_math_is_left_as_text_to_avoid_currency_collisions() {
    let document = parse("It costs $5 and $6 today.").expect("markdown should parse");

    let [Block::Paragraph(inlines)] = document.blocks() else {
        panic!("expected paragraph");
    };
    assert_eq!(
        inlines.as_slice(),
        &[Inline::Text("It costs $5 and $6 today.".to_string())]
    );
}

#[test]
fn reference_markdown_preserves_fallback_source_until_resolution_exists() {
    let document = parse(
        "[guide][docs]\n\
         \n\
         ![diagram][image]\n\
         \n\
         [docs]: https://example.invalid/docs \"Docs\"\n\
         [image]: artifact://diagram.png",
    )
    .expect("markdown should parse");

    let blocks = document.blocks();
    assert_eq!(blocks.len(), 4);

    let Block::Paragraph(link_ref) = &blocks[0] else {
        panic!("expected link reference paragraph");
    };
    let [Inline::Unsupported(link)] = link_ref.as_slice() else {
        panic!("expected unsupported link reference");
    };
    assert_eq!(link.kind(), UnsupportedKind::Reference);
    assert_eq!(link.source(), "[guide][docs]");

    let Block::Paragraph(image_ref) = &blocks[1] else {
        panic!("expected image reference paragraph");
    };
    let [Inline::Unsupported(image)] = image_ref.as_slice() else {
        panic!("expected unsupported image reference");
    };
    assert_eq!(image.kind(), UnsupportedKind::Reference);
    assert_eq!(image.source(), "![diagram][image]");

    for block in &blocks[2..] {
        let Block::Unsupported(definition) = block else {
            panic!("expected unsupported definition");
        };
        assert_eq!(definition.kind(), UnsupportedKind::Definition);
        assert!(definition.source().starts_with('['));
    }
}

#[test]
fn raw_html_source_fallback_uses_byte_offsets_for_non_ascii_text() {
    let document = parse("é <span>x</span>").expect("markdown should parse");

    let [Block::Paragraph(inlines)] = document.blocks() else {
        panic!("expected paragraph");
    };
    assert_eq!(inlines[0], Inline::Text("é ".to_string()));

    let Inline::Unsupported(open_tag) = &inlines[1] else {
        panic!("expected raw html open tag");
    };
    assert_eq!(open_tag.kind(), UnsupportedKind::Html);
    assert_eq!(open_tag.source(), "<span>");

    assert_eq!(inlines[2], Inline::Text("x".to_string()));

    let Inline::Unsupported(close_tag) = &inlines[3] else {
        panic!("expected raw html close tag");
    };
    assert_eq!(close_tag.kind(), UnsupportedKind::Html);
    assert_eq!(close_tag.source(), "</span>");
}

#[test]
fn incomplete_streaming_markdown_still_returns_useful_structure() {
    let fenced = parse("```rust\nfn main()").expect("unterminated fence should parse");
    let [Block::Code(code)] = fenced.blocks() else {
        panic!("expected code block for incomplete fence");
    };
    assert_eq!(code.language(), Some("rust"));
    assert_eq!(code.source(), "fn main()");

    let dangling = parse("This is **partial and [link").expect("dangling markdown should parse");
    let [Block::Paragraph(inlines)] = dangling.blocks() else {
        panic!("expected paragraph for dangling markdown");
    };
    assert_eq!(
        inlines.as_slice(),
        &[Inline::Text("This is **partial and [link".to_string())]
    );
}

#[test]
fn parser_records_source_spans_for_markdown_copy_semantics() {
    let source = concat!(
        "# Title\n\n",
        "- *first* and [docs](https://example.invalid)\n\n",
        "> quote\n\n",
        "```rust\n",
        "fn main() {}\n",
        "```",
    );
    let document = parse(source).expect("markdown should parse");
    let source_map = document.source_map();

    assert_eq!(span_text(source_map.block_span("b0"), source), "# Title");
    assert_eq!(span_text(source_map.inline_span("b0.i0"), source), "Title");
    assert_eq!(
        span_text(source_map.block_span("b1"), source),
        "- *first* and [docs](https://example.invalid)\n"
    );
    assert_eq!(
        span_text(source_map.list_item_span("b1.i0"), source),
        "- *first* and [docs](https://example.invalid)\n"
    );
    assert_eq!(
        span_text(source_map.block_span("b1.i0.b0"), source),
        "*first* and [docs](https://example.invalid)"
    );
    assert_eq!(
        span_text(source_map.inline_span("b1.i0.b0.i0"), source),
        "*first*"
    );
    assert_eq!(
        span_text(source_map.inline_span("b1.i0.b0.i2"), source),
        "[docs](https://example.invalid)"
    );
    assert_eq!(span_text(source_map.block_span("b2"), source), "> quote");
    assert_eq!(span_text(source_map.block_span("b2.q.b0"), source), "quote");
    assert_eq!(
        span_text(source_map.block_span("b3"), source),
        "```rust\nfn main() {}\n```"
    );
}

fn span_text(span: Option<MarkdownSourceSpan>, source: &str) -> &str {
    span.and_then(|span| span.source_text(source))
        .expect("source span should slice source")
}
