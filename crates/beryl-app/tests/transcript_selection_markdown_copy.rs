#![allow(dead_code)]

#[path = "../src/shell/transcript_selection.rs"]
mod transcript_selection;

use transcript_selection::{
    TranscriptLineCopyGroup, TranscriptLineCopyText, TranscriptSelectionState,
    TranscriptTextLineKey, TranscriptTextPoint, VisibleTranscriptTextFrame,
    VisibleTranscriptTextLine,
};

fn key(row: &str, block: &str, line: usize) -> TranscriptTextLineKey {
    TranscriptTextLineKey::new(row, block, line)
}

fn point(key: TranscriptTextLineKey, offset: usize) -> TranscriptTextPoint {
    TranscriptTextPoint::new(key, offset)
}

fn frame_with_copy_text(
    lines: impl IntoIterator<
        Item = (
            TranscriptTextLineKey,
            usize,
            &'static str,
            TranscriptLineCopyText,
            usize,
        ),
    >,
) -> VisibleTranscriptTextFrame {
    let mut frame = VisibleTranscriptTextFrame::default();
    for (key, order, text, copy_text, break_before) in lines {
        frame.insert_line(VisibleTranscriptTextLine::with_copy_text(
            key,
            order,
            text,
            copy_text,
            break_before,
        ));
    }
    frame.finish_insertions();
    frame
}

#[test]
fn selected_text_wraps_partial_emphasis_strong_and_link_segments() {
    let line = key("row", "assistant", 0);
    let mut copy_text = TranscriptLineCopyText::default();
    copy_text.push_wrapped_run("em".to_string(), "*".to_string(), "*".to_string());
    copy_text.push_plain_run(" ".to_string());
    copy_text.push_wrapped_run("strong".to_string(), "**".to_string(), "**".to_string());
    copy_text.push_plain_run(" ".to_string());
    copy_text.push_wrapped_run(
        "docs".to_string(),
        "[".to_string(),
        "](https://example.invalid)".to_string(),
    );
    let frame = frame_with_copy_text([(line.clone(), 0, "em strong docs", copy_text, 1)]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(line.clone(), 1), &frame);
    selection.extend(point(line, "em strong docs".len()), &frame);

    assert_eq!(
        selection.selected_text(),
        Some("*m* **strong** [docs](https://example.invalid)")
    );
}

#[test]
fn selected_text_preserves_markdown_across_block_kinds() {
    let heading = key("row", "assistant", 0);
    let paragraph = key("row", "assistant", 1);
    let item = key("row", "assistant", 2);

    let heading_copy = TranscriptLineCopyText::plain("Title".to_string())
        .with_prefixes(String::new(), "# ".to_string());
    let mut paragraph_copy = TranscriptLineCopyText::default();
    paragraph_copy.push_plain_run("Hello, ".to_string());
    paragraph_copy.push_wrapped_run("world".to_string(), "`".to_string(), "`".to_string());
    let item_copy = TranscriptLineCopyText::plain("item".to_string())
        .with_prefixes(String::new(), "2. ".to_string());
    let frame = frame_with_copy_text([
        (heading.clone(), 0, "Title", heading_copy, 1),
        (paragraph.clone(), 1, "Hello, world", paragraph_copy, 2),
        (item.clone(), 2, "item", item_copy, 2),
    ]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(heading.clone(), 0), &frame);
    selection.extend(point(item.clone(), "item".len()), &frame);

    assert_eq!(
        selection.selected_text(),
        Some("# Title\n\nHello, `world`\n\n2. item")
    );
    let ranges = selection.selected_line_ranges(&frame);
    assert_eq!(ranges[0].key, heading);
    assert_eq!(ranges[0].start, 0);
    assert_eq!(ranges[0].end, "Title".len());
    assert_eq!(ranges[1].key, paragraph);
    assert_eq!(ranges[1].start, 0);
    assert_eq!(ranges[1].end, "Hello, world".len());
}

#[test]
fn selected_text_adds_fenced_markdown_for_code_block_groups() {
    let before = key("row", "assistant", 0);
    let code = key("row", "assistant", 1);
    let after = key("row", "assistant", 2);
    let code_group = TranscriptLineCopyGroup::new("row:code", "```rust", "```");
    let code_copy =
        TranscriptLineCopyText::plain("fn main() {}".to_string()).with_group(code_group);
    let frame = frame_with_copy_text([
        (
            before.clone(),
            0,
            "before",
            TranscriptLineCopyText::plain("before".to_string()),
            0,
        ),
        (code.clone(), 1, "fn main() {}", code_copy, 2),
        (
            after.clone(),
            2,
            "after",
            TranscriptLineCopyText::plain("after".to_string()),
            2,
        ),
    ]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(before, 0), &frame);
    selection.extend(point(after, "after".len()), &frame);

    assert_eq!(
        selection.selected_text(),
        Some("before\n\n```rust\nfn main() {}\n```\n\nafter")
    );
    let ranges = selection.selected_line_ranges(&frame);
    assert_eq!(ranges[1].key, code);
    assert_eq!(ranges[1].start, 0);
    assert_eq!(ranges[1].end, "fn main() {}".len());
}

