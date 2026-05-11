#![allow(dead_code)]

use std::collections::HashSet;

#[path = "../src/shell/transcript_selection.rs"]
#[allow(dead_code)]
mod transcript_selection;

use transcript_selection::{
    TranscriptLineCopyText, TranscriptSelectionState, TranscriptTextLineKey, TranscriptTextPoint,
    VisibleTranscriptTextFrame, VisibleTranscriptTextLine, transcript_context_line_break_before,
    transcript_narrative_block_break_before, vertical_hit_candidate_range,
};

fn key(row: &str, block: &str, line: usize) -> TranscriptTextLineKey {
    TranscriptTextLineKey::new(row, block, line)
}

fn point(key: TranscriptTextLineKey, offset: usize) -> TranscriptTextPoint {
    TranscriptTextPoint::new(key, offset)
}

fn frame(
    lines: impl IntoIterator<Item = (TranscriptTextLineKey, usize, &'static str, usize)>,
) -> VisibleTranscriptTextFrame {
    let mut frame = VisibleTranscriptTextFrame::default();
    for (key, order, text, break_before) in lines {
        frame.insert_line(VisibleTranscriptTextLine::new(
            key,
            order,
            text,
            break_before,
        ));
    }
    frame.finish_insertions();
    frame
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
fn selected_text_uses_visible_order_not_key_order() {
    let first = key("row-b", "assistant", 0);
    let second = key("row-a", "assistant", 0);
    let frame = frame([
        (second.clone(), 20, "second", 1),
        (first.clone(), 10, "first", 1),
    ]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(first, 0), &frame);
    selection.extend(point(second, "second".len()), &frame);

    assert_eq!(selection.selected_text(), Some("first\nsecond"));
}

#[test]
fn selected_text_normalizes_reverse_drag() {
    let first = key("row", "assistant", 0);
    let second = key("row", "assistant", 1);
    let frame = frame([
        (first.clone(), 0, "alpha", 1),
        (second.clone(), 1, "beta", 1),
    ]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(second, 4), &frame);
    selection.extend(point(first, 2), &frame);

    assert_eq!(selection.selected_text(), Some("pha\nbeta"));
}

#[test]
fn selected_text_slices_single_line_and_clamps_character_boundary() {
    let line = key("row", "assistant", 0);
    let frame = frame([(line.clone(), 0, "\u{e9}clair", 1)]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(line.clone(), 1), &frame);
    selection.extend(point(line, "\u{e9}cl".len()), &frame);

    assert_eq!(selection.selected_text(), Some("\u{e9}cl"));
}

#[test]
fn word_selection_selects_word_under_point() {
    let line = key("row", "assistant", 0);
    let frame = frame([(line.clone(), 0, "Hello, world", 1)]);
    let mut selection = TranscriptSelectionState::default();

    selection.select_word(point(line.clone(), "Hello, wo".len()), &frame);

    assert_eq!(selection.selected_text(), Some("world"));
    assert!(!selection.is_dragging());
    let ranges = selection.selected_line_ranges(&frame);
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].key, line);
    assert_eq!(ranges[0].start, "Hello, ".len());
    assert_eq!(ranges[0].end, "Hello, world".len());
}

#[test]
fn word_selection_uses_markdown_copy_text_for_selected_word() {
    let line = key("row", "assistant", 0);
    let mut copy_text = TranscriptLineCopyText::default();
    copy_text.push_plain_run("Hello, ".to_string());
    copy_text.push_wrapped_run("world".to_string(), "`".to_string(), "`".to_string());
    let frame = frame_with_copy_text([(line.clone(), 0, "Hello, world", copy_text, 1)]);
    let mut selection = TranscriptSelectionState::default();

    selection.select_word(point(line, "Hello, wo".len()), &frame);

    assert_eq!(selection.selected_text(), Some("`world`"));
}

#[test]
fn word_selection_clears_when_point_is_on_whitespace() {
    let line = key("row", "assistant", 0);
    let frame = frame([(line.clone(), 0, "Hello world", 1)]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(line.clone(), 0), &frame);
    selection.extend(point(line.clone(), "Hello".len()), &frame);
    assert_eq!(selection.selected_text(), Some("Hello"));

    assert!(selection.select_word(point(line, "Hello".len()), &frame));
    assert_eq!(selection.selected_text(), None);
}

