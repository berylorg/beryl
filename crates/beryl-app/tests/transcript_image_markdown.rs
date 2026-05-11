#[path = "../src/shell/execution_detail.rs"]
mod execution_detail;
#[path = "../src/shell/render/transcript/image_markdown.rs"]
mod image_markdown;
#[path = "../src/shell/transcript_markdown.rs"]
mod transcript_markdown;

mod shell {
    pub mod execution_detail {
        pub(crate) use crate::execution_detail::*;
    }
}

use execution_detail::{
    TranscriptImageMarkerSpec, TranscriptImagePreviewState, UserInputFragment,
    transcript_image_source_from_local_image,
};
use image_markdown::markdown_source_with_image_marker_placeholders;
use transcript_markdown::{BlockRenderNode, block_render_plan_with_copy_source, parse};

#[test]
fn image_marker_placeholder_prevents_marker_from_becoming_markdown_link_text() {
    let fragment = image_fragment("See [A](url)", 4..7);
    let markdown_source = markdown_source_with_image_marker_placeholders(
        fragment.text.as_str(),
        fragment.image_markers(),
    );

    assert_eq!(markdown_source, "See {A}(url)");

    let document = parse(&markdown_source).expect("placeholder source parses");
    let plan = block_render_plan_with_copy_source(&document, &markdown_source);
    let BlockRenderNode::Paragraph { lines, .. } = &plan.blocks[0] else {
        panic!("expected paragraph");
    };
    assert_eq!(line_text(&lines[0]), "See {A}(url)");
}

#[test]
fn image_marker_placeholder_keeps_code_block_marker_source_addressable() {
    let fragment = image_fragment("```\n[A]\n```", 4..7);
    let markdown_source = markdown_source_with_image_marker_placeholders(
        fragment.text.as_str(),
        fragment.image_markers(),
    );

    assert_eq!(markdown_source, "```\n{A}\n```");

    let document = parse(&markdown_source).expect("placeholder source parses");
    let plan = block_render_plan_with_copy_source(&document, &markdown_source);
    let BlockRenderNode::Code(code) = &plan.blocks[0] else {
        panic!("expected code block");
    };
    assert_eq!(code.source, "{A}");
    assert_eq!(
        code.content_source_span.map(|span| span.range()),
        Some(4..7)
    );
}

fn image_fragment(text: &str, marker_range: std::ops::Range<usize>) -> UserInputFragment {
    UserInputFragment::from_backend_input_with_image_markers(
        text.to_string(),
        Vec::new(),
        vec![TranscriptImageMarkerSpec::new(
            "A",
            marker_range,
            transcript_image_source_from_local_image(
                "/tmp/a.png",
                Some("asset-a".to_string()),
                TranscriptImagePreviewState::Available,
            ),
        )],
    )
}

fn line_text(line: &transcript_markdown::InlineRenderLine) -> String {
    line.fragments
        .iter()
        .map(|fragment| fragment.text.as_str())
        .collect()
}