#[test]
fn selected_text_adds_fences_for_partial_code_block_group_selection() {
    let code = key("row", "assistant", 0);
    let code_group = TranscriptLineCopyGroup::new("row:code", "```rust", "```");
    let code_copy = TranscriptLineCopyText::plain("abcdef".to_string()).with_group(code_group);
    let frame = frame_with_copy_text([(code.clone(), 0, "abcdef", code_copy, 1)]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(code.clone(), 2), &frame);
    selection.extend(point(code.clone(), 5), &frame);

    assert_eq!(selection.selected_text(), Some("```rust\ncde\n```"));
    let ranges = selection.selected_line_ranges(&frame);
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].key, code);
    assert_eq!(ranges[0].start, 2);
    assert_eq!(ranges[0].end, 5);
}

#[test]
fn selected_text_keeps_soft_wrapped_code_segments_inside_one_fence() {
    let first = key("row", "assistant", 0);
    let second = key("row", "assistant", 1);
    let third = key("row", "assistant", 2);
    let fourth = key("row", "assistant", 3);
    let code_group = TranscriptLineCopyGroup::new("row:code", "```text", "```");
    let frame = frame_with_copy_text([
        (
            first.clone(),
            0,
            "ab",
            TranscriptLineCopyText::plain("ab".to_string()).with_group(code_group.clone()),
            1,
        ),
        (
            second.clone(),
            1,
            "cd",
            TranscriptLineCopyText::plain("cd".to_string()).with_group(code_group.clone()),
            0,
        ),
        (
            third.clone(),
            2,
            "ef",
            TranscriptLineCopyText::plain("ef".to_string()).with_group(code_group.clone()),
            0,
        ),
        (
            fourth.clone(),
            3,
            "gh",
            TranscriptLineCopyText::plain("gh".to_string()).with_group(code_group),
            1,
        ),
    ]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(first, 0), &frame);
    selection.extend(point(fourth, "gh".len()), &frame);

    assert_eq!(selection.selected_text(), Some("```text\nabcdef\ngh\n```"));
}

#[test]
fn selection_retains_markdown_copy_text_when_endpoint_leaves_visible_frame() {
    let first = key("row-a", "assistant", 0);
    let second = key("row-b", "assistant", 0);
    let mut second_copy = TranscriptLineCopyText::default();
    second_copy.push_wrapped_run("world".to_string(), "`".to_string(), "`".to_string());
    let full_frame = frame_with_copy_text([
        (
            first.clone(),
            0,
            "Hello",
            TranscriptLineCopyText::plain("Hello".to_string()),
            1,
        ),
        (second.clone(), 1, "world", second_copy, 1),
    ]);
    let partial_frame = frame_with_copy_text([(
        first.clone(),
        0,
        "Hello",
        TranscriptLineCopyText::plain("Hello".to_string()),
        1,
    )]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(first.clone(), 0), &full_frame);
    selection.extend(point(second, "world".len()), &full_frame);

    assert_eq!(selection.selected_text(), Some("Hello\n`world`"));
    assert!(!selection.sync_visible_frame(&partial_frame));
    assert_eq!(selection.selected_text(), Some("Hello\n`world`"));

    let ranges = selection.selected_line_ranges(&partial_frame);
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].key, first);
}

#[test]
fn selected_text_preserves_markdown_image_source_between_text_segments() {
    let before = key("row", "assistant:before", 0);
    let media = key("row", "media-run:1", 0);
    let after = key("row", "assistant:after", 0);
    let mut media_copy = TranscriptLineCopyText::default();
    media_copy.push_atomic_run("cat".to_string(), "![cat](images/cat.png)".to_string());
    let frame = frame_with_copy_text([
        (
            before.clone(),
            0,
            "Before",
            TranscriptLineCopyText::plain("Before".to_string()),
            1,
        ),
        (media.clone(), 1, "cat", media_copy, 2),
        (
            after.clone(),
            2,
            "After",
            TranscriptLineCopyText::plain("After".to_string()),
            2,
        ),
    ]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(before, 0), &frame);
    selection.extend(point(after, "After".len()), &frame);

    assert_eq!(
        selection.selected_text(),
        Some("Before\n\n![cat](images/cat.png)\n\nAfter")
    );
    let ranges = selection.selected_line_ranges(&frame);
    assert_eq!(ranges.len(), 3);
    assert_eq!(ranges[1].key, media);
    assert_eq!(ranges[1].start, 0);
    assert_eq!(ranges[1].end, "cat".len());
}