#[test]
fn word_selection_expands_intersecting_atomic_marker() {
    let line = key("row", "user", 0);
    let mut copy_text = TranscriptLineCopyText::default();
    copy_text.push_plain_run("Look ".to_string());
    copy_text.push_atomic_run("[A]".to_string(), "[Image A]".to_string());
    copy_text.push_plain_run(" here".to_string());
    let frame = frame_with_copy_text([(line.clone(), 0, "Look [A] here", copy_text, 1)]);
    let mut selection = TranscriptSelectionState::default();

    selection.select_word(point(line.clone(), "Look [".len()), &frame);

    assert_eq!(selection.selected_text(), Some("[Image A]"));
    let ranges = selection.selected_line_ranges(&frame);
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].key, line);
    assert_eq!(ranges[0].start, "Look ".len());
    assert_eq!(ranges[0].end, "Look [A]".len());
}

#[test]
fn selected_text_preserves_requested_block_breaks() {
    let first = key("row", "assistant", 0);
    let second = key("row", "assistant", 1);
    let third = key("row", "assistant", 2);
    let frame = frame([
        (first.clone(), 0, "paragraph", 0),
        (second.clone(), 1, "next paragraph", 2),
        (third.clone(), 2, "- item", 1),
    ]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(first, 0), &frame);
    selection.extend(point(third, "- item".len()), &frame);

    assert_eq!(
        selection.selected_text(),
        Some("paragraph\n\nnext paragraph\n- item")
    );
}

#[test]
fn selected_text_preserves_narrative_block_breaks() {
    let first = key("row", "item:first", 0);
    let second = key("row", "item:second", 0);
    let continuation = key("row", "item:second", 1);
    let frame = frame([
        (first.clone(), 0, "first paragraph", 0),
        (
            second.clone(),
            1,
            "second paragraph ",
            transcript_narrative_block_break_before(1),
        ),
        (continuation.clone(), 2, "continued", 0),
    ]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(first, 0), &frame);
    selection.extend(point(continuation, "continued".len()), &frame);

    assert_eq!(
        selection.selected_text(),
        Some("first paragraph\n\nsecond paragraph continued")
    );
}

#[test]
fn context_line_break_policy_preserves_soft_wrap_continuations() {
    assert_eq!(transcript_narrative_block_break_before(0), 0);
    assert_eq!(transcript_narrative_block_break_before(1), 2);
    assert_eq!(transcript_context_line_break_before(0, 2, Some(0)), 2);
    assert_eq!(transcript_context_line_break_before(1, 1, Some(0)), 0);
}

#[test]
fn selected_text_concatenates_soft_wrapped_segments_without_newlines() {
    let first = key("row", "code", 0);
    let continuation = key("row", "code", 1);
    let next_line = key("row", "code", 2);
    let frame = frame([
        (first.clone(), 0, "alpha ", 0),
        (continuation.clone(), 1, "beta", 0),
        (next_line.clone(), 2, "gamma", 1),
    ]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(first, 0), &frame);
    selection.extend(point(next_line, "gamma".len()), &frame);

    assert_eq!(selection.selected_text(), Some("alpha beta\ngamma"));
}

#[test]
fn selected_text_uses_markdown_copy_text_without_changing_display_ranges() {
    let line = key("row", "assistant", 0);
    let mut copy_text = TranscriptLineCopyText::default();
    copy_text.push_plain_run("Hello, ".to_string());
    copy_text.push_wrapped_run("world".to_string(), "`".to_string(), "`".to_string());
    let frame = frame_with_copy_text([(line.clone(), 0, "Hello, world", copy_text, 1)]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(line.clone(), "Hello, ".len()), &frame);
    selection.extend(point(line.clone(), "Hello, world".len()), &frame);

    assert_eq!(selection.selected_text(), Some("`world`"));
    let ranges = selection.selected_line_ranges(&frame);
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].key, line);
    assert_eq!(ranges[0].start, "Hello, ".len());
    assert_eq!(ranges[0].end, "Hello, world".len());
}

#[test]
fn selected_text_treats_atomic_image_marker_as_replacement_text() {
    let line = key("row", "user", 0);
    let mut copy_text = TranscriptLineCopyText::default();
    copy_text.push_plain_run("Look ".to_string());
    copy_text.push_atomic_run("[A]".to_string(), "[Image A]".to_string());
    copy_text.push_plain_run(" here".to_string());
    let frame = frame_with_copy_text([(line.clone(), 0, "Look [A] here", copy_text, 1)]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(line.clone(), "Look [".len()), &frame);
    selection.extend(point(line.clone(), "Look [A".len()), &frame);

    assert_eq!(selection.selected_text(), Some("[Image A]"));
    let ranges = selection.selected_line_ranges(&frame);
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].key, line);
    assert_eq!(ranges[0].start, "Look ".len());
    assert_eq!(ranges[0].end, "Look [A]".len());
}

