#[path = "../src/shell/transcript_markdown.rs"]
mod transcript_markdown;

use transcript_markdown::{
    Block, Document, HeadingLevel, Inline, List, ListItem, ListKind, UnsupportedKind,
    normalize_line_endings,
};

#[test]
fn line_endings_normalize_without_collapsing_newlines() {
    assert_eq!(
        normalize_line_endings("alpha\r\nbeta\rgamma\n\n"),
        "alpha\nbeta\ngamma\n\n"
    );

    let document = Document::new(vec![Block::paragraph(vec![Inline::text(
        "one\r\ntwo\rthree",
    )])]);

    let [Block::Paragraph(children)] = document.blocks() else {
        panic!("expected one paragraph");
    };
    assert_eq!(
        children.as_slice(),
        &[Inline::Text("one\ntwo\nthree".to_string())]
    );
}

#[test]
fn soft_and_hard_breaks_are_distinct_inline_nodes() {
    let paragraph = Block::paragraph(vec![
        Inline::text("alpha"),
        Inline::soft_break(),
        Inline::text("beta"),
        Inline::hard_break(),
        Inline::text("gamma"),
    ]);
    let document = Document::new(vec![paragraph]);

    let [Block::Paragraph(children)] = document.blocks() else {
        panic!("expected one paragraph");
    };
    assert_eq!(
        children.as_slice(),
        &[
            Inline::Text("alpha".to_string()),
            Inline::SoftBreak,
            Inline::Text("beta".to_string()),
            Inline::HardBreak,
            Inline::Text("gamma".to_string()),
        ]
    );
}

#[test]
fn document_shape_keeps_headings_lists_quotes_links_and_code_metadata() {
    let heading_level = HeadingLevel::new(2).expect("level two is valid");
    assert!(HeadingLevel::new(0).is_none());
    assert!(HeadingLevel::new(7).is_none());

    let document = Document::new(vec![
        Block::heading(
            heading_level,
            vec![Inline::emphasis(vec![Inline::text("Transcript")])],
        ),
        Block::list(List::ordered(
            3,
            false,
            vec![ListItem::new(vec![Block::paragraph(vec![Inline::link(
                "https://example.invalid/docs",
                Some("Docs".to_string()),
                vec![Inline::strong(vec![Inline::text("read more")])],
            )])])],
        )),
        Block::block_quote(vec![Block::paragraph(vec![Inline::text("quoted")])]),
        Block::code_block(
            Some("rust".to_string()),
            Some("ignore\rmeta".to_string()),
            "fn main() {\r\n}\r",
        ),
        Block::ThematicBreak,
    ]);

    let blocks = document.blocks();
    assert_eq!(blocks.len(), 5);

    let Block::Heading(heading) = &blocks[0] else {
        panic!("expected heading");
    };
    assert_eq!(heading.level().get(), 2);
    assert_eq!(
        heading.children(),
        &[Inline::Emphasis(vec![Inline::Text(
            "Transcript".to_string()
        )])]
    );

    let Block::List(list) = &blocks[1] else {
        panic!("expected ordered list");
    };
    assert_eq!(list.kind(), ListKind::Ordered { start: 3 });
    assert!(!list.tight());
    assert_eq!(list.items().len(), 1);

    let [Block::Paragraph(item_children)] = list.items()[0].blocks() else {
        panic!("expected paragraph list item");
    };
    let [Inline::Link(link)] = item_children.as_slice() else {
        panic!("expected link inline");
    };
    assert_eq!(link.destination(), "https://example.invalid/docs");
    assert_eq!(link.title(), Some("Docs"));
    assert_eq!(
        link.children(),
        &[Inline::Strong(vec![Inline::Text("read more".to_string())])]
    );

    let Block::BlockQuote(quote_blocks) = &blocks[2] else {
        panic!("expected block quote");
    };
    assert_eq!(
        quote_blocks.as_slice(),
        &[Block::Paragraph(vec![Inline::Text("quoted".to_string())])]
    );

    let Block::Code(code_block) = &blocks[3] else {
        panic!("expected code block");
    };
    assert_eq!(code_block.language(), Some("rust"));
    assert_eq!(code_block.meta(), Some("ignore\nmeta"));
    assert_eq!(code_block.source(), "fn main() {\n}\n");
    assert_eq!(blocks[4], Block::ThematicBreak);
}

#[test]
fn unordered_list_metadata_is_preserved() {
    let list = List::unordered(
        true,
        vec![
            ListItem::new(vec![Block::paragraph(vec![Inline::text("first")])]),
            ListItem::new(vec![Block::paragraph(vec![Inline::text("second")])]),
        ],
    );

    assert_eq!(list.kind(), ListKind::Unordered);
    assert!(list.tight());
    assert_eq!(list.items().len(), 2);
}

#[test]
fn math_nodes_are_distinct_from_text_for_future_renderers() {
    let document = Document::new(vec![
        Block::paragraph(vec![Inline::text("Area "), Inline::math("a\r\nb")]),
        Block::math_block("\\int_0^1 x^2\r\ndx"),
    ]);

    let [Block::Paragraph(inlines), Block::Math(math_block)] = document.blocks() else {
        panic!("expected inline math and block math");
    };
    let [Inline::Text(prefix), Inline::Math(math_span)] = inlines.as_slice() else {
        panic!("expected text followed by inline math");
    };

    assert_eq!(prefix, "Area ");
    assert_eq!(math_span.source(), "a\nb");
    assert_eq!(math_block.source(), "\\int_0^1 x^2\ndx");
}

#[test]
fn image_nodes_preserve_metadata_without_becoming_html() {
    let inline = Inline::image(
        "diagram\r\nalt",
        "artifact://diagram.png",
        Some("Diagram\rTitle".to_string()),
    );

    let Inline::Image(image) = inline else {
        panic!("expected image inline");
    };
    assert_eq!(image.alt(), "diagram\nalt");
    assert_eq!(image.destination(), "artifact://diagram.png");
    assert_eq!(image.title(), Some("Diagram\nTitle"));
}

#[test]
fn document_image_requests_preserve_standalone_markdown_order_and_source_spans() {
    let source = "Intro ![first](images/a.png) and [![second](images/b.png)](target).";
    let document = transcript_markdown::parse(source).expect("markdown should parse");

    let requests = document.image_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].alt(), "first");
    assert_eq!(requests[0].destination(), "images/a.png");
    assert_eq!(
        requests[0]
            .source_span()
            .and_then(|span| span.source_text(source)),
        Some("![first](images/a.png)")
    );
}

#[test]
fn raw_html_is_unsupported_source_not_renderable_html() {
    let document = Document::new(vec![
        Block::unsupported(UnsupportedKind::Html, "<section>\r\nhello</section>"),
        Block::paragraph(vec![Inline::unsupported(
            UnsupportedKind::Html,
            "<span>inline</span>",
        )]),
    ]);

    let [Block::Unsupported(block), Block::Paragraph(inlines)] = document.blocks() else {
        panic!("expected unsupported html nodes");
    };
    assert_eq!(block.kind(), UnsupportedKind::Html);
    assert_eq!(block.source(), "<section>\nhello</section>");

    let [Inline::Unsupported(inline)] = inlines.as_slice() else {
        panic!("expected unsupported inline html");
    };
    assert_eq!(inline.kind(), UnsupportedKind::Html);
    assert_eq!(inline.source(), "<span>inline</span>");
}
