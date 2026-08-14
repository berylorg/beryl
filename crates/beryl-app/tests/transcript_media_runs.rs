mod shell {
    #[path = "../../src/shell/transcript_markdown.rs"]
    pub(crate) mod transcript_markdown;
    #[path = "../../src/shell/transcript_media.rs"]
    pub(crate) mod transcript_media;
    #[path = "../../src/shell/transcript_media_runs.rs"]
    pub(crate) mod transcript_media_runs;
}

use shell::transcript_media::{TranscriptMediaLoadOutcome, TranscriptMediaSource};
use shell::transcript_media_runs::{
    TranscriptMediaRunSegment, markdown_media_run_segments, media_run_copy_line,
};
use std::sync::Arc;

#[test]
fn one_markdown_image_in_prose_splits_to_text_media_text() {
    let segments = segments_for("Intro ![cat](images/cat.png) outro");

    assert_eq!(
        segments,
        vec![
            TranscriptMediaRunSegment::Markdown("Intro ".to_string()),
            media("cat", "images/cat.png"),
            TranscriptMediaRunSegment::Markdown(" outro".to_string()),
        ]
    );
}

#[test]
fn adjacent_markdown_images_form_one_logical_media_sequence() {
    let segments = segments_for("Look:\n![cat](cat.png)\n  ![hat](hat.png)\nDone.");

    assert_eq!(
        segments,
        vec![
            TranscriptMediaRunSegment::Markdown("Look:\n".to_string()),
            media("cat", "cat.png"),
            media("hat", "hat.png"),
            TranscriptMediaRunSegment::Markdown("\nDone.".to_string()),
        ]
    );
}

#[test]
fn text_between_images_splits_media_runs() {
    let segments = segments_for("![cat](cat.png) and then ![hat](hat.png)");

    assert_eq!(
        segments,
        vec![
            media("cat", "cat.png"),
            TranscriptMediaRunSegment::Markdown(" and then ".to_string()),
            media("hat", "hat.png"),
        ]
    );
}

#[test]
fn linked_image_stays_markdown_instead_of_media_run() {
    let source = "Before [![cat](images/cat.png)](details.md) after";

    assert_eq!(
        segments_for(source),
        vec![TranscriptMediaRunSegment::Markdown(source.to_string())]
    );
}

#[test]
fn emphasized_images_still_form_media_segments() {
    let segments = segments_for("*![cat](cat.png)* and **![hat](hat.png)**");

    assert_eq!(
        segments,
        vec![
            media("cat", "cat.png"),
            TranscriptMediaRunSegment::Markdown(" and ".to_string()),
            media("hat", "hat.png"),
        ]
    );
}

#[test]
fn emphasis_with_text_and_image_stays_markdown_to_avoid_broken_wrappers() {
    let source = "*see ![cat](cat.png)*";

    assert_eq!(
        segments_for(source),
        vec![TranscriptMediaRunSegment::Markdown(source.to_string())]
    );
}

#[test]
fn unsupported_markdown_parse_fallback_keeps_source_as_markdown() {
    let markdown = shell::transcript_markdown::TranscriptMarkdownCache::default()
        .lookup(
            shell::transcript_markdown::TranscriptMarkdownCacheKey::new("fallback"),
            "plain text",
        )
        .markdown;

    assert_eq!(
        markdown_media_run_segments(markdown.as_ref()),
        vec![TranscriptMediaRunSegment::Markdown(
            "plain text".to_string()
        )]
    );
}

#[test]
fn media_copy_line_preserves_markdown_image_source() {
    let source = TranscriptMediaSource::markdown_image(
        "cat",
        "images/cat.png",
        Some("Cheshire".to_string()),
    );
    let line = media_run_copy_line([(&source, None)]).expect("media line should exist");

    assert_eq!(line.display_text, "cat");
    assert_eq!(line.copy_text, "![cat](images/cat.png \"Cheshire\")");
}

#[test]
fn media_copy_line_uses_fallback_text_for_failed_markdown_image() {
    let source = TranscriptMediaSource::markdown_image("cat", "cat.svg", None);
    let outcome = TranscriptMediaLoadOutcome::RenderNotSupported {
        alt: "cat".to_string(),
    };
    let line = media_run_copy_line([(&source, Some(&outcome))]).expect("media line should exist");

    assert_eq!(line.display_text, "cat (render not supported)");
    assert_eq!(line.copy_text, "cat (render not supported)");
}

#[test]
fn media_copy_line_keeps_native_generated_image_prompt() {
    let source = TranscriptMediaSource::native_image_generation(
        "image_1",
        Some("A glass cat".to_string()),
        None::<Arc<String>>,
        None,
        false,
    );
    let outcome = TranscriptMediaLoadOutcome::Pending {
        alt: "A glass cat".to_string(),
    };
    let line = media_run_copy_line([(&source, Some(&outcome))]).expect("media line should exist");

    assert_eq!(line.display_text, "A glass cat");
    assert_eq!(line.copy_text, "A glass cat");
}

#[test]
fn media_copy_line_joins_consecutive_media_sources_in_order() {
    let cat = TranscriptMediaSource::markdown_image("cat", "cat.png", None);
    let hat = TranscriptMediaSource::markdown_image("hat", "hat.png", None);
    let line = media_run_copy_line([(&cat, None), (&hat, None)]).expect("media line should exist");

    assert_eq!(line.display_text, "cat hat");
    assert_eq!(line.copy_text, "![cat](cat.png)\n![hat](hat.png)");
}

fn segments_for(source: &str) -> Vec<TranscriptMediaRunSegment> {
    let document = shell::transcript_markdown::parse(source).expect("markdown should parse");
    let markdown =
        shell::transcript_markdown::ParsedTranscriptMarkdown::from_test_document(document, source);
    markdown_media_run_segments(&markdown)
}

fn media(alt: &str, destination: &str) -> TranscriptMediaRunSegment {
    TranscriptMediaRunSegment::Media(TranscriptMediaSource::markdown_image(
        alt,
        destination,
        None,
    ))
}