#[test]
fn selected_text_preserves_markdown_wrappers_around_image_marker_replacements() {
    let line = key("row", "user", 0);
    let mut copy_text = TranscriptLineCopyText::default();
    copy_text.push_wrapped_run_with_atomic_replacements(
        "see [A] now".to_string(),
        "**".to_string(),
        "**".to_string(),
        [(4..7, "[Image A]".to_string())],
    );
    let frame = frame_with_copy_text([(line.clone(), 0, "see [A] now", copy_text, 1)]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(line.clone(), 0), &frame);
    selection.extend(point(line, "see [A] now".len()), &frame);

    assert_eq!(selection.selected_text(), Some("**see [Image A] now**"));
}

#[test]
fn selected_text_orders_unsorted_atomic_replacements_by_display_position() {
    let line = key("row", "user", 0);
    let mut copy_text = TranscriptLineCopyText::default();
    copy_text.push_wrapped_run_with_atomic_replacements(
        "[A] then [B]".to_string(),
        String::new(),
        String::new(),
        [
            (9..12, "[Image B]".to_string()),
            (0..3, "[Image A]".to_string()),
        ],
    );
    let frame = frame_with_copy_text([(line.clone(), 0, "[A] then [B]", copy_text, 1)]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(line.clone(), 0), &frame);
    selection.extend(point(line, "[A] then [B]".len()), &frame);

    assert_eq!(selection.selected_text(), Some("[Image A] then [Image B]"));
}

#[test]
fn selection_retains_atomic_marker_copy_when_endpoint_leaves_visible_frame() {
    let marker = key("row", "user", 0);
    let other = key("row", "assistant", 0);
    let mut copy_text = TranscriptLineCopyText::default();
    copy_text.push_plain_run("Look ".to_string());
    copy_text.push_atomic_run("[A]".to_string(), "[Image A]".to_string());
    let frame = frame_with_copy_text([
        (marker.clone(), 0, "Look [A]", copy_text, 1),
        (
            other.clone(),
            1,
            "done",
            TranscriptLineCopyText::plain("done".to_string()),
            2,
        ),
    ]);
    let visible_marker_only = frame_with_copy_text([(
        marker.clone(),
        0,
        "Look [A]",
        {
            let mut copy_text = TranscriptLineCopyText::default();
            copy_text.push_plain_run("Look ".to_string());
            copy_text.push_atomic_run("[A]".to_string(), "[Image A]".to_string());
            copy_text
        },
        1,
    )]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(marker.clone(), "Look ".len()), &frame);
    selection.extend(point(other, "done".len()), &frame);
    selection.sync_visible_frame(&visible_marker_only);

    assert_eq!(selection.selected_text(), Some("[Image A]\n\ndone"));
    let ranges = selection.selected_line_ranges(&visible_marker_only);
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].key, marker);
    assert_eq!(ranges[0].start, "Look ".len());
    assert_eq!(ranges[0].end, "Look [A]".len());
}

#[test]
fn selected_text_wraps_partial_inline_markdown_copy_segments() {
    let line = key("row", "assistant", 0);
    let mut copy_text = TranscriptLineCopyText::default();
    copy_text.push_wrapped_run("world".to_string(), "`".to_string(), "`".to_string());
    let frame = frame_with_copy_text([(line.clone(), 0, "world", copy_text, 1)]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(line.clone(), 1), &frame);
    selection.extend(point(line, 4), &frame);

    assert_eq!(selection.selected_text(), Some("`orl`"));
}

#[test]
fn selected_text_preserves_line_prefixes_for_markdown_blocks() {
    let quote = key("row", "assistant", 0);
    let item = key("row", "assistant", 1);
    let quote_copy = TranscriptLineCopyText::plain("quote".to_string())
        .with_prefixes("> ".to_string(), String::new());
    let item_copy = TranscriptLineCopyText::plain("item".to_string())
        .with_prefixes(String::new(), "- ".to_string());
    let frame = frame_with_copy_text([
        (quote.clone(), 0, "quote", quote_copy, 1),
        (item.clone(), 1, "item", item_copy, 1),
    ]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(quote, 1), &frame);
    selection.extend(point(item, "item".len()), &frame);

    assert_eq!(selection.selected_text(), Some("> uote\n- item"));
}

#[test]
fn selection_retains_text_when_endpoint_leaves_visible_frame() {
    let first = key("row-a", "assistant", 0);
    let second = key("row-b", "assistant", 0);
    let full_frame = frame([
        (first.clone(), 0, "first", 1),
        (second.clone(), 1, "second", 1),
    ]);
    let partial_frame = frame([(first.clone(), 0, "first", 1)]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(first.clone(), 0), &full_frame);
    selection.extend(point(second, "second".len()), &full_frame);

    assert_eq!(selection.selected_text(), Some("first\nsecond"));
    assert!(!selection.sync_visible_frame(&partial_frame));
    assert_eq!(selection.selected_text(), Some("first\nsecond"));

    let ranges = selection.selected_line_ranges(&partial_frame);
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].key, first);
    assert_eq!(ranges[0].start, 0);
    assert_eq!(ranges[0].end, "first".len());
}

#[test]
fn selection_clears_when_selected_row_leaves_loaded_content() {
    let first = key("row-a", "assistant", 0);
    let second = key("row-b", "assistant", 0);
    let frame = frame([
        (first.clone(), 0, "first", 1),
        (second.clone(), 1, "second", 1),
    ]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(first, 0), &frame);
    selection.extend(point(second, "second".len()), &frame);

    assert_eq!(selection.selected_text(), Some("first\nsecond"));
    assert!(selection.clear_if_intersects_row_identities(&HashSet::from(["row-b".to_string()])));
    assert_eq!(selection.selected_text(), None);
}

#[test]
fn selected_line_ranges_cover_display_offsets() {
    let first = key("row", "assistant", 0);
    let second = key("row", "assistant", 1);
    let frame = frame([
        (first.clone(), 0, "> quoted", 1),
        (second.clone(), 1, "tail", 1),
    ]);
    let mut selection = TranscriptSelectionState::default();

    selection.begin(point(first.clone(), 2), &frame);
    selection.extend(point(second.clone(), 2), &frame);

    let ranges = selection.selected_line_ranges(&frame);
    assert_eq!(ranges.len(), 2);
    assert_eq!(ranges[0].key, first);
    assert_eq!(ranges[0].start, 2);
    assert_eq!(ranges[0].end, "> quoted".len());
    assert_eq!(ranges[1].key, second);
    assert_eq!(ranges[1].start, 0);
    assert_eq!(ranges[1].end, 2);
}

#[test]
fn frame_sorts_once_after_batched_line_registration() {
    let first = key("row", "assistant", 0);
    let second = key("row", "assistant", 1);
    let mut frame = VisibleTranscriptTextFrame::default();

    frame.insert_line(VisibleTranscriptTextLine::new(
        second.clone(),
        20,
        "second",
        1,
    ));
    frame.insert_line(VisibleTranscriptTextLine::new(
        first.clone(),
        10,
        "first",
        1,
    ));
    frame.finish_insertions();

    let mut selection = TranscriptSelectionState::default();
    selection.begin(point(first, 0), &frame);
    selection.extend(point(second, "second".len()), &frame);

    assert_eq!(selection.selected_text(), Some("first\nsecond"));
}

#[test]
fn vertical_hit_candidate_range_skips_lines_above_pointer() {
    let ranges = [(0, 9), (10, 19), (20, 29), (30, 39), (40, 49), (50, 59)];

    assert_eq!(
        vertical_hit_candidate_range(&ranges, 45, |range| range.0, |range| range.1),
        4..5
    );
}

#[test]
fn vertical_hit_candidate_range_keeps_boundary_hits_inclusive() {
    let ranges = [(0, 9), (10, 19), (20, 29)];

    assert_eq!(
        vertical_hit_candidate_range(&ranges, 10, |range| range.0, |range| range.1),
        1..2
    );
    assert_eq!(
        vertical_hit_candidate_range(&ranges, 19, |range| range.0, |range| range.1),
        1..2
    );
}

#[test]
fn vertical_hit_candidate_range_keeps_overlapping_candidates() {
    let ranges = [(0, 20), (10, 30), (20, 40), (50, 60)];

    assert_eq!(
        vertical_hit_candidate_range(&ranges, 20, |range| range.0, |range| range.1),
        0..3
    );
}
